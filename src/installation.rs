//! Optional install server owned by a single machine worker, never the SDL thread.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const IMAGE: &str = "localhost/prismatic-instigator:2db7231aa257";
const CONFIG: &str = "instigator.yaml";
const LOG_LINES: usize = 32;
const LINE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Off,
    Native,
    Podman,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub backend: Backend,
    pub folder: Option<String>,
    pub program: String,
    pub image: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            backend: Backend::Off,
            folder: None,
            program: "instigator".into(),
            image: IMAGE.into(),
        }
    }
}
impl Settings {
    pub fn enabled(&self) -> bool {
        self.backend != Backend::Off
    }
    pub fn cycle(&mut self) {
        self.backend = match self.backend {
            Backend::Off => Backend::Native,
            Backend::Native if cfg!(target_os = "linux") => Backend::Podman,
            _ => Backend::Off,
        };
    }
    pub fn validate(&self) -> Result<(), String> {
        validate_program(&self.program)?;
        validate_image(&self.image)?;
        if self
            .folder
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.contains('\0'))
        {
            return Err("Choose an installation setup folder".into());
        }
        Ok(())
    }
    pub fn validate_runtime(&self) -> Result<(), String> {
        self.validate()?;
        if !cfg!(unix) {
            return Err("Managed install servers currently require Linux or macOS; use an external TCP stream server".into());
        }
        if self.backend == Backend::Podman
            && (!cfg!(target_os = "linux") || Path::new("/.flatpak-info").exists())
        {
            return Err("Managed Podman requires native Linux Prismatic; in Flatpak use a native helper or external stream server".into());
        }
        if self.folder.is_none() {
            return Err(
                "Choose a setup folder containing instigator.yaml and your installation media"
                    .into(),
            );
        }
        Ok(())
    }
    /// File checks only: Save never starts an executable or a container.
    pub fn validate_files(&mut self) -> Result<(), String> {
        self.validate_runtime()?;
        let folder = fs::canonicalize(self.folder.as_ref().unwrap())
            .map_err(|e| format!("Installation folder: {e}"))?;
        let config = fs::canonicalize(folder.join(CONFIG))
            .map_err(|e| format!("Installation needs {CONFIG}: {e}"))?;
        if !config.starts_with(&folder) || !config.is_file() {
            return Err("instigator.yaml must be a file inside the setup folder".into());
        }
        let len = fs::File::open(&config)
            .and_then(|f| f.metadata())
            .map_err(|e| format!("Cannot read instigator.yaml: {e}"))?
            .len();
        if len == 0 || len > 1024 * 1024 {
            return Err("instigator.yaml must be nonempty and at most 1 MiB".into());
        }
        let folder = folder.to_str().ok_or("Installation folder must be UTF-8")?;
        if self.backend == Backend::Podman && folder.contains(':') {
            return Err("Podman setup folder paths cannot contain a colon".into());
        }
        self.folder = Some(folder.into());
        Ok(())
    }
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "Install server",
                match self.backend {
                    Backend::Off => "Off / external server",
                    Backend::Native => "Native Instigator",
                    Backend::Podman => "Temporary Podman container",
                }
                .into(),
            ),
            (
                "Setup folder",
                self.folder
                    .clone()
                    .unwrap_or_else(|| "Unset - Enter to choose".into()),
            ),
            ("Instigator executable", self.program.clone()),
            ("Podman image", self.image.clone()),
            (
                "Use installation network",
                "Select Stream; F9 saves, Ctrl+P boots".into(),
            ),
            (
                "Installation status",
                "Show running server and recent messages".into(),
            ),
        ]
    }
    pub fn hints(&self) -> Vec<String> {
        vec![
            "Setup folder: instigator.yaml + media. Use relative media paths; select a suitable blank disk in Media.".into(),
            "At boot: check media, start server, connect the guest. No host BOOTP/TFTP/RSH ports are opened.".into(),
            "Install manually in the PROM/inst. Return to NAT and Save when finished; the helper stops with the machine.".into(),
        ]
    }
}
pub fn validate_program(text: &str) -> Result<(), String> {
    if text.is_empty()
        || text.contains('\0')
        || text.starts_with('-')
        || (!Path::new(text).is_absolute()
            && (text.contains('/') || text.contains('\\') || text.chars().any(char::is_whitespace)))
    {
        return Err("Use an executable name on PATH or an absolute executable path".into());
    }
    Ok(())
}
pub fn validate_image(text: &str) -> Result<(), String> {
    if text.is_empty()
        || text.len() > 512
        || text.starts_with('-')
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-:@".contains(&b))
    {
        return Err("Use a Podman image name or digest, without command arguments".into());
    }
    Ok(())
}

#[derive(Clone, Default)]
struct Log(Arc<Mutex<VecDeque<String>>>);
impl Log {
    fn push(&self, bytes: &[u8]) {
        let text: String = String::from_utf8_lossy(bytes)
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        if text.is_empty() {
            return;
        }
        let mut lines = self.0.lock().unwrap();
        if lines.len() == LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(text);
    }
    fn read(&self, mut input: impl Read) {
        let mut buf = [0; 512];
        let mut line = Vec::new();
        while let Ok(n) = input.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if b == b'\n' || line.len() == LINE_BYTES {
                    self.push(&line);
                    line.clear();
                }
                if b != b'\n' {
                    line.push(b);
                }
            }
        }
        self.push(&line);
    }
    fn tail(&self) -> String {
        self.0
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

struct Process {
    child: Child,
    log: Log,
    readers: Vec<thread::JoinHandle<()>>,
    container: Option<String>,
}
impl Process {
    fn spawn(mut command: Command, container: Option<String>) -> Result<Self, String> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start install helper: {e}"))?;
        let log = Log::default();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let readers = [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)]
            .into_iter()
            .map(|input| {
                let log = log.clone();
                thread::spawn(move || log.read(input))
            })
            .collect();
        Ok(Self {
            child,
            log,
            readers,
            container,
        })
    }
    fn exited(&mut self) -> Result<Option<std::process::ExitStatus>, String> {
        self.child
            .try_wait()
            .map_err(|e| format!("Install helper status: {e}"))
    }
    fn wait(&mut self, stop: &AtomicBool, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if stop.load(Ordering::Acquire) {
                return Err("Installation helper startup cancelled".into());
            }
            if let Some(status) = self.exited()? {
                self.join();
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "Installation media check failed ({status}):\n{}",
                        self.log.tail()
                    ))
                };
            }
            if Instant::now() > deadline {
                return Err("Installation media check timed out (120 seconds)".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn join(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGTERM,
            );
            let deadline = Instant::now() + Duration::from_millis(500);
            while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
        }
        if let Some(name) = &self.container {
            // An attached podman client can exit while its container is still alive.
            // Remove only our unique container, also on timeout or startup failure.
            if let Ok(mut cleanup) = Command::new("podman")
                .args(["rm", "--force", "--time", "1", name])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let deadline = Instant::now() + Duration::from_secs(5);
                while matches!(cleanup.try_wait(), Ok(None)) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(20));
                }
                let _ = cleanup.kill();
                let _ = cleanup.wait();
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.join();
    }
}

/// The private directory and process outlive Iris, then are removed together.
pub struct Server {
    process: Process,
    _runtime: tempfile::TempDir,
    endpoint: String,
}
impl Server {
    pub fn start(settings: &Settings, stop: &AtomicBool) -> Result<Self, String> {
        settings.validate_runtime()?;
        if stop.load(Ordering::Acquire) {
            return Err("Installation helper startup cancelled".into());
        }
        let runtime = tempfile::Builder::new()
            .prefix("prismatic-install-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let socket = runtime.path().join("net.sock");
        // sockaddr_un is small, including on macOS where temp paths can be long.
        if socket.as_os_str().len() >= 100 {
            return Err("Installation socket path is too long; use a shorter TMPDIR".into());
        }
        let name = runtime
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let (command, container) = helper_command(settings, runtime.path(), &name, false)?;
        Process::spawn(command, container)?.wait(stop, Duration::from_secs(120))?;
        if stop.load(Ordering::Acquire) {
            return Err("Installation helper startup cancelled".into());
        }
        let (command, container) = helper_command(settings, runtime.path(), &name, true)?;
        let mut process = Process::spawn(command, container)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if stop.load(Ordering::Acquire) {
                return Err("Installation helper startup cancelled".into());
            }
            if let Some(status) = process.exited()? {
                process.join();
                return Err(format!(
                    "Install helper exited before listening ({status}):\n{}",
                    process.log.tail()
                ));
            }
            if socket.exists() {
                break;
            } // Never probe-connect: Instigator accepts one emulator.
            if Instant::now() > deadline {
                return Err(format!(
                    "Install helper did not create its socket:\n{}",
                    process.log.tail()
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(Self {
            process,
            endpoint: format!("unix:{}", socket.display()),
            _runtime: runtime,
        })
    }
    #[cfg(iris_stream_network)]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn report(&mut self) -> String {
        let state = match self.process.exited() {
            Ok(None) => "Running".into(),
            Ok(Some(status)) => {
                format!("Exited ({status}); installation success is not established")
            }
            Err(error) => error,
        };
        format!(
            "Install server: {state}\n{}\n{}",
            self.endpoint,
            self.process.log.tail()
        )
    }
}

fn helper_command(
    settings: &Settings,
    runtime: &Path,
    name: &str,
    serve: bool,
) -> Result<(Command, Option<String>), String> {
    let folder = settings
        .folder
        .as_ref()
        .ok_or("No installation setup folder")?;
    let (mut cmd, container, socket, config, cache) = match settings.backend {
        Backend::Native => (
            Command::new(&settings.program),
            None,
            runtime.join("net.sock"),
            PathBuf::from(folder).join(CONFIG),
            runtime.join("cache"),
        ),
        Backend::Podman => {
            let mut cmd = Command::new("podman");
            cmd.args([
                "run",
                "--rm",
                "--pull=never",
                "--name",
                name,
                "--network=none",
                "--read-only",
                "--cap-drop=all",
                "--userns=keep-id",
                "--security-opt=no-new-privileges",
                "--security-opt=label=disable",
                "--tmpfs=/tmp:rw,nosuid,noexec,size=64m",
                "--workdir=/install",
            ]);
            cmd.arg("--volume").arg(format!("{folder}:/install:ro"));
            cmd.arg("--volume")
                .arg(format!("{}:/run/prismatic:rw", runtime.display()));
            cmd.args([
                "--env",
                "XDG_CACHE_HOME=/run/prismatic/cache",
                "--env",
                "HOME=/tmp",
                &settings.image,
            ]);
            (
                cmd,
                Some(name.to_owned()),
                PathBuf::from("/run/prismatic/net.sock"),
                PathBuf::from("/install/instigator.yaml"),
                runtime.join("cache"),
            )
        }
        Backend::Off => return Err("Installation helper is disabled".into()),
    };
    cmd.current_dir(folder)
        .env("XDG_CACHE_HOME", cache)
        .env_remove("NOTIFY_SOCKET");
    if serve {
        cmd.arg("serve").arg("--network-socket").arg(socket);
    } else {
        cmd.args(["check", "--full"]);
    }
    cmd.arg(config);
    Ok((cmd, container))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_fields_reject_arguments_and_bad_paths() {
        for text in ["", "-bad", "instigator --verbose", "./helper", "bad\0name"] {
            assert!(validate_program(text).is_err(), "{text}");
        }
        assert!(validate_program("instigator").is_ok());
        assert!(
            validate_program(
                std::env::temp_dir()
                    .join("a path/instigator")
                    .to_str()
                    .unwrap()
            )
            .is_ok()
        );
        for text in ["", "--privileged", "image --network=host", "name\n"] {
            assert!(validate_image(text).is_err());
        }
    }

    #[test]
    #[cfg(unix)]
    fn save_validation_never_executes_and_rejects_missing_or_escaped_config() {
        let folder = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            backend: Backend::Native,
            folder: Some(folder.path().to_str().unwrap().into()),
            program: "/nonexistent/helper".into(),
            ..Default::default()
        };
        assert!(settings.validate_files().is_err());
        fs::write(folder.path().join(CONFIG), b"server_ip: 192.168.0.1\n").unwrap();
        settings.validate_files().unwrap(); // Does not try the executable.
        fs::remove_file(folder.path().join(CONFIG)).unwrap();
        let other = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(other.path(), folder.path().join(CONFIG)).unwrap();
        assert!(settings.validate_files().unwrap_err().contains("inside"));
    }

    #[test]
    fn container_arguments_limit_access_and_preserve_spaces() {
        let settings = Settings {
            backend: Backend::Podman,
            folder: Some("/downloads/IRIX media".into()),
            ..Default::default()
        };
        let (cmd, name) =
            helper_command(&settings, Path::new("/tmp/run"), "unique-machine", true).unwrap();
        assert_eq!(cmd.get_program(), "podman");
        assert_eq!(name.as_deref(), Some("unique-machine"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert!(args.contains(&"/downloads/IRIX media:/install:ro"));
        assert!(args.contains(&"--pull=never"));
        assert!(args.contains(&"--network=none"));
        assert!(args.contains(&"--read-only"));
        assert!(args.contains(&"--cap-drop=all"));
        assert!(args.ends_with(&[
            IMAGE,
            "serve",
            "--network-socket",
            "/run/prismatic/net.sock",
            "/install/instigator.yaml"
        ]));
    }

    #[test]
    fn log_is_bounded_even_without_newlines() {
        let log = Log::default();
        log.read(std::io::Cursor::new(vec![b'x'; 1024 * 1024]));
        let lines = log.0.lock().unwrap();
        assert_eq!(lines.len(), LOG_LINES);
        assert!(lines.iter().all(|line| line.len() <= LINE_BYTES));
    }

    #[test]
    #[cfg(unix)]
    fn failed_check_reports_stderr_and_cancellation_reaps_child() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo 'missing Foundation media' >&2; exit 9"]);
        let mut process = Process::spawn(cmd, None).unwrap();
        let error = process
            .wait(&AtomicBool::new(false), Duration::from_secs(2))
            .unwrap_err();
        assert!(error.contains("missing Foundation media"));
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let mut process = Process::spawn(cmd, None).unwrap();
        let started = Instant::now();
        assert!(
            process
                .wait(&AtomicBool::new(true), Duration::from_secs(60))
                .unwrap_err()
                .contains("cancelled")
        );
        drop(process);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
