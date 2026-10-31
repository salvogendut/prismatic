//! Image selection, read-only validation and Iris storage adapters.
use crate::{
    activity::{self, Activity},
    settings::Settings,
};
use iris::{
    block_node::{BlockNode, BlockStatus, ChdCdNode, CowNode, RawNode, open_chd_cow},
    config::{ScsiDeviceConfig, ScsiKind},
    scsi::{ScsiDevice, ScsiIdentity},
    scsi_monitor::ScsiTargets,
    workstation::Workstation,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Disk,
    Cdrom,
}
impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disk => "Hard disk",
            Self::Cdrom => "CD-ROM",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Medium {
    pub kind: MediaKind,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaSettings {
    pub devices: BTreeMap<u8, Medium>,
}

impl MediaSettings {
    pub fn validate(&self) -> Result<(), String> {
        for (&id, medium) in &self.devices {
            if !(1..=7).contains(&id) {
                return Err("Media SCSI IDs must be 1 through 7 on controller 0".into());
            }
            if medium.path.is_empty() || medium.path.contains('\0') {
                return Err(format!(
                    "SCSI {id}: choose an image or disconnect the device"
                ));
            }
        }
        Ok(())
    }

    pub fn resolve_paths(&mut self, directory: &Path) -> Result<(), String> {
        for medium in self.devices.values_mut() {
            if Path::new(&medium.path).is_relative() {
                medium.path = std::path::absolute(directory.join(&medium.path))
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Image paths must be UTF-8")?
                    .to_owned();
            }
        }
        Ok(())
    }

    /// Never opens a writable backend or creates overlays while editing a draft.
    pub fn validate_files(&mut self) -> Result<(), String> {
        self.validate()?;
        let mut disks = Vec::new();
        for (&id, medium) in &mut self.devices {
            *medium = Medium::inspect(Path::new(&medium.path), medium.kind)
                .map_err(|e| format!("SCSI {id}: {e}"))?;
            if medium.kind == MediaKind::Disk {
                if disks.contains(&medium.path) {
                    return Err("The same hard disk cannot occupy two SCSI targets".into());
                }
                disks.push(medium.path.clone());
            }
        }
        Ok(())
    }

    pub fn iris_devices(&self) -> BTreeMap<u8, ScsiDeviceConfig> {
        self.devices
            .iter()
            .map(|(&id, m)| {
                (
                    id,
                    ScsiDeviceConfig {
                        path: m.path.clone(),
                        kind_field: match m.kind {
                            MediaKind::Disk => ScsiKind::Disk,
                            MediaKind::Cdrom => ScsiKind::Cdrom,
                        },
                        overlay: m.kind == MediaKind::Disk,
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    pub fn row_value(&self, id: u8) -> String {
        self.devices.get(&id).map_or_else(
            || "Not connected".into(),
            |m| {
                let name = Path::new(&m.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("{}: {}", m.kind.label(), name)
            },
        )
    }
}

fn is_chd(path: &Path) -> io::Result<bool> {
    let mut header = [0; 8];
    let count = File::open(path)?.read(&mut header)?;
    Ok((count == 8 && &header == b"MComprHD") || iris::chd_disk::is_chd(&path.to_string_lossy()))
}

impl Medium {
    pub fn inspect(path: &Path, kind: MediaKind) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let name = path.to_str().ok_or("Image paths must be UTF-8")?;
        let metadata = path.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Choose a regular image file".into());
        }
        let file = File::open(&path).map_err(|e| e.to_string())?;
        let mut sample = [0; 512];
        if is_chd(&path).map_err(|e| e.to_string())? {
            match kind {
                MediaKind::Disk => {
                    let chd = libchdman_rs::Chd::open(name, false, None)
                        .map_err(|e| format!("Cannot read CHD: {e:?}"))?;
                    let geometry = libchdman_rs::hd::read_geometry(&chd)
                        .map_err(|e| format!("Not a hard-disk CHD: {e:?}"))?;
                    if geometry.sector_bytes != 512
                        || chd.logical_bytes() < 512
                        || !chd.logical_bytes().is_multiple_of(512)
                    {
                        return Err("Hard-disk CHDs must use 512-byte sectors".into());
                    }
                    chd.read_bytes(0, &mut sample)
                        .map_err(|e| format!("Cannot read CHD data: {e:?}"))?;
                }
                MediaKind::Cdrom => {
                    let mut cd =
                        ChdCdNode::open(name).map_err(|e| format!("Unsupported CD CHD: {e}"))?;
                    cd.read(0, &mut sample).map_err(|e| e.to_string())?;
                }
            }
        } else {
            if metadata.len() < 512 || !metadata.len().is_multiple_of(512) {
                return Err(
                    "Raw disk/CD data must be nonempty and aligned to 512-byte sectors".into(),
                );
            }
            let mut file = file;
            file.read_exact(&mut sample).map_err(|e| e.to_string())?;
            if sample.starts_with(b"QFI\xfb")
                || sample.starts_with(b"PK\x03\x04")
                || sample.starts_with(&[0x1f, 0x8b])
            {
                return Err(
                    "Choose a raw image or CHD, not a compressed archive or QCOW image".into(),
                );
            }
            if kind == MediaKind::Cdrom
                && sample[..12] == [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]
            {
                return Err(
                    "Raw 2352-byte CD sectors need conversion to cooked data or CHD".into(),
                );
            }
        }
        Ok(Self {
            kind,
            path: name.to_owned(),
        })
    }
}

pub enum Completion {
    Picked { id: u8, medium: Medium },
    Validated { settings: Settings, confirmed: bool },
    Cancelled,
}

#[derive(Default)]
pub struct MediaJobs {
    receiver: Option<mpsc::Receiver<Result<Completion, String>>>,
}
impl MediaJobs {
    /// The smoke harness supplies a temporary fixture instead of a native dialog.
    pub fn pick_path(&mut self, id: u8, kind: MediaKind, path: PathBuf) {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        spawn(sender, move || {
            Ok(Completion::Picked {
                id,
                medium: Medium::inspect(&path, kind)?,
            })
        });
    }
    pub fn cancel(&mut self) {
        self.receiver = None;
    }
    pub fn poll(&mut self) -> Option<Result<Completion, String>> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err("Media operation stopped unexpectedly".into()),
        };
        self.receiver = None;
        Some(result)
    }
    pub fn validate(&mut self, mut settings: Settings, confirmed: bool) {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        spawn(sender, move || {
            settings.media.validate_files()?;
            Ok(Completion::Validated {
                settings,
                confirmed,
            })
        });
    }
    pub fn pick(
        &mut self,
        window: &sdl3::video::Window,
        id: u8,
        kind: MediaKind,
        current: Option<&str>,
    ) -> Result<(), String> {
        use sdl3::dialog::{DialogError, DialogFileFilter, show_open_file_dialog};
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        let filters = [
            DialogFileFilter {
                name: "Disk and CD images",
                pattern: "chd;img;raw;dsk;hdd;iso;efs;bin;image",
            },
            DialogFileFilter {
                name: "All files",
                pattern: "*",
            },
        ];
        show_open_file_dialog(
            &filters,
            current.map(Path::new),
            false,
            window,
            Box::new(move |result, _| match result {
                Ok(paths) if paths.len() == 1 => {
                    let path = paths[0].clone();
                    spawn(sender.clone(), move || {
                        Ok(Completion::Picked {
                            id,
                            medium: Medium::inspect(&path, kind)?,
                        })
                    });
                }
                Err(DialogError::Canceled) => {
                    let _ = sender.try_send(Ok(Completion::Cancelled));
                }
                Err(error) => {
                    let _ = sender.try_send(Err(error.to_string()));
                }
                _ => {
                    let _ = sender.try_send(Err("Choose one image file".into()));
                }
            }),
        )
        .map_err(|e| {
            self.receiver = None;
            e.to_string()
        })
    }
}

fn spawn(
    sender: mpsc::SyncSender<Result<Completion, String>>,
    work: impl FnOnce() -> Result<Completion, String> + Send + 'static,
) {
    let fallback = sender.clone();
    if let Err(error) = thread::Builder::new()
        .name("prismatic-media".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err("Media validation failed unexpectedly".into()));
            let _ = sender.try_send(result);
        })
    {
        let _ = fallback.try_send(Err(error.to_string()));
    }
}

/// Block-node adapter preserves Iris's backend behavior and records actual I/O.
/// Retained error slots let the worker report flush failures after device drop.
struct MediaNode {
    inner: Box<dyn BlockNode>,
    id: u8,
    activity: Activity,
    error: Arc<Mutex<Option<String>>>,
}
impl Drop for MediaNode {
    fn drop(&mut self) {
        if let Err(error) = self.inner.flush() {
            *self.error.lock().unwrap() =
                Some(format!("SCSI {}: could not flush media: {error}", self.id));
        }
    }
}
impl BlockNode for MediaNode {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read(&mut self, offset: u64, data: &mut [u8]) -> io::Result<()> {
        self.activity.signal(activity::disk_read(self.id));
        self.inner.read(offset, data)
    }
    fn write(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.activity.signal(activity::disk_write(self.id));
        self.inner.write(offset, data)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
    fn discard(&mut self, offset: u64, len: u64) -> io::Result<()> {
        self.inner.discard(offset, len)
    }
    fn block_status(&self, offset: u64, len: u64) -> io::Result<BlockStatus> {
        self.inner.block_status(offset, len)
    }
    fn backing(&self) -> Option<&dyn BlockNode> {
        self.inner.backing()
    }
    fn dirty_count(&self) -> usize {
        self.inner.dirty_count()
    }
    fn is_cow(&self) -> bool {
        self.inner.is_cow()
    }
    fn commit(&mut self) -> io::Result<usize> {
        self.inner.commit()
    }
    fn rollback(&mut self) -> io::Result<()> {
        self.inner.rollback()
    }
    fn overlay_paths(&self) -> Option<(PathBuf, PathBuf)> {
        self.inner.overlay_paths()
    }
    fn pending_sync(&self) -> Option<(PathBuf, PathBuf)> {
        self.inner.pending_sync()
    }
    fn export_overlay(&mut self, dest: &Path) -> io::Result<Vec<u64>> {
        self.inner.export_overlay(dest)
    }
    fn import_overlay(&mut self, source: &Path, dirty: Vec<u64>) -> io::Result<()> {
        self.inner.import_overlay(source, dirty)
    }
}

#[derive(Default)]
pub struct MediaErrors(Vec<Arc<Mutex<Option<String>>>>);
impl MediaErrors {
    pub fn check(&self) -> Result<(), String> {
        let errors: Vec<_> = self
            .0
            .iter()
            .filter_map(|e| e.lock().unwrap().clone())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

pub fn attach(
    machine: &mut dyn Workstation,
    settings: &MediaSettings,
    activity: &Activity,
    errors: &mut MediaErrors,
) -> Result<(), String> {
    let mut validated = settings.clone();
    validated.validate_files()?;
    let machine = machine
        .as_any_mut()
        .downcast_mut::<iris::machine::Machine>()
        .ok_or("Media requires an Indy or Indigo2")?;
    for (&id, medium) in &validated.devices {
        let node = open_node(medium).map_err(|e| format!("SCSI {id}: {}: {e}", medium.path))?;
        let size = node.size();
        let error = Arc::new(Mutex::new(None));
        errors.0.push(error.clone());
        let node = MediaNode {
            inner: node,
            id,
            activity: activity.clone(),
            error,
        };
        let cd = medium.kind == MediaKind::Cdrom;
        let identity = if cd {
            ScsiIdentity::new(b"SONY    ", b"CD-ROM CDU-76S  ", b"1.0 ")
        } else {
            ScsiIdentity::new(b"SGI     ", b"PRISMATIC DISK  ", b"1.0 ")
        };
        let device = ScsiDevice::new(
            Box::new(node),
            size,
            cd,
            medium.path.clone(),
            vec![],
            identity,
        );
        machine
            .hpc3()
            .scsi()
            .with_targets(|targets| targets[id as usize] = Some(device));
    }
    Ok(())
}

fn open_node(medium: &Medium) -> io::Result<Box<dyn BlockNode>> {
    let path = Path::new(&medium.path);
    if is_chd(path)? {
        return match medium.kind {
            MediaKind::Disk => open_chd_cow(&medium.path),
            MediaKind::Cdrom => Ok(Box::new(ChdCdNode::open(&medium.path)?)),
        };
    }
    let raw = RawNode::open_readonly(path)?;
    if medium.kind == MediaKind::Cdrom {
        return Ok(Box::new(raw));
    }
    let size = raw.size();
    Ok(Box::new(CowNode::new(
        Some(Box::new(raw)),
        format!("{}.overlay", medium.path),
        size,
    )?))
}

/// Called only after the CPU and device threads stop, before machine teardown.
pub fn detach(machine: &mut dyn Workstation) {
    if let Some(machine) = machine
        .as_any_mut()
        .downcast_mut::<iris::machine::Machine>()
    {
        machine.hpc3().scsi().with_targets(|targets| {
            for target in targets {
                *target = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn raw(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, vec![0x5a; 65536]).unwrap();
        path
    }

    #[test]
    fn drafts_validate_without_creating_overlays_and_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let image = raw(dir.path(), "disk.img");
        let mut settings = Settings::default();
        settings.media.devices.insert(
            7,
            Medium {
                kind: MediaKind::Disk,
                path: "disk.img".into(),
            },
        );
        let config = dir.path().join("settings.toml");
        settings.save(&config).unwrap();
        let mut loaded = Settings::load(&config).unwrap();
        loaded.media.validate_files().unwrap();
        assert_eq!(loaded.media.devices[&7].path, image.to_str().unwrap());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        loaded
            .media
            .devices
            .insert(1, loaded.media.devices[&7].clone());
        assert!(
            loaded
                .media
                .validate_files()
                .unwrap_err()
                .contains("same hard disk")
        );
        loaded
            .media
            .devices
            .insert(0, loaded.media.devices[&7].clone());
        assert!(loaded.media.validate().is_err());
    }

    #[test]
    fn bad_media_is_rejected_before_attach() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Medium::inspect(dir.path(), MediaKind::Disk).is_err());
        let path = dir.path().join("bad.img");
        assert!(Medium::inspect(&path, MediaKind::Disk).is_err());
        for bytes in [
            vec![],
            vec![0; 513],
            [b"QFI\xfb".as_slice(), &[0; 508]].concat(),
            [b"MComprHD".as_slice(), &[0; 504]].concat(),
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(Medium::inspect(&path, MediaKind::Disk).is_err());
        }
    }

    fn write_and_reopen(medium: &Medium) {
        let original = fs::read(&medium.path).unwrap();
        let mut node = open_node(medium).unwrap();
        let mut data = [0; 512];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        node.write(512, &[0x23; 512]).unwrap();
        node.flush().unwrap();
        drop(node);
        let mut node = open_node(medium).unwrap();
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x23; 512]);
        node.read(1024, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        assert_eq!(fs::read(&medium.path).unwrap(), original);
    }

    #[test]
    fn raw_disk_writes_persist_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = raw(dir.path(), "disk.raw");
        write_and_reopen(&Medium::inspect(&path, MediaKind::Disk).unwrap());
        assert!(dir.path().join("disk.raw.overlay").is_file());
    }

    #[test]
    fn compressed_chd_disk_writes_persist_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        libchdman_rs::hd::create_from_reader(
            &[0x5a; 65536][..],
            &path,
            libchdman_rs::hd::HdCreateOptions {
                logical_size: 65536,
                ..Default::default()
            },
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(Medium::inspect(&path, MediaKind::Cdrom).is_err());
        write_and_reopen(&medium);
    }

    fn hashless_chd(path: &Path) {
        let mut chd =
            libchdman_rs::Chd::create(path.to_str().unwrap(), 65536, 4096, 512, [0; 4]).unwrap();
        chd.write_metadata(
            u32::from_be_bytes(*b"GDDD"),
            0,
            b"CYLS:1,HEADS:1,SECS:128,BPS:512\0",
            1,
        )
        .unwrap();
        chd.write_bytes(0, &[0x5a; 65536]).unwrap();
        assert_eq!(chd.sha1(), [0; 20]);
    }

    #[test]
    fn hashless_chd_writes_persist_and_rollback_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        hashless_chd(&path);
        let original = fs::read(&path).unwrap();
        let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        write_and_reopen(&medium);
        let mut node = open_node(&medium).unwrap();
        assert!(node.is_cow());
        assert!(node.pending_sync().is_none());
        assert_eq!(node.dirty_count(), 1);
        assert_eq!(
            node.commit().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        node.rollback().unwrap();
        node.flush().unwrap();
        drop(node);
        let mut node = open_node(&medium).unwrap();
        let mut data = [0; 512];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        assert_eq!(node.dirty_count(), 0);
        assert!(node.read(65536, &mut data).is_err());
        assert!(node.write(65536, &data).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn hashless_chd_recovers_after_legacy_empty_diff_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        hashless_chd(&path);
        let diff = iris::chd_disk::diff_path_for(&path);
        assert!(libchdman_rs::hd::HdImage::open_with_diff(&path, &diff).is_err());
        let original_diff = fs::read(&diff).unwrap();
        write_and_reopen(&Medium::inspect(&path, MediaKind::Disk).unwrap());
        assert_eq!(fs::read(diff).unwrap(), original_diff);
    }

    #[test]
    fn hashless_chd_never_bypasses_damaged_or_written_diffs() {
        for damaged in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("disk.chd");
            hashless_chd(&path);
            let diff = iris::chd_disk::diff_path_for(&path);
            assert!(libchdman_rs::hd::HdImage::open_with_diff(&path, &diff).is_err());
            if damaged {
                fs::write(&diff, b"damaged CHD").unwrap();
            } else {
                // Allocated zeroes still represent guest data, not an empty diff.
                let mut child =
                    libchdman_rs::Chd::open(diff.to_str().unwrap(), true, None).unwrap();
                child.write_bytes(512, &[0x7f; 512]).unwrap();
                child.write_bytes(512, &[0; 512]).unwrap();
            }
            let before = fs::read(&diff).unwrap();
            let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
            let error = open_node(&medium).err().unwrap();
            assert!(error.to_string().contains("preserve it for recovery"));
            assert_eq!(fs::read(diff).unwrap(), before);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn raw_cd_data_is_filesystem_agnostic_and_readonly() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["iso9660.iso", "efs.img"] {
            let path = raw(dir.path(), name);
            let medium = Medium::inspect(&path, MediaKind::Cdrom).unwrap();
            let mut node = open_node(&medium).unwrap();
            let mut data = [0; 2048];
            node.read(2048, &mut data).unwrap();
            assert_eq!(data, [0x5a; 2048]);
            assert!(node.write(0, &[0x23; 512]).is_err());
            assert_eq!(fs::read(path).unwrap(), vec![0x5a; 65536]);
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn mode1_cd_chd_reads_cooked_sectors_and_rejects_disk_selection() {
        let dir = tempfile::tempdir().unwrap();
        let iso = raw(dir.path(), "data.iso");
        let chd = dir.path().join("data.chd");
        libchdman_rs::cd::create_from_iso(&iso, &chd, Default::default(), &mut |_| {}, &|| false)
            .unwrap();
        let medium = Medium::inspect(&chd, MediaKind::Cdrom).unwrap();
        assert!(Medium::inspect(&chd, MediaKind::Disk).is_err());
        let mut node = open_node(&medium).unwrap();
        let mut data = [0; 2048];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 2048]);
        assert!(node.write(0, &[0; 512]).is_err());
    }

    struct FailingFlush;
    impl BlockNode for FailingFlush {
        fn size(&self) -> u64 {
            512
        }
        fn read(&mut self, _: u64, _: &mut [u8]) -> io::Result<()> {
            Ok(())
        }
        fn write(&mut self, _: u64, _: &[u8]) -> io::Result<()> {
            Ok(())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("disk full"))
        }
        fn discard(&mut self, _: u64, _: u64) -> io::Result<()> {
            Ok(())
        }
        fn block_status(&self, _: u64, _: u64) -> io::Result<BlockStatus> {
            Err(io::Error::other("unused"))
        }
        fn backing(&self) -> Option<&dyn BlockNode> {
            None
        }
    }

    #[test]
    fn device_io_lights_leds_and_drop_retains_flush_errors() {
        let activity = Activity::default();
        let error = Arc::new(Mutex::new(None));
        let errors = MediaErrors(vec![error.clone()]);
        let mut node = MediaNode {
            inner: Box::new(FailingFlush),
            id: 7,
            activity: activity.clone(),
            error,
        };
        assert_eq!(activity.take(), 0);
        node.read(0, &mut [0; 512]).unwrap();
        assert_eq!(activity.take(), activity::disk_read(7));
        node.write(0, &[0; 512]).unwrap();
        assert_eq!(activity.take(), activity::disk_write(7));
        drop(node);
        assert!(
            errors
                .check()
                .unwrap_err()
                .contains("SCSI 7: could not flush media: disk full")
        );
    }

    #[test]
    fn cancellation_ignores_late_picker_results() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut jobs = MediaJobs {
            receiver: Some(receiver),
        };
        jobs.cancel();
        assert!(sender.try_send(Ok(Completion::Cancelled)).is_err());
        assert!(jobs.poll().is_none());
    }
}
