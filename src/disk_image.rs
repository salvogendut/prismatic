//! Blank hard disks, prepared off the UI thread and published without replacement.
use crate::media::{MediaKind, Medium};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub const MAX_MIB: u64 = 1_048_576; // 1 TiB, within the guest's 32-bit sector addressing.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Raw,
    Chd,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Raw => "Raw (.img / .raw / .dsk / .hdd)",
            Self::Chd => "CHD (.chd)",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Raw => "img",
            Self::Chd => "chd",
        }
    }
}

pub fn parse_size(value: &str) -> Result<u64, String> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|&n| (1..=MAX_MIB).contains(&n))
        .ok_or_else(|| format!("Enter a whole number from 1 to {MAX_MIB} MiB"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub format: Format,
    pub size_mib: u64,
    pub path: PathBuf,
}

impl Request {
    fn destination(&self) -> Result<PathBuf, String> {
        parse_size(&self.size_mib.to_string())?;
        if self.format == Format::Chd && !cfg!(iris_chd_cow) {
            return Err("CHD hard disks are unavailable in this Iris build; choose Raw".into());
        }
        let mut path = self.path.clone();
        if path.file_name().is_none() || path.to_str().is_none() {
            return Err("Choose a disk filename using UTF-8 characters".into());
        }
        if path.extension().is_none() {
            path.set_extension(self.format.extension());
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let valid = match self.format {
            Format::Raw => matches!(extension.as_str(), "img" | "raw" | "dsk" | "hdd"),
            Format::Chd => extension == "chd",
        };
        if !valid {
            return Err(format!(
                "Choose a filename ending in {}",
                self.format.label()
            ));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = fs::canonicalize(parent).map_err(|e| format!("Destination folder: {e}"))?;
        let path = parent.join(path.file_name().unwrap());
        if path.to_str().is_none() {
            return Err("Destination path must use UTF-8".into());
        }
        Ok(path)
    }
}

fn require_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
        Ok(_) => Err(format!(
            "Already exists: {}. Choose another filename.",
            path.display()
        )),
    }
}

fn check_destination(path: &Path, format: Format) -> Result<(), String> {
    require_absent(path)?;
    // Never accidentally reattach data left over from a deleted base image.
    for suffix in [".overlay", ".overlay.dirty", ".diff.chd"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        require_absent(Path::new(&name))?;
    }
    #[cfg(iris_chd_cow)]
    if format == Format::Chd {
        require_absent(&iris::chd_disk::diff_path_for(path))?;
    }
    let _ = format;
    Ok(())
}

fn create(
    request: &Request,
    cancelled: &AtomicBool,
    progress: &AtomicU8,
) -> Result<Option<Medium>, String> {
    let cancelled = || cancelled.load(Ordering::Relaxed);
    if cancelled() {
        return Ok(None);
    }
    let path = request.destination()?;
    check_destination(&path, request.format)?;
    let temporary = tempfile::Builder::new()
        .prefix(".prismatic-disk-")
        .suffix(&format!(".{}", request.format.extension()))
        .tempfile_in(path.parent().unwrap())
        .map_err(|e| format!("Cannot create disk: {e}"))?
        // Let the native CHD writer reopen the path without a competing
        // handle, while retaining automatic cleanup on error/cancellation.
        .into_temp_path();
    let bytes = request.size_mib * 1024 * 1024;
    match request.format {
        Format::Raw => fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .and_then(|file| file.set_len(bytes))
            .map_err(|e| format!("Cannot size disk: {e}"))?,
        Format::Chd => {
            if !compress(&temporary, request.size_mib, &cancelled, progress)? {
                return Ok(None);
            }
        }
    }
    if cancelled() {
        return Ok(None);
    }
    // Inspection is read-only: no overlays or guest writes before Save/boot.
    Medium::inspect(&temporary, MediaKind::Disk)?;
    // Windows FlushFileBuffers requires write access, even after inspection.
    fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("Cannot flush disk: {e}"))?;
    check_destination(&path, request.format)?;
    if cancelled() {
        return Ok(None);
    }
    temporary
        .persist_noclobber(&path)
        .map_err(|e| format!("Cannot save disk: {}", e.error))?;
    progress.store(100, Ordering::Relaxed);
    Ok(Some(Medium {
        kind: MediaKind::Disk,
        path: path
            .to_str()
            .ok_or("Destination path must use UTF-8")?
            .into(),
    }))
}

// libchdman's compressor drains the entire image even after cancellation.
// Isolate its native workers so cancellation can stop them promptly, while
// the parent owns cleanup and is the only process allowed to publish a disk.
struct Compressor(Child);
impl Drop for Compressor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn compress(
    path: &Path,
    size_mib: u64,
    cancelled: &dyn Fn() -> bool,
    progress: &AtomicU8,
) -> Result<bool, String> {
    let progress_file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let errors = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(not(test))]
    command
        .arg("--create-chd-worker")
        .arg(path)
        .arg(size_mib.to_string())
        .arg(progress_file.path());
    #[cfg(test)]
    command
        .args([
            "--ignored",
            "--exact",
            "disk_image::tests::chd_worker_entry",
            "--nocapture",
        ])
        .env("PRISMATIC_TEST_CHD_PATH", path)
        .env("PRISMATIC_TEST_CHD_MIB", size_mib.to_string())
        .env("PRISMATIC_TEST_CHD_PROGRESS", progress_file.path());
    let mut child = Compressor(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(errors.reopen().map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|e| format!("Cannot start CHD writer: {e}"))?,
    );
    loop {
        if cancelled() {
            return Ok(false);
        } // Drop kills and reaps before temporary files are removed.
        if let Some(status) = child.0.try_wait().map_err(|e| format!("CHD writer: {e}"))? {
            if status.success() {
                return Ok(true);
            }
            let detail = fs::read_to_string(errors.path()).unwrap_or_default();
            return Err(format!(
                "CHD writer failed ({status}): {}",
                detail.trim().chars().take(512).collect::<String>()
            ));
        }
        if let Ok(value) = fs::read_to_string(progress_file.path())
            && let Ok(value) = value.parse::<u8>()
        {
            progress.store(value.min(99), Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn write_chd(path: &Path, size_mib: u64, progress: &Path) -> Result<(), String> {
    let bytes = parse_size(&size_mib.to_string())? * 1024 * 1024;
    let mut previous = None;
    libchdman_rs::hd::create_from_reader(
        std::io::empty(),
        path,
        libchdman_rs::hd::HdCreateOptions {
            logical_size: bytes,
            ..Default::default()
        },
        &mut |p| {
            let percent = (p.bytes_done.saturating_mul(100) / bytes).min(99) as u8;
            if previous != Some(percent) {
                let _ = fs::write(progress, percent.to_string());
                previous = Some(percent);
            }
        },
        &|| false,
    )
    .map_err(|e| format!("Cannot create CHD: {e:?}"))
}

/// Internal subprocess entry, handled before SDL or configuration is opened.
pub fn worker_entry() -> Option<Result<(), String>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--create-chd-worker")) {
        return None;
    }
    Some((|| {
        let path = PathBuf::from(args.next().ok_or("Missing CHD output")?);
        let size = args.next().ok_or("Missing CHD capacity")?;
        let size = parse_size(size.to_str().ok_or("Invalid CHD capacity")?)?;
        let progress = PathBuf::from(args.next().ok_or("Missing CHD progress file")?);
        if args.next().is_some() {
            return Err("Unexpected CHD worker arguments".into());
        }
        write_chd(&path, size, &progress)
    })())
}

pub struct Job {
    receiver: mpsc::Receiver<Result<Option<Medium>, String>>,
    thread: Option<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    progress: Arc<AtomicU8>,
}

impl Job {
    pub fn start(request: Request) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU8::new(0));
        let (cancel, completed) = (cancelled.clone(), progress.clone());
        let thread = thread::Builder::new()
            .name("prismatic-new-disk".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| create(&request, &cancel, &completed))
                    .unwrap_or_else(|_| Err("Disk creation stopped unexpectedly".into()));
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            receiver,
            thread: Some(thread),
            cancelled,
            progress,
        })
    }
    pub fn progress(&self) -> u8 {
        self.progress.load(Ordering::Relaxed)
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn poll(&self) -> Option<Result<Option<Medium>, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Disk creation stopped unexpectedly".into())),
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};

    #[test]
    #[ignore = "subprocess entry used by disk creation tests"]
    fn chd_worker_entry() {
        write_chd(
            &PathBuf::from(std::env::var_os("PRISMATIC_TEST_CHD_PATH").unwrap()),
            parse_size(&std::env::var("PRISMATIC_TEST_CHD_MIB").unwrap()).unwrap(),
            &PathBuf::from(std::env::var_os("PRISMATIC_TEST_CHD_PROGRESS").unwrap()),
        )
        .unwrap();
    }

    fn request(path: PathBuf, format: Format) -> Request {
        Request {
            path,
            format,
            size_mib: 1,
        }
    }
    fn run(request: &Request) -> Result<Option<Medium>, String> {
        create(request, &AtomicBool::new(false), &AtomicU8::new(0))
    }

    fn writable_without_changing_base(medium: &Medium) {
        let before = fs::read(&medium.path).unwrap();
        let mut node = crate::media::open_node(medium).unwrap();
        let mut buffer = [0xff; 512];
        node.read(1024 * 1024 - 512, &mut buffer).unwrap();
        assert_eq!(buffer, [0; 512]);
        node.write(512, &[0x5a; 512]).unwrap();
        if let Some(background) = node.background() {
            background.sync_pending().unwrap();
        }
        node.flush_data().unwrap();
        node.flush().unwrap();
        drop(node);
        let mut node = crate::media::open_node(medium).unwrap();
        node.read(512, &mut buffer).unwrap();
        assert_eq!(buffer, [0x5a; 512]);
        assert_eq!(fs::read(&medium.path).unwrap(), before);
    }

    #[test]
    fn raw_capacity_zero_content_extensions_and_protected_guest_writes() {
        let dir = tempfile::tempdir().unwrap();
        for extension in ["img", "raw", "dsk", "hdd"] {
            let path = dir.path().join(format!("disk.{extension}"));
            let medium = run(&request(path.clone(), Format::Raw)).unwrap().unwrap();
            let mut file = fs::File::open(&path).unwrap();
            assert_eq!(file.metadata().unwrap().len(), 1024 * 1024);
            let mut bytes = [1; 512];
            for offset in [0, 512, 1024 * 1024 - 512] {
                file.seek(SeekFrom::Start(offset)).unwrap();
                file.read_exact(&mut bytes).unwrap();
                assert_eq!(bytes, [0; 512]);
            }
            assert!(!PathBuf::from(format!("{}.overlay", path.display())).exists());
            writable_without_changing_base(&medium);
        }
        let medium = run(&request(dir.path().join("extensionless"), Format::Raw))
            .unwrap()
            .unwrap();
        assert!(medium.path.ends_with("extensionless.img"));
    }

    #[test]
    #[cfg(iris_chd_cow)]
    fn chd_has_capacity_geometry_hash_zero_sectors_and_writable_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        let medium = run(&request(path.clone(), Format::Chd)).unwrap().unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        {
            let chd = libchdman_rs::Chd::open(&medium.path, false, None).unwrap();
            assert_eq!(chd.logical_bytes(), 1024 * 1024);
            let geometry = libchdman_rs::hd::read_geometry(&chd).unwrap();
            assert_eq!(geometry.sector_bytes, 512);
            assert_eq!(geometry.logical_bytes(), chd.logical_bytes());
            assert_ne!(chd.sha1(), [0; 20]);
            let mut bytes = [1; 512];
            for offset in [0, 512, 1024 * 1024 - 512] {
                chd.read_bytes(offset, &mut bytes).unwrap();
                assert_eq!(bytes, [0; 512]);
            }
        }
        assert!(fs::metadata(path).unwrap().len() < 1024 * 1024);
        writable_without_changing_base(&medium);
    }

    #[test]
    fn invalid_requests_existing_files_and_old_sidecars_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.img");
        for size in [0, MAX_MIB + 1, u64::MAX] {
            let mut r = request(path.clone(), Format::Raw);
            r.size_mib = size;
            assert!(run(&r).is_err());
        }
        for name in ["disk.chd", "disk.iso", "disk.qcow2"] {
            assert!(run(&request(dir.path().join(name), Format::Raw)).is_err());
        }
        assert!(run(&request(dir.path().join("missing/disk.img"), Format::Raw)).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        for suffix in ["", ".overlay", ".overlay.dirty", ".diff.chd"] {
            let existing = PathBuf::from(format!("{}{suffix}", path.display()));
            fs::write(&existing, b"keep me").unwrap();
            assert!(run(&request(path.clone(), Format::Raw)).is_err());
            assert_eq!(fs::read(&existing).unwrap(), b"keep me");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
            fs::remove_file(existing).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing-target", &path).unwrap();
            assert!(run(&request(path.clone(), Format::Raw)).is_err());
            assert_eq!(fs::read_link(path).unwrap(), Path::new("missing-target"));
        }
    }

    #[test]
    fn cancel_before_creation_and_drop_during_compression_remove_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let r = request(dir.path().join("disk.img"), Format::Raw);
        assert!(
            create(&r, &AtomicBool::new(true), &AtomicU8::new(0))
                .unwrap()
                .is_none()
        );
        #[cfg(iris_chd_cow)]
        {
            let mut r = request(dir.path().join("large.chd"), Format::Chd);
            r.size_mib = 4096;
            let job = Job::start(r).unwrap();
            // Wait for actual compression, then require prompt cancellation.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while job.progress() == 0 {
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let start = std::time::Instant::now();
            drop(job); // cancellation + join, never a detached writer
            assert!(start.elapsed() < Duration::from_secs(3));
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn size_parser_rejects_fractions_overflow_and_units() {
        assert_eq!(parse_size(" 4096 ").unwrap(), 4096);
        for value in ["0", "-1", "1.5", "2 GiB", "1048577", "18446744073709551616"] {
            assert!(parse_size(value).is_err());
        }
    }

    #[test]
    #[cfg(not(iris_chd_cow))]
    fn unavailable_chd_creation_leaves_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let error = run(&request(dir.path().join("disk.chd"), Format::Chd)).unwrap_err();
        assert!(error.contains("unavailable"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
