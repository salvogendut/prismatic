//! Saved Advanced pages and safe startup integration for Iris library services.
use iris::config::{MachineConfig, MachineProfile};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Core,
    Performance,
    Connections,
    Developer,
}
impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Performance => "Performance",
            Self::Connections => "Connections",
            Self::Developer => "Developer",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Core => Self::Performance,
            Self::Performance => Self::Connections,
            Self::Connections => Self::Developer,
            Self::Developer => Self::Core,
        }
    }
}

/// Integer kHz keeps draft equality exact while exposing MHz in TOML and the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct CountMhz(u32);
impl TryFrom<f64> for CountMhz {
    type Error = String;
    fn try_from(mhz: f64) -> Result<Self, String> {
        if !mhz.is_finite()
            || !(1.0..=1000.0).contains(&mhz)
            || (mhz * 1000.0 - (mhz * 1000.0).round()).abs() > 0.000001
        {
            return Err("Count clock must be 1-1000 MHz, with at most three decimal places".into());
        }
        Ok(Self((mhz * 1000.0).round() as u32))
    }
}
impl From<CountMhz> for f64 {
    fn from(value: CountMhz) -> Self {
        f64::from(value.0) / 1000.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExpertSettings {
    pub count_mhz: Option<CountMhz>,
    pub idle_parking: bool,
    pub thread_affinity: bool,
    pub cpu_core: Option<u32>,
    pub rex3_core: Option<u32>,
    pub refresh_core: Option<u32>,
    pub jit_threads: usize,
    pub jit_cache: bool,
    pub monitor_port: u16,
    pub serial_port_a: u16,
    pub serial_port_b: u16,
    pub serial_a: crate::serial::Route,
    pub serial_b: crate::serial::Route,
    pub gdb_port: Option<u16>,
    pub vnc_port: Option<u16>,
    pub vnc_max_fps: u32,
    pub boot_elf: Option<String>,
    pub trace_on_boot: bool,
    pub trace_categories: String,
    pub trace_guest_clock: bool,
}
impl Default for ExpertSettings {
    fn default() -> Self {
        Self {
            count_mhz: None,
            idle_parking: true,
            thread_affinity: false,
            cpu_core: None,
            rex3_core: None,
            refresh_core: None,
            jit_threads: 1,
            jit_cache: false,
            monitor_port: 0,
            serial_port_a: 0,
            serial_port_b: 0,
            serial_a: Default::default(),
            serial_b: Default::default(),
            gdb_port: None,
            vnc_port: None,
            vnc_max_fps: 30,
            boot_elf: None,
            trace_on_boot: false,
            trace_categories: "all".into(),
            trace_guest_clock: false,
        }
    }
}
impl ExpertSettings {
    /// Runtime projection only: unavailable preferences remain in the saved draft.
    pub fn effective(&self) -> Self {
        let mut settings = self.clone();
        if !crate::compat::AFFINITY {
            settings.thread_affinity = false;
            settings.cpu_core = None;
            settings.rex3_core = None;
            settings.refresh_core = None;
        }
        if !iris::build_features::JITV2 {
            settings.jit_threads = 1;
        }
        if !crate::compat::JIT_CACHE {
            settings.jit_cache = false;
        }
        if !crate::compat::GDB {
            settings.gdb_port = None;
        }
        if !crate::compat::VNC {
            settings.vnc_port = None;
        }
        if !crate::compat::TRACE {
            settings.trace_on_boot = false;
        }
        if !crate::compat::SERIAL {
            settings.serial_a = crate::serial::Route::Tcp;
            settings.serial_b = crate::serial::Route::Tcp;
        }
        settings
    }

    pub fn validate(&self) -> Result<(), String> {
        if crate::compat::SERIAL {
            self.serial_a.validate()?;
            self.serial_b.validate()?;
        }
        if !(1..=64).contains(&self.jit_threads) {
            return Err("JIT compiler workers must be 1-64".into());
        }
        if !(1..=60).contains(&self.vnc_max_fps) {
            return Err("VNC frame limit must be 1-60 fps".into());
        }
        let mut ports = std::collections::BTreeSet::new();
        for port in [
            Some(self.monitor_port),
            self.serial_a.tcp_port(self.serial_port_a),
            self.serial_b.tcp_port(self.serial_port_b),
            self.gdb_port,
            self.vnc_port,
        ]
        .into_iter()
        .flatten()
        .filter(|p| *p != 0)
        {
            if !ports.insert(port) {
                return Err(format!(
                    "Port {port} is assigned to more than one connection"
                ));
            }
        }
        if self
            .boot_elf
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.chars().any(char::is_control))
        {
            return Err("Boot ELF path must be nonempty and contain no control characters".into());
        }
        if self.trace_categories.is_empty()
            || self.trace_categories.split(',').any(|c| {
                ![
                    "all", "cpu", "irq", "gfx", "scsi", "net", "dma", "jit", "timer", "audio",
                    "misc", "prim",
                ]
                .contains(&c.trim().strip_prefix('-').unwrap_or(c.trim()))
            })
        {
            return Err("Trace categories: all, cpu, irq, gfx, scsi, net, dma, jit, timer, audio, misc, prim".into());
        }
        Ok(())
    }
    pub fn validate_runtime(&self, profile: MachineProfile) -> Result<(), String> {
        self.effective().validate_available(profile)
    }
    fn validate_available(&self, profile: MachineProfile) -> Result<(), String> {
        self.validate()?;
        if self.boot_elf.is_some()
            && matches!(profile, MachineProfile::O2Ip32 | MachineProfile::OctaneIp30)
        {
            return Err("Iris supports direct ELF loading on Indy and Indigo2 only".into());
        }
        #[cfg(iris_affinity)]
        if self.thread_affinity {
            let available = iris::thread_affinity::available_cores()
                .ok_or("Host thread affinity is unavailable on this platform")?;
            for core in [self.cpu_core, self.rex3_core, self.refresh_core]
                .into_iter()
                .flatten()
            {
                if !available.contains(&core) {
                    return Err(format!(
                        "Host CPU {core} is not available; allowed IDs: {available:?}"
                    ));
                }
            }
        }
        if !iris::build_features::JITV2 && (self.jit_cache || self.jit_threads != 1) {
            return Err("These JIT options require a build with --features jitv2".into());
        }
        if self.gdb_port.is_some() && iris::build_features::LIGHTNING {
            return Err("GDB requires a build without Iris lightning".into());
        }
        if self.trace_on_boot && !cfg!(feature = "perfetto") {
            return Err("Tracing requires a build with --features perfetto".into());
        }
        Ok(())
    }
    pub fn resolve_paths(&mut self, directory: &Path) -> Result<(), String> {
        if let Some(path) = &mut self.boot_elf {
            let path_buf =
                std::path::absolute(directory.join(&*path)).map_err(|e| e.to_string())?;
            *path = path_buf
                .to_str()
                .ok_or("Iris requires a UTF-8 ELF path")?
                .to_owned();
        }
        Ok(())
    }
    /// Read-only draft validation; never create logs, cache files or listeners.
    pub fn validate_files(&self) -> Result<(), String> {
        if let Some(path) = &self.boot_elf {
            let metadata = std::fs::metadata(path).map_err(|e| format!("Boot ELF: {e}"))?;
            if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
                return Err("Boot ELF must be a regular file no larger than 256 MiB".into());
            }
            let data = std::fs::read(path).map_err(|e| format!("Boot ELF: {e}"))?;
            iris::elf::parse(&data).map_err(|e| format!("Boot ELF: {e}"))?;
        }
        Ok(())
    }
    pub fn apply(&self, config: &mut MachineConfig) -> Result<(), String> {
        self.effective().apply_available(config)
    }
    fn apply_available(&self, config: &mut MachineConfig) -> Result<(), String> {
        self.validate_runtime(config.machine.profile)?;
        config.clock.fixed_mhz = self.count_mhz.map(f64::from);
        config.perf.thread_affinity = self.thread_affinity;
        config.perf.cpu_core = self.cpu_core;
        config.perf.rex3_core = self.rex3_core;
        config.perf.refresh_core = self.refresh_core;
        config.monitor_port = Some(self.monitor_port);
        config.serial_port_a = Some(self.serial_port_a);
        config.serial_port_b = Some(self.serial_port_b);
        #[cfg(feature = "jitv2")]
        {
            config.jitv2.threads = self.jit_threads;
        }
        // GDB, VNC, ELF and tracing are owned explicitly by this frontend.
        Ok(())
    }
    /// Called only by the machine worker, after Save and before construction.
    pub fn prepare(&self, config: &MachineConfig, _state: &Path) -> Result<(), String> {
        self.validate_files()?;
        // Probe fixed endpoints together before constructors bind. GDB/VNC
        // also report actual bind failures; the core monitor is checked after
        // start. Serial endpoints are bound and held by serial::Endpoints.
        let mut probes = Vec::new();
        for (name, port) in [
            ("Monitor", Some(self.monitor_port)),
            ("GDB", self.gdb_port),
            ("VNC", self.vnc_port),
        ] {
            if let Some(port) = port.filter(|p| *p != 0) {
                probes.push(
                    std::net::TcpListener::bind(("127.0.0.1", port))
                        .map_err(|e| format!("{name} port {port}: {e}"))?,
                );
            }
        }
        iris::thread_affinity::init(config.perf.clone());
        #[cfg(feature = "idle-pause")]
        iris::cpu::idle_park::set_idle_park_enabled(self.idle_parking);
        #[cfg(all(feature = "jitv2", iris_jit_cache))]
        iris::cpu::jitv2::pcache::configure(
            self.jit_cache.then(|| _state.join("jit-cache")).as_deref(),
        )?;
        Ok(())
    }
    pub fn rows(&self, page: Page) -> Vec<(&'static str, String)> {
        let boolean = |v| if v { "On".into() } else { "Off".into() };
        let feature = |built, value| {
            if built {
                value
            } else {
                format!("Not built (saved: {value})")
            }
        };
        let mut rows = match page {
            Page::Core => vec![],
            Page::Performance => vec![
                ("Count clock (MHz)", Field::CountMhz.value(self)),
                (
                    "Idle parking",
                    feature(iris::build_features::IDLE_PAUSE, boolean(self.idle_parking)),
                ),
                ("Host thread affinity", boolean(self.thread_affinity)),
                ("CPU host core", Field::CpuCore.value(self)),
                ("REX3 host core", Field::Rex3Core.value(self)),
                ("Refresh host core", Field::RefreshCore.value(self)),
                (
                    "JIT compiler workers",
                    feature(iris::build_features::JITV2, self.jit_threads.to_string()),
                ),
                (
                    "JIT disk cache",
                    feature(iris::build_features::JITV2, boolean(self.jit_cache)),
                ),
            ],
            Page::Connections => vec![
                ("Monitor port", port_label(Some(self.monitor_port))),
                ("Serial A routing", self.serial_a.label().into()),
                (
                    "Serial A TCP port",
                    serial_port_label(self.serial_a, self.serial_port_a),
                ),
                ("Serial B routing", self.serial_b.label().into()),
                (
                    "Serial B TCP port",
                    serial_port_label(self.serial_b, self.serial_port_b),
                ),
                ("GDB port", port_label(self.gdb_port)),
                ("VNC port", port_label(self.vnc_port)),
                ("VNC frame limit", format!("{} fps", self.vnc_max_fps)),
                (
                    "Connection status",
                    "Show active ports and PTY paths".into(),
                ),
            ],
            Page::Developer => vec![
                (
                    "Boot ELF",
                    self.boot_elf
                        .clone()
                        .unwrap_or_else(|| "PROM (default)".into()),
                ),
                (
                    "Trace on boot",
                    feature(cfg!(feature = "perfetto"), boolean(self.trace_on_boot)),
                ),
                ("Trace categories", self.trace_categories.clone()),
                (
                    "Trace timeline",
                    if self.trace_guest_clock {
                        "Guest instructions"
                    } else {
                        "Host time"
                    }
                    .into(),
                ),
                (
                    "Runtime status",
                    "Show connections, tracing and build features".into(),
                ),
            ],
        };
        rows.push((
            "Advanced page",
            format!("{} - Enter for next", page.label()),
        ));
        rows
    }
}
fn port_label(port: Option<u16>) -> String {
    match port {
        None => "Off".into(),
        Some(0) => "Automatic".into(),
        Some(p) => format!("127.0.0.1:{p}"),
    }
}

fn serial_port_label(route: crate::serial::Route, port: u16) -> String {
    let label = port_label(Some(port));
    if route == crate::serial::Route::Tcp || !crate::compat::SERIAL {
        label
    } else {
        format!("{label} (saved; TCP inactive)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    CountMhz,
    CpuCore,
    Rex3Core,
    RefreshCore,
    JitThreads,
    Monitor,
    SerialA,
    SerialB,
    Gdb,
    Vnc,
    VncFps,
    BootElf,
    TraceCategories,
}
impl Field {
    pub fn title(self) -> &'static str {
        match self {
            Self::CountMhz => "CP0 Count clock (MHz)",
            Self::CpuCore => "CPU host core",
            Self::Rex3Core => "REX3 host core",
            Self::RefreshCore => "Refresh host core",
            Self::JitThreads => "JIT compiler workers",
            Self::Monitor => "Monitor TCP port",
            Self::SerialA => "Serial A TCP port",
            Self::SerialB => "Serial B TCP port",
            Self::Gdb => "GDB TCP port",
            Self::Vnc => "VNC TCP port",
            Self::VncFps => "VNC frame limit",
            Self::BootElf => "Boot ELF path",
            Self::TraceCategories => "Perfetto trace categories",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::CountMhz => {
                "MHz (1-1000). Blank/auto uses model default. This is Count rate, not a speed limit."
            }
            Self::CpuCore | Self::Rex3Core | Self::RefreshCore => {
                "Host processor ID, or blank/auto for scheduler choice. REX3 fields affect Newport only."
            }
            Self::JitThreads => {
                "1-64 workers; experimental jitv2 build required. Unavailable builds retain the saved value."
            }
            Self::Monitor => {
                "TCP port 1-65535, or 0/auto for automatic. Loopback only; applies after restart."
            }
            Self::SerialA | Self::SerialB => {
                "0/auto or 1-65535. Used with TCP routing; retained with PTY/Off. Applies after restart."
            }
            Self::Gdb | Self::Vnc => {
                "Blank/off disables; 0/auto chooses a port. Loopback only, without authentication."
            }
            Self::VncFps => {
                "1-60 fps. Iris exports the primary display; VNC shares guest keyboard and mouse."
            }
            Self::BootElf => {
                "Static big-endian MIPS ELF32 (Indy/Indigo2). Blank restores PROM boot. Relative to config."
            }
            Self::TraceCategories => {
                "Comma-separated: all, cpu, irq, gfx, scsi, net, dma, jit, timer, audio, misc, prim."
            }
        }
    }
    pub fn value(self, s: &ExpertSettings) -> String {
        let core = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_else(|| "auto".into());
        let port = |v: Option<u16>| v.map(|v| v.to_string()).unwrap_or_else(|| "off".into());
        match self {
            Self::CountMhz => s
                .count_mhz
                .map(|v| f64::from(v).to_string())
                .unwrap_or_else(|| "auto".into()),
            Self::CpuCore => core(s.cpu_core),
            Self::Rex3Core => core(s.rex3_core),
            Self::RefreshCore => core(s.refresh_core),
            Self::JitThreads => s.jit_threads.to_string(),
            Self::Monitor => s.monitor_port.to_string(),
            Self::SerialA => s.serial_port_a.to_string(),
            Self::SerialB => s.serial_port_b.to_string(),
            Self::Gdb => port(s.gdb_port),
            Self::Vnc => port(s.vnc_port),
            Self::VncFps => s.vnc_max_fps.to_string(),
            Self::BootElf => s.boot_elf.clone().unwrap_or_default(),
            Self::TraceCategories => s.trace_categories.clone(),
        }
    }
    pub fn apply(self, settings: &mut ExpertSettings, text: &str) -> Result<(), String> {
        let mut draft = settings.clone();
        let text = text.trim();
        let auto = text.is_empty() || text.eq_ignore_ascii_case("auto");
        let core = || {
            if auto {
                Ok(None)
            } else {
                text.parse()
                    .map(Some)
                    .map_err(|_| "Enter a host CPU ID or auto".to_string())
            }
        };
        let port = || {
            if text.eq_ignore_ascii_case("auto") {
                Ok(0)
            } else {
                text.parse()
                    .map_err(|_| "Enter a TCP port (0-65535)".to_string())
            }
        };
        match self {
            Self::CountMhz => {
                draft.count_mhz = if auto {
                    None
                } else {
                    Some(CountMhz::try_from(
                        text.parse::<f64>().map_err(|_| "Enter MHz or auto")?,
                    )?)
                }
            }
            Self::CpuCore => draft.cpu_core = core()?,
            Self::Rex3Core => draft.rex3_core = core()?,
            Self::RefreshCore => draft.refresh_core = core()?,
            Self::JitThreads => {
                draft.jit_threads = text.parse().map_err(|_| "Enter 1-64 workers")?
            }
            Self::Monitor => draft.monitor_port = port()?,
            Self::SerialA => draft.serial_port_a = port()?,
            Self::SerialB => draft.serial_port_b = port()?,
            Self::Gdb | Self::Vnc => {
                let value = if text.is_empty() || text.eq_ignore_ascii_case("off") {
                    None
                } else {
                    Some(port()?)
                };
                if self == Self::Gdb {
                    draft.gdb_port = value;
                } else {
                    draft.vnc_port = value;
                }
            }
            Self::VncFps => draft.vnc_max_fps = text.parse().map_err(|_| "Enter 1-60 fps")?,
            Self::BootElf => draft.boot_elf = (!text.is_empty()).then(|| text.to_owned()),
            Self::TraceCategories => draft.trace_categories = text.to_owned(),
        }
        draft.validate()?;
        *settings = draft;
        Ok(())
    }
}

pub fn hint(page: Page, row: usize) -> &'static str {
    match (page, row) {
        (Page::Performance, 0) => Field::CountMhz.hint(),
        (Page::Performance, 1) => {
            "idle-pause build required. Parks known idle loops to reduce host CPU usage."
        }
        (Page::Performance, 2..=5) => Field::CpuCore.hint(),
        (Page::Performance, 6) => Field::JitThreads.hint(),
        (Page::Performance, 7) => {
            "Per-label compiled-page reuse. Requires jitv2 and the per-machine cache API."
        }
        (Page::Connections, 0) => Field::Monitor.hint(),
        (Page::Connections, 1 | 3) => {
            "TCP / Pseudoterminal / Off. Connection status shows the active port or PTY path after boot."
        }
        (Page::Connections, 2 | 4) => Field::SerialA.hint(),
        (Page::Connections, 5..=6) => Field::Gdb.hint(),
        (Page::Connections, 7) => Field::VncFps.hint(),
        (Page::Connections, 8) | (Page::Developer, 4) => {
            "Shows the loaded machine, independent of drafts. Services begin on Ctrl+P boot."
        }
        (Page::Developer, 0) => Field::BootElf.hint(),
        (Page::Developer, 1) => {
            "New trace per boot in the label state folder. Requires perfetto and the owned tracing API."
        }
        (Page::Developer, 2) => Field::TraceCategories.hint(),
        (Page::Developer, 3) => {
            "Host time or retired guest instructions. Traces flush when the machine stops."
        }
        _ => "Enter cycles Core, Performance, Connections and Developer pages.",
    }
}

/// Drop before stopping/replacing the workstation. These handles keep remote
/// clients from retaining the CPU or input devices of an earlier label.
pub struct Services {
    gdb: Option<crate::compat::GdbServer>,
    vnc: Option<iris::vnc::VncServer>,
    #[cfg(all(feature = "perfetto", iris_trace))]
    trace: Option<std::path::PathBuf>,
}
impl Services {
    pub fn start(_settings: &ExpertSettings, _state: &Path) -> Result<Self, String> {
        let services = Self {
            gdb: None,
            vnc: None,
            #[cfg(all(feature = "perfetto", iris_trace))]
            trace: None,
        };
        #[cfg(all(feature = "perfetto", iris_trace))]
        let mut services = services;
        #[cfg(all(feature = "perfetto", iris_trace))]
        if _settings.trace_on_boot {
            let directory = _state.join("traces");
            std::fs::create_dir_all(&directory).map_err(|e| format!("Trace folder: {e}"))?;
            let file = tempfile::Builder::new()
                .prefix("boot-")
                .suffix(".pftrace")
                .tempfile_in(directory)
                .map_err(|e| e.to_string())?;
            let path = file.path().to_str().ok_or("Trace path must be UTF-8")?;
            iris::perfetto::start(
                path,
                iris::perfetto::cat::parse(&_settings.trace_categories)?,
                if _settings.trace_guest_clock {
                    iris::perfetto::Clock::Guest
                } else {
                    iris::perfetto::Clock::Host
                },
            )?;
            // Keep each unique trace after shutdown, including failed boots.
            match file.keep() {
                Ok((_, path)) => services.trace = Some(path),
                Err(error) => {
                    let _ = iris::perfetto::stop();
                    return Err(error.to_string());
                }
            }
        }
        Ok(services)
    }
    pub fn start_remote(
        &mut self,
        settings: &ExpertSettings,
        machine: &dyn iris::workstation::Workstation,
    ) -> Result<(), String> {
        if let Some(port) = settings.gdb_port {
            self.gdb = Some(
                crate::compat::GdbServer::start(port, machine.get_cpu_debug())
                    .map_err(|e| format!("GDB: {e}"))?,
            );
        }
        if let Some(port) = settings.vnc_port {
            let input = iris::hostinput::HostInput::attach(machine);
            self.vnc = Some(iris::vnc::VncServer::start(
                input,
                &format!("127.0.0.1:{port}"),
                Some(settings.vnc_max_fps),
                "Prismatic",
            )?);
        }
        Ok(())
    }
    pub fn report(
        &self,
        serial: &crate::serial::Endpoints,
        monitor: Option<std::net::SocketAddr>,
    ) -> String {
        let addr = |addr: Option<std::net::SocketAddr>| {
            addr.map(|a| a.to_string()).unwrap_or_else(|| "Off".into())
        };
        let mut lines = vec![
            format!("Monitor: {}", addr(monitor)),
            serial.describe(0),
            serial.describe(1),
            "PTY: minicom -o -D PATH -b 9600; disable hardware/software flow control.".into(),
            format!("GDB: {} (CPU 0)", addr(self.gdb.as_ref().map(|s| s.addr()))),
            format!(
                "VNC: {} (primary display)",
                addr(self.vnc.as_ref().map(|s| s.addr()))
            ),
            "Remote services bind to loopback, without authentication.".into(),
            iris::thread_affinity::status_line(),
            crate::compat::report(),
            format!(
                "Build features: {}",
                iris::build_features::enabled().join(", ")
            ),
        ];
        #[cfg(all(feature = "perfetto", iris_trace))]
        lines.push(iris::perfetto::status());
        #[cfg(not(all(feature = "perfetto", iris_trace)))]
        lines.push("Perfetto: not built (--features perfetto)".into());
        lines.join("\n")
    }
    pub fn finish(&mut self) -> Result<(), String> {
        self.gdb.take();
        self.vnc.take();
        #[cfg(all(feature = "perfetto", iris_trace))]
        {
            self.trace.take();
            // Also close a recording started through the Iris monitor: its
            // guest-cycle source must not survive this workstation.
            if let Some(message) = iris::perfetto::stop_if_running()? {
                eprintln!("Prismatic: {message}");
            }
        }
        Ok(())
    }
}
impl Drop for Services {
    fn drop(&mut self) {
        if let Err(e) = self.finish() {
            eprintln!("Prismatic: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serial_routes_migrate_persist_per_label_and_only_active_tcp_ports_collide() {
        use crate::serial::Route;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        std::fs::write(
            &path,
            "[advanced.expert]\nserial_port_a = 8880\nserial_port_b = 8881\n",
        )
        .unwrap();
        let mut settings = crate::settings::Settings::load(&path).unwrap();
        assert_eq!(settings.advanced.expert.serial_a, Route::Tcp);
        assert_eq!(settings.advanced.expert.serial_b, Route::Tcp);
        settings.save_as_machine("TCP").unwrap();
        settings.advanced.expert.serial_b = if cfg!(unix) { Route::Pty } else { Route::Off };
        settings.save_as_machine("Terminal").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = crate::settings::Settings::load(&path).unwrap();
        assert_eq!(
            loaded.advanced.expert.serial_b,
            settings.advanced.expert.serial_b
        );
        loaded.select_machine("TCP").unwrap();
        assert_eq!(loaded.advanced.expert.serial_b, Route::Tcp);
        assert_eq!(loaded.advanced.expert.serial_port_b, 8881);
        let mut draft = loaded.clone();
        draft.advanced.expert.serial_b = Route::Off;
        assert!(draft.machine_differs_from(&loaded));
        Field::SerialB
            .apply(&mut draft.advanced.expert, "8880")
            .unwrap();
        draft.advanced.expert.serial_b = Route::Tcp;
        assert!(draft.advanced.validate().is_err());
        std::fs::write(&path, "[advanced.expert]\nserial_a = 'unknown'\n").unwrap();
        assert!(crate::settings::Settings::load(&path).is_err());
    }

    #[test]
    fn invalid_edits_are_atomic_and_endpoints_cannot_collide() {
        let mut settings = ExpertSettings::default();
        for (field, value) in [
            (Field::CountMhz, "NaN"),
            (Field::CountMhz, "0"),
            (Field::CountMhz, "33.1234"),
            (Field::JitThreads, "0"),
            (Field::JitThreads, "65"),
            (Field::VncFps, "61"),
            (Field::Gdb, "65536"),
            (Field::TraceCategories, "not-a-category"),
            (Field::TraceCategories, "all,--cpu"),
        ] {
            assert!(
                field.apply(&mut settings, value).is_err(),
                "{field:?}: {value}"
            );
            assert_eq!(settings, ExpertSettings::default());
        }
        Field::CountMhz.apply(&mut settings, "97.5").unwrap();
        Field::Monitor.apply(&mut settings, "8888").unwrap();
        let before = settings.clone();
        assert!(Field::SerialA.apply(&mut settings, "8888").is_err());
        assert_eq!(settings, before);
        Field::Gdb.apply(&mut settings, "auto").unwrap();
        assert_eq!(settings.gdb_port, Some(0));
        Field::Gdb.apply(&mut settings, "off").unwrap();
        assert_eq!(settings.gdb_port, None);
    }
    #[test]
    fn per_label_settings_roundtrip_and_paths_are_config_relative() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("prismatic.cfg");
        let mut settings = crate::settings::Settings::default();
        settings.advanced.expert.count_mhz = Some(CountMhz::try_from(97.5).unwrap());
        settings.advanced.expert.boot_elf = Some("test.elf".into());
        settings.advanced.expert.gdb_port = Some(9000);
        settings.save_as_machine("Development").unwrap();
        settings.advanced.expert = ExpertSettings::default();
        settings.save_as_machine("Desktop").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = crate::settings::Settings::load(&path).unwrap();
        assert_eq!(loaded.advanced.expert, ExpertSettings::default());
        loaded.select_machine("Development").unwrap();
        assert_eq!(loaded.advanced.expert.count_mhz.map(f64::from), Some(97.5));
        assert_eq!(
            loaded.advanced.expert.boot_elf,
            Some(directory.path().join("test.elf").to_str().unwrap().into())
        );
        assert_eq!(loaded.advanced.expert.gdb_port, Some(9000));
        // Loading/saving a profile never starts listeners or creates an ELF.
        assert!(!directory.path().join("test.elf").exists());
    }
    #[test]
    fn unsupported_preferences_are_inert_and_invalid_available_inputs_are_errors() {
        let mut settings = ExpertSettings {
            boot_elf: Some("missing.elf".into()),
            ..Default::default()
        };
        assert!(
            settings
                .validate_runtime(MachineProfile::O2Ip32)
                .unwrap_err()
                .contains("Indy and Indigo2")
        );
        assert!(settings.validate_files().is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.elf");
        std::fs::write(&path, [0; 100]).unwrap();
        settings.boot_elf = Some(path.to_str().unwrap().into());
        assert!(settings.validate_files().unwrap_err().contains("magic"));
        settings.boot_elf = None;
        settings.thread_affinity = true;
        settings.cpu_core = Some(u32::MAX);
        assert_eq!(
            settings.validate_runtime(MachineProfile::IndyIp24).is_err(),
            crate::compat::AFFINITY
        );
        settings.thread_affinity = false;
        if !iris::build_features::JITV2 {
            settings.jit_cache = true;
            settings.validate_runtime(MachineProfile::IndyIp24).unwrap();
            assert!(!settings.effective().jit_cache);
            assert!(settings.jit_cache);
            settings.jit_cache = false;
        }
        if !cfg!(feature = "perfetto") {
            settings.trace_on_boot = true;
            settings.validate_runtime(MachineProfile::IndyIp24).unwrap();
            assert!(!settings.effective().trace_on_boot);
            assert!(settings.trace_on_boot);
        }
    }
}
