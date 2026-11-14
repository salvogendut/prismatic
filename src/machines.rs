//! Model-specific frontend choices and isolated persistent platform state.
#[cfg(iris_cpu_parts)]
use crate::compat::CpuModel;
use crate::settings::MachineSettings;
use iris::config::{MachineConfig, MachineProfile};
use std::path::Path;

impl MachineSettings {
    pub const LEGACY_PROFILES: [MachineProfile; 5] = [
        MachineProfile::IndyIp24,
        MachineProfile::Indigo2Ip22,
        MachineProfile::Indigo2Ip28,
        MachineProfile::O2Ip32,
        MachineProfile::OctaneIp30,
    ];

    pub fn legacy_devices(&self) -> bool {
        matches!(
            self.profile,
            MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28
        )
    }

    pub fn fibre_only(&self) -> bool {
        #[cfg(iris_modern_models)]
        {
            matches!(
                self.profile,
                MachineProfile::Origin3200Ip35 | MachineProfile::Onyx3000Ip35
            )
        }
        #[cfg(not(iris_modern_models))]
        {
            false
        }
    }

    pub fn native_storage(&self) -> bool {
        !Self::LEGACY_PROFILES.contains(&self.profile) && !cfg!(iris_frontend_bus)
    }

    pub fn numa(&self) -> bool {
        #[cfg(iris_modern_models)]
        {
            matches!(
                self.profile,
                MachineProfile::Origin2000Ip27
                    | MachineProfile::Onyx2Ip27
                    | MachineProfile::Origin3200Ip35
            )
        }
        #[cfg(not(iris_modern_models))]
        {
            false
        }
    }

    pub fn node_count(&self) -> u8 {
        #[cfg(iris_modern_models)]
        if self.profile == MachineProfile::Onyx3000Ip35 {
            return if self.graphics_board() == Some(iris::config::GraphicsBoard::V12) {
                2
            } else {
                1
            };
        }
        self.nodes
    }

    pub fn processor_count(&self) -> u8 {
        self.cpus * self.node_count()
    }

    pub fn platform_serial_lines(&self) -> usize {
        #[cfg(iris_modern_models)]
        return match self.profile {
            MachineProfile::TezroIp53 => 3,
            MachineProfile::FuelIp34
            | MachineProfile::Origin3200Ip35
            | MachineProfile::Onyx3000Ip35 => 1,
            _ => 0,
        };
        #[cfg(not(iris_modern_models))]
        0
    }

    pub fn cpu_counts(&self) -> &'static [u8] {
        #[cfg(iris_modern_models)]
        if self.profile == MachineProfile::TezroIp53 {
            // Upstream qualifies the higher-frequency parts only in these
            // populations; keep the menu consistent with core validation.
            return match self.cpu_part.as_deref() {
                Some("r16000_800" | "r16000_900") => &[4],
                Some("r16000_1000") => &[2, 4],
                _ => &[1, 2, 4],
            };
        }
        if self.profile == MachineProfile::OctaneIp30 {
            &[1, 2]
        } else {
            &[1]
        }
    }

    pub fn cycle_cpu_count(&mut self) {
        let choices = self.cpu_counts();
        let index = choices.iter().position(|&v| v == self.cpus).unwrap_or(0);
        self.cpus = choices[(index + 1) % choices.len()];
    }

    pub fn cycle_part(&mut self) {
        #[cfg(iris_cpu_parts)]
        {
            let parts = self.profile.cpus();
            let index = self
                .cpu_part
                .as_ref()
                .and_then(|name| parts.iter().position(|p| p.name() == *name));
            let part = parts[index.map_or(0, |i| (i + 1) % parts.len())];
            self.cpu = family(part);
            self.cpu_part = Some(part.name());
            if !self.cpu_counts().contains(&self.cpus) {
                self.cpus = self.cpu_counts()[0];
            }
        }
    }

    pub fn serial_only(&self) -> bool {
        #[cfg(iris_modern_models)]
        {
            self.graphics_board() == Some(iris::config::GraphicsBoard::None)
        }
        #[cfg(not(iris_modern_models))]
        {
            false
        }
    }

    pub fn apply_platform(&self, config: &mut MachineConfig, state: &Path) -> Result<(), String> {
        #[cfg(iris_cpu_parts)]
        if let Some(part) = &self.cpu_part {
            config.machine.cpu = Some(part.clone());
        }
        #[cfg(iris_modern_models)]
        {
            let path = |name| {
                state
                    .join(name)
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "State paths must be UTF-8".to_string())
            };
            config.banks_set = true;
            config.graphics.flash = Some(path("graphics-flash.bin")?);
            match self.profile {
                MachineProfile::Origin200Ip29
                | MachineProfile::Origin2000Ip27
                | MachineProfile::Onyx2Ip27 => {
                    let section = match self.profile {
                        MachineProfile::Origin200Ip29 => &mut config.origin200,
                        MachineProfile::Origin2000Ip27 => &mut config.origin2000,
                        _ => &mut config.onyx2,
                    };
                    section.nodes = self.nodes;
                    section.flash = Some(path("flash.bin")?);
                    section.controller_store = Some(path("controller.bin")?);
                }
                MachineProfile::FuelIp34 => {
                    config.fuel.flash = Some(path("flash.bin")?);
                    config.fuel.l1_port = Some(0);
                }
                MachineProfile::TezroIp53 => {
                    config.tezro.flash = Some(path("flash.bin")?);
                    config.tezro.l1_port = Some(0);
                    config.tezro.serial_port_c = Some(0);
                    config.tezro.serial_port_d = Some(0);
                }
                MachineProfile::Origin3200Ip35 => {
                    config.origin3200.flash = Some(path("flash.bin")?);
                    config.origin3200.c_bricks = self.nodes;
                    config.origin3200.l1_port = Some(0);
                }
                MachineProfile::Onyx3000Ip35 => {
                    config.onyx3000.flash = Some(path("flash.bin")?);
                    config.onyx3000.l1_port = Some(0);
                }
                _ => {}
            }
        }
        let _ = (config, state);
        Ok(())
    }
}

#[cfg(iris_cpu_parts)]
pub fn family(part: &iris::cpu::parts::Part) -> CpuModel {
    match part.family {
        iris::cpu::parts::Family::R4400 => CpuModel::R4400,
        iris::cpu::parts::Family::R5000 => CpuModel::R5000,
        iris::cpu::parts::Family::R10000 => CpuModel::R10000,
        iris::cpu::parts::Family::R12000 => CpuModel::R12000,
        iris::cpu::parts::Family::R14000 => CpuModel::R14000,
        iris::cpu::parts::Family::R16000 => CpuModel::R16000,
    }
}
