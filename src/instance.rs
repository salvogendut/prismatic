//! Keep independent windows from writing the same machine state or disk overlay.
use crate::media::{MediaKind, MediaSettings};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub struct Lease {
    _file: File,
}

impl Lease {
    pub fn disk(path: &Path) -> Result<Self, String> {
        let base = std::fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut name = base.into_os_string();
        name.push(".prismatic-lock");
        Self::acquire(Path::new(&name))
    }

    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("Cannot open instance lock {}: {e}", path.display()))?;
        file.try_lock().map_err(|e| {
            format!(
                "Machine or disk may already be in use: {}: {e}",
                path.display()
            )
        })?;
        // Keep the sidecar after unlocking: unlinking could let another process
        // lock a different inode while a waiter still holds the old one.
        Ok(Self { _file: file })
    }
}

pub fn disks(media: &MediaSettings) -> Result<Vec<Lease>, String> {
    media
        .devices
        .values()
        .filter(|medium| medium.kind == MediaKind::Disk)
        .map(|medium| Lease::disk(Path::new(&medium.path)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leases_exclude_another_instance_and_release_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machine.lock");
        let lease = Lease::acquire(&path).unwrap();
        assert!(Lease::acquire(&path).is_err());
        drop(lease);
        assert!(Lease::acquire(&path).is_ok());
    }
}
