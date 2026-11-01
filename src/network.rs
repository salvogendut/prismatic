//! Per-machine network preferences and validation; no host services start here.
use iris::config::{MachineConfig, MachineProfile, NetMode, NfsConfig};
use iris::net::{PcapStatus, nfsudp::NfsVersion};
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, path::Path};

#[cfg(test)]
mod service_tests;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkSettings {
    pub mode: NetMode,
    pub nat_subnet: String,
    pub pcap_interface: Option<String>,
    pub nfs_dir: Option<String>,
    pub nfs_version: NfsVersion,
    pub nfs_pcap_ip: Option<Ipv4Addr>,
    pub tftp_dir: Option<String>,
    pub forwards: Vec<Forward>,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            mode: NetMode::Nat,
            nat_subnet: "192.168.0.0/24".into(),
            pcap_interface: None,
            nfs_dir: None,
            nfs_version: NfsVersion::Auto,
            nfs_pcap_ip: None,
            tftp_dir: None,
            forwards: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// Forwards listen on localhost. LAN exposure requires an explicit future UI.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Forward {
    pub protocol: Protocol,
    pub host_port: u16,
    pub guest_port: u16,
}

impl Forward {
    fn text(&self) -> String {
        format!(
            "{}:{}:{}",
            if self.protocol == Protocol::Tcp {
                "tcp"
            } else {
                "udp"
            },
            self.host_port,
            self.guest_port
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    Nfs,
    Tftp,
}

impl Folder {
    pub fn title(self) -> &'static str {
        match self {
            Self::Nfs => "NFS shared folder (read/write)",
            Self::Tftp => "TFTP boot folder (read-only)",
        }
    }
    pub fn path(self, settings: &NetworkSettings) -> Option<&str> {
        match self {
            Self::Nfs => settings.nfs_dir.as_deref(),
            Self::Tftp => settings.tftp_dir.as_deref(),
        }
    }
    pub fn set(self, settings: &mut NetworkSettings, value: Option<String>) {
        match self {
            Self::Nfs => settings.nfs_dir = value,
            Self::Tftp => settings.tftp_dir = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Subnet,
    Interface,
    NfsIp,
    Forwards,
    Directory(Folder),
}

impl Field {
    pub fn title(self) -> &'static str {
        match self {
            Self::Subnet => "NAT subnet",
            Self::Interface => "PCAP host interface",
            Self::NfsIp => "NFS virtual address on the bridged LAN",
            Self::Forwards => "Local port forwards",
            Self::Directory(folder) => folder.title(),
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Subnet => "IPv4 network/prefix, e.g. 192.168.5.0/24. Gateway .1, guest .2.",
            Self::Interface => "Host interface name or 1-based index. Empty selects automatically.",
            Self::NfsIp => "An unused IPv4 address on your LAN. Required for NFS with PCAP.",
            Self::Forwards => "tcp:2323:23, tcp:2222:22, udp:11177:177 (host:guest). Empty clears.",
            Self::Directory(_) => {
                "Existing folder. Relative paths use the config folder. Empty disables."
            }
        }
    }
    pub fn value(self, settings: &NetworkSettings) -> String {
        match self {
            Self::Subnet => settings.nat_subnet.clone(),
            Self::Interface => settings.pcap_interface.clone().unwrap_or_default(),
            Self::NfsIp => settings
                .nfs_pcap_ip
                .map(|ip| ip.to_string())
                .unwrap_or_default(),
            Self::Forwards => settings
                .forwards
                .iter()
                .map(Forward::text)
                .collect::<Vec<_>>()
                .join(", "),
            Self::Directory(folder) => folder.path(settings).unwrap_or_default().into(),
        }
    }
    pub fn apply(self, settings: &mut NetworkSettings, text: &str) -> Result<(), String> {
        let text = text.trim();
        let optional = || (!text.is_empty()).then(|| text.to_owned());
        match self {
            Self::Subnet => {
                iris::config::parse_nat_subnet(text)?;
                settings.nat_subnet = text.into();
            }
            Self::Interface => settings.pcap_interface = optional(),
            Self::NfsIp => {
                let ip = if text.is_empty() {
                    None
                } else {
                    let ip = text
                        .parse::<Ipv4Addr>()
                        .map_err(|_| "Enter a valid IPv4 address")?;
                    validate_ip(ip)?;
                    Some(ip)
                };
                settings.nfs_pcap_ip = ip;
            }
            Self::Forwards => {
                let mut forwards = Vec::new();
                if !text.is_empty() {
                    for item in text.split(',') {
                        let parts: Vec<_> = item.trim().split(':').collect();
                        if parts.len() != 3 {
                            return Err(
                                "Use tcp:HOST:GUEST or udp:HOST:GUEST, separated by commas".into(),
                            );
                        }
                        let protocol = match parts[0].to_ascii_lowercase().as_str() {
                            "tcp" => Protocol::Tcp,
                            "udp" => Protocol::Udp,
                            _ => return Err("Forward protocol must be tcp or udp".into()),
                        };
                        let port = |s: &str| {
                            s.parse::<u16>()
                                .map_err(|_| "Ports must be between 1 and 65535".to_owned())
                        };
                        forwards.push(Forward {
                            protocol,
                            host_port: port(parts[1])?,
                            guest_port: port(parts[2])?,
                        });
                    }
                }
                validate_forwards(&forwards)?;
                settings.forwards = forwards;
            }
            Self::Directory(folder) => folder.set(settings, optional()),
        }
        Ok(())
    }
}

fn validate_ip(ip: Ipv4Addr) -> Result<(), String> {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() || ip.is_broadcast() {
        return Err("Use an unused unicast IPv4 address on your LAN".into());
    }
    Ok(())
}

fn validate_forwards(forwards: &[Forward]) -> Result<(), String> {
    if forwards.len() > 16 {
        return Err("Use at most 16 port forwards".into());
    }
    for (index, rule) in forwards.iter().enumerate() {
        if rule.host_port == 0 || rule.guest_port == 0 {
            return Err("Ports must be between 1 and 65535".into());
        }
        if forwards[..index]
            .iter()
            .any(|other| other.protocol == rule.protocol && other.host_port == rule.host_port)
        {
            return Err(format!(
                "Host port {} has duplicate rules for the same protocol",
                rule.host_port
            ));
        }
    }
    Ok(())
}

impl NetworkSettings {
    pub fn supports_off(profile: MachineProfile) -> bool {
        matches!(profile, MachineProfile::O2Ip32 | MachineProfile::OctaneIp30)
    }
    pub fn reconcile_profile(&mut self, profile: MachineProfile) {
        if self.mode == NetMode::Off && !Self::supports_off(profile) {
            self.mode = NetMode::Nat;
        }
    }
    pub fn cycle_mode(&mut self, profile: MachineProfile) {
        self.mode = match self.mode {
            NetMode::Nat => NetMode::Pcap,
            NetMode::Pcap if Self::supports_off(profile) => NetMode::Off,
            _ => NetMode::Nat,
        };
    }
    pub fn cycle_nfs_version(&mut self) {
        self.nfs_version = match self.nfs_version {
            NfsVersion::Auto => NfsVersion::V2,
            NfsVersion::V2 => NfsVersion::V3,
            NfsVersion::V3 => NfsVersion::Auto,
        };
    }
    /// Syntax only: inactive labels can retain PCAP settings in a non-PCAP build.
    pub fn validate(&self, profile: MachineProfile) -> Result<(), String> {
        iris::config::parse_nat_subnet(&self.nat_subnet).map_err(|e| format!("NAT subnet: {e}"))?;
        if self.mode == NetMode::Off && !Self::supports_off(profile) {
            return Err("Ethernet Off is only supported on O2 and Octane".into());
        }
        if let Some(ip) = self.nfs_pcap_ip {
            validate_ip(ip)?;
        }
        for path in [&self.nfs_dir, &self.tftp_dir].into_iter().flatten() {
            if path.trim().is_empty() || path.contains('\0') {
                return Err("Choose a folder or disable the network service".into());
            }
        }
        if self
            .pcap_interface
            .as_ref()
            .is_some_and(|name| name.contains('\0') || name.len() > 1024)
        {
            return Err("Invalid PCAP interface name".into());
        }
        validate_forwards(&self.forwards)
    }
    pub fn validate_runtime(&self, profile: MachineProfile) -> Result<(), String> {
        self.validate(profile)?;
        if self.mode == NetMode::Pcap {
            if !cfg!(feature = "pcap") {
                return Err(
                    "PCAP is unavailable in this build; build with --features pcap or select NAT"
                        .into(),
                );
            }
            if self.nfs_dir.is_some() && self.nfs_pcap_ip.is_none() {
                return Err("PCAP NFS sharing needs an unused LAN address in NFS bridge IP".into());
            }
        }
        Ok(())
    }
    pub fn resolve_paths(&mut self, directory: &Path) -> Result<(), String> {
        for path in [&mut self.nfs_dir, &mut self.tftp_dir]
            .into_iter()
            .flatten()
        {
            if Path::new(path).is_relative() {
                *path = std::path::absolute(directory.join(&*path))
                    .map_err(|e| e.to_string())?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| "Network folder paths must be UTF-8")?;
            }
        }
        Ok(())
    }
    /// Run on a worker: validate existing directories without creating or exporting them.
    pub fn validate_files(&mut self) -> Result<(), String> {
        if self.mode != NetMode::Off
            && let Some(path) = &mut self.nfs_dir
        {
            *path = inspect_folder(Path::new(path))?;
        }
        if self.mode == NetMode::Nat
            && let Some(path) = &mut self.tftp_dir
        {
            *path = inspect_folder(Path::new(path))?;
        }
        Ok(())
    }
    pub fn apply(&self, config: &mut MachineConfig) -> Result<(), String> {
        self.validate_runtime(config.machine.profile)?;
        config.network.mode = self.mode;
        config.nat_subnet = Some(self.nat_subnet.clone());
        config.network.pcap_interface = self.pcap_interface.clone();
        config.network.nfs_pcap_ip = self.nfs_pcap_ip;
        config.nfs = if self.mode == NetMode::Off {
            None
        } else {
            self.nfs_dir.as_ref().map(|dir| NfsConfig {
                shared_dir: dir.clone(),
                version: self.nfs_version,
            })
        };
        config.network.tftp_dir = if self.mode == NetMode::Nat {
            self.tftp_dir.clone()
        } else {
            None
        };
        config.port_forward = if self.mode == NetMode::Nat {
            self.forwards
                .iter()
                .map(|rule| iris::config::PortForwardConfig {
                    proto: match rule.protocol {
                        Protocol::Tcp => iris::config::ForwardProto::Tcp,
                        Protocol::Udp => iris::config::ForwardProto::Udp,
                    },
                    host_port: rule.host_port,
                    guest_port: rule.guest_port,
                    bind: iris::config::ForwardBind::Localhost,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(())
    }
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let nat = self.mode == NetMode::Nat;
        let pcap = self.mode == NetMode::Pcap;
        vec![
            (
                "Network backend",
                match self.mode {
                    NetMode::Nat => "NAT (shared networking)",
                    NetMode::Pcap if !cfg!(feature = "pcap") => "PCAP (unavailable in this build)",
                    NetMode::Pcap => "PCAP (LAN bridge)",
                    NetMode::Off => "Off (unplugged)",
                }
                .into(),
            ),
            (
                "NAT subnet",
                if nat {
                    self.nat_subnet.clone()
                } else {
                    "NAT only (saved value retained)".into()
                },
            ),
            (
                "PCAP host interface",
                if pcap {
                    self.pcap_interface
                        .clone()
                        .unwrap_or("Automatic - Enter to choose".into())
                } else {
                    "PCAP only".into()
                },
            ),
            (
                "NFS shared folder",
                if self.mode == NetMode::Off {
                    "Network off (saved value retained)".into()
                } else {
                    self.nfs_dir
                        .clone()
                        .unwrap_or("Off - Enter to choose".into())
                },
            ),
            ("NFS protocol", format!("{:?}", self.nfs_version)),
            (
                "NFS bridge IP",
                if pcap {
                    self.nfs_pcap_ip
                        .map(|ip| ip.to_string())
                        .unwrap_or("Unset".into())
                } else {
                    "PCAP only; NAT uses the gateway".into()
                },
            ),
            (
                "TFTP boot folder",
                if nat {
                    self.tftp_dir
                        .clone()
                        .unwrap_or("Off - Enter to choose".into())
                } else {
                    "NAT only (saved value retained)".into()
                },
            ),
            (
                "Local port forwards",
                if nat {
                    if self.forwards.is_empty() {
                        "Off - Enter to edit".into()
                    } else {
                        Field::Forwards.value(self)
                    }
                } else {
                    "NAT only (saved rules retained)".into()
                },
            ),
        ]
    }
    pub fn hints(&self, row: usize) -> Vec<String> {
        let detail = match row {
            0 => {
                "NAT shares host connectivity. PCAP joins your LAN and needs host capture permission."
            }
            1 => {
                "IRIX must use this subnet too; changing it does not rewrite guest network configuration."
            }
            2 => {
                "Choose a host interface or enter its name. Wired interfaces work best for bridging."
            }
            3 => {
                "NFS exports the selected host folder READ/WRITE. Choose only files you want to share."
            }
            4 => "Auto supports NFSv2 and v3. IRIX 5.3 uses v2; IRIX 6.x supports v3.",
            5 => "Use an unused address on your LAN for the built-in NFS server when bridging.",
            6 => {
                "TFTP serves the selected folder READ-ONLY at the NAT gateway for PROM bootp() booting."
            }
            _ => "Forward host localhost ports to guest services. Example: tcp:2323:23 for Telnet.",
        };
        let address = if self.mode == NetMode::Nat {
            iris::config::parse_nat_subnet(&self.nat_subnet)
                .map(|(gateway, guest, mask)| {
                    format!("Gateway/DNS {gateway}   Guest {guest}   Mask {mask}")
                })
                .unwrap_or_default()
        } else if self.mode == NetMode::Pcap {
            "PCAP uses your LAN's addresses and DNS. NAT forwarding and TFTP are inactive.".into()
        } else {
            "The guest Ethernet cable is unplugged.".into()
        };
        vec![detail.into(), address, "Network changes apply on Save and require restart. Hiding Tinker keeps saved values active.".into()]
    }
}

pub fn inspect_folder(path: &Path) -> Result<String, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("Cannot access network folder {}: {e}", path.display()))?;
    if !path.is_dir() {
        return Err("Network shares require an existing folder".into());
    }
    std::fs::read_dir(&path)
        .map_err(|e| format!("Cannot read network folder {}: {e}", path.display()))?;
    path.into_os_string()
        .into_string()
        .map_err(|_| "Network folder paths must be UTF-8".into())
}

pub struct Interface {
    pub name: String,
    pub label: String,
}

pub fn interfaces() -> Result<Vec<Interface>, String> {
    #[cfg(feature = "pcap")]
    {
        iris::net::net_pcap::list_interfaces().map(|list| {
            list.into_iter()
                .take(128)
                .map(|i| Interface {
                    label: format!(
                        "{} [{}{}]",
                        i.name,
                        if i.up { "up" } else { "down" },
                        if i.loopback { ", loopback" } else { "" }
                    ),
                    name: i.name,
                })
                .collect()
        })
    }
    #[cfg(not(feature = "pcap"))]
    {
        Err("PCAP is unavailable in this build; build with --features pcap".into())
    }
}

pub fn pcap_message(status: PcapStatus) -> Option<&'static str> {
    match status {
        PcapStatus::PermissionDenied => {
            Some("PCAP permission denied - network disconnected; choose NAT or enable host capture")
        }
        PcapStatus::DeviceError => {
            Some("PCAP interface failed - network disconnected; check Network options")
        }
        PcapStatus::Active => Some("PCAP bridge active"),
        PcapStatus::Inactive => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{MachineSettings, Settings};

    #[test]
    fn invalid_edits_leave_the_previous_value_and_saved_file_intact() {
        let mut network = NetworkSettings::default();
        Field::Forwards
            .apply(&mut network, "tcp:2323:23, udp:2323:23")
            .unwrap();
        let original = network.clone();
        for (field, value) in [
            (Field::Subnet, "192.168.5.7/24"),
            (Field::Subnet, "192.168.5.0/31"),
            (Field::Subnet, "not a subnet"),
            (Field::NfsIp, "127.0.0.1"),
            (Field::NfsIp, "224.1.2.3"),
            (Field::NfsIp, "nonsense"),
            (Field::Forwards, "tcp:0:23"),
            (Field::Forwards, "tcp:2323:99999"),
            (Field::Forwards, "tcp:2323:23,tcp:2323:24"),
            (Field::Forwards, "ftp:2121:21"),
        ] {
            assert!(field.apply(&mut network, value).is_err(), "{value}");
            assert_eq!(network, original);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings {
            network,
            ..Default::default()
        };
        settings.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        settings.network.nat_subnet = "invalid".into();
        assert!(settings.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn network_settings_map_to_iris_on_every_profile() {
        let dir = tempfile::tempdir().unwrap();
        let network = NetworkSettings {
            nat_subnet: "10.23.0.0/24".into(),
            nfs_dir: Some(dir.path().to_str().unwrap().into()),
            nfs_version: NfsVersion::V2,
            tftp_dir: Some(dir.path().to_str().unwrap().into()),
            forwards: vec![Forward {
                protocol: Protocol::Tcp,
                host_port: 2323,
                guest_port: 23,
            }],
            ..Default::default()
        };
        let mut hardware = MachineSettings::default();
        for _ in MachineSettings::PROFILES {
            let config = crate::emulator::machine_config(
                dir.path(),
                &hardware,
                &Default::default(),
                &Default::default(),
                &network,
            )
            .unwrap();
            let net = config.network();
            assert_eq!(net.mode, NetMode::Nat);
            assert_eq!(
                net.nat_subnet.unwrap().gateway_ip,
                Ipv4Addr::new(10, 23, 0, 1)
            );
            assert_eq!(
                net.nat_subnet.unwrap().client_ip,
                Ipv4Addr::new(10, 23, 0, 2)
            );
            assert_eq!(net.nfs.unwrap().version, NfsVersion::V2);
            assert_eq!(net.tftp_dir.as_deref(), Some(dir.path()));
            assert_eq!(net.port_forward[0].host_port, 2323);
            assert_eq!(net.port_forward[0].guest_port, 23);
            assert_eq!(
                net.port_forward[0].bind,
                iris::config::ForwardBind::Localhost
            );
            hardware.cycle_profile();
        }
    }

    #[test]
    fn old_configs_default_to_nat_and_labels_keep_independent_networks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        std::fs::write(&path, "scanlines = true\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.network, NetworkSettings::default());
        settings.network.nfs_dir = Some("share".into());
        settings.network.nat_subnet = "10.23.0.0/24".into();
        settings.save_as_machine("Indy").unwrap();
        settings.save(&path).unwrap();
        // Repeated saves must not confuse normalized paths with concurrent edits.
        settings.save(&path).unwrap();
        settings.network.nfs_dir = None;
        settings.network.nat_subnet = "10.24.0.0/24".into();
        settings.save_as_machine("O2").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.network.nat_subnet, "10.24.0.0/24");
        assert!(loaded.network.nfs_dir.is_none());
        let before = loaded.clone();
        loaded.select_machine("Indy").unwrap();
        assert_eq!(loaded.network.nat_subnet, "10.23.0.0/24");
        assert_eq!(
            loaded.network.nfs_dir.as_deref(),
            dir.path().join("share").to_str()
        );
        assert!(loaded.machine_differs_from(&before));
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("[machines.Indy.network]")
        );
        assert!(!dir.path().join("share").exists()); // Merely editing/saving cannot export/create it.
    }

    #[test]
    fn folders_must_exist_and_validation_has_no_write_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("absent");
        let mut settings = NetworkSettings {
            nfs_dir: Some(absent.to_str().unwrap().into()),
            ..Default::default()
        };
        assert!(settings.validate_files().is_err());
        assert!(!absent.exists());
        let file = dir.path().join("file");
        std::fs::write(&file, b"untouched").unwrap();
        assert!(inspect_folder(&file).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"untouched");
        settings.nfs_dir = Some(dir.path().to_str().unwrap().into());
        settings.tftp_dir = Some(dir.path().to_str().unwrap().into());
        settings.validate_files().unwrap();
    }

    #[test]
    fn off_is_model_specific_and_pcap_never_silently_uses_nat() {
        let mut network = NetworkSettings {
            mode: NetMode::Off,
            nfs_dir: Some("share".into()),
            tftp_dir: Some("boot".into()),
            ..Default::default()
        };
        assert!(network.validate(MachineProfile::IndyIp24).is_err());
        assert!(network.validate(MachineProfile::O2Ip32).is_ok());
        let mut config = MachineConfig::default();
        config.machine.profile = MachineProfile::O2Ip32;
        network.apply(&mut config).unwrap();
        assert!(config.nfs.is_none());
        assert!(config.network.tftp_dir.is_none());
        network.reconcile_profile(MachineProfile::IndyIp24);
        assert_eq!(network.mode, NetMode::Nat);
        network.cycle_mode(MachineProfile::IndyIp24);
        assert_eq!(network.mode, NetMode::Pcap);
        assert!(network.validate_runtime(MachineProfile::IndyIp24).is_err());
        network.nfs_pcap_ip = Some(Ipv4Addr::new(192, 168, 5, 77));
        if cfg!(feature = "pcap") {
            network.apply(&mut config).unwrap();
            assert_eq!(config.network.mode, NetMode::Pcap);
            assert!(config.nfs.is_some());
            assert!(config.network.tftp_dir.is_none());
            assert!(config.port_forward.is_empty());
        } else {
            assert!(
                network
                    .apply(&mut config)
                    .unwrap_err()
                    .contains("unavailable")
            );
        }
        network.cycle_mode(MachineProfile::IndyIp24);
        assert_eq!(network.mode, NetMode::Nat);
    }
}
