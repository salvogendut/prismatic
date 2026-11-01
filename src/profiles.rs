//! Named machine configurations; the desktop preferences remain shared.
use crate::{
    media::MediaSettings,
    settings::{AdvancedSettings, MachineSettings, Settings},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// Save bookkeeping is shared by drafts, but is not a user setting or dirty edit.
#[derive(Clone, Debug, Default)]
pub struct CatalogOrigin(Arc<Mutex<BTreeMap<String, NamedMachine>>>);
impl PartialEq for CatalogOrigin {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for CatalogOrigin {}

impl CatalogOrigin {
    pub fn record(&self, machines: &BTreeMap<String, NamedMachine>) {
        *self.0.lock().unwrap() = machines.clone();
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NamedMachine {
    /// The first label adopts the previously unnamed machine's firmware state.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub legacy_state: bool,
    pub machine: MachineSettings,
    pub media: MediaSettings,
    pub advanced: AdvancedSettings,
    pub network: crate::network::NetworkSettings,
}

pub fn validate_label(label: &str) -> Result<(), String> {
    if label.is_empty()
        || label.len() > 32
        || label.trim() != label
        || !label.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
    {
        return Err("Use 1-32 printable ASCII characters, without leading/trailing spaces".into());
    }
    Ok(())
}

impl Settings {
    /// Keep labels created/edited by another window, and reject overlapping edits.
    /// Caller holds the configuration's file lock through atomic publication.
    pub fn merge_machine_catalog(&mut self, path: &Path) -> Result<(), String> {
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        let disk = Self::load(path)?;
        let baseline = self.catalog_origin.0.lock().unwrap();
        let mut merged = disk.machines;
        for (label, machine) in &self.machines {
            if baseline.get(label) != Some(machine) {
                if merged.get(label) != baseline.get(label) && merged.get(label) != Some(machine) {
                    return Err(format!(
                        "Machine '{label}' changed in another window; reopen the configuration before saving"
                    ));
                }
                merged.insert(label.clone(), machine.clone());
            }
        }
        drop(baseline);
        self.machines = merged;
        self.validate_machines()?;
        if let Some(label) = self.machine_label.clone() {
            // Keep the legacy/current fields consistent with the authoritative
            // stanza, including an unchanged label updated in another window.
            self.select_machine(&label)?;
        }
        Ok(())
    }

    fn named_machine(&self, legacy_state: bool) -> NamedMachine {
        NamedMachine {
            legacy_state,
            machine: self.machine.clone(),
            media: self.media.clone(),
            advanced: self.advanced.clone(),
            network: self.network.clone(),
        }
    }

    pub fn save_as_machine(&mut self, label: &str) -> Result<(), String> {
        validate_label(label)?;
        if self.machines.contains_key(label) {
            return Err("That label already exists; select it to edit its options".into());
        }
        let machine = self.named_machine(self.machine_label.is_none());
        self.machines.insert(label.into(), machine);
        self.machine_label = Some(label.into());
        Ok(())
    }

    /// Update only the draft until the complete configuration is saved.
    pub fn remember_machine(&mut self) {
        if let Some(label) = self.machine_label.clone()
            && let Some(saved) = self.machines.get(&label)
        {
            self.machines
                .insert(label, self.named_machine(saved.legacy_state));
        }
    }

    pub fn select_machine(&mut self, label: &str) -> Result<(), String> {
        let saved = self
            .machines
            .get(label)
            .cloned()
            .ok_or_else(|| format!("Unknown machine label: {label}"))?;
        self.machine_label = Some(label.into());
        self.machine = saved.machine;
        self.media = saved.media;
        self.advanced = saved.advanced;
        self.network = saved.network;
        Ok(())
    }

    pub fn next_machine(&mut self) -> bool {
        if self.machines.is_empty() {
            return false;
        }
        self.remember_machine();
        let labels: Vec<_> = self.machines.keys().cloned().collect();
        let index = self
            .machine_label
            .as_ref()
            .and_then(|label| labels.iter().position(|l| l == label))
            .map_or(0, |i| (i + 1) % labels.len());
        self.select_machine(&labels[index]).unwrap();
        true
    }

    pub fn machine_state_key(&self) -> Option<&str> {
        self.machine_label.as_deref().filter(|label| {
            self.machines
                .get(*label)
                .is_some_and(|saved| !saved.legacy_state)
        })
    }

    pub fn machine_state_dir(&self, preferences: &Path, override_dir: Option<&Path>) -> PathBuf {
        if let Some(label) = self.machine_state_key() {
            // Full byte encoding is stable, portable and cannot alias another label
            // or escape the state directory, including labels containing slashes.
            let encoded: String = label.bytes().map(|b| format!("{b:02x}")).collect();
            let root = override_dir
                .unwrap_or(preferences)
                .join("machines")
                .join(encoded);
            self.machine.state_dir(&root, None)
        } else {
            self.machine.state_dir(preferences, override_dir)
        }
    }

    pub fn validate_machines(&self) -> Result<(), String> {
        for (label, machine) in &self.machines {
            validate_label(label)?;
            machine.machine.validate()?;
            machine.media.validate_for(machine.machine.profile)?;
            machine.advanced.validate()?;
            machine.advanced.extensions.validate(
                &machine.machine,
                &machine.media,
                &machine.network,
            )?;
            machine.network.validate(machine.machine.profile)?;
        }
        if let Some(label) = &self.machine_label
            && !self.machines.contains_key(label)
        {
            return Err(format!("Selected machine label does not exist: {label}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris::config::MachineProfile;

    #[test]
    fn a_shortcut_save_keeps_the_named_source_current_before_save_as() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings::default();
        settings.save_as_machine("Original").unwrap();
        settings.save(&path).unwrap();
        // --keyboard is a live override, and a window-scale shortcut persists it.
        settings.machine.keyboard = iris::config::KeyboardLayout::Iso;
        settings
            .save_window_scale(&path, settings.window_scale.step(true))
            .unwrap();
        settings.save_as_machine("Copy").unwrap();
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(
            loaded.machines["Original"].machine.keyboard,
            iris::config::KeyboardLayout::Iso
        );
    }

    #[test]
    fn relative_paths_do_not_look_like_concurrent_edits_on_a_second_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings::default();
        settings.media.devices.insert(
            1,
            crate::media::Medium {
                path: "disk.raw".into(),
                kind: crate::media::MediaKind::Disk,
            },
        );
        settings.save_as_machine("Relative").unwrap();
        settings.save(&path).unwrap();
        settings.machine.ram_mb = 128;
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.machine.ram_mb, 128);
        assert_eq!(
            loaded.media.devices[&1].path,
            dir.path().join("disk.raw").to_str().unwrap()
        );
    }

    #[test]
    fn labels_roundtrip_as_stanzas_without_overwriting_the_source_machine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings::default();
        settings.save_as_machine("My Indy").unwrap();
        settings.save(&path).unwrap();
        settings.machine.profile = MachineProfile::Indigo2Ip22;
        settings.machine.ram_mb = 128;
        settings.save_as_machine("Indigo2 IRIX").unwrap();
        settings.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[machines.\"My Indy\".machine]"));
        assert!(text.contains("[machines.\"Indigo2 IRIX\".machine]"));
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.machine_label.as_deref(), Some("Indigo2 IRIX"));
        assert_eq!(loaded.machine.profile, MachineProfile::Indigo2Ip22);
        let indigo_state = loaded.machine_state_dir(dir.path(), None);
        loaded.next_machine();
        assert_eq!(loaded.machine_label.as_deref(), Some("My Indy"));
        assert_eq!(loaded.machine.profile, MachineProfile::IndyIp24);
        assert_eq!(loaded.machine.ram_mb, 64);
        assert_eq!(
            loaded.machine_state_dir(dir.path(), None),
            dir.path().join("indy")
        );
        assert_ne!(loaded.machine_state_dir(dir.path(), None), indigo_state);
        loaded.machine.ram_mb = 256;
        loaded.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().machine.ram_mb, 256);
    }

    #[test]
    fn invalid_and_duplicate_labels_leave_the_draft_unchanged() {
        let mut settings = Settings::default();
        settings.save_as_machine("Indy").unwrap();
        let before = settings.clone();
        for label in [
            "",
            " Indy",
            "Indy",
            "bad\nlabel",
            "é",
            "012345678901234567890123456789012",
        ] {
            assert!(settings.save_as_machine(label).is_err());
            assert_eq!(settings, before);
        }
        settings.save_as_machine("../Indy").unwrap();
        let path = settings.machine_state_dir(Path::new("/tmp/state"), None);
        assert!(path.starts_with("/tmp/state/machines"));
        assert!(
            !path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        );
        assert!(settings.select_machine("missing").is_err());
    }

    #[test]
    fn switching_labels_requires_restart_even_if_state_was_manually_shared() {
        let mut active = Settings::default();
        active.save_as_machine("One").unwrap();
        active
            .machines
            .insert("Two".into(), active.machines["One"].clone());
        let mut draft = active.clone();
        draft.select_machine("Two").unwrap();
        assert_eq!(draft.machine_state_key(), active.machine_state_key());
        assert!(draft.machine_differs_from(&active));
    }

    #[test]
    fn separate_windows_preserve_other_labels_and_reject_conflicting_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut initial = Settings::default();
        initial.save_as_machine("Indy").unwrap();
        initial.save(&path).unwrap();
        let mut first = Settings::load(&path).unwrap();
        let mut second = Settings::load(&path).unwrap();
        first.save_as_machine("Scratch").unwrap();
        first.save(&path).unwrap();
        second.machine.ram_mb = 128;
        second.save(&path).unwrap();
        assert!(
            Settings::load(&path)
                .unwrap()
                .machines
                .contains_key("Scratch")
        );
        first.select_machine("Indy").unwrap();
        first.machine.ram_mb = 256;
        let before = std::fs::read(&path).unwrap();
        assert!(first.save(&path).unwrap_err().contains("another window"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn removing_the_selected_label_elsewhere_cannot_publish_an_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut settings = Settings::default();
        settings.save_as_machine("Indy").unwrap();
        settings.save(&path).unwrap();
        std::fs::write(&path, "scanlines = true\n").unwrap();
        assert!(
            settings
                .save(&path)
                .unwrap_err()
                .contains("Selected machine label does not exist")
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "scanlines = true\n"
        );
    }
}
