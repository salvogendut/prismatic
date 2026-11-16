//! Bounded serial handoff. Device callbacks only queue bytes; the worker owns
//! the PTY and feeds a retained parser; SDL receives immutable visible pages.
use crate::{
    serial_input::SerialInput,
    serial_pty::SerialPty,
    serial_screen::{Scroll, SerialFrame, SerialScreen},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub const AVAILABLE: bool = cfg!(all(
    unix,
    iris_serial,
    iris_headless_devices,
    iris_prom,
    iris_platform_serial
));
const OUTPUT_LIMIT: usize = 64 * 1024;

pub struct Console(Mutex<State>, Mutex<SerialScreen>);
struct State {
    input: SerialInput,
    external: VecDeque<u8>,
    output: VecDeque<u8>,
    screen: Arc<SerialFrame>,
    focused: bool,
    running: bool,
    attached: bool,
    endpoint: String,
    path: String,
    dropped: u64,
    pty_dropped: u64,
}
#[derive(Clone)]
pub struct Snapshot {
    pub screen: Arc<SerialFrame>,
    pub endpoint: String,
    pub path: String,
    pub dropped: u64,
    pub attached: bool,
}
impl Default for Console {
    fn default() -> Self {
        Self(
            Mutex::new(State {
                input: SerialInput::default(),
                external: VecDeque::new(),
                output: VecDeque::new(),
                screen: SerialScreen::default().publish().unwrap(),
                focused: false,
                running: false,
                attached: false,
                endpoint: String::new(),
                path: String::new(),
                dropped: 0,
                pty_dropped: 0,
            }),
            Mutex::new(SerialScreen::default()),
        )
    }
}
impl Console {
    pub fn snapshot(&self) -> Snapshot {
        let s = self.0.lock().unwrap();
        Snapshot {
            screen: s.screen.clone(),
            endpoint: s.endpoint.clone(),
            path: s.path.clone(),
            dropped: s.dropped + s.pty_dropped,
            attached: s.attached,
        }
    }
    pub fn focus(&self, focused: bool) {
        let mut s = self.0.lock().unwrap();
        s.focused = focused;
        let enabled = focused && s.running && s.attached;
        s.input.enable(enabled);
    }
    pub fn send(&self, bytes: &[u8]) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        let generation = s.input.generation();
        s.input.push(generation, bytes)?;
        drop(s);
        self.scroll(Scroll::To(0));
        Ok(())
    }
    pub fn scroll(&self, request: Scroll) {
        let mut terminal = self.1.lock().unwrap();
        terminal.scroll(request);
        if let Some(frame) = terminal.publish() {
            self.0.lock().unwrap().screen = frame;
        }
    }
    fn process(&self, bytes: &[u8]) {
        let mut terminal = self.1.lock().unwrap();
        terminal.process(bytes);
        if let Some(frame) = terminal.publish() {
            self.0.lock().unwrap().screen = frame;
        }
    }
}

#[cfg(iris_serial)]
impl iris::dev::serial_host::SerialBackend for Console {
    fn send_byte(&self, byte: u8) {
        let mut s = self.0.lock().unwrap();
        if !s.attached {
            return;
        }
        if s.output.len() == OUTPUT_LIMIT {
            s.output.pop_front();
            s.dropped += 1;
        }
        s.output.push_back(byte);
    }
    fn recv_byte(&self) -> std::io::Result<u8> {
        let mut s = self.0.lock().unwrap();
        let value = if let Some(byte) = s.input.front() {
            s.input.consume();
            Some(byte)
        } else {
            s.external.pop_front()
        };
        value.ok_or_else(|| std::io::ErrorKind::WouldBlock.into())
    }
}

pub struct Bridge {
    console: Arc<Console>,
    pty: SerialPty,
}
impl Bridge {
    pub fn open(console: Arc<Console>, alias: &str) -> Result<Self, String> {
        if !AVAILABLE {
            return Err("Headless guests require Unix and the IRIS headless-device APIs".into());
        }
        let mut pty = SerialPty::open(alias)?;
        pty.reset();
        {
            let mut s = console.0.lock().unwrap();
            s.input.reset();
            s.path = pty.path().display().to_string();
            s.endpoint = pty.endpoint().display().to_string();
            s.attached = true;
        }
        Ok(Self { console, pty })
    }
    pub fn poll(&mut self, running: bool) -> Result<(), String> {
        let bytes = {
            let mut s = self.console.0.lock().unwrap();
            s.running = running;
            let enabled = running && s.focused;
            s.input.enable(enabled);
            s.output.drain(..).collect::<Vec<_>>()
        };
        self.console.process(&bytes);
        self.pty.send(&bytes);
        self.pty.poll()?;
        let mut s = self.console.0.lock().unwrap();
        let room = OUTPUT_LIMIT.saturating_sub(s.external.len());
        let count = room.min(self.pty.input.len());
        s.external.extend(self.pty.input.drain(..count));
        s.pty_dropped = self.pty.dropped;
        Ok(())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.poll(false);
        let mut s = self.console.0.lock().unwrap_or_else(|e| e.into_inner());
        s.attached = false;
        s.running = false;
        s.input.enable(false);
        s.external.clear();
    }
}

pub fn minicom(endpoint: &str) -> String {
    format!(
        "minicom -o -8 -b 9600 -D '{}'",
        endpoint.replace('\'', "'\\''")
    )
}

#[cfg(iris_serial)]
pub fn channel(config: &iris::config::MachineConfig) -> usize {
    use iris::config::MachineProfile::*;
    match config.machine.profile {
        IndyIp24 | Indigo2Ip22 | Indigo2Ip28 => 1,
        #[cfg(iris_modern_models)]
        FuelIp34 => match config.fuel.console {
            iris::config::FuelConsole::L1 => 2,
            iris::config::FuelConsole::Ioc3A => 0,
            iris::config::FuelConsole::Ioc3B => 1,
        },
        #[cfg(iris_modern_models)]
        TezroIp53 => match config.tezro.console.index() {
            0 => 2,
            1 => 0,
            2 => 1,
            n => n,
        },
        #[cfg(iris_modern_models)]
        Origin3200Ip35 | Onyx3000Ip35 => {
            let c = if config.machine.profile == Origin3200Ip35 {
                config.origin3200.console
            } else {
                config.onyx3000.console
            };
            match c.index() {
                0 => 2,
                1 => 0,
                _ => 1,
            }
        }
        _ => 0,
    }
}

#[cfg(all(test, unix, iris_serial, iris_headless_devices))]
mod tests {
    use super::*;
    use iris::dev::serial_host::SerialBackend;
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
        os::unix::fs::OpenOptionsExt,
    };

    #[test]
    fn pty_and_embedded_console_share_output_and_independent_input() {
        let directory = tempfile::tempdir().unwrap();
        let alias = directory.path().join("Indy console");
        let console = Arc::new(Console::default());
        let mut bridge = Bridge::open(console.clone(), alias.to_str().unwrap()).unwrap();
        let mut client = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(&alias)
            .unwrap();
        let output = b"\x1b[31mPROM ready\x1b[0m\r\n";
        for byte in output {
            console.send_byte(*byte);
        }
        let mut received = Vec::new();
        for _ in 0..20 {
            bridge.poll(true).unwrap();
            let mut bytes = [0; 128];
            match client.read(&mut bytes) {
                Ok(n) => received.extend_from_slice(&bytes[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => panic!("{e}"),
            }
            if received.len() == output.len() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(received, output);
        assert!(console.snapshot().screen.contents().contains("PROM ready"));
        client.write_all(b"hinv\r").unwrap();
        bridge.poll(true).unwrap();
        assert_eq!(
            (0..5)
                .map(|_| console.recv_byte().unwrap())
                .collect::<Vec<_>>(),
            b"hinv\r"
        );
        console.focus(true);
        console.send(b"\x03").unwrap();
        assert_eq!(console.recv_byte().unwrap(), 3);
        console.send(b"stale").unwrap();
        console.focus(false);
        assert!(console.recv_byte().is_err());
        for row in 0..60 {
            for byte in format!("line {row}\r\n").bytes() {
                console.send_byte(byte);
            }
        }
        bridge.poll(false).unwrap();
        drop(client);
        drop(bridge);
        assert!(std::fs::symlink_metadata(&alias).is_err());
        assert!(!console.snapshot().attached);
        console.scroll(Scroll::To(30));
        assert_eq!(console.snapshot().screen.offset, 30);
        assert!(console.send(b"late").is_err());
    }

    #[test]
    fn headless_configuration_preserves_graphical_preferences_per_label() {
        let mut settings = crate::settings::Settings::default();
        settings.machine.headless = true;
        settings.machine.heads = 2;
        settings.machine.serial_pty_link = "/tmp/prismatic-indy".into();
        settings.save_as_machine("Serial Indy").unwrap();
        settings.machine.headless = false;
        settings.machine.serial_pty_link = "/tmp/prismatic-second".into();
        settings.save_as_machine("Graphical Indy").unwrap();
        let mut loaded: crate::settings::Settings =
            toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        loaded.select_machine("Serial Indy").unwrap();
        assert!(loaded.machine.headless);
        assert_eq!(loaded.machine.heads, 2);
        assert_eq!(loaded.machine.serial_pty_link, "/tmp/prismatic-indy");
        let dir = tempfile::tempdir().unwrap();
        let cfg = crate::emulator::machine_config(
            dir.path(),
            &loaded.machine,
            &loaded.media,
            &loaded.advanced,
            &loaded.network,
        )
        .unwrap();
        assert!(cfg.headless);
        assert_eq!(cfg.graphics_board(), iris::config::GraphicsBoard::None);
        assert_eq!(cfg.graphics.heads, 1);
        loaded.select_machine("Graphical Indy").unwrap();
        assert!(!loaded.machine.headless);
        assert_eq!(loaded.machine.heads, 2);
    }

    #[test]
    fn command_quotes_aliases_and_output_remains_bounded() {
        assert_eq!(
            minicom("/tmp/it's my indy"),
            "minicom -o -8 -b 9600 -D '/tmp/it'\\''s my indy'"
        );
        let console = Arc::new(Console::default());
        let _bridge = Bridge::open(console.clone(), "").unwrap();
        for _ in 0..OUTPUT_LIMIT + 3 {
            console.send_byte(b'x');
        }
        assert_eq!(console.0.lock().unwrap().output.len(), OUTPUT_LIMIT);
        assert_eq!(console.snapshot().dropped, 3);
    }
}
