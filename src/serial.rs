//! Frontend-owned, nonblocking serial transports. Created only on the worker.
use iris::{config::MachineConfig, dev::serial_host::SerialBackend};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Tcp,
    Pty,
    Off,
}

impl Route {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::Tcp if cfg!(unix) => Self::Pty,
            Self::Tcp | Self::Pty => Self::Off,
            Self::Off => Self::Tcp,
        };
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Pty => "Pseudoterminal",
            Self::Off => "Off",
        }
    }

    pub fn tcp_port(self, port: u16) -> Option<u16> {
        (self == Self::Tcp).then_some(port)
    }

    pub fn validate(self) -> Result<(), String> {
        if self == Self::Pty && !cfg!(unix) {
            return Err("Serial pseudoterminals require a Unix host; choose TCP or Off".into());
        }
        Ok(())
    }
}

struct Endpoint {
    backend: Arc<Transport>,
    description: String,
    dropped: Arc<AtomicU64>,
}

// Some Iris device graphs outlive their Workstation. Revoke access explicitly
// when the worker releases an endpoint, even if a device still holds this Arc.
struct Transport(Mutex<Option<Arc<dyn SerialBackend>>>);

impl SerialBackend for Transport {
    fn send_byte(&self, byte: u8) {
        if let Some(backend) = self.0.lock().unwrap().as_ref() {
            backend.send_byte(byte);
        }
    }

    fn recv_byte(&self) -> std::io::Result<u8> {
        match self.0.lock().unwrap().as_ref() {
            Some(backend) => backend.recv_byte(),
            None => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.backend
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

impl Endpoint {
    fn open(route: Route, port: u16) -> Result<Self, String> {
        route.validate()?;
        let dropped = Arc::new(AtomicU64::new(0));
        let (backend, description) = match route {
            Route::Tcp => {
                let (backend, bound) = iris::dev::serial_host::tcp_serial_backend(port)
                    .ok_or_else(|| {
                        format!("cannot bind TCP port {port}; choose a free port or auto")
                    })?;
                (backend, format!("TCP 127.0.0.1:{bound}"))
            }
            Route::Off => (iris::dev::serial_host::null_serial_backend(), "Off".into()),
            #[cfg(unix)]
            Route::Pty => {
                let (pty, path) = pty::Pty::open(dropped.clone())
                    .map_err(|e| format!("cannot create PTY: {e}"))?;
                (
                    Arc::new(pty) as Arc<dyn SerialBackend>,
                    format!("PTY {}", path.display()),
                )
            }
            #[cfg(not(unix))]
            Route::Pty => unreachable!("validated above"),
        };
        Ok(Self {
            backend: Arc::new(Transport(Mutex::new(Some(backend)))),
            description,
            dropped,
        })
    }
}

pub struct Endpoints([Endpoint; 2]);

impl Endpoints {
    /// Bind both endpoints before installing either. Failure releases everything.
    pub fn prepare(
        config: &mut MachineConfig,
        settings: &crate::expert::ExpertSettings,
    ) -> Result<Self, String> {
        let a = Endpoint::open(settings.serial_a, settings.serial_port_a)
            .map_err(|e| format!("Serial A: {e}"))?;
        let b = Endpoint::open(settings.serial_b, settings.serial_port_b)
            .map_err(|e| format!("Serial B: {e}"))?;
        config.serial_backends.a = Some(a.backend.clone());
        config.serial_backends.b = Some(b.backend.clone());
        let endpoints = Self([a, b]);
        for index in 0..2 {
            eprintln!("Prismatic: {}", endpoints.describe(index));
        }
        Ok(endpoints)
    }

    pub fn describe(&self, index: usize) -> String {
        let endpoint = &self.0[index];
        let mut line = format!(
            "Serial {}: {}",
            if index == 0 { "A" } else { "B" },
            endpoint.description
        );
        let dropped = endpoint.dropped.load(Ordering::Relaxed);
        if dropped != 0 {
            line.push_str(&format!(" ({dropped} output bytes dropped)"));
        }
        line
    }
}

#[cfg(unix)]
mod pty {
    use super::*;
    use nix::{
        fcntl::{FcntlArg, FdFlag, OFlag, fcntl},
        pty::openpty,
        sys::termios::{ControlFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr},
        unistd::ttyname,
    };
    use std::{
        collections::VecDeque,
        fs::File,
        io::{self, Read, Write},
        path::PathBuf,
        sync::Mutex,
    };

    // The kernel PTY buffer plus a bounded host queue retain early boot output.
    // A disconnected or slow terminal must never stall a CPU/device thread.
    const CAPACITY: usize = 64 * 1024;

    struct Io {
        master: File,
        pending: VecDeque<u8>,
    }

    pub(super) struct Pty {
        io: Mutex<Io>,
        // Keep the slave alive so disconnect/reconnect does not cause EIO.
        _slave: File,
        dropped: Arc<AtomicU64>,
    }

    impl Pty {
        pub(super) fn open(dropped: Arc<AtomicU64>) -> io::Result<(Self, PathBuf)> {
            let pair = openpty(None, None)?;
            let path = ttyname(&pair.slave)?;
            for fd in [&pair.master, &pair.slave] {
                fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
            }
            let mut termios = tcgetattr(&pair.slave)?;
            cfmakeraw(&mut termios);
            termios
                .control_flags
                .insert(ControlFlags::CLOCAL | ControlFlags::CREAD);
            termios.control_flags.remove(ControlFlags::CRTSCTS);
            tcsetattr(&pair.slave, SetArg::TCSANOW, &termios)?;
            let flags = OFlag::from_bits_truncate(fcntl(&pair.master, FcntlArg::F_GETFL)?);
            fcntl(&pair.master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
            Ok((
                Self {
                    io: Mutex::new(Io {
                        master: pair.master.into(),
                        pending: VecDeque::new(),
                    }),
                    _slave: pair.slave.into(),
                    dropped,
                },
                path,
            ))
        }

        fn flush(&self, io: &mut Io) {
            while !io.pending.is_empty() {
                let bytes = io.pending.as_slices().0;
                match io.master.write(bytes) {
                    Ok(0) => break,
                    Ok(written) => {
                        io.pending.drain(..written);
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        self.dropped
                            .fetch_add(io.pending.len() as u64, Ordering::Relaxed);
                        io.pending.clear();
                        break;
                    }
                }
            }
        }
    }

    impl SerialBackend for Pty {
        fn send_byte(&self, byte: u8) {
            let mut io = self.io.lock().unwrap();
            self.flush(&mut io);
            if io.pending.len() < CAPACITY {
                io.pending.push_back(byte);
                self.flush(&mut io);
            } else {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }

        fn recv_byte(&self) -> io::Result<u8> {
            let mut io = self.io.lock().unwrap();
            self.flush(&mut io);
            let mut byte = [0];
            match io.master.read(&mut byte)? {
                1 => Ok(byte[0]),
                _ => Err(io::ErrorKind::WouldBlock.into()),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::{Duration, Instant};

        fn terminal(path: &PathBuf) -> File {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .unwrap();
            fcntl(&file, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).unwrap();
            file
        }

        #[test]
        fn pty_is_raw_bidirectional_reconnectable_and_hangs_up_on_drop() {
            let (pty, path) = Pty::open(Arc::new(AtomicU64::new(0))).unwrap();
            let mut client = terminal(&path);
            let bytes: Vec<_> = (0..=255).collect();
            for &byte in &bytes {
                pty.send_byte(byte);
            }
            let mut received = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(2);
            while received.len() < bytes.len() && Instant::now() < deadline {
                let mut buf = [0; 256];
                match client.read(&mut buf) {
                    Ok(n) => received.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            }
            assert_eq!(received, bytes); // No echo, CR/LF conversion or flow control.
            assert_eq!(
                pty.recv_byte().unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            client.write_all(&bytes).unwrap();
            let mut received = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(2);
            while received.len() < bytes.len() && Instant::now() < deadline {
                match pty.recv_byte() {
                    Ok(b) => received.push(b),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            }
            assert_eq!(received, bytes);
            drop(client);
            pty.send_byte(b'r');
            let mut client = terminal(&path);
            let mut byte = [0];
            client.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [b'r']);
            drop(pty);
            assert!(
                tcgetattr(&client).is_err(),
                "old terminal must hang up after unload"
            );
        }

        #[test]
        fn a_slow_or_absent_reader_cannot_grow_the_queue_or_stall_output() {
            let dropped = Arc::new(AtomicU64::new(0));
            let (pty, _) = Pty::open(dropped.clone()).unwrap();
            for _ in 0..CAPACITY * 4 {
                pty.send_byte(b'x');
            }
            assert!(pty.io.lock().unwrap().pending.len() <= CAPACITY);
            assert!(dropped.load(Ordering::Relaxed) > 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expert::ExpertSettings;
    use std::net::TcpListener;

    #[test]
    fn automatic_tcp_ports_are_reported_and_released_and_failed_pairs_are_atomic() {
        let mut config = MachineConfig::default();
        let endpoints = Endpoints::prepare(&mut config, &ExpertSettings::default()).unwrap();
        let addresses: Vec<_> = (0..2)
            .map(|i| {
                endpoints.0[i]
                    .description
                    .strip_prefix("TCP ")
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_ne!(addresses[0], addresses[1]);
        for address in &addresses {
            assert!(TcpListener::bind(address).is_err());
        }
        drop(endpoints);
        // Old device/config handles must not keep a listening socket alive.
        assert!(
            config
                .serial_backends
                .a
                .as_ref()
                .unwrap()
                .recv_byte()
                .is_err()
        );
        let first = TcpListener::bind(&addresses[0]).unwrap();
        let second = TcpListener::bind(&addresses[1]).unwrap();
        let port_a = first.local_addr().unwrap().port();
        let port_b = second.local_addr().unwrap().port();
        drop(first);
        let settings = ExpertSettings {
            serial_port_a: port_a,
            serial_port_b: port_b,
            ..Default::default()
        };
        let mut config = MachineConfig::default();
        let error = Endpoints::prepare(&mut config, &settings).err().unwrap();
        assert!(error.contains("Serial B"));
        assert!(config.serial_backends.a.is_none());
        assert!(config.serial_backends.b.is_none());
        let _released = TcpListener::bind(&addresses[0]).unwrap();
    }

    #[test]
    fn off_does_not_bind_saved_ports() {
        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();
        let settings = ExpertSettings {
            serial_a: Route::Off,
            serial_b: Route::Off,
            serial_port_a: port,
            serial_port_b: port,
            ..Default::default()
        };
        settings.validate().unwrap();
        let mut config = MachineConfig::default();
        let endpoints = Endpoints::prepare(&mut config, &settings).unwrap();
        assert_eq!(endpoints.describe(0), "Serial A: Off");
        assert_eq!(endpoints.describe(1), "Serial B: Off");
        config.serial_backends.a.as_ref().unwrap().send_byte(b'x');
        assert!(
            config
                .serial_backends
                .b
                .as_ref()
                .unwrap()
                .recv_byte()
                .is_err()
        );
    }
}
