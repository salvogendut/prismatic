//! Image selection, read-only validation and Iris storage adapters.
use crate::{
    activity::{self, Activity},
    settings::Settings,
};
use iris::{
    block_node::{BlockNode, BlockStatus, ChdCdNode, CowNode, RawNode, open_chd_cow},
    config::{MachineProfile, ScsiDeviceConfig, ScsiKind},
    scsi::{ScsiDevice, ScsiIdentity},
    workstation::Workstation,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Disk,
    Cdrom,
}
impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disk => "Hard disk",
            Self::Cdrom => "CD-ROM",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Medium {
    pub kind: MediaKind,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaSettings {
    pub devices: BTreeMap<u8, Medium>,
    pub cd_changers: BTreeMap<u8, Vec<String>>,
    pub scratch: crate::storage::Scratch,
}

fn slot_label(slot: u8) -> String {
    format!("SCSI {}:{}", slot / 16, slot % 16)
}

impl MediaSettings {
    pub fn validate(&self) -> Result<(), String> {
        if ![64, 256, 1024].contains(&self.scratch.size_mb) {
            return Err("Scratch size must be 64, 256 or 1024 MB".into());
        }
        if let Some(slot) = self.scratch.slot
            && (!(1..=7).contains(&slot) || self.devices.contains_key(&slot))
        {
            return Err("Scratch needs an unused controller-0 target from 1 through 7".into());
        }
        for (slot, paths) in &self.cd_changers {
            if self
                .devices
                .get(slot)
                .is_none_or(|m| m.kind != MediaKind::Cdrom)
            {
                return Err("Changer lists require a configured CD-ROM drive".into());
            }
            if paths.len() > 16 || paths.iter().any(|p| p.is_empty() || p.contains('\0')) {
                return Err("A changer may contain up to 16 valid queued image paths".into());
            }
        }
        for (&id, medium) in &self.devices {
            if id >= 32 || id % 16 == 0 {
                return Err(
                    "Media slots must name SCSI targets 1 through 15 on controller 0 or 1".into(),
                );
            }
            if medium.path.is_empty() || medium.path.contains('\0') {
                return Err(format!(
                    "{}: choose an image or disconnect the device",
                    slot_label(id)
                ));
            }
        }
        Ok(())
    }

    pub fn validate_for(&self, profile: MachineProfile) -> Result<(), String> {
        self.validate()?;
        for &slot in self.devices.keys() {
            if slot / 16 >= profile.scsi_controllers() || slot % 16 > profile.max_scsi_id() {
                return Err(format!(
                    "SCSI {}:{} is unavailable on {}; disconnect it in Media or choose another model",
                    slot / 16,
                    slot % 16,
                    profile.label()
                ));
            }
        }
        Ok(())
    }

    pub fn resolve_paths(&mut self, directory: &Path) -> Result<(), String> {
        for path in self.cd_changers.values_mut().flatten() {
            if Path::new(path).is_relative() {
                *path = std::path::absolute(directory.join(&*path))
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Image paths must be UTF-8")?
                    .to_owned();
            }
        }
        for medium in self.devices.values_mut() {
            if Path::new(&medium.path).is_relative() {
                medium.path = std::path::absolute(directory.join(&medium.path))
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Image paths must be UTF-8")?
                    .to_owned();
            }
        }
        Ok(())
    }

    /// Never opens a writable backend or creates overlays while editing a draft.
    pub fn validate_files(&mut self) -> Result<(), String> {
        self.validate()?;
        for paths in self.cd_changers.values_mut() {
            for path in paths {
                *path = Medium::inspect(Path::new(path), MediaKind::Cdrom)?.path;
            }
        }
        let mut disks = Vec::new();
        for (&id, medium) in &mut self.devices {
            *medium = Medium::inspect(Path::new(&medium.path), medium.kind)
                .map_err(|e| format!("{}: {e}", slot_label(id)))?;
            if medium.kind == MediaKind::Disk {
                if disks.contains(&medium.path) {
                    return Err("The same hard disk cannot occupy two SCSI targets".into());
                }
                disks.push(medium.path.clone());
            }
        }
        Ok(())
    }

    pub fn iris_devices(&self) -> BTreeMap<u8, ScsiDeviceConfig> {
        self.devices
            .iter()
            .map(|(&id, m)| {
                (
                    id,
                    ScsiDeviceConfig {
                        path: m.path.clone(),
                        kind_field: match m.kind {
                            MediaKind::Disk => ScsiKind::Disk,
                            MediaKind::Cdrom => ScsiKind::Cdrom,
                        },
                        overlay: m.kind == MediaKind::Disk,
                        discs: self.cd_changers.get(&id).cloned().unwrap_or_default(),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    pub fn row_value(&self, id: u8) -> String {
        if self.scratch.slot == Some(id) {
            return "Scratch transfer drive (Extensions)".into();
        }
        self.devices.get(&id).map_or_else(
            || "Not connected".into(),
            |m| {
                let name = Path::new(&m.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("{}: {}", m.kind.label(), name)
            },
        )
    }
}

fn is_chd(path: &Path) -> io::Result<bool> {
    let mut header = [0; 8];
    let count = File::open(path)?.read(&mut header)?;
    Ok((count == 8 && &header == b"MComprHD") || iris::chd_disk::is_chd(&path.to_string_lossy()))
}

impl Medium {
    pub fn inspect(path: &Path, kind: MediaKind) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let name = path.to_str().ok_or("Image paths must be UTF-8")?;
        let metadata = path.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Choose a regular image file".into());
        }
        let file = File::open(&path).map_err(|e| e.to_string())?;
        let mut sample = [0; 512];
        if is_chd(&path).map_err(|e| e.to_string())? {
            match kind {
                MediaKind::Disk => {
                    let chd = libchdman_rs::Chd::open(name, false, None)
                        .map_err(|e| format!("Cannot read CHD: {e:?}"))?;
                    let geometry = libchdman_rs::hd::read_geometry(&chd)
                        .map_err(|e| format!("Not a hard-disk CHD: {e:?}"))?;
                    if geometry.sector_bytes != 512
                        || chd.logical_bytes() < 512
                        || !chd.logical_bytes().is_multiple_of(512)
                    {
                        return Err("Hard-disk CHDs must use 512-byte sectors".into());
                    }
                    chd.read_bytes(0, &mut sample)
                        .map_err(|e| format!("Cannot read CHD data: {e:?}"))?;
                }
                MediaKind::Cdrom => {
                    let mut cd =
                        ChdCdNode::open(name).map_err(|e| format!("Unsupported CD CHD: {e}"))?;
                    cd.read(0, &mut sample).map_err(|e| e.to_string())?;
                }
            }
        } else {
            if metadata.len() < 512 || !metadata.len().is_multiple_of(512) {
                return Err(
                    "Raw disk/CD data must be nonempty and aligned to 512-byte sectors".into(),
                );
            }
            let mut file = file;
            file.read_exact(&mut sample).map_err(|e| e.to_string())?;
            if sample.starts_with(b"QFI\xfb")
                || sample.starts_with(b"PK\x03\x04")
                || sample.starts_with(&[0x1f, 0x8b])
            {
                return Err(
                    "Choose a raw image or CHD, not a compressed archive or QCOW image".into(),
                );
            }
            if kind == MediaKind::Cdrom
                && sample[..12] == [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]
            {
                return Err(
                    "Raw 2352-byte CD sectors need conversion to cooked data or CHD".into(),
                );
            }
        }
        Ok(Self {
            kind,
            path: name.to_owned(),
        })
    }
}

pub enum Completion {
    Action(crate::storage::Action),
    FolderPicked {
        folder: crate::network::Folder,
        path: String,
    },
    Interfaces(Vec<crate::network::Interface>),
    Picked {
        id: u8,
        medium: Medium,
    },
    PromPicked {
        profile: MachineProfile,
        path: String,
    },
    Validated {
        settings: Box<Settings>,
        confirmed: bool,
    },
    Cancelled,
}

#[derive(Clone, Copy)]
enum FileChoice {
    Media { id: u8, kind: MediaKind },
    Prom(MachineProfile),
}

#[derive(Default)]
pub struct MediaJobs {
    receiver: Option<mpsc::Receiver<Result<Completion, String>>>,
}
impl MediaJobs {
    /// The smoke harness supplies a temporary fixture instead of a native dialog.
    pub fn pick_path(&mut self, id: u8, kind: MediaKind, path: PathBuf) {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        spawn(sender, move || {
            Ok(Completion::Picked {
                id,
                medium: Medium::inspect(&path, kind)?,
            })
        });
    }
    pub fn pick_action(
        &mut self,
        window: &sdl3::video::Window,
        operation: crate::storage::Operation,
    ) -> Result<(), String> {
        use crate::storage::{Action, Operation};
        use sdl3::dialog::{
            DialogError, DialogFileFilter, show_open_file_dialog, show_save_file_dialog,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        let callback: sdl3::dialog::DialogCallback = Box::new(
            move |result: Result<Vec<PathBuf>, DialogError>, _| match result {
                Ok(paths) if paths.len() == 1 => {
                    let path = paths[0].clone();
                    spawn(sender.clone(), move || {
                        let path =
                            if matches!(operation, Operation::LoadCd(_) | Operation::QueueCd(_)) {
                                PathBuf::from(Medium::inspect(&path, MediaKind::Cdrom)?.path)
                            } else {
                                path
                            };
                        Ok(Completion::Action(Action {
                            operation,
                            path: Some(path),
                        }))
                    });
                }
                Err(DialogError::Canceled) => {
                    let _ = sender.try_send(Ok(Completion::Cancelled));
                }
                Err(error) => {
                    let _ = sender.try_send(Err(error.to_string()));
                }
                _ => {
                    let _ = sender.try_send(Err("Choose one file".into()));
                }
            },
        );
        let filters = [DialogFileFilter {
            name: "Files",
            pattern: "*",
        }];
        let result = if operation == Operation::ExportScratch {
            show_save_file_dialog(&filters, None::<&Path>, window, callback)
        } else {
            show_open_file_dialog(&filters, None::<&Path>, false, window, callback)
        };
        result.map_err(|e| {
            self.receiver = None;
            e.to_string()
        })
    }

    pub fn cancel(&mut self) {
        self.receiver = None;
    }
    pub fn poll(&mut self) -> Option<Result<Completion, String>> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err("Media operation stopped unexpectedly".into()),
        };
        self.receiver = None;
        Some(result)
    }
    pub fn validate(&mut self, mut settings: Settings, confirmed: bool, directory: &Path) {
        let directory = directory.to_owned();
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        spawn(sender, move || {
            settings.machine.validate()?;
            settings.media.validate_for(settings.machine.profile)?;
            settings
                .network
                .validate_runtime(settings.machine.profile)?;
            settings.network.resolve_paths(&directory)?;
            settings.network.validate_files()?;
            settings
                .advanced
                .expert
                .validate_runtime(settings.machine.profile)?;
            settings.advanced.expert.resolve_paths(&directory)?;
            settings.advanced.expert.validate_files()?;
            settings.machine.validate_prom()?;
            settings.media.validate_files()?;
            Ok(Completion::Validated {
                settings: Box::new(settings),
                confirmed,
            })
        });
    }
    pub fn list_interfaces(&mut self) {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        spawn(sender, || {
            crate::network::interfaces().map(Completion::Interfaces)
        });
    }
    pub fn pick_folder(
        &mut self,
        window: &sdl3::video::Window,
        folder: crate::network::Folder,
        current: Option<&str>,
    ) {
        use sdl3::dialog::{DialogError, show_open_folder_dialog};
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        show_open_folder_dialog(
            current.map(Path::new),
            false,
            window,
            Box::new(move |result, _| match result {
                Ok(paths) if paths.len() == 1 => {
                    let path = paths[0].clone();
                    spawn(sender.clone(), move || {
                        Ok(Completion::FolderPicked {
                            folder,
                            path: crate::network::inspect_folder(&path)?,
                        })
                    });
                }
                Err(DialogError::Canceled) => {
                    let _ = sender.try_send(Ok(Completion::Cancelled));
                }
                Err(error) => {
                    let _ = sender.try_send(Err(error.to_string()));
                }
                _ => {
                    let _ = sender.try_send(Err("Choose one folder".into()));
                }
            }),
        );
    }
    pub fn pick(
        &mut self,
        window: &sdl3::video::Window,
        id: u8,
        kind: MediaKind,
        current: Option<&str>,
    ) -> Result<(), String> {
        self.pick_file(window, current, FileChoice::Media { id, kind })
    }

    pub fn pick_prom(
        &mut self,
        window: &sdl3::video::Window,
        profile: MachineProfile,
        current: Option<&str>,
    ) -> Result<(), String> {
        self.pick_file(window, current, FileChoice::Prom(profile))
    }

    fn pick_file(
        &mut self,
        window: &sdl3::video::Window,
        current: Option<&str>,
        choice: FileChoice,
    ) -> Result<(), String> {
        use sdl3::dialog::{DialogError, DialogFileFilter, show_open_file_dialog};
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        let filters = [
            DialogFileFilter {
                name: if matches!(choice, FileChoice::Prom(_)) {
                    "PROM firmware"
                } else {
                    "Disk and CD images"
                },
                pattern: if matches!(choice, FileChoice::Prom(_)) {
                    "bin;rom;prom"
                } else {
                    "chd;img;raw;dsk;hdd;iso;efs;bin;image"
                },
            },
            DialogFileFilter {
                name: "All files",
                pattern: "*",
            },
        ];
        show_open_file_dialog(
            &filters,
            current.map(Path::new),
            false,
            window,
            Box::new(move |result, _| match result {
                Ok(paths) if paths.len() == 1 => {
                    let path = paths[0].clone();
                    spawn(sender.clone(), move || {
                        Ok(match choice {
                            FileChoice::Media { id, kind } => Completion::Picked {
                                id,
                                medium: Medium::inspect(&path, kind)?,
                            },
                            FileChoice::Prom(profile) => Completion::PromPicked {
                                profile,
                                path: crate::settings::inspect_prom(profile, &path)?,
                            },
                        })
                    });
                }
                Err(DialogError::Canceled) => {
                    let _ = sender.try_send(Ok(Completion::Cancelled));
                }
                Err(error) => {
                    let _ = sender.try_send(Err(error.to_string()));
                }
                _ => {
                    let _ = sender.try_send(Err("Choose one image file".into()));
                }
            }),
        )
        .map_err(|e| {
            self.receiver = None;
            e.to_string()
        })
    }
}

fn spawn(
    sender: mpsc::SyncSender<Result<Completion, String>>,
    work: impl FnOnce() -> Result<Completion, String> + Send + 'static,
) {
    let fallback = sender.clone();
    if let Err(error) = thread::Builder::new()
        .name("prismatic-media".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err("Media validation failed unexpectedly".into()));
            let _ = sender.try_send(result);
        })
    {
        let _ = fallback.try_send(Err(error.to_string()));
    }
}

/// Block-node adapter preserves Iris's backend behavior and records actual I/O.
/// Retained error slots let the worker report flush failures after device drop.
struct MediaNode {
    inner: Box<dyn BlockNode>,
    id: u8,
    activity: Activity,
    error: Arc<Mutex<Option<String>>>,
}
impl Drop for MediaNode {
    fn drop(&mut self) {
        if let Err(error) = self.inner.flush() {
            *self.error.lock().unwrap() = Some(format!(
                "{}: could not flush media: {error}",
                slot_label(self.id)
            ));
        }
    }
}
impl BlockNode for MediaNode {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read(&mut self, offset: u64, data: &mut [u8]) -> io::Result<()> {
        self.activity.signal(activity::disk_read(self.id));
        self.inner.read(offset, data)
    }
    fn write(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.activity.signal(activity::disk_write(self.id));
        self.inner.write(offset, data)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
    fn discard(&mut self, offset: u64, len: u64) -> io::Result<()> {
        self.inner.discard(offset, len)
    }
    fn block_status(&self, offset: u64, len: u64) -> io::Result<BlockStatus> {
        self.inner.block_status(offset, len)
    }
    fn backing(&self) -> Option<&dyn BlockNode> {
        self.inner.backing()
    }
    fn dirty_count(&self) -> usize {
        self.inner.dirty_count()
    }
    fn is_cow(&self) -> bool {
        self.inner.is_cow()
    }
    fn commit(&mut self) -> io::Result<usize> {
        self.inner.commit()
    }
    fn rollback(&mut self) -> io::Result<()> {
        self.inner.rollback()
    }
    fn overlay_paths(&self) -> Option<(PathBuf, PathBuf)> {
        self.inner.overlay_paths()
    }
    fn pending_sync(&self) -> Option<(PathBuf, PathBuf)> {
        self.inner.pending_sync()
    }
    fn export_overlay(&mut self, dest: &Path) -> io::Result<Vec<u64>> {
        self.inner.export_overlay(dest)
    }
    fn import_overlay(&mut self, source: &Path, dirty: Vec<u64>) -> io::Result<()> {
        self.inner.import_overlay(source, dirty)
    }
}

#[derive(Default)]
pub struct MediaErrors(Vec<Arc<Mutex<Option<String>>>>);
impl MediaErrors {
    pub fn check(&self) -> Result<(), String> {
        let errors: Vec<_> = self
            .0
            .iter()
            .filter_map(|e| e.lock().unwrap().clone())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

pub fn attach(
    machine: &mut dyn Workstation,
    settings: &MediaSettings,
    activity: &Activity,
    errors: &mut MediaErrors,
) -> Result<(), String> {
    let mut validated = settings.clone();
    validated.validate_files()?;
    for (&id, medium) in &validated.devices {
        let node =
            open_node(medium).map_err(|e| format!("{}: {}: {e}", slot_label(id), medium.path))?;
        let error = Arc::new(Mutex::new(None));
        errors.0.push(error.clone());
        let node = MediaNode {
            inner: node,
            id,
            activity: activity.clone(),
            error,
        };
        let cd = medium.kind == MediaKind::Cdrom;
        let identity = if cd {
            ScsiIdentity::new(b"SONY    ", b"CD-ROM CDU-76S  ", b"1.0 ")
        } else {
            ScsiIdentity::new(b"SGI     ", b"PRISMATIC DISK  ", b"1.0 ")
        };
        let device = ScsiDevice::new(
            Box::new(node),
            cd,
            medium.path.clone(),
            settings.cd_changers.get(&id).cloned().unwrap_or_default(),
            identity,
        );
        let device: Box<dyn iris::scsi::ScsiTarget> = if cd {
            Box::new(crate::storage::ActivityTarget {
                device,
                slot: id,
                activity: activity.clone(),
            })
        } else {
            Box::new(device)
        };
        with_bus(machine, id / 16, |bus| bus.attach(id % 16, device))?
            .map_err(|e| format!("{}: {e}", slot_label(id)))?;
    }
    Ok(())
}

pub(crate) fn open_node(medium: &Medium) -> io::Result<Box<dyn BlockNode>> {
    let path = Path::new(&medium.path);
    if is_chd(path)? {
        return match medium.kind {
            MediaKind::Disk => open_chd_cow(&medium.path),
            MediaKind::Cdrom => Ok(Box::new(ChdCdNode::open(&medium.path)?)),
        };
    }
    let raw = RawNode::open_readonly(path)?;
    if medium.kind == MediaKind::Cdrom {
        return Ok(Box::new(raw));
    }
    let size = raw.size();
    Ok(Box::new(CowNode::new(
        Some(Box::new(raw)),
        format!("{}.overlay", medium.path),
        size,
    )?))
}

/// Called only after the CPU and device threads stop, before machine teardown.
pub fn detach(machine: &mut dyn Workstation) {
    for controller in 0..2 {
        let _ = with_bus(machine, controller, |bus| {
            bus.with_targets(|targets| {
                for target in targets {
                    *target = None;
                }
            });
        });
    }
}

/// Iris exposes a common target bus through WD33C93A, AIC7880 and ISP1040 controllers.
pub(crate) fn with_bus<R>(
    machine: &mut dyn Workstation,
    controller: u8,
    f: impl FnOnce(&iris::scsi::ScsiBus) -> R,
) -> Result<R, String> {
    let machine = machine.as_any_mut();
    if let Some(machine) = machine.downcast_ref::<iris::machine::Machine>() {
        let scsi = if controller == 0 {
            Some(machine.hpc3().scsi())
        } else {
            machine.hpc3().scsi1()
        }
        .ok_or("Iris did not create this SCSI controller")?;
        Ok(f(scsi.bus()))
    } else if let Some(machine) = machine.downcast_ref::<iris::ip32_machine::O2Machine>() {
        let controller = machine
            .bus
            .scsi
            .lock()
            .unwrap()
            .get(controller as usize)
            .cloned()
            .ok_or("Iris did not create the O2 SCSI controller")?;
        Ok(f(controller.bus()))
    } else if let Some(machine) = machine.downcast_ref::<iris::ip30_machine::OctaneMachine>() {
        Ok(f(machine
            .scsi
            .get(controller as usize)
            .ok_or("Iris did not create this SCSI controller")?
            .bus()))
    } else {
        Err("Unsupported Iris SCSI controller".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn raw(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, vec![0x5a; 65536]).unwrap();
        path
    }

    #[test]
    fn changer_and_scratch_validation_persistence_and_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let cd = raw(dir.path(), "disc.iso");
        let next = raw(dir.path(), "next.efs");
        let mut settings = Settings::default();
        settings
            .media
            .devices
            .insert(6, Medium::inspect(&cd, MediaKind::Cdrom).unwrap());
        settings
            .media
            .cd_changers
            .insert(6, vec![next.to_string_lossy().into_owned()]);
        settings.media.scratch.slot = Some(2);
        settings.save_as_machine("Transfer").unwrap();
        let path = dir.path().join("prismatic.cfg");
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().media, settings.media);
        settings.media.validate_files().unwrap();
        assert!(!settings.media.scratch.path(dir.path()).exists());
        assert_eq!(settings.media.iris_devices()[&6].discs.len(), 1);
        settings.media.scratch.slot = Some(6);
        assert!(settings.media.validate().is_err());
        settings.media.scratch.slot = None;
        settings.media.devices.get_mut(&6).unwrap().kind = MediaKind::Disk;
        assert!(settings.media.validate().is_err());
    }

    #[test]
    fn slot_validation_is_model_specific_and_maps_both_buses() {
        for profile in crate::settings::MachineSettings::PROFILES {
            for slot in 0..=32 {
                let mut media = MediaSettings::default();
                media.devices.insert(
                    slot,
                    Medium {
                        kind: MediaKind::Disk,
                        path: "disk.raw".into(),
                    },
                );
                let valid = slot / 16 < profile.scsi_controllers()
                    && (1..=profile.max_scsi_id()).contains(&(slot % 16));
                assert_eq!(
                    media.validate_for(profile).is_ok(),
                    valid,
                    "{profile:?}, slot {slot}"
                );
                if valid {
                    assert_eq!(
                        media.iris_devices()[&slot].placement(slot),
                        (slot / 16, slot % 16)
                    );
                }
            }
        }
    }

    #[test]
    fn duplicate_disks_across_controllers_are_rejected_without_creating_overlays() {
        let dir = tempfile::tempdir().unwrap();
        let path = raw(dir.path(), "disk.raw");
        let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
        let mut settings = MediaSettings::default();
        settings.devices.insert(1, medium.clone());
        settings.devices.insert(17, medium);
        assert!(
            settings
                .validate_files()
                .unwrap_err()
                .contains("same hard disk")
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn drafts_validate_without_creating_overlays_and_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let image = raw(dir.path(), "disk.img");
        let mut settings = Settings::default();
        settings.media.devices.insert(
            7,
            Medium {
                kind: MediaKind::Disk,
                path: "disk.img".into(),
            },
        );
        let config = dir.path().join("settings.toml");
        settings.save(&config).unwrap();
        let saved_files = fs::read_dir(dir.path()).unwrap().count();
        let mut loaded = Settings::load(&config).unwrap();
        loaded.media.validate_files().unwrap();
        assert_eq!(loaded.media.devices[&7].path, image.to_str().unwrap());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), saved_files);
        loaded
            .media
            .devices
            .insert(1, loaded.media.devices[&7].clone());
        assert!(
            loaded
                .media
                .validate_files()
                .unwrap_err()
                .contains("same hard disk")
        );
        loaded
            .media
            .devices
            .insert(0, loaded.media.devices[&7].clone());
        assert!(loaded.media.validate().is_err());
    }

    #[test]
    fn bad_media_is_rejected_before_attach() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Medium::inspect(dir.path(), MediaKind::Disk).is_err());
        let path = dir.path().join("bad.img");
        assert!(Medium::inspect(&path, MediaKind::Disk).is_err());
        for bytes in [
            vec![],
            vec![0; 513],
            [b"QFI\xfb".as_slice(), &[0; 508]].concat(),
            [b"MComprHD".as_slice(), &[0; 504]].concat(),
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(Medium::inspect(&path, MediaKind::Disk).is_err());
        }
    }

    fn write_and_reopen(medium: &Medium) {
        let original = fs::read(&medium.path).unwrap();
        let mut node = open_node(medium).unwrap();
        let mut data = [0; 512];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        node.write(512, &[0x23; 512]).unwrap();
        node.flush().unwrap();
        drop(node);
        let mut node = open_node(medium).unwrap();
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x23; 512]);
        node.read(1024, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        assert_eq!(fs::read(&medium.path).unwrap(), original);
    }

    #[test]
    fn raw_disk_writes_persist_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = raw(dir.path(), "disk.raw");
        write_and_reopen(&Medium::inspect(&path, MediaKind::Disk).unwrap());
        assert!(dir.path().join("disk.raw.overlay").is_file());
    }

    #[test]
    fn compressed_chd_disk_writes_persist_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        libchdman_rs::hd::create_from_reader(
            &[0x5a; 65536][..],
            &path,
            libchdman_rs::hd::HdCreateOptions {
                logical_size: 65536,
                ..Default::default()
            },
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(Medium::inspect(&path, MediaKind::Cdrom).is_err());
        write_and_reopen(&medium);
    }

    fn hashless_chd(path: &Path) {
        let mut chd =
            libchdman_rs::Chd::create(path.to_str().unwrap(), 65536, 4096, 512, [0; 4]).unwrap();
        chd.write_metadata(
            u32::from_be_bytes(*b"GDDD"),
            0,
            b"CYLS:1,HEADS:1,SECS:128,BPS:512\0",
            1,
        )
        .unwrap();
        chd.write_bytes(0, &[0x5a; 65536]).unwrap();
        assert_eq!(chd.sha1(), [0; 20]);
    }

    #[test]
    fn hashless_chd_writes_persist_and_rollback_without_changing_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        hashless_chd(&path);
        let original = fs::read(&path).unwrap();
        let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        write_and_reopen(&medium);
        let mut node = open_node(&medium).unwrap();
        assert!(node.is_cow());
        assert!(node.pending_sync().is_none());
        assert_eq!(node.dirty_count(), 1);
        assert_eq!(
            node.commit().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        node.rollback().unwrap();
        node.flush().unwrap();
        drop(node);
        let mut node = open_node(&medium).unwrap();
        let mut data = [0; 512];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 512]);
        assert_eq!(node.dirty_count(), 0);
        assert!(node.read(65536, &mut data).is_err());
        assert!(node.write(65536, &data).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn hashless_chd_recovers_after_legacy_empty_diff_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.chd");
        hashless_chd(&path);
        let diff = iris::chd_disk::diff_path_for(&path);
        assert!(libchdman_rs::hd::HdImage::open_with_diff(&path, &diff).is_err());
        let original_diff = fs::read(&diff).unwrap();
        write_and_reopen(&Medium::inspect(&path, MediaKind::Disk).unwrap());
        assert_eq!(fs::read(diff).unwrap(), original_diff);
    }

    #[test]
    fn hashless_chd_never_bypasses_damaged_or_written_diffs() {
        for damaged in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("disk.chd");
            hashless_chd(&path);
            let diff = iris::chd_disk::diff_path_for(&path);
            assert!(libchdman_rs::hd::HdImage::open_with_diff(&path, &diff).is_err());
            if damaged {
                fs::write(&diff, b"damaged CHD").unwrap();
            } else {
                // Allocated zeroes still represent guest data, not an empty diff.
                let mut child =
                    libchdman_rs::Chd::open(diff.to_str().unwrap(), true, None).unwrap();
                child.write_bytes(512, &[0x7f; 512]).unwrap();
                child.write_bytes(512, &[0; 512]).unwrap();
            }
            let before = fs::read(&diff).unwrap();
            let medium = Medium::inspect(&path, MediaKind::Disk).unwrap();
            let error = open_node(&medium).err().unwrap();
            assert!(error.to_string().contains("preserve it for recovery"));
            assert_eq!(fs::read(diff).unwrap(), before);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn raw_cd_data_is_filesystem_agnostic_and_readonly() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["iso9660.iso", "efs.img"] {
            let path = raw(dir.path(), name);
            let medium = Medium::inspect(&path, MediaKind::Cdrom).unwrap();
            let mut node = open_node(&medium).unwrap();
            let mut data = [0; 2048];
            node.read(2048, &mut data).unwrap();
            assert_eq!(data, [0x5a; 2048]);
            assert!(node.write(0, &[0x23; 512]).is_err());
            assert_eq!(fs::read(path).unwrap(), vec![0x5a; 65536]);
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn mode1_cd_chd_reads_cooked_sectors_and_rejects_disk_selection() {
        let dir = tempfile::tempdir().unwrap();
        let iso = raw(dir.path(), "data.iso");
        let chd = dir.path().join("data.chd");
        libchdman_rs::cd::create_from_iso(&iso, &chd, Default::default(), &mut |_| {}, &|| false)
            .unwrap();
        let medium = Medium::inspect(&chd, MediaKind::Cdrom).unwrap();
        assert!(Medium::inspect(&chd, MediaKind::Disk).is_err());
        let mut node = open_node(&medium).unwrap();
        let mut data = [0; 2048];
        node.read(512, &mut data).unwrap();
        assert_eq!(data, [0x5a; 2048]);
        assert!(node.write(0, &[0; 512]).is_err());
    }

    struct FailingFlush;
    impl BlockNode for FailingFlush {
        fn size(&self) -> u64 {
            512
        }
        fn read(&mut self, _: u64, _: &mut [u8]) -> io::Result<()> {
            Ok(())
        }
        fn write(&mut self, _: u64, _: &[u8]) -> io::Result<()> {
            Ok(())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("disk full"))
        }
        fn discard(&mut self, _: u64, _: u64) -> io::Result<()> {
            Ok(())
        }
        fn block_status(&self, _: u64, _: u64) -> io::Result<BlockStatus> {
            Err(io::Error::other("unused"))
        }
        fn backing(&self) -> Option<&dyn BlockNode> {
            None
        }
    }

    #[test]
    fn device_io_lights_leds_and_drop_retains_flush_errors() {
        let activity = Activity::default();
        let error = Arc::new(Mutex::new(None));
        let errors = MediaErrors(vec![error.clone()]);
        let mut node = MediaNode {
            inner: Box::new(FailingFlush),
            id: 7,
            activity: activity.clone(),
            error,
        };
        assert_eq!(activity.take(), 0);
        node.read(0, &mut [0; 512]).unwrap();
        assert_eq!(activity.take(), activity::disk_read(7));
        node.write(0, &[0; 512]).unwrap();
        assert_eq!(activity.take(), activity::disk_write(7));
        drop(node);
        assert!(
            errors
                .check()
                .unwrap_err()
                .contains("SCSI 0:7: could not flush media: disk full")
        );
    }

    #[test]
    fn save_validation_rechecks_firmware_and_creates_no_state_or_overlays() {
        let dir = tempfile::tempdir().unwrap();
        let prom = dir.path().join("prom.bin");
        fs::write(&prom, vec![0; 512 * 1024]).unwrap();
        let disk = raw(dir.path(), "disk.raw");
        let mut settings = Settings::default();
        settings.machine.profile = MachineProfile::O2Ip32;
        settings.machine.cpu = iris::config::CpuModel::R5000;
        settings.machine.set_prom(Some(
            crate::settings::inspect_prom(MachineProfile::O2Ip32, &prom).unwrap(),
        ));
        settings
            .media
            .devices
            .insert(1, Medium::inspect(&disk, MediaKind::Disk).unwrap());
        fs::remove_file(prom).unwrap(); // Disappears after picking, before Save.
        let mut jobs = MediaJobs::default();
        jobs.validate(settings, false, Path::new("."));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let result = loop {
            if let Some(result) = jobs.poll() {
                break result;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(matches!(result, Err(error) if error.contains("Cannot open PROM")));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn cancellation_ignores_late_picker_results() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut jobs = MediaJobs {
            receiver: Some(receiver),
        };
        jobs.cancel();
        assert!(sender.try_send(Ok(Completion::Cancelled)).is_err());
        assert!(jobs.poll().is_none());
    }
}
