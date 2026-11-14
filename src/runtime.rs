//! Whitelisted runtime options, shared by the overlay and the Iris adapter.
//!
//! Preferences for an unavailable device stay in the label, but are never
//! passed to a core which cannot implement them. Drafts only contain data.
use crate::{
    media::{MediaKind, Medium},
    settings::MachineSettings,
};
use iris::config::{MachineConfig, MachineProfile};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    Platform,
    Render,
    Disks,
    Pci,
    Sharing,
    Ports,
    Sound,
}
impl Topic {
    pub const ALL: [Self; 7] = [
        Self::Platform,
        Self::Render,
        Self::Disks,
        Self::Pci,
        Self::Sharing,
        Self::Ports,
        Self::Sound,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Platform => "Platform",
            Self::Render => "Render",
            Self::Disks => "Disks",
            Self::Pci => "PCI",
            Self::Sharing => "9P",
            Self::Ports => "Ports",
            Self::Sound => "Audio",
        }
    }
    pub fn indices(self) -> usize {
        match self {
            Self::Disks => 127,
            Self::Pci => 5,
            Self::Ports => 31,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
}
impl Value {
    fn text(&self) -> String {
        match self {
            Self::Bool(v) => if *v { "On" } else { "Off" }.into(),
            Self::Int(v) => v.to_string(),
            Self::Text(v) => v.clone(),
        }
    }
    fn toml(&self) -> toml::Value {
        match self {
            Self::Bool(v) => (*v).into(),
            Self::Int(v) => (*v).into(),
            Self::Text(v) => v.clone().into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct RuntimeSettings(pub BTreeMap<String, Value>);

pub struct Field {
    pub key: String,
    pub label: &'static str,
    pub hint: &'static str,
    pub default: Value,
    choices: &'static [&'static str],
    range: (i64, i64),
    pub unavailable: Option<&'static str>,
}
impl Field {
    fn new(
        key: impl Into<String>,
        label: &'static str,
        hint: &'static str,
        default: Value,
    ) -> Self {
        Self {
            key: key.into(),
            label,
            hint,
            default,
            choices: &[],
            range: (0, i64::from(u32::MAX)),
            unavailable: None,
        }
    }
    fn flag(
        key: impl Into<String>,
        label: &'static str,
        hint: &'static str,
        default: bool,
    ) -> Self {
        Self::new(key, label, hint, Value::Bool(default))
    }
    fn number(
        key: impl Into<String>,
        label: &'static str,
        hint: &'static str,
        default: i64,
        min: i64,
        max: i64,
    ) -> Self {
        Self {
            range: (min, max),
            ..Self::new(key, label, hint, Value::Int(default))
        }
    }
    fn text(
        key: impl Into<String>,
        label: &'static str,
        hint: &'static str,
        default: &str,
    ) -> Self {
        Self::new(key, label, hint, Value::Text(default.into()))
    }
    fn choice(
        key: impl Into<String>,
        label: &'static str,
        hint: &'static str,
        choices: &'static [&'static str],
    ) -> Self {
        Self {
            choices,
            ..Self::text(key, label, hint, choices[0])
        }
    }
    fn gate(mut self, available: bool, reason: &'static str) -> Self {
        if !available {
            self.unavailable = Some(reason);
        }
        self
    }
    pub fn value(&self, settings: &RuntimeSettings, index: usize) -> String {
        if self.key == "_index" {
            return index.to_string();
        }
        let text = settings.0.get(&self.key).unwrap_or(&self.default).text();
        if text.is_empty() {
            "Default / disconnected".into()
        } else {
            text
        }
    }
    pub fn input(&self, settings: &RuntimeSettings, index: usize) -> String {
        if self.key == "_index" {
            index.to_string()
        } else {
            settings.0.get(&self.key).unwrap_or(&self.default).text()
        }
    }
    pub fn cycle(&self, settings: &mut RuntimeSettings) -> bool {
        let value = settings.0.get(&self.key).unwrap_or(&self.default);
        let next = if let Value::Bool(v) = value {
            Value::Bool(!v)
        } else if !self.choices.is_empty() {
            let pos = self
                .choices
                .iter()
                .position(|&s| s == value.text())
                .unwrap_or(0);
            Value::Text(self.choices[(pos + 1) % self.choices.len()].into())
        } else {
            return false;
        };
        settings.0.insert(self.key.clone(), next);
        true
    }
    pub fn set(
        &self,
        settings: &mut RuntimeSettings,
        index: &mut usize,
        text: &str,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.contains('\0') {
            return Err("NUL is not allowed".into());
        }
        if text.is_empty() {
            settings.0.remove(&self.key);
            return Ok(());
        }
        let value = match self.default {
            Value::Bool(_) => return Err("Use Enter to toggle this setting".into()),
            Value::Int(_) => {
                let n: i64 = text.parse().map_err(|_| "Enter a whole number")?;
                if !(self.range.0..=self.range.1).contains(&n) {
                    return Err(format!(
                        "{} must be {} through {}",
                        self.label, self.range.0, self.range.1
                    ));
                }
                Value::Int(n)
            }
            Value::Text(_) => Value::Text(text.into()),
        };
        if self.key == "_index" {
            if let Value::Int(n) = value {
                *index = n as usize;
            }
        } else {
            settings.0.insert(self.key.clone(), value);
        }
        Ok(())
    }
}

pub fn fields(topic: Topic, index: usize, hardware: &MachineSettings) -> Vec<Field> {
    use MachineProfile::*;
    let modern = cfg!(iris_modern_models) && cfg!(iris_expansion);
    let p = hardware.profile;
    let octane = p == OctaneIp30;
    let legacy = hardware.legacy_devices();
    let profile = format!("{p:?}");
    let tezro = profile == "TezroIp53";
    let fuel = profile == "FuelIp34";
    let bricks = matches!(profile.as_str(), "Origin3200Ip35" | "Onyx3000Ip35");
    let virtio = legacy || octane || tezro;
    let unavailable = "This device or setting is unavailable on the selected machine/core; saved preferences are retained.";
    let mut rows = match topic {
        Topic::Render => vec![
            Field::number("graphics.threads", "Raster workers", "CPU fragment workers; changes take effect at the next power-on.", 2, 1, 64),
            Field::choice("graphics.raster", "Raster processor", "WGPU requires a wgpu build and a supported graphics board. Newport remains CPU-only upstream.", &["cpu", "wgpu"]).gate(cfg!(feature="wgpu") && !hardware.newport() && !hardware.serial_only(), "WGPU raster needs a wgpu build and a supported non-Newport board."),
            Field::choice("graphics.gpu_backend", "GPU backend", "Select the host compute API used by the WGPU raster processor.", &["auto", "vulkan", "metal", "dx12", "gl"]).gate(cfg!(feature="wgpu"), "Build with the wgpu feature to select a GPU backend."),
            Field::choice("jitv2.compile", "JIT compilation", "Async uses background workers; sync compiles on one guest CPU and is unsuitable for multiprocessor machines.", &["async", "sync"]).gate(iris::build_features::JITV2 && hardware.processor_count() == 1, "Synchronous JIT compilation requires jitv2 and one guest CPU."),
            Field::number("graphics.tile", "VPro tile size", "Power of two, 8 through 256 pixels. Iris validates the board's requirements.", 32, 8, 256),
        ],
        Topic::Sound => vec![
            Field::flag("no_audio", "Disable audio output", "Keeps the guest audio device, but creates no host output.", false),
            Field::number("audio.prebuf_ms", "Prebuffer (ms)", "Guest audio queued before playback starts.", 20, 0, 2000),
            Field::text("audio.file", "WAV file (replaces playback)", "Optional WAV output path. Iris writes the guest audio to this file instead of SDL playback.", ""),
            Field::flag("tezro.audio", "Tezro audio card", "Fit the optional ICE1724 audio controller; restart required.", false).gate(tezro, unavailable),
            Field::number("audio.channels", "Tezro output channels", "Output channels for the Tezro audio controller (2, 4, 6 or 8).", 2, 2, 8).gate(tezro, unavailable),
        ],
        Topic::Sharing => vec![
            Field::flag("virtio_9p.enabled", "Virtio 9P sharing", "Requires a guest virtio-9p driver; stock IRIX does not supply one.", false),
            Field::text("virtio_9p.path", "Shared directory", "Host directory exposed to the guest. Flatpak must have access to it.", ""),
            Field::text("virtio_9p.tag", "Mount tag", "Guest-visible share name, up to 64 bytes.", "share"),
            Field::flag("virtio_9p.readonly", "Read-only share", "Refuse guest writes to this host directory.", true),
            Field::choice("virtio_9p.slot", "GIO slot", "Indy/Indigo2 only: avoid Ultra64, the second Newport head and virtio serial.", &["exp0", "exp1"]).gate(legacy, "PCI machines choose the virtio slot internally."),
            Field::choice("virtio_9p.owners", "File ownership", "Squash reports fixed IDs; passthrough maps real host ownership and may need chown privileges.", &["squash", "passthrough"]),
            Field::number("virtio_9p.uid", "Guest UID", "Default owner or unmapped user ID.", 0, 0, i64::from(u32::MAX)),
            Field::number("virtio_9p.gid", "Guest GID", "Default group or unmapped group ID.", 0, 0, i64::from(u32::MAX)),
            Field::text("virtio_9p.uid_map", "UID mappings", "Passthrough only: guest:host[:count], separated by commas.", ""),
            Field::text("virtio_9p.gid_map", "GID mappings", "Passthrough only: guest:host[:count], separated by commas.", ""),
        ].into_iter().map(|f| f.gate(virtio && cfg!(unix), unavailable)).collect(),
        Topic::Ports => vec![
            Field::flag("virtio_serial.enabled", "Virtio serial", "Requires a guest virtio-console driver. This is separate from the machine's firmware serial ports.", false),
            Field::choice("virtio_serial.slot", "GIO slot", "Indy/Indigo2 only: avoid sharing the slot with 9P, Ultra64 or a second Newport head.", &["exp1", "exp0"]).gate(legacy, "PCI machines choose the virtio slot internally."),
            Field::number("_ports", "Number of ports", "One through 31 ports; unused port preferences are retained.", 1, 1, 31),
            Field::number("_index", "Edit port number", "Zero-based port index (0 through 30); this selection is not a machine setting.", 0, 0, 30),
            Field::text(format!("virtio_serial.port.{index}.name"), "Guest port name", "Optional guest-visible port name, unique and up to 200 bytes.", ""),
            Field::flag(format!("virtio_serial.port.{index}.console"), "Guest console port", "Mark this virtio port as a console.", index == 0),
            Field::number(format!("virtio_serial.port.{index}.tcp_port"), "Loopback TCP port", "Raw TCP, not telnet. Set a distinct port for each machine; use a Unix socket instead by filling Socket path.", 8890 + index as i64, 1, 65535),
            Field::text(format!("virtio_serial.port.{index}.socket"), "Unix socket path", "Optional Unix socket, replacing TCP for this port. Requires a Unix host.", "").gate(cfg!(unix), "Unix sockets require a Unix host."),
        ].into_iter().map(|f| f.gate(virtio, unavailable)).collect(),
        Topic::Disks => {
            let prefix = if index == 0 { "nvme".into() } else { format!("fibre_channel.disk.{}", index - 1) };
            let available = if index == 0 { octane || p == O2Ip32 } else { octane || bricks };
            vec![
                Field::number("_index", "Disk: 0=NVMe, 1..126=FC", "0 selects NVMe; 1 through 126 select Fibre Channel loop IDs 0 through 125.", 0, 0, 126),
                Field::text(format!("{prefix}.path"), "Disk image (blank=disconnect)", "Existing raw or CHD hard disk. Select an XIO board in PCI before adding a card to an Octane.", "").gate(available, unavailable),
                Field::flag(format!("{prefix}.overlay"), "Protect image (COW)", "Writes go to an adjacent .overlay file. Disable only to write the selected base image directly.", true).gate(available, unavailable),
                Field::flag(format!("{prefix}.write_cache"), "Volatile write cache", "On: guest flushes and periodic sync persist writes. Off: each write is synced before completion.", true).gate(available, unavailable),
            ]
        },
        Topic::Pci => vec![
            Field::number("_index", "Edit XIO board", "Boards 0 through 4; populate in order. Clearing a board requires clearing later boards too.", 0, 0, 4),
            Field::choice(format!("xio_board.{index}.kind"), "Board type", "Octane PCI shoehorn or PCI expansion cage. Off disconnects this board.", &["off", "pci-shoehorn", "pci-cage"]),
            Field::number(format!("xio_board.{index}.widget"), "Crossbow widget (0=auto)", "Shoehorns use widgets 9-12; the PCI cage uses 13. Zero lets Iris assign a free widget.", 0, 0, 13),
            Field::number("nvme.board", "NVMe board", "Zero-based XIO board holding the NVMe adapter.", 0, 0, 4),
            Field::number("nvme.slot", "NVMe slot (0=auto)", "PCI device number on the selected board; zero selects the first free slot.", 0, 0, 3),
            Field::flag("nvme.bridge", "NVMe PCI bridge", "Place the card behind a PCI-to-PCI bridge.", false),
            Field::number("fibre_channel.board", "FC board", "Zero-based XIO board holding the Fibre Channel adapter.", 0, 0, 4),
            Field::number("fibre_channel.slot", "FC slot (0=auto)", "PCI device number on the selected board; zero selects the first free slot.", 0, 0, 3),
            Field::flag("fibre_channel.bridge", "FC PCI bridge", "Place the Fibre Channel adapter behind a PCI-to-PCI bridge.", false),
        ].into_iter().map(|f| f.gate(octane, unavailable)).collect(),
        Topic::Platform => {
            let section = if fuel { "fuel" } else if tezro { "tezro" } else if profile == "Onyx3000Ip35" { "onyx3000" } else { "origin3200" };
            let sn0_section = if profile == "Onyx2Ip27" { "onyx2" } else { "origin2000" };
            vec![
                Field::text(format!("{sn0_section}.io_prom"), "BaseIO PROM override", "Optional BaseIO firmware image for Origin 2000/Onyx2; blank selects the core default.", "").gate(matches!(profile.as_str(), "Origin2000Ip27" | "Onyx2Ip27"), unavailable),
                Field::number("machine.emu_id", "Emulated machine ID", "Per-machine emulation identifier; influences synthetic hardware identities.", 0, 0, i64::from(u32::MAX)),
                Field::choice(format!("{section}.console"), "Platform console", "Select the platform's main console for the console log; route its ports in Advanced > Connections.", if tezro { &["l1", "ioc4_a", "ioc4_b", "ioc4_c", "ioc4_d"] } else { &["l1", "ioc3_a", "ioc3_b"] }).gate(fuel || tezro || bricks, unavailable),
                Field::flag("fuel.ps2_input", "Fuel PS/2 input", "Fit and route the PS/2 keyboard and mouse.", true).gate(fuel, unavailable),
                Field::flag("fuel.usb_input", "Fuel USB input", "Plug USB HID keyboard and mouse into OHCI; routes host input to USB.", false).gate(fuel, unavailable),
                Field::text(format!("{section}.l1_revision"), "L1 firmware revision", "Optional major.minor.patch revision reported by the L1 controller.", "").gate(fuel || bricks, unavailable),
                Field::text(format!("{section}.brick_serial"), "Brick serial", "Optional synthetic brick identity, at most 37 bytes.", "").gate(fuel || bricks, unavailable),
                Field::flag("serial_log_overwrite", "Overwrite serial logs", "Start serial logs afresh at power-on instead of appending.", false),
                Field::text("serial_log_a", "Serial A log", "Optional transcript path for serial A when it is not the main console.", ""),
                Field::text("serial_log_b", "Serial B log", "Optional transcript path for serial B when it is not the main console.", ""),
                Field::text(format!("{section}.l1_log"), "L1 log", "Optional independent L1 transcript when an IOC channel is the selected console.", "").gate(fuel || tezro || bricks, unavailable),
                Field::flag("o2.mouse", "O2 PS/2 mouse", "Fit the O2's PS/2 mouse.", false).gate(p == O2Ip32, unavailable),
                Field::flag("o2.arcs_fpu", "O2 ARCS FPU", "Enable the O2 firmware floating-point implementation.", false).gate(p == O2Ip32, unavailable),
            ]
        }
    };
    if topic == Topic::Sound && !crate::compat::AUDIO {
        for row in &mut rows {
            row.unavailable = Some(
                "This core lacks the frontend audio output API; audio preferences are retained.",
            );
        }
    }
    if topic == Topic::Pci && p == O2Ip32 {
        rows[5].unavailable = None;
    }
    for row in &mut rows {
        if !modern {
            row.unavailable =
                Some("These runtime options require the newer Iris embedding/configuration API.");
        }
    }
    rows
}

fn set_path(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }
    if let Some(array) = root.as_array_mut() {
        let index: usize = path[0].parse().expect("schema array index");
        while array.len() <= index {
            array.push(toml::Value::Table(Default::default()));
        }
        set_path(&mut array[index], &path[1..], value);
    } else {
        let table = root.as_table_mut().expect("schema table");
        let child = table.entry(path[0]).or_insert_with(|| {
            if path[0] == "xio_board" || path[0] == "port" {
                toml::Value::Array(vec![])
            } else {
                toml::Value::Table(Default::default())
            }
        });
        set_path(child, &path[1..], value);
    }
}

impl RuntimeSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.0.is_empty() {
            return Ok(());
        }
        let mut pending: std::collections::BTreeSet<_> = self.0.keys().cloned().collect();
        for profile in MachineSettings::PROFILES {
            let hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            for topic in Topic::ALL {
                for index in 0..topic.indices() {
                    for field in fields(topic, index, &hardware) {
                        if field.key != "_index" && pending.remove(&field.key) {
                            field.validate(&self.0[&field.key])?;
                        }
                    }
                }
            }
            if pending.is_empty() {
                return Ok(());
            }
        }
        Err(format!(
            "Unknown runtime option(s): {}",
            pending.into_iter().collect::<Vec<_>>().join(", ")
        ))
    }
    fn text(&self, key: &str) -> &str {
        match self.0.get(key) {
            Some(Value::Text(v)) => v,
            _ => "",
        }
    }
    fn enabled(&self, key: &str) -> bool {
        self.0.get(key) == Some(&Value::Bool(true))
    }
    fn port_count(&self) -> usize {
        match self.0.get("_ports") {
            Some(Value::Int(v)) => (*v).clamp(1, 31) as usize,
            _ => 1,
        }
    }

    pub fn apply(
        &self,
        config: &mut MachineConfig,
        hardware: &MachineSettings,
    ) -> Result<(), String> {
        if !cfg!(iris_modern_models) || !cfg!(iris_expansion) {
            return Ok(());
        }
        let mut tree = toml::Value::try_from(&*config).map_err(|e| e.to_string())?;
        let mut seen = std::collections::BTreeSet::new();
        for topic in Topic::ALL {
            for index in 0..topic.indices() {
                for field in fields(topic, index, hardware) {
                    let key = &field.key;
                    if key.starts_with('_')
                        || field.unavailable.is_some()
                        || !seen.insert(key.clone())
                    {
                        continue;
                    }
                    if key.starts_with("nvme.") && self.text("nvme.path").is_empty() {
                        continue;
                    }
                    if key.starts_with("fibre_channel.")
                        && !self.0.keys().any(|k| {
                            k.starts_with("fibre_channel.disk.")
                                && k.ends_with(".path")
                                && !self.text(k).is_empty()
                        })
                    {
                        continue;
                    }
                    if key.starts_with("fibre_channel.disk.") {
                        let prefix = key.rsplit_once('.').unwrap().0;
                        if self.text(&format!("{prefix}.path")).is_empty() {
                            continue;
                        }
                    }
                    if key.starts_with("xio_board.") {
                        let prefix = key.rsplit_once('.').unwrap().0;
                        if matches!(self.text(&format!("{prefix}.kind")), "" | "off") {
                            continue;
                        }
                    }
                    if key.starts_with("virtio_serial.") && !self.enabled("virtio_serial.enabled") {
                        continue;
                    }
                    if key.starts_with("virtio_serial.port.") {
                        if index >= self.port_count() {
                            continue;
                        }
                        if key.ends_with("tcp_port")
                            && !self
                                .text(&format!("virtio_serial.port.{index}.socket"))
                                .is_empty()
                        {
                            continue;
                        }
                    }
                    if key.starts_with("virtio_9p.") && !self.enabled("virtio_9p.enabled") {
                        continue;
                    }
                    let value = self.0.get(key).unwrap_or(&field.default);
                    // Empty text means the core default, disconnected media, or
                    // an absent optional path. Zero widget/slot means auto.
                    if matches!(value, Value::Text(s) if s.is_empty()) {
                        continue;
                    }
                    if matches!(value, Value::Int(0))
                        && (key.ends_with(".widget") || key.ends_with(".slot"))
                    {
                        continue;
                    }
                    field.validate(value)?;
                    set_path(&mut tree, &key.split('.').collect::<Vec<_>>(), value.toml());
                }
            }
        }
        let encoded = toml::to_string(&tree).map_err(|e| e.to_string())?;
        let mut updated: MachineConfig =
            toml::from_str(&encoded).map_err(|e| format!("Runtime options: {e}"))?;
        // These embedding-only fields are deliberately skipped by core serde.
        #[cfg(iris_modern_models)]
        {
            updated.banks_set = config.banks_set;
            updated.jitv2_threads_set = config.jitv2_threads_set;
        }
        #[cfg(iris_serial)]
        {
            updated.serial_backends = config.serial_backends.clone();
        }
        #[cfg(iris_audio)]
        {
            updated.audio.output = config.audio.output.clone();
        }
        Self::validate_ports(&updated, hardware)?;
        *config = updated;
        Ok(())
    }

    pub fn resolve_paths(&mut self, directory: &Path) -> Result<(), String> {
        for (key, value) in &mut self.0 {
            if is_path(key)
                && let Value::Text(path) = value
                && !path.is_empty()
                && Path::new(path).is_relative()
            {
                *path = std::path::absolute(directory.join(&*path))
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Paths must be UTF-8")?
                    .into();
            }
        }
        Ok(())
    }

    /// Worker-only filesystem validation; never creates overlays or opens output
    /// files while the user is editing a draft.
    pub fn validate_files(
        config: &MachineConfig,
        media: &crate::media::MediaSettings,
    ) -> Result<(), String> {
        #[cfg(iris_expansion)]
        {
            let mut paths = std::collections::BTreeSet::new();
            for path in media
                .devices
                .values()
                .filter(|m| m.kind == MediaKind::Disk)
                .map(|m| m.path.as_str())
                .chain(Self::disks(config))
            {
                let medium = Medium::inspect(Path::new(path), MediaKind::Disk)?;
                if !paths.insert(medium.path) {
                    return Err("The same disk cannot be attached to multiple SCSI, NVMe or Fibre Channel devices".into());
                }
            }
            #[cfg(iris_modern_models)]
            for path in [
                config.audio.file.as_deref(),
                config.serial_log_a.as_deref().map(Path::new),
                config.serial_log_b.as_deref().map(Path::new),
                config.fuel.l1_log.as_deref().map(Path::new),
                config.tezro.l1_log.as_deref().map(Path::new),
                config.origin3200.l1_log.as_deref().map(Path::new),
                config.onyx3000.l1_log.as_deref().map(Path::new),
            ]
            .into_iter()
            .flatten()
            {
                if std::fs::metadata(path).is_ok_and(|m| !m.is_file()) {
                    return Err(format!("Output must be a regular file: {}", path.display()));
                }
                if !path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .is_dir()
                {
                    return Err(format!(
                        "Output directory does not exist: {}",
                        path.display()
                    ));
                }
            }
            #[cfg(iris_modern_models)]
            for path in [&config.origin2000.io_prom, &config.onyx2.io_prom]
                .into_iter()
                .flatten()
            {
                if !Path::new(path).is_file() {
                    return Err(format!(
                        "BaseIO PROM does not exist or is not a regular file: {path}"
                    ));
                }
            }
            if config.virtio_9p.enabled && !Path::new(&config.virtio_9p.path).is_dir() {
                return Err("9P shared directory does not exist or is not accessible".into());
            }
        }
        let _ = (config, media);
        Ok(())
    }

    fn validate_ports(config: &MachineConfig, _hardware: &MachineSettings) -> Result<(), String> {
        #[cfg(iris_expansion)]
        {
            let mut ports = std::collections::BTreeSet::new();
            for port in [
                config.monitor_port,
                config.serial_port_a,
                config.serial_port_b,
                config.gdb_port,
            ]
            .into_iter()
            .flatten()
            .filter(|p| *p != 0)
            {
                ports.insert(port);
            }
            let mut sockets = std::collections::BTreeSet::new();
            if config.virtio_serial.enabled {
                for (index, entry) in config.virtio_serial.ports.iter().enumerate() {
                    if let Some(port) = entry.tcp_port.filter(|p| *p != 0)
                        && !ports.insert(port)
                    {
                        return Err(format!(
                            "Virtio port {index}: TCP {port} is already assigned"
                        ));
                    }
                    if let Some(path) = &entry.socket
                        && !sockets.insert(path)
                    {
                        return Err(format!(
                            "Virtio port {index}: socket path is already assigned"
                        ));
                    }
                }
            }
        }
        let _ = config;
        Ok(())
    }

    pub fn prepare_connections(config: &MachineConfig) -> Result<(), String> {
        // Core-owned virtio listeners must not silently become disconnected
        // ports. Probe on the machine worker before construction, not on Save.
        #[cfg(iris_expansion)]
        if config.virtio_serial.enabled {
            let mut probes = vec![];
            for entry in &config.virtio_serial.ports {
                if let Some(port) = entry.tcp_port {
                    probes.push(
                        std::net::TcpListener::bind(("127.0.0.1", port))
                            .map_err(|e| format!("Virtio TCP {port}: {e}"))?,
                    );
                }
                #[cfg(unix)]
                if let Some(path) = &entry.socket {
                    use std::os::unix::{fs::FileTypeExt, net::UnixStream};
                    if let Ok(meta) = std::fs::symlink_metadata(path)
                        && (!meta.file_type().is_socket() || UnixStream::connect(path).is_ok())
                    {
                        return Err(format!("Virtio socket {path} is already in use"));
                    }
                    if !Path::new(path).parent().is_some_and(Path::is_dir) {
                        return Err(format!("Virtio socket directory does not exist: {path}"));
                    }
                }
            }
        }
        let _ = config;
        Ok(())
    }

    pub fn disks(config: &MachineConfig) -> Vec<&str> {
        #[cfg(iris_expansion)]
        {
            let mut paths = vec![];
            if !config.nvme.path.is_empty() {
                paths.push(config.nvme.path.as_str());
            }
            if let Some(fc) = &config.fibre_channel {
                paths.extend(fc.disks.values().map(|d| d.path.as_str()));
            }
            paths
        }
        #[cfg(not(iris_expansion))]
        {
            let _ = config;
            vec![]
        }
    }
}

fn is_path(key: &str) -> bool {
    key.ends_with(".io_prom")
        || key.ends_with(".path")
        || key.ends_with(".socket")
        || key.ends_with(".l1_log")
        || matches!(key, "audio.file" | "serial_log_a" | "serial_log_b")
}
impl Field {
    fn validate(&self, value: &Value) -> Result<(), String> {
        let valid = match (&self.default, value) {
            (Value::Bool(_), Value::Bool(_)) => true,
            (Value::Int(_), Value::Int(v)) => (self.range.0..=self.range.1).contains(v),
            (Value::Text(_), Value::Text(v)) => {
                !v.contains('\0') && (self.choices.is_empty() || self.choices.contains(&v.as_str()))
            }
            _ => false,
        };
        let valid = valid
            && match (self.key.as_str(), value) {
                ("graphics.tile", Value::Int(v)) => (*v as u64).is_power_of_two(),
                ("audio.channels", Value::Int(v)) => [2, 4, 6, 8].contains(v),
                _ => true,
            };
        if valid {
            Ok(())
        } else {
            Err(format!("Invalid runtime setting: {}", self.label))
        }
    }
}

#[cfg(all(test, iris_modern_models, iris_expansion))]
mod tests {
    use super::*;
    fn hardware(profile: MachineProfile) -> MachineSettings {
        let mut h = MachineSettings {
            profile,
            ..Default::default()
        };
        h.cpu = h.cpus()[0];
        h.ram_mb = h.ram_sizes()[0];
        h
    }
    fn config(h: &MachineSettings, runtime: RuntimeSettings) -> Result<MachineConfig, String> {
        let advanced = crate::settings::AdvancedSettings {
            runtime,
            ..Default::default()
        };
        crate::emulator::machine_config(
            Path::new("/tmp/prismatic-runtime-fixture"),
            h,
            &Default::default(),
            &advanced,
            &Default::default(),
        )
    }
    fn settings(values: &[(&str, Value)]) -> RuntimeSettings {
        RuntimeSettings(
            values
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn every_new_model_part_and_graphics_choice_validates_without_creating_state() {
        for profile in MachineSettings::PROFILES {
            let mut h = hardware(profile);
            for part in profile.cpus() {
                h.cpu = crate::machines::family(part);
                h.cpu_part = Some(part.name());
                h.cpus = h.cpu_counts()[0];
                for board in h.graphics_choices() {
                    h.graphics = Some(*board);
                    let c = config(&h, Default::default())
                        .unwrap_or_else(|e| panic!("{profile:?} {} {board:?}: {e}", part.name()));
                    assert_eq!(c.cpu_part().unwrap().name(), part.name());
                }
            }
        }
        assert!(!Path::new("/tmp/prismatic-runtime-fixture").exists());
    }

    #[test]
    fn expansion_disks_are_protected_and_model_switch_preserves_preferences() {
        let saved = settings(&[
            ("xio_board.0.kind", Value::Text("pci-cage".into())),
            ("nvme.path", Value::Text("nvme.raw".into())),
            ("fibre_channel.disk.125.path", Value::Text("fc.chd".into())),
        ]);
        let c = config(&hardware(MachineProfile::OctaneIp30), saved.clone()).unwrap();
        assert!(c.nvme.overlay);
        assert!(c.fibre_channel.unwrap().disks[&125].overlay);
        let c = config(&hardware(MachineProfile::IndyIp24), saved.clone()).unwrap();
        assert!(c.nvme.path.is_empty());
        assert!(c.fibre_channel.is_none());
        let encoded = toml::to_string(&saved).unwrap();
        assert_eq!(toml::from_str::<RuntimeSettings>(&encoded).unwrap(), saved);
        let c = config(&hardware(MachineProfile::Origin3200Ip35), saved).unwrap();
        assert!(c.nvme.path.is_empty());
        assert_eq!(c.fibre_channel.unwrap().disks.len(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn virtio_ports_and_shares_validate_conflicts_and_host_paths() {
        let mut saved = settings(&[
            ("virtio_serial.enabled", Value::Bool(true)),
            ("virtio_9p.enabled", Value::Bool(true)),
            ("virtio_9p.path", Value::Text("/missing/share".into())),
            ("_ports", Value::Int(2)),
            (
                "virtio_serial.port.1.socket",
                Value::Text("/tmp/second-port.sock".into()),
            ),
        ]);
        let h = hardware(MachineProfile::IndyIp24);
        let c = config(&h, saved.clone()).unwrap();
        assert!(c.virtio_9p.readonly);
        assert_eq!(c.virtio_serial.ports.len(), 2);
        assert!(c.virtio_serial.ports[1].tcp_port.is_none());
        assert!(RuntimeSettings::validate_files(&c, &Default::default()).is_err());
        saved
            .0
            .insert("virtio_9p.slot".into(), Value::Text("exp1".into()));
        assert!(config(&h, saved).unwrap_err().contains("both in slot"));
    }

    #[test]
    fn invalid_settings_and_board_gaps_are_rejected_without_writes() {
        for (key, value) in [
            ("graphics.tile", Value::Int(31)),
            ("audio.channels", Value::Int(3)),
            ("unknown", Value::Bool(true)),
            ("_ports", Value::Int(32)),
        ] {
            assert!(settings(&[(key, value)]).validate().is_err());
        }
        let saved = settings(&[("xio_board.1.kind", Value::Text("pci-cage".into()))]);
        assert!(config(&hardware(MachineProfile::OctaneIp30), saved).is_err());
    }

    #[test]
    fn selectors_are_drafts_only_and_paths_resolve_relative_to_config() {
        let mut saved = settings(&[("nvme.path", Value::Text("disk.raw".into()))]);
        let mut index = 0;
        fields(Topic::Disks, 0, &hardware(MachineProfile::OctaneIp30))[0]
            .set(&mut saved, &mut index, "126")
            .unwrap();
        assert_eq!(index, 126);
        assert!(!saved.0.contains_key("_index"));
        assert!(
            fields(Topic::Disks, 0, &hardware(MachineProfile::OctaneIp30))[0]
                .set(&mut saved, &mut index, "127")
                .is_err()
        );
        saved.resolve_paths(Path::new("/tmp")).unwrap();
        assert_eq!(saved.text("nvme.path"), "/tmp/disk.raw");
    }
}
