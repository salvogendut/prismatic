use iris::config::{CpuModel, KeyboardLayout, MAX_BANKS, MachineProfile, banks_from4};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// The hardware currently supported by Prismatic's Newport display path.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MachineSettings {
    pub profile: MachineProfile,
    pub cpu: CpuModel,
    pub ram_mb: u32,
    pub keyboard: KeyboardLayout,
}

impl Default for MachineSettings {
    fn default() -> Self {
        Self {
            profile: MachineProfile::IndyIp24,
            cpu: CpuModel::R4400,
            ram_mb: 64,
            keyboard: KeyboardLayout::Ansi,
        }
    }
}

impl MachineSettings {
    pub fn ram_sizes(&self) -> &'static [u32] {
        match self.profile {
            MachineProfile::Indigo2Ip22 => &[32, 64, 128, 256, 384, 512],
            _ => &[32, 64, 128, 256],
        }
    }

    pub fn cpus(&self) -> &'static [CpuModel] {
        match self.profile {
            MachineProfile::IndyIp24 => &[CpuModel::R4400, CpuModel::R5000],
            _ => &[CpuModel::R4400],
        }
    }

    pub fn cycle_profile(&mut self) {
        self.profile = match self.profile {
            MachineProfile::IndyIp24 => MachineProfile::Indigo2Ip22,
            _ => MachineProfile::IndyIp24,
        };
        if !self.cpus().contains(&self.cpu) {
            self.cpu = self.cpus()[0];
        }
        if !self.ram_sizes().contains(&self.ram_mb) {
            self.ram_mb = *self.ram_sizes().last().unwrap();
        }
    }

    pub fn cycle_ram(&mut self) {
        self.ram_mb = next(self.ram_mb, self.ram_sizes());
    }
    pub fn cycle_cpu(&mut self) {
        self.cpu = next(self.cpu, self.cpus());
    }
    pub fn cycle_keyboard(&mut self) {
        self.keyboard = next(self.keyboard, &KeyboardLayout::ALL);
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.profile,
            MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22
        ) {
            return Err(
                "Prismatic currently supports Indy IP24 and Indigo2 IP22 with Newport graphics"
                    .into(),
            );
        }
        if !self.cpus().contains(&self.cpu) {
            return Err(format!(
                "{} is not supported on {}",
                self.cpu.label(),
                self.profile.label()
            ));
        }
        if !self.ram_sizes().contains(&self.ram_mb) {
            return Err(format!(
                "RAM must be one of {:?} MB for {}",
                self.ram_sizes(),
                self.profile.label()
            ));
        }
        Ok(())
    }

    pub fn banks(&self) -> Result<[u32; MAX_BANKS], String> {
        self.validate()?;
        let mut remaining = self.ram_mb;
        let mut banks = [0; 4];
        for bank in &mut banks {
            *bank = remaining.min(128);
            remaining -= *bank;
        }
        Ok(banks_from4(banks))
    }

    pub fn summary(&self) -> String {
        let model = if self.profile == MachineProfile::IndyIp24 {
            "Indy"
        } else {
            "Indigo2"
        };
        let cpu = self.cpu.label().trim_start_matches("MIPS ");
        format!("{model} / {cpu} / {} MB", self.ram_mb)
    }

    pub fn state_dir(&self, preferences: &Path, override_dir: Option<&Path>) -> PathBuf {
        let name = if self.profile == MachineProfile::IndyIp24 {
            "indy"
        } else {
            "indigo2"
        };
        match override_dir {
            // Preserve the existing --state-dir layout for Indy sessions.
            Some(path) if self.profile == MachineProfile::IndyIp24 => path.to_owned(),
            Some(path) => path.join(name),
            None => preferences.join(name),
        }
    }
}

fn next<T: Copy + PartialEq>(current: T, choices: &[T]) -> T {
    let index = choices
        .iter()
        .position(|&value| value == current)
        .unwrap_or(0);
    choices[(index + 1) % choices.len()]
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub scanlines: bool,
    pub show_status: bool,
    pub smoothing: bool,
    pub tinker: bool,
    pub machine: MachineSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scanlines: false,
            show_status: true,
            smoothing: false,
            tinker: false,
            machine: MachineSettings::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let settings: Self = toml::from_str(&text)
                    .map_err(|e| format!("Cannot read settings {}: {e}", path.display()))?;
                settings.machine.validate()?;
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    // Publish only complete files. A failed write leaves the previous settings intact.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.machine.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let text = toml::to_string_pretty(self)?;
            fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(text.as_bytes())?;
            file.as_file().sync_all()?;
            file.persist(path)?;
            Ok(())
        };
        write().map_err(|e| format!("Cannot save {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/prismatic.toml");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let settings = Settings {
            scanlines: true,
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        Settings::default().save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn malformed_and_unknown_settings_are_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.toml");
        for text in [
            "scanlines = 'yes'",
            "future_option = true",
            "[machine]\nram_mb = 0",
            "[machine]\nprofile = 'o2_ip32'",
            "[machine]\nprofile = 'indigo2_ip22'\ncpu = 'r5000'",
            "[machine]\nunknown = true",
        ] {
            fs::write(&path, text).unwrap();
            assert!(Settings::load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn old_preferences_get_default_hardware_and_new_hardware_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.toml");
        fs::write(&path, "scanlines = true").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert!(settings.scanlines);
        assert_eq!(settings.machine, MachineSettings::default());
        settings.machine.profile = MachineProfile::Indigo2Ip22;
        settings.machine.ram_mb = 384;
        settings.machine.keyboard = KeyboardLayout::Iso;
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        let original = fs::read(&path).unwrap();
        settings.machine.ram_mb = 100;
        assert!(settings.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn switching_models_keeps_supported_values_and_adjusts_incompatible_ones() {
        let mut hardware = MachineSettings {
            cpu: CpuModel::R5000,
            ram_mb: 128,
            ..Default::default()
        };
        hardware.cycle_profile();
        assert_eq!(hardware.profile, MachineProfile::Indigo2Ip22);
        assert_eq!(hardware.cpu, CpuModel::R4400);
        assert_eq!(hardware.ram_mb, 128);
        hardware.ram_mb = 512;
        hardware.cycle_profile();
        assert_eq!(hardware.ram_mb, 256);
        hardware.cycle_ram();
        assert_eq!(hardware.ram_mb, 32);
        hardware.validate().unwrap();
    }

    #[test]
    fn machine_state_is_isolated_and_existing_indy_paths_are_preserved() {
        let mut hardware = MachineSettings::default();
        let root = Path::new("/tmp/preferences");
        let custom = Path::new("/tmp/custom-machine");
        assert_eq!(hardware.state_dir(root, None), root.join("indy"));
        assert_eq!(hardware.state_dir(root, Some(custom)), custom);
        hardware.cycle_profile();
        assert_eq!(hardware.state_dir(root, None), root.join("indigo2"));
        assert_eq!(
            hardware.state_dir(root, Some(custom)),
            custom.join("indigo2")
        );
    }

    #[test]
    fn failed_write_does_not_replace_preferences_or_modify_settings() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("not-a-directory");
        fs::write(&parent, "keep me").unwrap();
        let settings = Settings::default();
        assert!(settings.save(&parent.join("prismatic.toml")).is_err());
        assert_eq!(fs::read_to_string(&parent).unwrap(), "keep me");
        assert_eq!(settings, Settings::default());
    }
}
