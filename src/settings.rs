use crate::media::MediaSettings;
use iris::{
    config::{
        CacheModel, CpuModel, GraphicsBoard, KeyboardLayout, MAX_BANKS, MachineProfile, RtcOffset,
        banks_from4,
    },
    dev::ng1::vc2_timings::NewportResolution,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Hardware supported by Iris's Newport, IMPACT and GBE display paths.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MachineSettings {
    pub profile: MachineProfile,
    pub cpu: CpuModel,
    pub ram_mb: u32,
    pub cpus: u8,
    pub graphics: Option<GraphicsBoard>,
    pub heads: u8,
    pub keyboard: KeyboardLayout,
    pub o2_prom: Option<String>,
    pub octane_prom: Option<String>,
    pub prom_boot: PromBoot,
    pub prom_console: PromConsole,
}

/// Independent of the OS boot preset and stored with each machine label.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromConsole {
    #[default]
    KeepExisting,
    Graphics,
    Serial,
}

impl PromConsole {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::KeepExisting => Self::Graphics,
            Self::Graphics => Self::Serial,
            Self::Serial => Self::KeepExisting,
        };
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::KeepExisting => "Keep existing",
            Self::Graphics => "Graphics",
            Self::Serial => "Serial",
        }
    }

    pub fn environment(self) -> Option<iris::prom_env::Console> {
        match self {
            Self::KeepExisting => None,
            Self::Graphics => Some(iris::prom_env::Console::Graphics),
            Self::Serial => Some(iris::prom_env::Console::Serial),
        }
    }

    pub fn hint(self, profile: MachineProfile) -> &'static str {
        match self {
            Self::KeepExisting => {
                "Leaves the PROM console unchanged, including a previously applied choice."
            }
            Self::Graphics => {
                "On boot: console=g. Use the guest display and keyboard in this window."
            }
            Self::Serial => match profile {
                MachineProfile::O2Ip32 | MachineProfile::OctaneIp30 => {
                    "On boot: console=d. Use Serial A; routing and endpoints are in Advanced > Connections."
                }
                _ => {
                    "On boot: console=d. Use Serial B; routing and endpoints are in Advanced > Connections."
                }
            },
        }
    }
}

/// A per-machine PROM preset; old configurations retain their firmware settings.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromBoot {
    #[default]
    KeepExisting,
    Irix,
    Netbsd,
}

impl PromBoot {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::KeepExisting => Self::Irix,
            Self::Irix => Self::Netbsd,
            Self::Netbsd => Self::KeepExisting,
        };
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::KeepExisting => "Keep existing",
            Self::Irix => "IRIX",
            Self::Netbsd => "NetBSD",
        }
    }

    pub fn environment(self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Self::KeepExisting => None,
            Self::Irix => Some(("sash", "/unix", "auto")),
            Self::Netbsd => Some(("boot", "netbsd.boot", "auto")),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::KeepExisting => {
                "Leaves the PROM boot variables unchanged, including any previously applied preset."
            }
            Self::Irix => "On boot: OSLoader=sash, OSLoadFilename=/unix, OSLoadOptions=auto.",
            Self::Netbsd => {
                "On boot: OSLoader=boot, OSLoadFilename=netbsd.boot, OSLoadOptions=auto."
            }
        }
    }
}

impl Default for MachineSettings {
    fn default() -> Self {
        Self {
            profile: MachineProfile::IndyIp24,
            cpu: CpuModel::R4400,
            ram_mb: 64,
            cpus: 1,
            graphics: None,
            heads: 1,
            keyboard: KeyboardLayout::Ansi,
            o2_prom: None,
            octane_prom: None,
            prom_boot: PromBoot::default(),
            prom_console: PromConsole::default(),
        }
    }
}

impl MachineSettings {
    pub const PROFILES: [MachineProfile; 5] = [
        MachineProfile::IndyIp24,
        MachineProfile::Indigo2Ip22,
        MachineProfile::Indigo2Ip28,
        MachineProfile::O2Ip32,
        MachineProfile::OctaneIp30,
    ];

    pub fn ram_sizes(&self) -> &'static [u32] {
        match self.profile {
            MachineProfile::Indigo2Ip22 => &[32, 64, 128, 256, 384, 512],
            MachineProfile::IndyIp24 => &[32, 64, 128, 256],
            MachineProfile::Indigo2Ip28 | MachineProfile::O2Ip32 => {
                &[32, 64, 128, 256, 384, 512, 768, 1024]
            }
            MachineProfile::OctaneIp30 => &[
                64, 128, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 6144, 8192,
            ],
        }
    }

    pub fn cpus(&self) -> &'static [CpuModel] {
        match self.profile {
            MachineProfile::IndyIp24 => &[CpuModel::R4400, CpuModel::R5000],
            MachineProfile::Indigo2Ip22 => &[CpuModel::R4400],
            MachineProfile::O2Ip32 => &[CpuModel::R5000, CpuModel::R10000],
            MachineProfile::Indigo2Ip28 | MachineProfile::OctaneIp30 => &[CpuModel::R10000],
        }
    }

    pub fn cycle_profile(&mut self) {
        self.profile = next(self.profile, &Self::PROFILES);
        if self
            .graphics
            .is_some_and(|board| !self.graphics_choices().contains(&board))
        {
            self.graphics = None;
        }
        if !self.newport() {
            self.heads = 1;
        }
        if self.profile != MachineProfile::OctaneIp30 {
            self.cpus = 1;
        }
        if !self.cpus().contains(&self.cpu) {
            self.cpu = self.cpus()[0];
        }
        if !self.ram_sizes().contains(&self.ram_mb) {
            self.ram_mb = self
                .ram_mb
                .clamp(self.ram_sizes()[0], *self.ram_sizes().last().unwrap());
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
        if !(1..=2).contains(&self.cpus)
            || (self.cpus != 1 && self.profile != MachineProfile::OctaneIp30)
        {
            return Err("Only Octane supports two CPUs; other models require one CPU".into());
        }
        if self
            .graphics
            .is_some_and(|board| !self.graphics_choices().contains(&board))
        {
            return Err(format!(
                "Graphics board is not supported on {}",
                self.profile.label()
            ));
        }
        if !(1..=2).contains(&self.heads) || (self.heads == 2 && !self.newport()) {
            return Err("Two display heads require Newport graphics".into());
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
        let max_bank = match self.profile {
            MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22 => 128,
            MachineProfile::O2Ip32 => 256,
            MachineProfile::OctaneIp30 => 2048,
            _ => 512,
        };
        for bank in &mut banks {
            *bank = self
                .profile
                .bank_sizes()
                .iter()
                .copied()
                .filter(|&size| {
                    size <= remaining && size <= max_bank
                        // Iris's IP28 PROM rejects 32/128 MB single-bank layouts.
                        // Keep the requested total using 16/64/256/512 MB banks.
                        && !(self.profile == MachineProfile::Indigo2Ip28 && matches!(size, 32 | 128))
                })
                .max()
                .unwrap_or(0);
            remaining -= *bank;
        }
        if remaining != 0 {
            return Err("RAM does not fit the machine's four banks".into());
        }
        Ok(banks_from4(banks))
    }

    pub fn summary(&self) -> String {
        let model = match self.profile {
            MachineProfile::IndyIp24 => "Indy",
            MachineProfile::Indigo2Ip22 => "Indigo2",
            MachineProfile::Indigo2Ip28 => "Indigo2 IMPACT",
            MachineProfile::O2Ip32 => "O2",
            MachineProfile::OctaneIp30 => "Octane",
        };
        let cpu = self.cpu.label().trim_start_matches("MIPS ");
        let processors = if self.cpus == 2 { "2x " } else { "" };
        format!("{model} / {processors}{cpu} / {} MB", self.ram_mb)
    }

    pub fn state_dir(&self, preferences: &Path, override_dir: Option<&Path>) -> PathBuf {
        let name = match self.profile {
            MachineProfile::IndyIp24 => "indy",
            MachineProfile::Indigo2Ip22 => "indigo2",
            MachineProfile::Indigo2Ip28 => "indigo2-ip28",
            MachineProfile::O2Ip32 => "o2",
            MachineProfile::OctaneIp30 => "octane",
        };
        match override_dir {
            // Preserve the existing --state-dir layout for Indy sessions.
            Some(path) if self.profile == MachineProfile::IndyIp24 => path.to_owned(),
            Some(path) => path.join(name),
            None => preferences.join(name),
        }
    }

    /// Choices that have a usable graphical path for this workstation.
    pub fn graphics_choices(&self) -> &'static [GraphicsBoard] {
        use GraphicsBoard::*;
        match self.profile {
            MachineProfile::IndyIp24 => &[Newport, Xz],
            MachineProfile::Indigo2Ip22 => &GraphicsBoard::ALL,
            MachineProfile::Indigo2Ip28 | MachineProfile::OctaneIp30 => {
                &[SolidImpact, HighImpact, MaxImpact]
            }
            MachineProfile::O2Ip32 => &[], // Integrated GBE, not a selectable board.
        }
    }

    pub fn graphics_board(&self) -> Option<GraphicsBoard> {
        self.graphics.or(match self.profile {
            MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22 => Some(GraphicsBoard::Newport),
            MachineProfile::Indigo2Ip28 | MachineProfile::OctaneIp30 => {
                Some(GraphicsBoard::SolidImpact)
            }
            MachineProfile::O2Ip32 => None,
        })
    }

    pub fn cycle_graphics(&mut self) {
        if let Some(board) = self.graphics_board() {
            self.graphics = Some(next(board, self.graphics_choices()));
            if !self.newport() {
                self.heads = 1;
            }
        }
    }

    pub fn newport(&self) -> bool {
        self.graphics_board() == Some(GraphicsBoard::Newport)
    }

    pub fn supports_prom_override(&self) -> bool {
        matches!(
            self.profile,
            MachineProfile::O2Ip32 | MachineProfile::OctaneIp30
        )
    }

    pub fn prom_path(&self) -> Option<&str> {
        match self.profile {
            MachineProfile::O2Ip32 => self.o2_prom.as_deref(),
            MachineProfile::OctaneIp30 => self.octane_prom.as_deref(),
            _ => None,
        }
    }

    pub fn set_prom(&mut self, path: Option<String>) {
        match self.profile {
            MachineProfile::O2Ip32 => self.o2_prom = path,
            MachineProfile::OctaneIp30 => self.octane_prom = path,
            _ => {}
        }
    }

    pub fn prom_label(&self) -> String {
        self.prom_path().map_or_else(
            || "Bundled with Iris".into(),
            |path| {
                Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        )
    }

    pub fn validate_prom(&mut self) -> Result<(), String> {
        if let Some(path) = self.prom_path() {
            let path = inspect_prom(self.profile, Path::new(path))?;
            self.set_prom(Some(path));
        }
        Ok(())
    }

    fn resolve_prom_paths(&mut self, directory: &Path) -> Result<(), String> {
        for path in [&mut self.o2_prom, &mut self.octane_prom]
            .into_iter()
            .flatten()
        {
            if Path::new(path).is_relative() {
                *path = std::path::absolute(directory.join(&*path))
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("PROM paths must be UTF-8")?
                    .to_owned();
            }
        }
        Ok(())
    }
}

/// Read-only draft validation. No flash/NVRAM file is opened or created here.
pub fn inspect_prom(profile: MachineProfile, path: &Path) -> Result<String, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("Cannot open PROM {}: {e}", path.display()))?;
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    let valid_size = match profile {
        MachineProfile::O2Ip32 => metadata.len() == 512 * 1024,
        MachineProfile::OctaneIp30 => (512 * 1024..=1024 * 1024).contains(&metadata.len()),
        _ => return Err("This machine uses bundled firmware".into()),
    };
    if !metadata.is_file() || !valid_size {
        return Err(match profile {
            MachineProfile::O2Ip32 => "O2 requires a 512 KiB IP32 PROM image",
            _ => "Octane requires an IP30 PROM image (512 KiB to 1 MiB)",
        }
        .into());
    }
    // Reject directories and special files before opening (a FIFO can block).
    fs::File::open(&path).map_err(|e| format!("Cannot read PROM: {e}"))?;
    path.to_str()
        .map(str::to_owned)
        .ok_or("PROM paths must be UTF-8".into())
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
    pub expert: crate::expert::ExpertSettings,
    pub extensions: crate::extensions::ExtensionsSettings,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            resolution: NewportResolution::Guest,
            cache: CacheModel::Auto,
            clock_offset_years: 0,
            scsi_deferred_interrupts: true,
            serial_logging: SerialLogging::Off,
            expert: crate::expert::ExpertSettings::default(),
            extensions: Default::default(),
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
        self.expert.validate()?;
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSettings {
    pub muted: bool,
    pub volume: u8,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            muted: false,
            volume: 100,
        }
    }
}

impl AudioSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.volume > 100 {
            return Err("Guest audio volume must be between 0 and 100".into());
        }
        Ok(())
    }

    pub fn cycle_volume(&mut self) {
        self.volume = if self.volume == 0 {
            100
        } else {
            self.volume.saturating_sub(10)
        };
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[serde(skip)]
    pub(crate) catalog_origin: crate::profiles::CatalogOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_label: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub machines: BTreeMap<String, crate::profiles::NamedMachine>,
    pub scanlines: bool,
    pub show_status: bool,
    pub smoothing: bool,
    pub tinker: bool,
    pub debug_output: DebugOutput,
    pub window_scale: WindowScale,
    pub gif: GifSettings,
    pub audio: AudioSettings,
    pub machine: MachineSettings,
    pub media: MediaSettings,
    pub advanced: AdvancedSettings,
    pub network: crate::network::NetworkSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            catalog_origin: crate::profiles::CatalogOrigin::default(),
            machine_label: None,
            machines: BTreeMap::new(),
            scanlines: false,
            show_status: true,
            smoothing: false,
            tinker: false,
            debug_output: DebugOutput::Off,
            window_scale: WindowScale::default(),
            gif: GifSettings::default(),
            audio: AudioSettings::default(),
            machine: MachineSettings::default(),
            media: MediaSettings::default(),
            advanced: AdvancedSettings::default(),
            network: crate::network::NetworkSettings::default(),
        }
    }
}

impl Settings {
    /// Save only live preferences. Menu drafts must never leak into a shortcut save.
    pub fn save_window_scale(&mut self, path: &Path, scale: WindowScale) -> Result<(), String> {
        if self.window_scale != scale {
            let mut next = self.clone();
            next.window_scale = scale;
            next.remember_machine();
            next.save(path)?;
            *self = next;
        }
        Ok(())
    }

    pub fn machine_differs_from(&self, active: &Self) -> bool {
        (self.machine_label.is_some()
            && active.machine_label.is_some()
            && self.machine_label != active.machine_label)
            || self.machine_state_key() != active.machine_state_key()
            || self.machine != active.machine
            || self.media != active.media
            || self.advanced != active.advanced
            || self.network.requires_restart(&active.network)
            || ((self.advanced.extensions.daynaport.is_some()
                || active.advanced.extensions.daynaport.is_some())
                && self.network != active.network)
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
                settings.validate_machines()?;
                let parent = path.parent().unwrap_or(Path::new("."));
                for saved in settings.machines.values_mut() {
                    saved.media.resolve_paths(parent)?;
                    saved.machine.resolve_prom_paths(parent)?;
                    saved.network.resolve_paths(parent)?;
                    saved.advanced.expert.resolve_paths(parent)?;
                }
                if let Some(label) = settings.machine_label.clone() {
                    settings.select_machine(&label)?;
                }
                settings.machine.validate()?;
                settings.media.validate_for(settings.machine.profile)?;
                settings.advanced.validate()?;
                settings.advanced.extensions.validate(
                    &settings.machine,
                    &settings.media,
                    &settings.network,
                )?;
                settings.gif.validate()?;
                settings.audio.validate()?;
                settings.network.validate(settings.machine.profile)?;
                settings.network.resolve_paths(parent)?;
                settings.advanced.expert.resolve_paths(parent)?;
                settings
                    .media
                    .resolve_paths(path.parent().unwrap_or(Path::new(".")))?;
                settings
                    .machine
                    .resolve_prom_paths(path.parent().unwrap_or(Path::new(".")))?;
                settings.catalog_origin.record(&settings.machines);
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    // Publish only complete files. A failed write leaves the previous settings intact.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate_machines()?;
        self.machine.validate()?;
        self.media.validate_for(self.machine.profile)?;
        self.advanced.validate()?;
        self.advanced
            .extensions
            .validate(&self.machine, &self.media, &self.network)?;
        self.gif.validate()?;
        self.audio.validate()?;
        self.network.validate(self.machine.profile)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(parent)?;
            let mut lock_path = path.as_os_str().to_owned();
            lock_path.push(".lock");
            let _lease = crate::instance::Lease::acquire(Path::new(&lock_path))?;
            let mut saved = self.clone();
            saved.network.resolve_paths(parent)?;
            saved.advanced.expert.resolve_paths(parent)?;
            saved.remember_machine();
            for machine in saved.machines.values_mut() {
                machine.media.resolve_paths(parent)?;
                machine.machine.resolve_prom_paths(parent)?;
                machine.network.resolve_paths(parent)?;
                machine.advanced.expert.resolve_paths(parent)?;
            }
            let own_catalog = saved.machines.clone();
            saved.merge_machine_catalog(path)?;
            let text = toml::to_string_pretty(&saved)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(text.as_bytes())?;
            file.as_file().sync_all()?;
            file.persist(path)?;
            self.catalog_origin.record(&own_catalog);
            Ok(())
        };
        write().map_err(|e| format!("Cannot save {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prom_console_migrates_and_roundtrips_independently_per_label() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "[machine]\nprom_boot = 'netbsd'\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.machine.prom_console, PromConsole::KeepExisting);
        settings.machine.prom_console.cycle();
        assert_eq!(settings.machine.prom_console, PromConsole::Graphics);
        settings.save_as_machine("Graphics Indy").unwrap();
        settings.machine.prom_console.cycle();
        assert_eq!(settings.machine.prom_console, PromConsole::Serial);
        settings.machine.cycle_profile();
        settings.save_as_machine("Serial Indigo2").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.machine.prom_console, PromConsole::Serial);
        loaded.select_machine("Graphics Indy").unwrap();
        assert_eq!(loaded.machine.prom_console, PromConsole::Graphics);
        assert_eq!(loaded.machine.prom_boot, PromBoot::Netbsd);
        assert_eq!(loaded.machine.profile, MachineProfile::IndyIp24);
        let mut draft = loaded.clone();
        draft.machine.prom_console = PromConsole::Serial;
        assert!(draft.machine_differs_from(&loaded));
        draft.machine.prom_console.cycle();
        assert_eq!(draft.machine.prom_console, PromConsole::KeepExisting);
        assert!(draft.machine_differs_from(&loaded));
        fs::write(&path, "[machine]\nprom_console = 'unknown'\n").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(PromConsole::KeepExisting.environment(), None);
        assert_eq!(PromConsole::Graphics.environment().unwrap().value(), "g");
        assert_eq!(PromConsole::Serial.environment().unwrap().value(), "d");
    }

    #[test]
    fn prom_boot_presets_migrate_and_roundtrip_per_machine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "[machine]\nram_mb = 64\n").unwrap();
        let mut settings = Settings::load(&path).unwrap();
        assert_eq!(settings.machine.prom_boot, PromBoot::KeepExisting);
        settings.machine.prom_boot = PromBoot::Irix;
        settings.save_as_machine("IRIX Indy").unwrap();
        settings.machine.prom_boot = PromBoot::Netbsd;
        settings.machine.cycle_profile();
        settings.save_as_machine("NetBSD Indigo2").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.machine.prom_boot, PromBoot::Netbsd);
        loaded.select_machine("IRIX Indy").unwrap();
        assert_eq!(loaded.machine.prom_boot, PromBoot::Irix);
        assert_eq!(loaded.machine.profile, MachineProfile::IndyIp24);
        let mut draft = loaded.clone();
        draft.machine.prom_boot = PromBoot::Netbsd;
        assert!(draft.machine_differs_from(&loaded));
        draft.machine.prom_boot = PromBoot::KeepExisting;
        assert!(draft.machine_differs_from(&loaded));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("prom_boot = \"irix\""));
        assert!(text.contains("prom_boot = \"netbsd\""));
        fs::write(&path, "[machine]\nprom_boot = 'unknown'\n").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(PromBoot::KeepExisting.environment(), None);
        assert_eq!(
            PromBoot::Irix.environment(),
            Some(("sash", "/unix", "auto"))
        );
        assert_eq!(
            PromBoot::Netbsd.environment(),
            Some(("boot", "netbsd.boot", "auto"))
        );
    }

    #[test]
    fn optional_device_stanzas_roundtrip_per_label_and_force_network_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings::default();
        settings.advanced.extensions.vino = crate::extensions::Source::Black;
        settings.advanced.extensions.daynaport = Some(5);
        settings.advanced.extensions.camera_index = 3;
        settings.save_as_machine("Indy-video").unwrap();
        settings.advanced.extensions = Default::default();
        settings.machine.profile = MachineProfile::Indigo2Ip22;
        settings.save_as_machine("Indigo2").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        loaded.select_machine("Indy-video").unwrap();
        assert_eq!(loaded.advanced.extensions.camera_index, 3);
        assert_eq!(loaded.advanced.extensions.daynaport, Some(5));
        let mut draft = loaded.clone();
        draft.network.nat_subnet = "10.1.0.0/24".into();
        assert!(draft.machine_differs_from(&loaded));
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("advanced.extensions")
        );
    }

    #[test]
    fn extensions_migrate_persist_per_label_and_reject_invalid_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(
            &path,
            "[machine]\nprofile = 'indigo2_ip28'\ncpu = 'r10000'\n",
        )
        .unwrap();
        let old = Settings::load(&path).unwrap();
        assert_eq!(
            old.machine.graphics_board(),
            Some(GraphicsBoard::SolidImpact)
        );
        assert_eq!((old.machine.cpus, old.machine.heads), (1, 1));
        let mut settings = Settings::default();
        settings.machine.heads = 2;
        settings.save_as_machine("Twin Indy").unwrap();
        settings.machine = MachineSettings {
            profile: MachineProfile::OctaneIp30,
            cpu: CpuModel::R10000,
            cpus: 2,
            ram_mb: 8192,
            graphics: Some(GraphicsBoard::MaxImpact),
            ..Default::default()
        };
        settings.media.devices.insert(
            31,
            crate::media::Medium {
                kind: crate::media::MediaKind::Disk,
                path: "/tmp/octane.raw".into(),
            },
        );
        settings.save_as_machine("Octane").unwrap();
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.machine.cpus, 2);
        assert_eq!(loaded.machine.banks().unwrap().iter().sum::<u32>(), 8192);
        assert_eq!(
            loaded.machine.graphics_board(),
            Some(GraphicsBoard::MaxImpact)
        );
        assert!(loaded.media.devices.contains_key(&31));
        loaded.select_machine("Twin Indy").unwrap();
        assert_eq!(loaded.machine.heads, 2);
        assert_eq!(
            loaded.machine.graphics_board(),
            Some(GraphicsBoard::Newport)
        );
        assert!(loaded.media.devices.is_empty());
        let saved = fs::read(&path).unwrap();
        loaded.machine.cpus = 2;
        assert!(loaded.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        for invalid in [
            "cpus = 0",
            "cpus = 3",
            "heads = 0",
            "heads = 3",
            "graphics = 'solidimpact'",
            "graphics = 'xz'\nheads = 2",
        ] {
            fs::write(&path, format!("[machine]\n{invalid}\n")).unwrap();
            assert!(Settings::load(&path).is_err(), "{invalid}");
        }
    }

    #[test]
    fn changing_models_and_boards_reconciles_extensions() {
        let mut machine = MachineSettings {
            heads: 2,
            ..Default::default()
        };
        machine.cycle_graphics();
        assert_eq!(machine.graphics_board(), Some(GraphicsBoard::Xz));
        assert_eq!(machine.heads, 1);
        machine.cycle_profile();
        assert_eq!(machine.graphics_board(), Some(GraphicsBoard::Xz));
        machine.cycle_profile();
        assert_eq!(machine.graphics_board(), Some(GraphicsBoard::SolidImpact));
        machine.cycle_profile();
        assert_eq!(machine.graphics_board(), None);
        machine.cycle_graphics(); // O2 integrated graphics cannot change.
        machine.validate().unwrap();
        machine.cycle_profile();
        machine.cpus = 2;
        machine.ram_mb = 8192;
        machine.cycle_profile();
        assert_eq!((machine.cpus, machine.ram_mb), (1, 256));
        assert_eq!(machine.graphics_board(), Some(GraphicsBoard::Newport));
        machine.validate().unwrap();
    }

    #[test]
    fn audio_defaults_persist_without_restart_and_invalid_volume_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        fs::write(&path, "scanlines = true\n").unwrap();
        let original = Settings::load(&path).unwrap();
        assert_eq!(
            original.audio,
            AudioSettings {
                muted: false,
                volume: 100
            }
        );
        let mut edited = original.clone();
        edited.audio.muted = true;
        edited.audio.volume = 40;
        assert!(!edited.machine_differs_from(&original));
        edited.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().audio, edited.audio);
        let saved = fs::read(&path).unwrap();
        edited.audio.volume = 101;
        assert!(edited.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        fs::write(&path, "[audio]\nvolume = 101\n").unwrap();
        assert!(Settings::load(&path).is_err());
    }

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
            ..Default::default()
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
        for profile in [
            MachineProfile::Indigo2Ip28,
            MachineProfile::O2Ip32,
            MachineProfile::OctaneIp30,
        ] {
            hardware.cycle_profile();
            assert_eq!(hardware.profile, profile);
            assert_eq!(hardware.cpu, CpuModel::R10000);
            assert_eq!(hardware.ram_mb, 512);
            hardware.validate().unwrap();
        }
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
    fn all_machine_state_paths_are_distinct() {
        let mut paths = std::collections::HashSet::new();
        for profile in MachineSettings::PROFILES {
            let hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            assert!(paths.insert(hardware.state_dir(Path::new("/tmp/preferences"), None)));
        }
    }

    #[test]
    fn ip28_presets_use_bank_layouts_that_pass_prom_sizing() {
        let mut hardware = MachineSettings {
            profile: MachineProfile::Indigo2Ip28,
            cpu: CpuModel::R10000,
            ..Default::default()
        };
        for (ram_mb, banks) in [
            (32, [16, 16, 0, 0]),
            (128, [64, 64, 0, 0]),
            (384, [256, 64, 64, 0]),
            (1024, [512, 512, 0, 0]),
        ] {
            hardware.ram_mb = ram_mb;
            assert_eq!(hardware.banks().unwrap(), banks_from4(banks));
        }
    }

    #[test]
    fn firmware_paths_roundtrip_per_model_and_validate_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let o2 = dir.path().join("ip32prom.bin");
        let octane = dir.path().join("IP30prom.bin");
        fs::write(&o2, vec![0x5a; 512 * 1024]).unwrap();
        fs::write(&octane, vec![0x5a; 879_312]).unwrap();
        let mut settings = Settings::default();
        settings.machine.o2_prom = Some("ip32prom.bin".into());
        settings.machine.octane_prom = Some("IP30prom.bin".into());
        let cfg = dir.path().join("prismatic.cfg");
        settings.save(&cfg).unwrap();
        let saved_files = fs::read_dir(dir.path()).unwrap().count();
        let mut loaded = Settings::load(&cfg).unwrap().machine;
        for (profile, path) in [
            (MachineProfile::O2Ip32, &o2),
            (MachineProfile::OctaneIp30, &octane),
        ] {
            loaded.profile = profile;
            loaded.cpu = loaded.cpus()[0];
            loaded.validate_prom().unwrap();
            assert_eq!(loaded.prom_path(), path.to_str());
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), saved_files);
        for profile in [MachineProfile::O2Ip32, MachineProfile::OctaneIp30] {
            loaded.profile = profile;
            loaded.set_prom(None);
            loaded.validate_prom().unwrap();
            assert_eq!(loaded.prom_label(), "Bundled with Iris");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), saved_files);
        assert!(inspect_prom(MachineProfile::O2Ip32, &octane).is_err());
        for data in [vec![], vec![0; 512], vec![0; 1024 * 1024 + 1]] {
            fs::write(&octane, data).unwrap();
            assert!(inspect_prom(MachineProfile::OctaneIp30, &octane).is_err());
        }
        assert!(inspect_prom(MachineProfile::O2Ip32, dir.path()).is_err());
        assert!(inspect_prom(MachineProfile::O2Ip32, &dir.path().join("missing")).is_err());
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
