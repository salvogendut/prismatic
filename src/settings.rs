use crate::media::MediaSettings;
use iris::{
    config::{
        CacheModel, CpuModel, KeyboardLayout, MAX_BANKS, MachineProfile, RtcOffset, banks_from4,
    },
    dev::ng1::vc2_timings::NewportResolution,
};
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SerialLogging {
    #[default]
    Off,
    Raw,
    Timestamped,
}

impl SerialLogging {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Raw => "Raw",
            Self::Timestamped => "Timestamped (UTC)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebugOutput {
    #[default]
    Off,
    Screen,
    Console,
}

impl DebugOutput {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Screen => "Screen",
            Self::Console => "Console",
        }
    }

    pub fn cycle(&mut self) {
        *self = next(*self, &[Self::Off, Self::Screen, Self::Console]);
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GifEncoder {
    #[default]
    BuiltIn,
    Ffmpeg,
}

impl GifEncoder {
    pub fn label(self) -> &'static str {
        match self {
            Self::BuiltIn => "Built-in",
            Self::Ffmpeg => "FFmpeg optimize (if available)",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct GifSettings {
    /// Zero selects the guest's native resolution at recording start.
    pub width: u16,
    pub fps: u16,
    pub encoder: GifEncoder,
}

impl Default for GifSettings {
    fn default() -> Self {
        Self {
            width: 0,
            fps: 25,
            encoder: GifEncoder::BuiltIn,
        }
    }
}

impl GifSettings {
    pub fn cycle_width(&mut self) {
        self.width = next(self.width, &[0, 320, 480, 640]);
    }
    pub fn resolution_label(&self) -> String {
        if self.width == 0 {
            "Native (guest screen resolution)".into()
        } else {
            format!("{} px wide (guest aspect ratio)", self.width)
        }
    }
    pub fn cycle_fps(&mut self) {
        self.fps = next(self.fps, &[10, 20, 25]);
    }
    pub fn cycle_encoder(&mut self) {
        self.encoder = next(self.encoder, &[GifEncoder::BuiltIn, GifEncoder::Ffmpeg]);
    }
    pub fn validate(&self) -> Result<(), String> {
        if ![0, 320, 480, 640].contains(&self.width) {
            return Err("GIF width must be 0 (native), 320, 480 or 640 pixels".into());
        }
        if ![10, 20, 25].contains(&self.fps) {
            return Err("GIF frame rate must be 10, 20 or 25 fps".into());
        }
        Ok(())
    }
}

/// Core settings stay active even when Tinker hides their controls.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AdvancedSettings {
    pub resolution: NewportResolution,
    pub cache: CacheModel,
    pub clock_offset_years: i64,
    pub scsi_deferred_interrupts: bool,
    pub serial_logging: SerialLogging,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            resolution: NewportResolution::Guest,
            cache: CacheModel::Auto,
            clock_offset_years: 0,
            scsi_deferred_interrupts: true,
            serial_logging: SerialLogging::Off,
        }
    }
}

impl AdvancedSettings {
    pub fn cycle_resolution(&mut self) {
        self.resolution = next(self.resolution, &NewportResolution::ALL);
    }
    pub fn cycle_cache(&mut self) {
        self.cache = next(
            self.cache,
            &[CacheModel::Auto, CacheModel::Full, CacheModel::Shadow],
        );
    }
    pub fn cycle_clock_offset(&mut self) {
        self.clock_offset_years = next(self.clock_offset_years, &[0, -5, -10, -15, -20, -25, -30]);
    }
    pub fn cycle_serial_logging(&mut self) {
        self.serial_logging = next(
            self.serial_logging,
            &[
                SerialLogging::Off,
                SerialLogging::Raw,
                SerialLogging::Timestamped,
            ],
        );
    }

    pub fn cache_label(&self) -> &'static str {
        match self.cache {
            CacheModel::Auto => "Auto",
            CacheModel::Full => "Full",
            CacheModel::Shadow => "Shadow",
        }
    }
    pub fn clock_label(&self) -> String {
        if self.clock_offset_years == 0 {
            "Host time".into()
        } else {
            format!("Host time {} years", self.clock_offset_years)
        }
    }
    pub fn rtc_offset(&self) -> RtcOffset {
        RtcOffset {
            years: self.clock_offset_years,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        // The file also accepts individual years between the menu presets.
        if !(-30..=0).contains(&self.clock_offset_years) {
            return Err("Advanced clock offset must be between -30 and 0 years".into());
        }
        Ok(())
    }
}

/// Quarter steps keep persisted factors exact and comparable in menu drafts.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(try_from = "f64", into = "f64")]
pub struct WindowScale(u8);

impl Default for WindowScale {
    fn default() -> Self {
        Self(4)
    }
}

impl TryFrom<f64> for WindowScale {
    type Error = String;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !(0.75..=4.0).contains(&value) || (value * 4.0).fract() != 0.0 {
            return Err("Window scale must be 0.75 through 4.0 in steps of 0.25".into());
        }
        Ok(Self((value * 4.0) as u8))
    }
}

impl From<WindowScale> for f64 {
    fn from(value: WindowScale) -> Self {
        f64::from(value.0) / 4.0
    }
}

impl WindowScale {
    pub fn step(self, increase: bool) -> Self {
        Self(if increase {
            (self.0 + 1).min(16)
        } else {
            (self.0 - 1).max(3)
        })
    }

    pub fn percent(self) -> u32 {
        u32::from(self.0) * 25
    }

    pub fn dimensions(self, width: u32, height: u32) -> (u32, u32) {
        (
            width * u32::from(self.0) / 4,
            height * u32::from(self.0) / 4,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub scanlines: bool,
    pub show_status: bool,
    pub smoothing: bool,
    pub tinker: bool,
    pub debug_output: DebugOutput,
    pub window_scale: WindowScale,
    pub gif: GifSettings,
    pub machine: MachineSettings,
    pub media: MediaSettings,
    pub advanced: AdvancedSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scanlines: false,
            show_status: true,
            smoothing: false,
            tinker: false,
            debug_output: DebugOutput::Off,
            window_scale: WindowScale::default(),
            gif: GifSettings::default(),
            machine: MachineSettings::default(),
            media: MediaSettings::default(),
            advanced: AdvancedSettings::default(),
        }
    }
}

impl Settings {
    /// Save only live preferences. Menu drafts must never leak into a shortcut save.
    pub fn save_window_scale(&mut self, path: &Path, scale: WindowScale) -> Result<(), String> {
        if self.window_scale != scale {
            let mut next = self.clone();
            next.window_scale = scale;
            next.save(path)?;
            *self = next;
        }
        Ok(())
    }

    pub fn machine_differs_from(&self, active: &Self) -> bool {
        self.machine != active.machine
            || self.media != active.media
            || self.advanced != active.advanced
    }
    /// Create a complete configuration on first launch, or migrate the previous
    /// default filename. An existing (even invalid) destination is never replaced.
    pub fn open(path: &Path, legacy: Option<&Path>) -> Result<Self, String> {
        if path
            .try_exists()
            .map_err(|e| format!("Cannot access {}: {e}", path.display()))?
        {
            return Self::load(path);
        }
        let settings = match legacy {
            Some(previous) => Self::load(previous)?,
            None => Self::default(),
        };
        settings.save(path)?;
        Ok(settings)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let mut settings: Self = toml::from_str(&text)
                    .map_err(|e| format!("Cannot read settings {}: {e}", path.display()))?;
                settings.machine.validate()?;
                settings.media.validate()?;
                settings.advanced.validate()?;
                settings.gif.validate()?;
                settings
                    .media
                    .resolve_paths(path.parent().unwrap_or(Path::new(".")))?;
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    // Publish only complete files. A failed write leaves the previous settings intact.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.machine.validate()?;
        self.media.validate()?;
        self.advanced.validate()?;
        self.gif.validate()?;
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
    fn window_scale_migrates_persists_and_rejects_invalid_factors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "scanlines = true").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(f64::from(settings.window_scale), 1.0);
        let initial = settings.clone();
        for factor in [0.75, 1.0, 1.25, 2.5, 4.0] {
            settings
                .save_window_scale(&path, WindowScale::try_from(factor).unwrap())
                .unwrap();
            assert_eq!(Settings::load(&path).unwrap(), settings);
            assert!(!settings.machine_differs_from(&initial));
            assert!(settings.scanlines);
        }
        for value in ["0", "0.5", "1.1", "4.25", "-1", "nan", "inf"] {
            let invalid = format!("window_scale = {value}\n");
            fs::write(&path, &invalid).unwrap();
            assert!(Settings::load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
    }

    #[test]
    fn window_scale_bounds_and_save_failure_preserve_live_preferences() {
        let mut scale = WindowScale::default();
        for _ in 0..50 {
            scale = scale.step(true);
        }
        assert_eq!(f64::from(scale), 4.0);
        for _ in 0..50 {
            scale = scale.step(false);
        }
        assert_eq!(f64::from(scale), 0.75);
        assert_eq!(scale.dimensions(960, 720), (720, 540));
        assert_eq!(scale.step(true), WindowScale::default());
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-directory");
        fs::write(&blocked, "keep this file").unwrap();
        let mut settings = Settings::default();
        let before = settings.clone();
        assert!(
            settings
                .save_window_scale(&blocked.join("prismatic.cfg"), scale)
                .is_err()
        );
        assert_eq!(settings, before);
        assert_eq!(fs::read_to_string(blocked).unwrap(), "keep this file");
    }

    #[test]
    fn advanced_settings_migrate_roundtrip_and_reject_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "tinker = false").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.advanced, AdvancedSettings::default());
        settings.advanced = AdvancedSettings {
            resolution: NewportResolution::Res1024x768,
            cache: CacheModel::Shadow,
            clock_offset_years: -18,
            scsi_deferred_interrupts: false,
            serial_logging: SerialLogging::Timestamped,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        let original = fs::read(&path).unwrap();
        settings.advanced.clock_offset_years = -31;
        assert!(settings.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        for value in [
            "clock_offset_years = 1",
            "resolution = '800x600'",
            "cache = 'fast'",
            "serial_logging = 'append'",
            "unknown = true",
        ] {
            fs::write(&path, format!("[advanced]\n{value}\n")).unwrap();
            assert!(Settings::load(&path).is_err());
        }
    }

    #[test]
    fn only_core_changes_require_restart_even_when_tinker_is_hidden() {
        let active = Settings::default();
        let mut draft = active.clone();
        draft.tinker = true;
        draft.smoothing = true;
        draft.scanlines = true;
        draft.show_status = false;
        draft.debug_output = DebugOutput::Screen;
        draft.gif.cycle_width();
        draft.gif.cycle_fps();
        draft.gif.cycle_encoder();
        assert!(!draft.machine_differs_from(&active));
        for change in [
            AdvancedSettings::cycle_resolution,
            AdvancedSettings::cycle_cache,
            AdvancedSettings::cycle_clock_offset,
            AdvancedSettings::cycle_serial_logging,
            |a: &mut AdvancedSettings| a.scsi_deferred_interrupts = !a.scsi_deferred_interrupts,
        ] {
            draft.advanced = active.advanced.clone();
            change(&mut draft.advanced);
            draft.tinker = false;
            assert!(draft.machine_differs_from(&active));
        }
    }

    #[test]
    fn debug_output_defaults_to_off_persists_with_tinker_hidden_and_validates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "tinker = false\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.debug_output, DebugOutput::Off);
        for output in [DebugOutput::Screen, DebugOutput::Console, DebugOutput::Off] {
            settings.debug_output.cycle();
            assert_eq!(settings.debug_output, output);
            settings.save(&path).unwrap();
            assert_eq!(Settings::load(&path).unwrap(), settings);
            assert!(!settings.machine_differs_from(&Settings::default()));
        }
        fs::write(&path, "debug_output = 'file'\n").unwrap();
        assert!(Settings::load(&path).is_err());
    }

    #[test]
    fn gif_settings_migrate_persist_and_reject_invalid_values_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "tinker = false\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.gif, GifSettings::default());
        assert_eq!(settings.gif.width, 0);
        settings.gif = GifSettings {
            width: 640,
            fps: 10,
            encoder: GifEncoder::Ffmpeg,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        let original = fs::read(&path).unwrap();
        settings.gif.width = 1;
        assert!(settings.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        for invalid in ["width = 2048", "fps = 0", "fps = 60", "encoder = 'unknown'"] {
            fs::write(&path, format!("[gif]\n{invalid}\n")).unwrap();
            assert!(Settings::load(&path).is_err());
        }
    }

    #[test]
    fn gif_native_and_scaled_choices_cycle_persist_and_keep_existing_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "[gif]\nwidth = 640\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.gif.width, 640);
        for width in [0, 320, 480, 640, 0] {
            settings.gif.cycle_width();
            assert_eq!(settings.gif.width, width);
            assert!(!settings.machine_differs_from(&Settings::default()));
            settings.save(&path).unwrap();
            assert_eq!(Settings::load(&path).unwrap(), settings);
        }
        assert_eq!(
            settings.gif.resolution_label(),
            "Native (guest screen resolution)"
        );
    }

    #[test]
    fn cfg_is_created_migrates_old_preferences_and_wins_on_subsequent_launches() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("prismatic.cfg");
        let legacy = dir.path().join("prismatic.toml");
        let old = Settings {
            scanlines: true,
            ..Default::default()
        };
        old.save(&legacy).unwrap();
        let old_bytes = fs::read(&legacy).unwrap();
        assert_eq!(Settings::open(&cfg, Some(&legacy)).unwrap(), old);
        let mut updated = old.clone();
        updated.machine.ram_mb = 128;
        updated.media.devices.insert(
            4,
            crate::media::Medium {
                kind: crate::media::MediaKind::Cdrom,
                path: "/media/install.efs".into(),
            },
        );
        updated.save(&cfg).unwrap();
        assert_eq!(Settings::open(&cfg, Some(&legacy)).unwrap(), updated);
        assert_eq!(fs::read(&legacy).unwrap(), old_bytes);
        fs::write(&cfg, "bad configuration").unwrap();
        assert!(Settings::open(&cfg, Some(&legacy)).is_err());
        assert_eq!(fs::read_to_string(&cfg).unwrap(), "bad configuration");
        fs::remove_file(&cfg).unwrap();
        fs::write(&legacy, "bad legacy configuration").unwrap();
        assert!(Settings::open(&cfg, Some(&legacy)).is_err());
        assert!(!cfg.exists());
        assert_eq!(Settings::open(&cfg, None).unwrap(), Settings::default());
        assert!(cfg.is_file());
    }

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
