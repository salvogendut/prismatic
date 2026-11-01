//! Explicit storage operations. Configuration drafts never execute these actions.
use crate::{
    activity::Activity,
    media::{self, MediaKind, MediaSettings, Medium},
};
use iris::{
    block_node::Removable,
    scsi::{ScsiDevice, ScsiTarget},
    workstation::Workstation,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Scratch {
    pub slot: Option<u8>,
    pub size_mb: u32,
}
impl Default for Scratch {
    fn default() -> Self {
        Self {
            slot: None,
            size_mb: 64,
        }
    }
}
impl Scratch {
    pub fn path(&self, state: &Path) -> PathBuf {
        state.join(format!("scratch-{}.raw", self.size_mb))
    }
    pub fn cycle_slot(&mut self) {
        self.slot = match self.slot {
            None => Some(1),
            Some(7) => None,
            Some(slot) => Some(slot + 1),
        };
    }
    pub fn cycle_size(&mut self) {
        self.size_mb = match self.size_mb {
            64 => 256,
            256 => 1024,
            _ => 64,
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Info(u8),
    LoadCd(u8),
    NextCd(u8),
    EmptyCd(u8),
    QueueCd(u8),
    Commit(u8),
    Reset(u8),
    ImportScratch,
    ExportScratch,
}
impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Info(_) => "Storage status",
            Self::LoadCd(_) => "Insert CD in the live drive",
            Self::NextCd(_) => "Load the next CD in the changer",
            Self::EmptyCd(_) => "Empty the live CD tray",
            Self::QueueCd(_) => "Add CD to the saved changer list",
            Self::Commit(_) => "Commit disk changes to the base image",
            Self::Reset(_) => "Discard all uncommitted disk changes",
            Self::ImportScratch => "Replace scratch payload from a host file",
            Self::ExportScratch => "Export scratch payload to a new host file",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::Commit(_) => {
                "Permanent: merges all overlay writes into the base image. No guest may be loaded."
            }
            Self::Reset(_) => {
                "Permanent: discards all overlay writes, restoring the base. No guest may be loaded."
            }
            Self::ImportScratch => {
                "Overwrites scratch payload, keeping its volume header. Stop guest use of this raw device first."
            }
            Self::ExportScratch => {
                "Copies the entire payload, including trailing bytes. Stop guest use first; existing files are preserved."
            }
            _ => {
                "Unmount the CD in the guest first. This changes the live tray; saved boot media stays unchanged."
            }
        }
    }
    pub fn offline(self) -> bool {
        matches!(self, Self::Commit(_) | Self::Reset(_))
    }
    pub fn picks_file(self) -> bool {
        matches!(
            self,
            Self::LoadCd(_) | Self::QueueCd(_) | Self::ImportScratch | Self::ExportScratch
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub operation: Operation,
    pub path: Option<PathBuf>,
}
impl Action {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            path: None,
        }
    }
    fn path(&self) -> Result<&Path, String> {
        self.path.as_deref().ok_or("Choose a file first".into())
    }
}

pub struct Job(
    pub mpsc::Receiver<Result<String, String>>,
    Option<std::thread::JoinHandle<()>>,
);
impl Drop for Job {
    fn drop(&mut self) {
        // Normal completion already has a result. On a fatal UI error, keep
        // the process alive until an offline disk mutation finishes too.
        if let Some(worker) = self.1.take() {
            let _ = worker.join();
        }
    }
}
impl Job {
    pub fn live(receiver: mpsc::Receiver<Result<String, String>>) -> Self {
        Self(receiver, None)
    }
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self.0.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Operation stopped unexpectedly".into())),
        }
    }
    pub fn offline(action: Action, settings: MediaSettings) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("prismatic-storage".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| offline(&action, &settings))
                    .unwrap_or_else(|_| Err("Storage operation failed unexpectedly".into()));
                let _ = tx.try_send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self(rx, Some(worker)))
    }
}

pub fn offline(action: &Action, settings: &MediaSettings) -> Result<String, String> {
    let (slot, commit) = match action.operation {
        Operation::Commit(slot) => (slot, true),
        Operation::Reset(slot) => (slot, false),
        _ => return Err("This action needs a loaded machine".into()),
    };
    let medium = settings
        .devices
        .get(&slot)
        .filter(|m| m.kind == MediaKind::Disk)
        .ok_or("Select a hard disk")?;
    let _lease = crate::instance::Lease::disk(Path::new(&medium.path))?;
    let medium = Medium::inspect(Path::new(&medium.path), MediaKind::Disk)?;
    let mut node = media::open_node(&medium).map_err(|e| e.to_string())?;
    if !node.is_cow() {
        return Err("This backend has no copy-on-write overlay".into());
    }
    if commit {
        node.commit().map_err(|e| format!("Commit failed: {e}"))?;
    } else {
        node.rollback().map_err(|e| format!("Reset failed: {e}"))?;
    }
    node.flush().map_err(|e| e.to_string())?;
    Ok(if commit {
        "Disk overlay committed to base"
    } else {
        "Disk overlay discarded"
    }
    .into())
}

pub fn live(
    machine: &mut dyn Workstation,
    action: &Action,
    settings: &MediaSettings,
    state: &Path,
    frontend_paused: bool,
) -> Result<String, String> {
    match action.operation {
        Operation::ImportScratch | Operation::ExportScratch => {
            settings
                .scratch
                .slot
                .ok_or("Enable a scratch drive in Extensions and boot first")?;
            // The worker serializes this with lifecycle commands. Quiesce all
            // controllers before host I/O; preserve a paused/halted CPU state.
            let path = settings.scratch.path(state);
            if frontend_paused && !machine.cpu_is_running() {
                // Ctrl+P has already stopped every device. Do not restart
                // peripherals merely to perform a host-side transfer.
                transfer(&path, action)
            } else {
                machine.with_paused(|| transfer(&path, action))
            }
        }
        Operation::Info(slot)
        | Operation::LoadCd(slot)
        | Operation::NextCd(slot)
        | Operation::EmptyCd(slot) => {
            if matches!(action.operation, Operation::LoadCd(_)) {
                Medium::inspect(action.path()?, MediaKind::Cdrom)?;
            }
            media::with_bus(machine, slot / 16, |bus| {
                bus.with_targets(|targets| {
                    let dev = targets
                        .get_mut((slot % 16) as usize)
                        .and_then(Option::as_mut)
                        .ok_or("Drive is not attached")?;
                    if matches!(action.operation, Operation::Info(_)) {
                        let mut lines = dev.status((slot % 16) as usize);
                        if let Some(changer) = dev.changer() {
                            lines.push(format!("Tray: {}", changer.current().unwrap_or("empty")));
                            lines.extend(changer.discs().iter().enumerate()
                                .map(|(i, path)| format!("{i}: {path}")));
                        }
                        return Ok(lines.join("\n"));
                    }
                    let changer = dev.changer().ok_or("Select a CD-ROM drive")?;
                    match action.operation {
                        Operation::LoadCd(_) => {
                            let path = action.path()?.to_str().ok_or("Image paths must be UTF-8")?;
                            changer.load(path).map_err(|e| e.to_string())?;
                        }
                        Operation::NextCd(_) => {
                            if changer.discs().len() < 2 {
                                return Err("The changer needs at least two discs; add images and Save first".into());
                            }
                            changer.eject().map_err(|e| e.to_string())?;
                        }
                        Operation::EmptyCd(_) => changer.unload(),
                        _ => unreachable!(),
                    }
                    Ok(format!("SCSI {}:{} tray: {}", slot / 16, slot % 16,
                        changer.current().unwrap_or("empty")))
                })
            })?
        }
        _ => Err("This action is only available before boot or after power-off".into()),
    }
}

pub fn attach_scratch(
    machine: &mut dyn Workstation,
    settings: &MediaSettings,
    state: &Path,
    activity: &Activity,
) -> Result<Option<crate::instance::Lease>, String> {
    let Some(slot) = settings.scratch.slot else {
        return Ok(None);
    };
    let path = settings.scratch.path(state);
    if !path.try_exists().map_err(|e| e.to_string())? {
        // Build completely before publishing, so interrupted creation cannot
        // leave a half-formatted volume that a later launch treats as valid.
        let tmp = tempfile::NamedTempFile::new_in(state).map_err(|e| e.to_string())?;
        iris::sgi_vh::create_scratch_image(
            tmp.path(),
            u64::from(settings.scratch.size_mb) * 1024 * 1024,
        )
        .map_err(|e| e.to_string())?;
        tmp.persist_noclobber(&path).map_err(|e| e.to_string())?;
    }
    let lease = crate::instance::Lease::disk(&path)?;
    let capacity = validate_scratch(&path)?;
    if capacity + iris::sgi_vh::SCRATCH_PAYLOAD_OFFSET
        != u64::from(settings.scratch.size_mb) * 1024 * 1024
    {
        return Err("Scratch file size does not match its saved size".into());
    }
    let node = iris::block_node::RawNode::open(&path).map_err(|e| e.to_string())?;
    let device = ScsiDevice::new(
        Box::new(node),
        false,
        path.to_string_lossy().into_owned(),
        vec![],
        iris::scsi::DISK_IDENTITY,
    );
    media::with_bus(machine, slot / 16, |bus| {
        bus.attach(
            slot % 16,
            Box::new(ActivityTarget {
                device,
                slot,
                activity: activity.clone(),
            }),
        )
    })?
    .map_err(|e| e.to_string())?;
    Ok(Some(lease))
}

fn validate_scratch(path: &Path) -> Result<u64, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0; 512];
    f.read_exact(&mut header).map_err(|e| e.to_string())?;
    let vh = iris::sgi_vh::VolumeHeader::from_bytes(&header).map_err(|e| e.to_string())?;
    if !vh.csum_valid() {
        return Err("Scratch volume header checksum is invalid".into());
    }
    let length = f.metadata().map_err(|e| e.to_string())?.len();
    let capacity = length
        .checked_sub(iris::sgi_vh::SCRATCH_PAYLOAD_OFFSET)
        .ok_or("Scratch image is too small")?;
    if !capacity.is_multiple_of(512)
        || !vh.partitions().iter().any(|p| {
            p.slot == 0
                && u64::from(p.first_block) * 512 == iris::sgi_vh::SCRATCH_PAYLOAD_OFFSET
                && u64::from(p.nblks) * 512 == capacity
        })
    {
        return Err("Scratch payload partition was changed; refusing to overwrite it".into());
    }
    Ok(capacity)
}

fn transfer(scratch: &Path, action: &Action) -> Result<String, String> {
    let capacity = validate_scratch(scratch)?;
    let host = action.path()?;
    let count = match action.operation {
        Operation::ImportScratch => {
            let metadata = fs::metadata(host).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > capacity {
                return Err("Choose a regular file that fits the scratch payload".into());
            }
            if host.canonicalize().map_err(|e| e.to_string())?
                == scratch.canonicalize().map_err(|e| e.to_string())?
            {
                return Err("Cannot copy scratch onto itself".into());
            }
            let mut source = fs::File::open(host).map_err(|e| e.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .open(scratch)
                .map_err(|e| e.to_string())?;
            output
                .seek(SeekFrom::Start(iris::sgi_vh::SCRATCH_PAYLOAD_OFFSET))
                .map_err(|e| e.to_string())?;
            let n = std::io::copy(&mut Read::by_ref(&mut source).take(capacity), &mut output)
                .map_err(|e| e.to_string())?;
            // Pad the partial sector for raw dd readers; preserve the rest.
            let pad = (512 - n % 512) % 512;
            output
                .write_all(&vec![0; pad as usize])
                .map_err(|e| e.to_string())?;
            output.sync_all().map_err(|e| e.to_string())?;
            n
        }
        Operation::ExportScratch => {
            let parent = host.parent().unwrap_or(Path::new("."));
            let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            let mut source = fs::File::open(scratch).map_err(|e| e.to_string())?;
            source
                .seek(SeekFrom::Start(iris::sgi_vh::SCRATCH_PAYLOAD_OFFSET))
                .map_err(|e| e.to_string())?;
            let n = std::io::copy(&mut source.take(capacity), output.as_file_mut())
                .map_err(|e| e.to_string())?;
            output.as_file().sync_all().map_err(|e| e.to_string())?;
            output.persist_noclobber(host).map_err(|e| e.to_string())?;
            n
        }
        _ => return Err("Not a scratch transfer".into()),
    };
    Ok(format!("Scratch: transferred {count} bytes"))
}

/// Instrument the SCSI boundary so CD activity survives the changer replacing
/// its block node. Scratch uses this too; neither target has a COW layer.
pub struct ActivityTarget {
    pub device: ScsiDevice,
    pub slot: u8,
    pub activity: Activity,
}
impl ScsiTarget for ActivityTarget {
    fn request(
        &mut self,
        req: &iris::scsi::ScsiRequest,
    ) -> std::io::Result<iris::scsi::ScsiResponse> {
        let response = ScsiTarget::request(&mut self.device, req)?;
        if response.status == 0 {
            match req.cdb.first() {
                Some(0x08 | 0x28) => self.activity.signal(crate::activity::disk_read(self.slot)),
                Some(0x0a | 0x2a) => self.activity.signal(crate::activity::disk_write(self.slot)),
                _ => {}
            }
        }
        Ok(response)
    }
    fn power_on(&mut self) {
        self.device.power_on();
    }
    fn status(&self, id: usize) -> Vec<String> {
        self.device.status(id)
    }
    fn block_size(&self) -> Option<u64> {
        self.device.block_size()
    }
    fn reset_block_size(&mut self, cdrom_512: bool) {
        self.device.reset_block_size(cdrom_512);
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.device.flush()
    }
    fn changer(&mut self) -> Option<&mut iris::block_node::ChangerNode> {
        self.device.changer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_transfer_preserves_header_bounds_and_existing_exports() {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("scratch.raw");
        iris::sgi_vh::create_scratch_image(&scratch, 65536).unwrap();
        let header = fs::read(&scratch).unwrap()[..4096].to_vec();
        let source = dir.path().join("source.bin");
        fs::write(&source, vec![0x7a; 513]).unwrap();
        let import = Action {
            operation: Operation::ImportScratch,
            path: Some(source.clone()),
        };
        transfer(&scratch, &import).unwrap();
        let bytes = fs::read(&scratch).unwrap();
        assert_eq!(&bytes[..4096], header);
        assert_eq!(&bytes[4096..4609], vec![0x7a; 513]);
        assert_eq!(&bytes[4609..5120], vec![0; 511]);
        let output = dir.path().join("export.bin");
        let export = Action {
            operation: Operation::ExportScratch,
            path: Some(output.clone()),
        };
        transfer(&scratch, &export).unwrap();
        assert_eq!(fs::read(&output).unwrap(), bytes[4096..]);
        assert!(transfer(&scratch, &export).is_err());
        assert_eq!(fs::read(&output).unwrap(), bytes[4096..]);
        fs::write(&source, vec![1; 65536]).unwrap();
        assert!(transfer(&scratch, &import).is_err());
        assert_eq!(fs::read(&scratch).unwrap(), bytes);
        let mut vh = iris::sgi_vh::VolumeHeader::from_bytes(&header[..512]).unwrap();
        vh.set_partition(0, 10, 0, 3).unwrap();
        let mut f = fs::OpenOptions::new().write(true).open(&scratch).unwrap();
        f.write_all(vh.as_bytes()).unwrap();
        assert!(
            validate_scratch(&scratch)
                .unwrap_err()
                .contains("partition")
        );
    }

    #[test]
    fn offline_disk_actions_commit_reset_and_respect_other_instances() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("disk.raw");
        fs::write(&base, vec![0x22; 65536]).unwrap();
        let medium = Medium::inspect(&base, MediaKind::Disk).unwrap();
        let mut settings = MediaSettings::default();
        settings.devices.insert(17, medium.clone());
        let write_overlay = |byte| {
            let mut node = media::open_node(&medium).unwrap();
            node.write(0, &[byte; 512]).unwrap();
            node.flush().unwrap();
        };
        write_overlay(0x55);
        let commit = Action::new(Operation::Commit(17));
        let leases = crate::instance::disks(&settings).unwrap();
        assert!(offline(&commit, &settings).is_err());
        assert_eq!(fs::read(&base).unwrap()[0], 0x22);
        drop(leases);
        offline(&commit, &settings).unwrap();
        assert_eq!(fs::read(&base).unwrap()[0], 0x55);
        write_overlay(0x66);
        offline(&Action::new(Operation::Reset(17)), &settings).unwrap();
        let mut node = media::open_node(&medium).unwrap();
        let mut sector = [0; 512];
        node.read(0, &mut sector).unwrap();
        assert_eq!(sector, [0x55; 512]);
        assert_eq!(node.dirty_count(), 0);
    }

    #[test]
    fn changer_failed_load_preserves_disc_and_activity_survives_successful_swap() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("one.iso");
        let second = dir.path().join("two.efs");
        fs::write(&first, vec![0x11; 65536]).unwrap();
        fs::write(&second, vec![0x22; 65536]).unwrap();
        let medium = Medium::inspect(&first, MediaKind::Cdrom).unwrap();
        let activity = Activity::default();
        let mut target = ActivityTarget {
            device: ScsiDevice::new(
                media::open_node(&medium).unwrap(),
                true,
                medium.path.clone(),
                vec![second.to_str().unwrap().into()],
                iris::scsi::CDROM_IDENTITY,
            ),
            slot: 23,
            activity: activity.clone(),
        };
        let changer = target.changer().unwrap();
        assert!(
            changer
                .load(dir.path().join("missing.iso").to_str().unwrap())
                .is_err()
        );
        assert_eq!(changer.current(), Some(medium.path.as_str()));
        changer.eject().unwrap();
        let request = iris::scsi::ScsiRequest {
            cdb: vec![0x28, 0, 0, 0, 0, 0, 0, 0, 1, 0],
            data_len: iris::scsi::ScsiDataLength::Fixed(512),
            data_in: None,
        };
        let first = target.request(&request).unwrap(); // Unit attention, then data.
        let response = if first.status == 0 {
            first
        } else {
            target.request(&request).unwrap()
        };
        assert_eq!(response.status, 0);
        assert_eq!(response.data, vec![0x22; 512]);
        assert_eq!(activity.take(), crate::activity::disk_read(23));
        target.changer().unwrap().unload();
        assert!(target.changer().unwrap().current().is_none());
        assert_ne!(target.request(&request).unwrap().status, 0);
    }
}
