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
    Advanced,
    About,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
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
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Save(Settings),
    Restart(Settings),
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
            vec![Tab::General, Tab::Advanced, Tab::About]
        } else {
            vec![Tab::General, Tab::About]
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
            Tab::Advanced => vec![("Bilinear filtering", on_off(self.draft.smoothing))],
            Tab::About => vec![],
        }
    }
}

#[derive(Default)]
pub struct Overlay {
    pub session: Option<Session>,
    pub dialog: Option<Dialog>,
}

impl Overlay {
    pub fn confirm_restart(&mut self) {
        self.dialog = Some(Dialog::Restart { confirm: false });
    }

    pub fn save_failed(&mut self) {
        self.dialog = None;
    }

    pub fn saved(&mut self) {
        self.session = None;
        self.dialog = None;
    }

    pub fn key(&mut self, key: Key, live: &Settings) -> Effect {
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
                (Tab::Advanced, 0) => session.draft.smoothing = !session.draft.smoothing,
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
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::About);
        for key in [Key::Left, Key::Up, Key::Enter, Key::Right] {
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
