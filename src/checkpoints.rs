//! Conservative, self-contained native checkpoints with frontend provenance.
use crate::{
    media::{MediaKind, MediaSettings},
    network::NetworkSettings,
    settings::{AdvancedSettings, MachineSettings},
};
use iris::{config::MachineProfile, snapshot::DiskRef, workstation::Workstation};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    List,
    Save,
    Restore,
}
impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::List => "List checkpoints",
            Self::Save => "Save checkpoint?",
            Self::Restore => "Restore checkpoint?",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::List => "Checkpoints for this machine label.",
            Self::Save => {
                "Briefly stops the guest. Saves RAM and disk overlays to a new checkpoint."
            }
            Self::Restore => {
                "Replaces RAM and disk overlays. External files are not rolled back. Leaves PAUSED."
            }
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Context {
    hardware: MachineSettings,
    media: MediaSettings,
    advanced: AdvancedSettings,
    network: NetworkSettings,
}
impl Context {
    pub fn new(
        hardware: &MachineSettings,
        media: &MediaSettings,
        advanced: &AdvancedSettings,
        network: &NetworkSettings,
    ) -> Self {
        Self {
            hardware: hardware.clone(),
            media: media.clone(),
            advanced: advanced.clone(),
            network: network.clone(),
        }
    }
    pub fn set_network(&mut self, network: NetworkSettings) {
        self.network = network;
    }
    pub fn capability(&self) -> Result<(), String> {
        capability(&self.hardware, &self.media, &self.advanced)
    }
    fn provenance(&self) -> Result<Vec<DiskRef>, String> {
        self.media
            .devices
            .iter()
            .map(|(&id, disk)| {
                if crate::media::is_chd(Path::new(&disk.path)).map_err(|e| e.to_string())? {
                    return Err(
                        "CHD disk overlays are not captured by Iris checkpoints; use raw COW disks"
                            .into(),
                    );
                }
                Ok(DiskRef {
                    id,
                    path: disk.path.clone(),
                    size_bytes: fs::metadata(&disk.path).map_err(|e| e.to_string())?.len(),
                })
            })
            .collect()
    }
    fn signature(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }
}

pub fn capability(
    hardware: &MachineSettings,
    media: &MediaSettings,
    advanced: &AdvancedSettings,
) -> Result<(), String> {
    if !crate::compat::CHECKPOINTS {
        return Err("Paused checkpoints are unavailable in this Iris build".into());
    }
    if !matches!(
        hardware.profile,
        MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22
    ) || !hardware.newport()
        || hardware.heads != 1
    {
        return Err("Requires Indy/IP22 with one Newport head".into());
    }
    let e = &advanced.extensions;
    if e.vino != crate::extensions::Source::Off || e.daynaport.is_some() || e.ultra64 {
        return Err("Disable VINO, DaynaPort and Ultra64 for checkpoints".into());
    }
    if media.scratch.slot.is_some() || media.devices.values().any(|m| m.kind != MediaKind::Disk) {
        return Err("Only raw COW hard disks or diskless machines; detach CDs and scratch".into());
    }
    let expert = advanced.expert.effective();
    if expert.gdb_port.is_some() || expert.trace_on_boot {
        return Err("Disable GDB and boot tracing for checkpoints".into());
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Use 1-48 ASCII letters, digits, hyphens or underscores".into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    build: String,
    configuration: String,
    bases: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
}
fn hash(path: &Path) -> Result<String, String> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err(format!("Not a regular file: {}", path.display()));
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = [0u8; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&bytes[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
fn inventory(root: &Path) -> Result<BTreeMap<String, String>, String> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("Checkpoint contains a symbolic link".into());
            }
            if kind.is_dir() {
                walk(root, &path, files)?;
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .ok_or("Non UTF-8 checkpoint path")?;
                if name != "prismatic.toml" {
                    files.insert(name.into(), hash(&path)?);
                }
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files)?;
    Ok(files)
}
fn bases(context: &Context) -> Result<BTreeMap<String, String>, String> {
    context
        .media
        .devices
        .values()
        .map(|m| Ok((m.path.clone(), hash(Path::new(&m.path))?)))
        .collect()
}
fn verify(root: &Path, context: &Context) -> Result<(), String> {
    if !fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Checkpoint must be a directory, not a symlink".into());
    }
    let text = fs::read_to_string(root.join("prismatic.toml"))
        .map_err(|e| format!("Incomplete or non-Prismatic checkpoint: {e}"))?;
    let manifest: Manifest = toml::from_str(&text).map_err(|e| e.to_string())?;
    if manifest.version != 1 || manifest.configuration != context.signature()? {
        return Err("Checkpoint configuration differs from this machine".into());
    }
    if manifest.build != hash(&std::env::current_exe().map_err(|e| e.to_string())?)? {
        return Err("Checkpoint was made by a different Prismatic build".into());
    }
    if manifest.bases != bases(context)? {
        return Err("Base disk content changed since this checkpoint".into());
    }
    if manifest.files != inventory(root)? {
        return Err("Checkpoint files are missing, changed or incomplete".into());
    }
    if !manifest.files.contains_key("state/snapshot.toml") {
        return Err("Checkpoint has no native manifest".into());
    }
    Ok(())
}

/// The boolean signals a destructive native restore failure: never resume that machine.
pub fn execute(
    machine: &mut dyn Workstation,
    context: &Context,
    state: &Path,
    name: &str,
    operation: Operation,
    paused: &mut bool,
) -> (Result<String, String>, bool) {
    let root = state.join("checkpoints");
    if operation == Operation::List {
        let result = (|| {
            if !root.exists() {
                return Ok("No checkpoints saved for this machine label".into());
            }
            let mut names = Vec::new();
            for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if validate_name(&name).is_ok()
                    && entry.file_type().map_err(|e| e.to_string())?.is_dir()
                    && entry.path().join("prismatic.toml").is_file()
                {
                    names.push(name);
                }
            }
            names.sort();
            Ok(format!(
                "{}\n{}",
                root.display(),
                if names.is_empty() {
                    "No checkpoints".into()
                } else {
                    names.join("\n")
                }
            ))
        })();
        return (result, false);
    }
    let mut fatal = false;
    let result = (|| {
        #[cfg(feature = "perfetto")]
        if iris::perfetto::status() != "perfetto: idle" {
            return Err("Stop Perfetto tracing before checkpoint actions".into());
        }
        validate_name(name)?;
        context.capability()?;
        let disks = context.provenance()?;
        let destination = root.join(name);
        if operation == Operation::Restore {
            verify(&destination, context)?;
        } else if destination.exists() {
            return Err("Checkpoint already exists; choose a new name".into());
        }
        let running = machine.cpu_is_running();
        machine.stop();
        *paused = true;
        let result = (|| {
            let native = machine
                .as_any_mut()
                .downcast_mut::<iris::machine::Machine>()
                .ok_or("Unsupported checkpoint machine")?;
            if operation == Operation::Save {
                fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                let temporary = tempfile::Builder::new()
                    .prefix(".saving-")
                    .tempdir_in(&root)
                    .map_err(|e| e.to_string())?;
                crate::compat::snapshot_context(native, temporary.path().into(), disks.clone());
                crate::compat::save_snapshot(native, "state")?;
                let manifest = Manifest {
                    version: 1,
                    build: hash(&std::env::current_exe().map_err(|e| e.to_string())?)?,
                    configuration: context.signature()?,
                    bases: bases(context)?,
                    files: inventory(temporary.path())?,
                };
                fs::write(
                    temporary.path().join("prismatic.toml"),
                    toml::to_string(&manifest).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                fs::rename(temporary.path(), &destination).map_err(|e| e.to_string())?;
                Ok(format!("Saved checkpoint {name}"))
            } else {
                crate::compat::snapshot_context(native, destination.clone(), disks.clone());
                fatal = true;
                crate::compat::load_snapshot(native, "state")?;
                fatal = false;
                Ok(format!(
                    "Restored {name}. PAUSED - Ctrl+P resumes. Network connections and audio may need restarting."
                ))
            }
        })();
        // Never leave monitor save/load pointing into a committed or temporary package.
        if let Some(native) = machine
            .as_any_mut()
            .downcast_mut::<iris::machine::Machine>()
        {
            crate::compat::snapshot_context(native, state.join("monitor-saves"), disks);
        }
        if operation == Operation::Save && running {
            machine.start_guest()?;
            *paused = false;
        }
        result
    })();
    (result, fatal)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_and_unsupported_devices() {
        for name in ["", "..", "../guest", "/tmp/guest", "a/b"] {
            assert!(validate_name(name).is_err());
        }
        validate_name("netbsd-boot_1").unwrap();
        let mut context = Context::new(
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        assert_eq!(context.capability().is_ok(), crate::compat::CHECKPOINTS);
        if !crate::compat::TRACE {
            context.advanced.expert.trace_on_boot = true;
            assert_eq!(context.capability().is_ok(), crate::compat::CHECKPOINTS);
        }
        context.hardware.heads = 2;
        assert!(context.capability().is_err());
    }
    #[test]
    fn integrity_rejects_changed_files_build_config_and_same_size_base() {
        let dir = tempfile::tempdir().unwrap();
        let disk = dir.path().join("disk.raw");
        fs::write(&disk, b"AAAA").unwrap();
        let mut context = Context::new(
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        context.media.devices.insert(
            1,
            crate::media::Medium {
                kind: MediaKind::Disk,
                path: disk.to_str().unwrap().into(),
            },
        );
        let root = dir.path().join("checkpoint");
        fs::create_dir_all(root.join("state")).unwrap();
        fs::write(root.join("state/snapshot.toml"), "native").unwrap();
        let manifest = Manifest {
            version: 1,
            build: hash(&std::env::current_exe().unwrap()).unwrap(),
            configuration: context.signature().unwrap(),
            bases: bases(&context).unwrap(),
            files: inventory(&root).unwrap(),
        };
        fs::write(
            root.join("prismatic.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        verify(&root, &context).unwrap();
        fs::write(&disk, b"BBBB").unwrap();
        assert!(verify(&root, &context).unwrap_err().contains("Base disk"));
        fs::write(&disk, b"AAAA").unwrap();
        fs::write(root.join("state/snapshot.toml"), "changed").unwrap();
        assert!(verify(&root, &context).is_err());
        context.hardware.ram_mb = 128;
        assert!(
            verify(&root, &context)
                .unwrap_err()
                .contains("configuration")
        );
    }
}
