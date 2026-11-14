//! Optional Iris devices. Draft validation never opens cameras or IPC bridges.
use crate::{media::MediaSettings, network::NetworkSettings, settings::MachineSettings};
use iris::config::{MachineConfig, ScsiDeviceConfig, ScsiKind, VinoSource, VinoStandard};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Off,
    TestPattern,
    Black,
    Camera,
}
impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::TestPattern => "Test pattern",
            Self::Black => "Black",
            Self::Camera => "Host camera",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::TestPattern,
            Self::TestPattern => Self::Black,
            Self::Black => Self::Camera,
            Self::Camera => Self::Off,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ExtensionsSettings {
    pub vino: Source,
    pub pal: bool,
    pub camera_index: u32,
    pub daynaport: Option<u8>,
    pub dayna_mac: String,
    pub dayna_subnet: String,
    pub ultra64: bool,
}
impl Default for ExtensionsSettings {
    fn default() -> Self {
        Self {
            vino: Source::Off,
            pal: false,
            camera_index: 0,
            daynaport: None,
            dayna_mac: String::new(),
            dayna_subnet: "192.168.10.0/24".into(),
            ultra64: false,
        }
    }
}
impl ExtensionsSettings {
    pub fn effective(&self) -> Self {
        if crate::compat::DEVICES {
            self.clone()
        } else {
            Self::default()
        }
    }

    pub fn dayna_config(&self) -> ScsiDeviceConfig {
        ScsiDeviceConfig {
            kind_field: ScsiKind::Daynaport,
            mac: (!self.dayna_mac.is_empty()).then(|| self.dayna_mac.clone()),
            subnet: Some(self.dayna_subnet.clone()),
            ..Default::default()
        }
    }
    pub fn validate(
        &self,
        hardware: &MachineSettings,
        media: &MediaSettings,
        network: &NetworkSettings,
    ) -> Result<(), String> {
        if !hardware.legacy_devices()
            && (self.vino != Source::Off || self.daynaport.is_some() || self.ultra64)
        {
            return Err("VINO, DaynaPort and Ultra64 require an Indy/Indigo2 machine in Iris; disable them before changing model".into());
        }
        let params = self
            .dayna_config()
            .daynaport_params(self.daynaport.unwrap_or(5) % 16)?;
        if params.mac[0] & 1 != 0 {
            return Err("DaynaPort MAC must be unicast".into());
        }
        if let Some(slot) = self.daynaport {
            if network.mode == crate::network::NetMode::Stream {
                return Err("DaynaPort cannot join a private stream network; disable it for network installation".into());
            }
            if !(1..=7).contains(&(slot % 16)) || slot / 16 >= hardware.profile.scsi_controllers() {
                return Err("DaynaPort needs an available SCSI target 1-7 on this machine".into());
            }
            if media.devices.contains_key(&slot) || media.scratch.slot == Some(slot) {
                return Err(format!(
                    "DaynaPort conflicts with media at SCSI {}:{}",
                    slot / 16,
                    slot % 16
                ));
            }
            let (gateway, _, mask) = iris::config::parse_nat_subnet(&network.nat_subnet)?;
            let common = u32::from(mask) & u32::from(params.subnet.netmask);
            if u32::from(gateway) & common == u32::from(params.subnet.gateway_ip) & common {
                return Err(
                    "DaynaPort and onboard Ethernet need non-overlapping NAT subnets".into(),
                );
            }
        }
        Ok(())
    }
    pub fn apply(&self, config: &mut MachineConfig) {
        let effective = self.effective();
        let this = &effective;
        config.vino.source = match this.vino {
            Source::Off => VinoSource::Off,
            Source::Camera => VinoSource::Camera,
            Source::Black => VinoSource::Black,
            Source::TestPattern => VinoSource::TestPattern,
        };
        config.vino.standard = if this.pal {
            VinoStandard::Pal
        } else {
            VinoStandard::Ntsc
        };
        config.vino.camera_index = this.camera_index;
        config.ultra64.enabled = this.ultra64;
    }
    pub fn attach(
        &self,
        workstation: &mut dyn iris::workstation::Workstation,
    ) -> Result<(), String> {
        if let Some(machine) = workstation
            .as_any_mut()
            .downcast_mut::<iris::machine::Machine>()
        {
            #[cfg(iris_devices)]
            if self.ultra64 && !machine.has_ultra64() {
                return Err("Ultra64 bridge could not initialize (already in use or unavailable); enable Console debug for details".into());
            }
            if let Some(slot) = self.daynaport {
                machine
                    .hpc3()
                    .add_scsi_daynaport(
                        slot / 16,
                        usize::from(slot % 16),
                        self.dayna_config().daynaport_params(slot % 16)?,
                    )
                    .map_err(|e| format!("DaynaPort SCSI {}:{}: {e}", slot / 16, slot % 16))?;
            }
        }
        Ok(())
    }
    pub fn cycle_dayna(&mut self, hardware: &MachineSettings) {
        self.daynaport = match self.daynaport {
            None => Some(1),
            Some(slot) if slot % 16 < 7 => Some(slot + 1),
            Some(7) if hardware.profile.scsi_controllers() > 1 => Some(17),
            _ => None,
        };
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Hardware,
    Storage,
    Devices,
    Checkpoints,
    Runtime(crate::runtime::Topic),
}
impl Page {
    pub const ALL: [Self; 11] = [
        Self::Hardware,
        Self::Storage,
        Self::Devices,
        Self::Checkpoints,
        Self::Runtime(crate::runtime::Topic::Platform),
        Self::Runtime(crate::runtime::Topic::Render),
        Self::Runtime(crate::runtime::Topic::Disks),
        Self::Runtime(crate::runtime::Topic::Pci),
        Self::Runtime(crate::runtime::Topic::Sharing),
        Self::Runtime(crate::runtime::Topic::Ports),
        Self::Runtime(crate::runtime::Topic::Sound),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hardware => "Hardware",
            Self::Storage => "Storage",
            Self::Devices => "Devices",
            Self::Checkpoints => "Checkpoints",
            Self::Runtime(topic) => topic.label(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    CameraIndex,
    DaynaMac,
    DaynaSubnet,
}
impl Field {
    pub fn title(self) -> &'static str {
        match self {
            Self::CameraIndex => "Host camera index",
            Self::DaynaMac => "DaynaPort MAC",
            Self::DaynaSubnet => "DaynaPort NAT subnet",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::CameraIndex => {
                "Zero-based host camera index; opened only when the machine boots."
            }
            Self::DaynaMac => {
                "Six hexadecimal bytes separated by colons; blank uses Iris's target-based default."
            }
            Self::DaynaSubnet => {
                "Private CIDR subnet, separate from onboard Ethernet. Restart required."
            }
        }
    }
    pub fn value(self, s: &ExtensionsSettings) -> String {
        match self {
            Self::CameraIndex => s.camera_index.to_string(),
            Self::DaynaMac => s.dayna_mac.clone(),
            Self::DaynaSubnet => s.dayna_subnet.clone(),
        }
    }
    pub fn apply(self, s: &mut ExtensionsSettings, value: &str) -> Result<(), String> {
        let value = value.trim();
        match self {
            Self::CameraIndex => {
                s.camera_index = value
                    .parse()
                    .map_err(|_| "Enter a nonnegative camera index")?
            }
            Self::DaynaMac => {
                if !value.is_empty() {
                    let mut config = s.dayna_config();
                    config.mac = Some(value.into());
                    let params = config.daynaport_params(5)?;
                    if params.mac[0] & 1 != 0 {
                        return Err("MAC must be unicast".into());
                    }
                }
                s.dayna_mac = value.into();
            }
            Self::DaynaSubnet => {
                iris::config::parse_nat_subnet(value)?;
                s.dayna_subnet = value.into();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris::config::MachineProfile;
    #[test]
    fn device_conflicts_and_subnets_are_validated_without_side_effects() {
        let hardware = MachineSettings::default();
        let mut media = MediaSettings::default();
        let network = NetworkSettings::default();
        let mut s = ExtensionsSettings {
            daynaport: Some(5),
            ..Default::default()
        };
        assert!(s.validate(&hardware, &media, &network).is_ok());
        media.scratch.slot = Some(5);
        assert!(s.validate(&hardware, &media, &network).is_err());
        media.scratch.slot = None;
        s.dayna_subnet = "192.168.0.0/16".into();
        assert!(s.validate(&hardware, &media, &network).is_err());
        s.dayna_subnet = "192.168.10.0/24".into();
        s.daynaport = Some(17);
        assert!(s.validate(&hardware, &media, &network).is_err());
        s.daynaport = None;
        s.vino = Source::TestPattern;
        assert!(
            s.validate(
                &MachineSettings {
                    profile: MachineProfile::O2Ip32,
                    ..hardware
                },
                &media,
                &network
            )
            .is_err()
        );
    }
}
