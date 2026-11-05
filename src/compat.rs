//! Capabilities of the linked Iris checkout, independent of its branch name.
use serde::{Deserialize, Serialize};

pub const AUDIO: bool = cfg!(iris_audio);
pub const DIAGNOSTICS: bool = cfg!(iris_diagnostics);
pub const PROM: bool = cfg!(iris_prom);
pub const SERIAL: bool = cfg!(iris_serial);
pub const AFFINITY: bool = cfg!(iris_affinity);
pub const GDB: bool = cfg!(iris_gdb) && !iris::build_features::LIGHTNING;
pub const VNC: bool = cfg!(iris_vnc);
pub const TRACE: bool = cfg!(all(iris_trace, feature = "perfetto"));
pub const JIT_CACHE: bool = cfg!(all(iris_jit_cache, feature = "jitv2"));
pub const SGIEMU_OPTIMIZATIONS: bool = cfg!(iris_sgiemu_optimizations);
pub const FIRMWARE: bool = cfg!(iris_firmware);
pub const DEVICES: bool = cfg!(iris_devices);
pub const CHECKPOINTS: bool = cfg!(iris_checkpoints);
pub const LIVE_NETWORK: bool = cfg!(iris_live_network);
pub const STREAM_NETWORK: bool = cfg!(iris_stream_network);

pub fn report() -> String {
    let features = [
        ("SDL audio", AUDIO),
        ("diagnostic routing", DIAGNOSTICS),
        ("PROM presets", PROM),
        ("serial PTY/Off", SERIAL),
        ("protected CHD disks", cfg!(iris_chd_cow)),
        ("host affinity", AFFINITY),
        ("owned GDB", GDB),
        ("owned VNC", VNC),
        ("boot tracing", TRACE),
        ("JIT disk cache", JIT_CACHE),
        ("SGIEmu optimizations", SGIEMU_OPTIMIZATIONS),
        ("bundled O2/Octane PROMs", FIRMWARE),
        ("optional devices", DEVICES),
        ("paused checkpoints", CHECKPOINTS),
        ("live network changes", LIVE_NETWORK),
        ("private stream networking", STREAM_NETWORK),
        ("Octane R12000/R14000", cfg!(iris_octane_cpus)),
    ];
    let unavailable: Vec<_> = features
        .iter()
        .filter(|(_, supported)| !supported)
        .map(|(name, _)| *name)
        .collect();
    if unavailable.is_empty() {
        "Iris compatibility: all frontend APIs available".into()
    } else {
        format!("Unavailable in this build: {}", unavailable.join(", "))
    }
}

#[cfg(iris_gdb)]
pub use iris::gdb_stub::GdbServer;
#[cfg(not(iris_gdb))]
pub struct GdbServer;
#[cfg(not(iris_gdb))]
impl GdbServer {
    pub fn start(_: u16, _: std::sync::Arc<dyn iris::gdb_stub::CpuDebug>) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Owned GDB services are unavailable in this Iris build",
        ))
    }
    pub fn addr(&self) -> std::net::SocketAddr {
        unreachable!("GDB cannot start without the owned API")
    }
}

// Own the saved enum so a label created by a newer core still loads on main.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuModel {
    R4400,
    R5000,
    R10000,
    R12000,
    R14000,
}
impl CpuModel {
    pub const OCTANE: [Self; 3] = [Self::R10000, Self::R12000, Self::R14000];
    pub fn label(self) -> &'static str {
        match self {
            Self::R4400 => "MIPS R4400",
            Self::R5000 => "MIPS R5000",
            Self::R10000 => "MIPS R10000",
            Self::R12000 => "MIPS R12000",
            Self::R14000 => "MIPS R14000",
        }
    }
    pub fn iris(self) -> Result<iris::config::CpuModel, String> {
        use iris::config::CpuModel as Core;
        Ok(match self {
            Self::R4400 => Core::R4400,
            Self::R5000 => Core::R5000,
            Self::R10000 => Core::R10000,
            #[cfg(iris_octane_cpus)]
            Self::R12000 => Core::R12000,
            #[cfg(iris_octane_cpus)]
            Self::R14000 => Core::R14000,
            #[cfg(not(iris_octane_cpus))]
            _ => {
                return Err(format!(
                    "{} is unavailable in this Iris build; select a supported CPU in General",
                    self.label()
                ));
            }
        })
    }
}

pub fn powered_off(machine: &dyn iris::workstation::Workstation) -> bool {
    #[cfg(iris_poweroff)]
    {
        machine.status().state == iris::machine_state::GuestState::PoweredOff
    }
    #[cfg(not(iris_poweroff))]
    {
        let _ = machine;
        false
    }
}

pub fn vino_status(machine: &mut dyn iris::workstation::Workstation) -> String {
    #[cfg(iris_devices)]
    {
        machine
            .as_any_mut()
            .downcast_mut::<iris::machine::Machine>()
            .map_or_else(|| "Unavailable".into(), |m| m.vino_source_status())
    }
    #[cfg(not(iris_devices))]
    {
        let _ = machine;
        "Unavailable in this Iris build".into()
    }
}

pub fn save_snapshot(machine: &mut iris::machine::Machine, name: &str) -> Result<(), String> {
    #[cfg(iris_checkpoints)]
    {
        machine.save_snapshot_paused(name).map(|_| ())
    }
    #[cfg(not(iris_checkpoints))]
    {
        let _ = (machine, name);
        Err("Paused checkpoints are unavailable in this Iris build".into())
    }
}
pub fn snapshot_context(
    machine: &mut iris::machine::Machine,
    path: std::path::PathBuf,
    disks: Vec<iris::snapshot::DiskRef>,
) {
    #[cfg(iris_checkpoints)]
    machine.set_snapshot_context(path, disks);
    #[cfg(not(iris_checkpoints))]
    {
        let _ = (machine, path, disks);
    }
}
pub fn load_snapshot(machine: &mut iris::machine::Machine, name: &str) -> Result<(), String> {
    #[cfg(iris_checkpoints)]
    {
        machine.load_snapshot_paused(name).map(|_| ())
    }
    #[cfg(not(iris_checkpoints))]
    {
        let _ = (machine, name);
        Err("Paused checkpoints are unavailable in this Iris build".into())
    }
}

pub fn open_chd_cow(path: &str) -> std::io::Result<Box<dyn iris::block_node::BlockNode>> {
    #[cfg(iris_chd_cow)]
    {
        iris::block_node::open_chd_cow(path)
    }
    #[cfg(not(iris_chd_cow))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Protected CHD hard disks are unavailable in this Iris build; use a raw disk image or a core with CHD COW support",
        ))
    }
}

/// Older Iris only offers its documented environment override. Re-exec with a
/// child environment rather than mutating the environment of a threaded process.
pub fn initialize_host() -> Result<(), String> {
    #[cfg(iris_poweroff)]
    iris::workstation::set_exit_on_guest_poweroff(false);
    #[cfg(not(iris_poweroff))]
    if std::env::var_os("IRIS_NO_EXIT_ON_POWEROFF").as_deref() != Some(std::ffi::OsStr::new("1")) {
        let mut command =
            std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args(std::env::args_os().skip(1))
            .env("IRIS_NO_EXIT_ON_POWEROFF", "1");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            return Err(command.exec().to_string());
        }
        #[cfg(not(unix))]
        std::process::exit(
            command
                .status()
                .map_err(|e| e.to_string())?
                .code()
                .unwrap_or(1),
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_unavailable_preferences_survive_load_save_and_runtime_projection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut saved = crate::settings::Settings::default();
        saved.machine.profile = iris::config::MachineProfile::OctaneIp30;
        saved.machine.cpu = CpuModel::R14000;
        saved.machine.prom_boot = crate::settings::PromBoot::Netbsd;
        saved.machine.prom_console = crate::settings::PromConsole::Serial;
        saved.advanced.expert.serial_b = crate::serial::Route::Pty;
        saved.advanced.expert.serial_port_b = 9002;
        saved.advanced.expert.jit_cache = true;
        saved.advanced.expert.jit_threads = 4;
        saved.advanced.expert.gdb_port = Some(9001);
        saved.advanced.expert.trace_on_boot = true;
        saved.save_as_machine("Later core").unwrap();
        saved.save(&path).unwrap();
        let loaded = crate::settings::Settings::load(&path).unwrap();
        assert_eq!(loaded, saved);
        let active = loaded.advanced.expert.effective();
        assert_eq!(active.jit_cache, JIT_CACHE);
        assert_eq!(active.trace_on_boot, TRACE);
        assert_eq!(active.gdb_port.is_some(), GDB);
        assert_eq!(active.serial_b == crate::serial::Route::Pty, SERIAL);
        assert_eq!(active.serial_port_b, 9002);
        assert_eq!(loaded.machine.cpu.iris().is_ok(), cfg!(iris_octane_cpus));
        loaded.save(&path).unwrap();
        assert_eq!(crate::settings::Settings::load(&path).unwrap(), saved);
    }
}
