use crate::media::{MediaKind, Medium};
use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Options,
    Escape,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Media,
    Advanced,
    About,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Media => "Media",
            Self::Advanced => "Advanced",
            Self::About => "About",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Unsaved { choice: usize }, // Save, Discard, Cancel
    Quit { confirm: bool },
    Restart { confirm: bool },
    Media { id: u8, choice: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Save(Settings),
    Restart(Settings),
    Pick { id: u8, kind: MediaKind },
    CancelMedia,
    Quit,
}

pub struct Session {
    pub draft: Settings,
    original: Settings,
    pub tab: Tab,
    pub row: usize,
}

impl Session {
    pub fn dirty(&self) -> bool {
        self.draft != self.original
    }

    pub fn tabs(&self) -> Vec<Tab> {
        if self.draft.tinker {
            vec![Tab::General, Tab::Media, Tab::Advanced, Tab::About]
        } else {
            vec![Tab::General, Tab::Media, Tab::About]
        }
    }

    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let on_off = |value| if value { "On".into() } else { "Off".into() };
        match self.tab {
            Tab::General => vec![
                ("Machine", self.draft.machine.profile.label().into()),
                ("RAM", format!("{} MB", self.draft.machine.ram_mb)),
                (
                    "CPU",
                    if self.draft.machine.cpus().len() == 1 {
                        format!("{} (fixed)", self.draft.machine.cpu.label())
                    } else {
                        self.draft.machine.cpu.label().into()
                    },
                ),
                ("Keyboard", self.draft.machine.keyboard.label().into()),
                ("CRT scanlines", on_off(self.draft.scanlines)),
                ("Status strip", on_off(self.draft.show_status)),
                ("Tinker", on_off(self.draft.tinker)),
            ],
            Tab::Advanced => vec![
                (
                    "Guest resolution",
                    self.draft.advanced.resolution.label().replace('×', "x"),
                ),
                ("CPU cache model", self.draft.advanced.cache_label().into()),
                ("Guest clock offset", self.draft.advanced.clock_label()),
                (
                    "SCSI interrupt timing",
                    if self.draft.advanced.scsi_deferred_interrupts {
                        "Deferred (default)"
                    } else {
                        "Immediate"
                    }
                    .into(),
                ),
                (
                    "Serial console log",
                    self.draft.advanced.serial_logging.label().into(),
                ),
                ("Bilinear filtering", on_off(self.draft.smoothing)),
                ("Debug output", self.draft.debug_output.label().into()),
            ],
            Tab::Media => [
                "SCSI 0:1", "SCSI 0:2", "SCSI 0:3", "SCSI 0:4", "SCSI 0:5", "SCSI 0:6", "SCSI 0:7",
            ]
            .iter()
            .enumerate()
            .map(|(index, &label)| (label, self.draft.media.row_value(index as u8 + 1)))
            .collect(),
            Tab::About => vec![],
        }
    }
}

#[derive(Default)]
pub struct Overlay {
    pub session: Option<Session>,
    pub dialog: Option<Dialog>,
    pub busy: bool,
}

impl Overlay {
    pub fn picked(&mut self, id: u8, medium: Medium) {
        if let Some(session) = &mut self.session {
            session.draft.media.devices.insert(id, medium);
        }
        self.busy = false;
    }
    pub fn confirm_restart(&mut self) {
        self.dialog = Some(Dialog::Restart { confirm: false });
    }

    pub fn save_failed(&mut self) {
        self.dialog = None;
        self.busy = false;
    }

    pub fn saved(&mut self) {
        self.session = None;
        self.dialog = None;
        self.busy = false;
    }

    pub fn key(&mut self, key: Key, live: &Settings) -> Effect {
        if self.busy {
            if matches!(key, Key::Escape | Key::Quit) {
                self.busy = false;
                if key == Key::Quit {
                    self.dialog = Some(Dialog::Quit { confirm: false });
                }
                return Effect::CancelMedia;
            }
            return Effect::None;
        }
        if let Some(dialog) = self.dialog {
            match (dialog, key) {
                (_, Key::Escape) => self.dialog = None,
                (Dialog::Unsaved { choice }, Key::Left | Key::Right) => {
                    let step = if key == Key::Left { 2 } else { 1 };
                    self.dialog = Some(Dialog::Unsaved {
                        choice: (choice + step) % 3,
                    });
                }
                (Dialog::Unsaved { choice: 0 }, Key::Enter) => {
                    return Effect::Save(self.session.as_ref().unwrap().draft.clone());
                }
                (Dialog::Unsaved { choice: 1 }, Key::Enter) => self.saved(),
                (Dialog::Unsaved { .. }, Key::Enter) => self.dialog = None,
                (Dialog::Quit { confirm }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Quit { confirm: !confirm });
                }
                (Dialog::Quit { confirm: true }, Key::Enter) => return Effect::Quit,
                (Dialog::Quit { confirm: false }, Key::Enter) => self.dialog = None,
                (Dialog::Restart { confirm }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Restart { confirm: !confirm });
                }
                (Dialog::Restart { confirm: true }, Key::Enter) => {
                    return Effect::Restart(self.session.as_ref().unwrap().draft.clone());
                }
                (Dialog::Restart { confirm: false }, Key::Enter) => self.dialog = None,
                (Dialog::Media { id, choice }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Media {
                        id,
                        choice: (choice + if key == Key::Left { 3 } else { 1 }) % 4,
                    });
                }
                (Dialog::Media { id, choice: 0 | 1 }, Key::Enter) => {
                    let kind = if matches!(dialog, Dialog::Media { choice: 0, .. }) {
                        MediaKind::Disk
                    } else {
                        MediaKind::Cdrom
                    };
                    self.dialog = None;
                    self.busy = true;
                    return Effect::Pick { id, kind };
                }
                (Dialog::Media { id, choice: 2 }, Key::Enter) => {
                    self.session
                        .as_mut()
                        .unwrap()
                        .draft
                        .media
                        .devices
                        .remove(&id);
                    self.dialog = None;
                }
                (Dialog::Media { .. }, Key::Enter) => self.dialog = None,
                _ => {}
            }
            return Effect::None;
        }
        if key == Key::Quit {
            self.dialog = Some(Dialog::Quit { confirm: false });
            return Effect::None;
        }
        let Some(session) = &mut self.session else {
            if key == Key::Options {
                self.session = Some(Session {
                    draft: live.clone(),
                    original: live.clone(),
                    tab: Tab::General,
                    row: 0,
                });
            }
            return Effect::None;
        };
        match key {
            Key::Options => return Effect::Save(session.draft.clone()),
            Key::Escape if session.dirty() => self.dialog = Some(Dialog::Unsaved { choice: 2 }),
            Key::Escape => self.session = None,
            Key::Left | Key::Right => {
                let tabs = session.tabs();
                let current = tabs.iter().position(|t| *t == session.tab).unwrap_or(0);
                let step = if key == Key::Left { tabs.len() - 1 } else { 1 };
                session.tab = tabs[(current + step) % tabs.len()];
                session.row = 0;
            }
            Key::Up | Key::Down => {
                let count = session.rows().len();
                if count > 0 {
                    let step = if key == Key::Up { count - 1 } else { 1 };
                    session.row = (session.row + step) % count;
                }
            }
            Key::Enter => match (session.tab, session.row) {
                (Tab::General, 0) => session.draft.machine.cycle_profile(),
                (Tab::General, 1) => session.draft.machine.cycle_ram(),
                (Tab::General, 2) => session.draft.machine.cycle_cpu(),
                (Tab::General, 3) => session.draft.machine.cycle_keyboard(),
                (Tab::General, 4) => session.draft.scanlines = !session.draft.scanlines,
                (Tab::General, 5) => session.draft.show_status = !session.draft.show_status,
                (Tab::General, 6) => session.draft.tinker = !session.draft.tinker,
                (Tab::Advanced, 0) => session.draft.advanced.cycle_resolution(),
                (Tab::Advanced, 1) => session.draft.advanced.cycle_cache(),
                (Tab::Advanced, 2) => session.draft.advanced.cycle_clock_offset(),
                (Tab::Advanced, 3) => {
                    session.draft.advanced.scsi_deferred_interrupts =
                        !session.draft.advanced.scsi_deferred_interrupts
                }
                (Tab::Advanced, 4) => session.draft.advanced.cycle_serial_logging(),
                (Tab::Advanced, 5) => session.draft.smoothing = !session.draft.smoothing,
                (Tab::Advanced, 6) => session.draft.debug_output.cycle(),
                (Tab::Media, row) => {
                    self.dialog = Some(Dialog::Media {
                        id: row as u8 + 1,
                        choice: 3,
                    })
                }
                _ => {}
            },
            _ => {}
        }
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_changes_are_drafts_until_saved_and_discard_restores_output() {
        use crate::settings::DebugOutput;
        let live = Settings {
            tinker: true,
            ..Default::default()
        };
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Right, Key::Right, Key::Up, Key::Enter] {
            overlay.key(key, &live);
        }
        assert_eq!(
            overlay.session.as_ref().unwrap().draft.debug_output,
            DebugOutput::Screen
        );
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected a Save draft");
        };
        assert!(!draft.machine_differs_from(&live));
        assert_eq!(live.debug_output, DebugOutput::Off);
        overlay.save_failed();
        assert!(overlay.session.as_ref().unwrap().dirty());
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(
            overlay.session.as_ref().unwrap().draft.debug_output,
            DebugOutput::Off
        );
    }

    #[test]
    fn advanced_changes_remain_drafts_survive_hiding_and_can_be_discarded() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Up, Key::Enter, Key::Right, Key::Right] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::Advanced);
        for row in 0..5 {
            overlay.key(Key::Enter, &live);
            if row != 4 {
                overlay.key(Key::Down, &live);
            }
        }
        let advanced = overlay.session.as_ref().unwrap().draft.advanced.clone();
        assert_ne!(advanced, live.advanced);
        // Return to General and hide Tinker. Core settings must not reset.
        for key in [Key::Left, Key::Left, Key::Up, Key::Enter] {
            overlay.key(key, &live);
        }
        let session = overlay.session.as_ref().unwrap();
        assert!(!session.tabs().contains(&Tab::Advanced));
        assert_eq!(session.draft.advanced, advanced);
        assert!(
            matches!(overlay.key(Key::Options, &live), Effect::Save(draft) if draft.advanced == advanced)
        );
        overlay.confirm_restart();
        overlay.key(Key::Enter, &live); // Cancel is selected initially.
        assert_eq!(overlay.session.as_ref().unwrap().draft.advanced, advanced);
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
    }

    #[test]
    fn media_choice_cancel_disconnect_and_discard_preserve_live_devices() {
        let mut live = Settings::default();
        live.media.devices.insert(
            1,
            Medium {
                kind: MediaKind::Disk,
                path: "/disk.img".into(),
            },
        );
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Right, Key::Enter] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.dialog, Some(Dialog::Media { id: 1, choice: 3 }));
        overlay.key(Key::Enter, &live);
        assert!(!overlay.session.as_ref().unwrap().dirty());
        for key in [Key::Enter, Key::Left, Key::Enter] {
            overlay.key(key, &live);
        }
        assert!(
            overlay
                .session
                .as_ref()
                .unwrap()
                .draft
                .media
                .devices
                .is_empty()
        );
        assert_eq!(live.media.devices.len(), 1);
        for key in [
            Key::Escape,
            Key::Left,
            Key::Enter,
            Key::Options,
            Key::Right,
            Key::Enter,
            Key::Right,
            Key::Right,
        ] {
            overlay.key(key, &live);
        }
        assert_eq!(
            overlay.key(Key::Enter, &live),
            Effect::Pick {
                id: 1,
                kind: MediaKind::Cdrom
            }
        );
        assert!(overlay.busy);
        assert_eq!(overlay.key(Key::Options, &live), Effect::None);
        assert_eq!(overlay.key(Key::Escape, &live), Effect::CancelMedia);
        assert!(!overlay.session.as_ref().unwrap().dirty());
    }

    #[test]
    fn discard_does_not_change_live_settings() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Enter, Key::Escape] {
            overlay.key(key, &live);
        }
        assert!(!live.scanlines);
        assert_eq!(overlay.dialog, Some(Dialog::Unsaved { choice: 2 }));
        overlay.key(Key::Left, &live);
        overlay.key(Key::Enter, &live);
        assert!(overlay.session.is_none());
        overlay.key(Key::Options, &live);
        assert_eq!(overlay.session.unwrap().draft, live);
    }

    #[test]
    fn save_keeps_draft_until_persistence_succeeds() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        for _ in 0..4 {
            overlay.key(Key::Down, &live);
        }
        overlay.key(Key::Enter, &live);
        let effect = overlay.key(Key::Options, &live);
        assert_eq!(
            effect,
            Effect::Save(Settings {
                scanlines: true,
                ..live
            })
        );
        assert!(overlay.session.as_ref().unwrap().dirty());
        overlay.saved();
        assert!(overlay.session.is_none());
    }

    #[test]
    fn advanced_tab_requires_tinker() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        overlay.key(Key::Right, &live);
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::Media);
        overlay.key(Key::Right, &live);
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::About);
        for key in [Key::Right, Key::Up, Key::Enter, Key::Right, Key::Right] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::Advanced);
    }

    #[test]
    fn quit_is_cancelled_by_default_and_dialog_consumes_options_key() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        for key in [Key::Quit, Key::Options, Key::Enter] {
            assert_eq!(overlay.key(key, &live), Effect::None);
        }
        assert!(overlay.dialog.is_none());
        overlay.key(Key::Quit, &live);
        overlay.key(Key::Left, &live);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::Quit);
    }

    #[test]
    fn restart_confirmation_defaults_to_cancel_and_retains_hardware_draft() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Down, Key::Enter] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft.machine.ram_mb, 128);
        assert!(matches!(overlay.key(Key::Options, &live), Effect::Save(_)));
        overlay.confirm_restart();
        assert_eq!(overlay.dialog, Some(Dialog::Restart { confirm: false }));
        assert_eq!(overlay.key(Key::Options, &live), Effect::None);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::None);
        assert!(overlay.dialog.is_none());
        assert!(overlay.session.as_ref().unwrap().dirty());
        assert_eq!(live.machine.ram_mb, 64);
        overlay.confirm_restart();
        overlay.key(Key::Left, &live);
        assert!(
            matches!(overlay.key(Key::Enter, &live), Effect::Restart(draft) if draft.machine.ram_mb == 128)
        );
        overlay.save_failed();
        assert!(overlay.dialog.is_none());
        assert!(overlay.session.as_ref().unwrap().dirty());
        for key in [Key::Escape, Key::Left, Key::Enter] {
            overlay.key(key, &live);
        }
        assert!(overlay.session.is_none());
    }
}
