use crate::expert::{Field as ExpertField, Page};
use crate::media::{MediaKind, Medium};
use crate::network::{Field, Folder, Interface};
use crate::settings::{Settings, WindowScale};
use crate::storage::{Action, Operation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    NextMachine,
    Reset,
    Backspace,
    SelectAll,
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
    Network,
    Extensions,
    About,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Media => "Media",
            Self::Advanced => "Advanced",
            Self::Network => "Network",
            Self::Extensions => "Extensions",
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
    Firmware { choice: usize }, // Choose, Clear, Cancel
    Reset { confirm: bool },
    Folder { folder: Folder, choice: usize },
    Storage { slot: u8, cd: bool, choice: usize },
    Action { operation: Operation, confirm: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Save(Settings),
    Restart(Settings),
    Pick { id: u8, kind: MediaKind },
    PickProm,
    CancelMedia,
    Quit,
    Reset,
    PickFolder(Folder),
    ListInterfaces,
    Storage(Operation),
    RunAction(Action),
    RuntimeStatus,
}

pub struct Session {
    pub draft: Settings,
    original: Settings,
    pub tab: Tab,
    pub row: usize,
    media_controller: u8,
    storage_index: usize,
    pub advanced_page: Page,
}

impl Session {
    pub fn dirty(&self) -> bool {
        self.draft != self.original
    }

    pub fn tabs(&self) -> Vec<Tab> {
        if self.draft.tinker {
            vec![
                Tab::General,
                Tab::Media,
                Tab::Advanced,
                Tab::Network,
                Tab::Extensions,
                Tab::About,
            ]
        } else {
            vec![Tab::General, Tab::Media, Tab::About]
        }
    }

    fn media_controllers(&self) -> u8 {
        self.draft.machine.profile.scsi_controllers().max(
            if self.draft.media.devices.keys().any(|&slot| slot >= 17) {
                2
            } else {
                1
            },
        )
    }

    fn media_targets(&self) -> u8 {
        self.draft
            .media
            .devices
            .keys()
            .filter(|&&slot| slot / 16 == self.media_controller)
            .map(|&slot| slot % 16)
            .max()
            .unwrap_or(0)
            .max(self.draft.machine.profile.max_scsi_id())
    }

    pub fn media_slot(&self) -> Option<u8> {
        (self.row < self.media_targets() as usize)
            .then_some(self.media_controller * 16 + self.row as u8 + 1)
    }

    pub fn storage_slot(&self) -> Option<u8> {
        let count = self.draft.media.devices.len();
        if count == 0 {
            None
        } else {
            self.draft
                .media
                .devices
                .keys()
                .nth(self.storage_index % count)
                .copied()
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
                ("PROM firmware", self.draft.machine.prom_label()),
                (
                    "Machine label",
                    self.draft
                        .machine_label
                        .clone()
                        .unwrap_or_else(|| "Unnamed - use Save machine as".into()),
                ),
                ("Save machine as...", "Enter a new label".into()),
                ("Guest audio", on_off(!self.draft.audio.muted)),
                ("Guest volume", format!("{}%", self.draft.audio.volume)),
            ],
            Tab::Advanced if self.advanced_page != Page::Core => {
                self.draft.advanced.expert.rows(self.advanced_page)
            }
            Tab::Advanced => vec![
                (
                    "Guest resolution",
                    if self.draft.machine.newport() {
                        self.draft.advanced.resolution.label().replace('×', "x")
                    } else {
                        "Guest controlled".into()
                    },
                ),
                (
                    "CPU cache model",
                    if self.draft.machine.newport() {
                        self.draft.advanced.cache_label()
                    } else {
                        "Shadow (fixed)"
                    }
                    .into(),
                ),
                (
                    "Guest clock offset",
                    if self.draft.machine.supports_prom_override() {
                        "Host time (fixed)".into()
                    } else {
                        self.draft.advanced.clock_label()
                    },
                ),
                (
                    "SCSI interrupt timing",
                    if self.draft.machine.supports_prom_override() {
                        "Managed by SCSI controller"
                    } else if self.draft.advanced.scsi_deferred_interrupts {
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
                ("GIF resolution", self.draft.gif.resolution_label()),
                ("GIF frame rate", format!("{} fps", self.draft.gif.fps)),
                ("GIF encoder", self.draft.gif.encoder.label().into()),
                ("Advanced page", "Core - Enter for next".into()),
            ],
            Tab::Extensions => vec![
                (
                    "Graphics board",
                    self.draft
                        .machine
                        .graphics_board()
                        .map_or("Integrated GBE (fixed)", |board| board.label())
                        .into(),
                ),
                (
                    "Display heads",
                    if self.draft.machine.newport() {
                        self.draft.machine.heads.to_string()
                    } else {
                        "1 (fixed)".into()
                    },
                ),
                (
                    "CPU count",
                    if self.draft.machine.profile == iris::config::MachineProfile::OctaneIp30 {
                        self.draft.machine.cpus.to_string()
                    } else {
                        "1 (fixed)".into()
                    },
                ),
                (
                    "Scratch target",
                    self.draft.media.scratch.slot.map_or_else(
                        || "Off".into(),
                        |slot| format!("SCSI 0:{slot} (must be unused)"),
                    ),
                ),
                (
                    "Scratch size",
                    format!("{} MB", self.draft.media.scratch.size_mb),
                ),
                (
                    "Storage action target",
                    self.storage_slot().map_or_else(
                        || "No media configured".into(),
                        |slot| {
                            format!(
                                "SCSI {}:{} - {}",
                                slot / 16,
                                slot % 16,
                                self.draft.media.row_value(slot)
                            )
                        },
                    ),
                ),
                (
                    "Add CD to changer...",
                    "Choose an image for the saved queue".into(),
                ),
                (
                    "Clear queued CDs",
                    format!(
                        "{} queued images",
                        self.storage_slot()
                            .and_then(|slot| self.draft.media.cd_changers.get(&slot))
                            .map_or(0, Vec::len)
                    ),
                ),
                (
                    "Storage actions...",
                    "Status / CD tray / offline disk commit or reset".into(),
                ),
                (
                    "Import scratch payload...",
                    "Choose host file (overwrites payload)".into(),
                ),
                (
                    "Export scratch payload...",
                    "Save full payload to a new host file".into(),
                ),
            ],
            Tab::Media => {
                const LABELS: [&str; 32] = [
                    "",
                    "SCSI 0:1",
                    "SCSI 0:2",
                    "SCSI 0:3",
                    "SCSI 0:4",
                    "SCSI 0:5",
                    "SCSI 0:6",
                    "SCSI 0:7",
                    "SCSI 0:8",
                    "SCSI 0:9",
                    "SCSI 0:10",
                    "SCSI 0:11",
                    "SCSI 0:12",
                    "SCSI 0:13",
                    "SCSI 0:14",
                    "SCSI 0:15",
                    "",
                    "SCSI 1:1",
                    "SCSI 1:2",
                    "SCSI 1:3",
                    "SCSI 1:4",
                    "SCSI 1:5",
                    "SCSI 1:6",
                    "SCSI 1:7",
                    "SCSI 1:8",
                    "SCSI 1:9",
                    "SCSI 1:10",
                    "SCSI 1:11",
                    "SCSI 1:12",
                    "SCSI 1:13",
                    "SCSI 1:14",
                    "SCSI 1:15",
                ];
                let mut rows: Vec<_> = (1..=self.media_targets())
                    .map(|id| {
                        let slot = self.media_controller * 16 + id;
                        let mut value = self.draft.media.row_value(slot);
                        if self.media_controller >= self.draft.machine.profile.scsi_controllers()
                            || id > self.draft.machine.profile.max_scsi_id()
                        {
                            value = format!("Unavailable: {value}");
                        }
                        (LABELS[slot as usize], value)
                    })
                    .collect();
                if self.media_controllers() > 1 {
                    rows.push((
                        "SCSI controller",
                        format!("{} (Enter to switch bus)", self.media_controller),
                    ));
                }
                rows
            }
            Tab::About => vec![],
            Tab::Network => self.draft.network.rows(),
        }
    }
}

#[derive(Default)]
pub struct Overlay {
    pub session: Option<Session>,
    pub dialog: Option<Dialog>,
    pub busy: bool,
    pub text_edit: Option<TextEdit>,
    pub interfaces: Option<InterfacePicker>,
    pub action: Option<Action>,
    pub action_busy: bool,
    pub report: Option<Vec<String>>,
    pub report_offset: usize,
}

#[derive(Clone, Copy)]
pub enum TextField {
    Label,
    Network(Field),
    Expert(ExpertField),
}

pub struct TextEdit {
    pub text: String,
    pub error: Option<String>,
    pub field: TextField,
    pub selected: bool,
}

pub struct InterfacePicker {
    pub choices: Vec<Interface>,
    pub row: usize,
}

impl Overlay {
    pub fn action_picked(&mut self, action: Action) {
        self.busy = false;
        if let Operation::QueueCd(slot) = action.operation {
            if let Some(session) = &mut self.session
                && let Some(path) = action.path.and_then(|p| p.to_str().map(str::to_owned))
            {
                session
                    .draft
                    .media
                    .cd_changers
                    .entry(slot)
                    .or_default()
                    .push(path);
            }
        } else {
            self.dialog = Some(Dialog::Action {
                operation: action.operation,
                confirm: false,
            });
            self.action = Some(action);
        }
    }
    pub fn show_report(&mut self, message: String) {
        self.busy = false;
        self.action_busy = false;
        self.action = None;
        self.dialog = None;
        self.report = Some(
            message
                .lines()
                .flat_map(|line| {
                    let chars: Vec<_> = line.chars().collect();
                    chars
                        .chunks(96)
                        .map(|chunk| chunk.iter().collect::<String>())
                        .collect::<Vec<_>>()
                })
                .collect(),
        );
        self.report_offset = 0;
    }

    pub fn folder_picked(&mut self, folder: Folder, path: String) {
        if let Some(session) = &mut self.session {
            folder.set(&mut session.draft.network, Some(path));
        }
        self.busy = false;
    }
    pub fn interfaces_listed(&mut self, choices: Vec<Interface>) {
        self.busy = false;
        self.interfaces = Some(InterfacePicker { choices, row: 0 });
    }
    fn edit_network(&mut self, field: Field) {
        self.text_edit = Some(TextEdit {
            text: field.value(&self.session.as_ref().unwrap().draft.network),
            field: TextField::Network(field),
            error: None,
            selected: true,
        });
    }
    /// A separately saved window preference must survive Save and Discard,
    /// without changing whether the rest of this session has unsaved edits.
    pub fn window_scale_saved(&mut self, scale: WindowScale) {
        if let Some(session) = &mut self.session {
            session.original.window_scale = scale;
            session.draft.window_scale = scale;
        }
    }

    pub fn picked(&mut self, id: u8, medium: Medium) {
        if let Some(session) = &mut self.session {
            if medium.kind != MediaKind::Cdrom {
                session.draft.media.cd_changers.remove(&id);
            }
            session.draft.media.devices.insert(id, medium);
        }
        self.busy = false;
    }
    pub fn prom_picked(&mut self, profile: iris::config::MachineProfile, path: String) {
        if let Some(session) = &mut self.session
            && session.draft.machine.profile == profile
        {
            session.draft.machine.set_prom(Some(path));
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
        self.text_edit = None;
        self.interfaces = None;
        self.action = None;
        self.report = None;
    }

    pub fn text_input(&mut self, text: &str) {
        if let Some(edit) = &mut self.text_edit {
            if edit.selected {
                edit.text.clear();
                edit.selected = false;
            }
            for ch in text.chars() {
                let label = matches!(edit.field, TextField::Label);
                let max = if label { 32 } else { 1024 };
                if edit.text.len() + ch.len_utf8() <= max
                    && !ch.is_control()
                    && (!label || ch.is_ascii())
                {
                    edit.text.push(ch);
                }
            }
            edit.error = None;
        }
    }

    pub fn key(&mut self, key: Key, live: &Settings) -> Effect {
        if self.action_busy {
            return Effect::None;
        }
        if let Some(lines) = &self.report {
            match key {
                Key::Enter | Key::Escape => self.report = None,
                Key::Up => self.report_offset = self.report_offset.saturating_sub(1),
                Key::Down => {
                    self.report_offset = (self.report_offset + 1).min(lines.len().saturating_sub(1))
                }
                _ => {}
            }
            return Effect::None;
        }
        if let Some(picker) = &mut self.interfaces {
            let count = picker.choices.len() + 2; // Automatic, devices, enter name.
            match key {
                Key::Escape => self.interfaces = None,
                Key::Up => picker.row = (picker.row + count - 1) % count,
                Key::Down => picker.row = (picker.row + 1) % count,
                Key::Enter => {
                    let row = picker.row;
                    if row == count - 1 {
                        self.interfaces = None;
                        self.edit_network(Field::Interface);
                    } else {
                        self.session.as_mut().unwrap().draft.network.pcap_interface = if row == 0 {
                            None
                        } else {
                            Some(picker.choices[row - 1].name.clone())
                        };
                        self.interfaces = None;
                    }
                }
                Key::Quit => {
                    self.interfaces = None;
                    self.dialog = Some(Dialog::Quit { confirm: false });
                }
                _ => {}
            }
            return Effect::None;
        }
        if let Some(edit) = &mut self.text_edit {
            match key {
                Key::Escape => self.text_edit = None,
                Key::SelectAll => edit.selected = true,
                Key::Left | Key::Right => edit.selected = false,
                Key::Backspace => {
                    if edit.selected {
                        edit.text.clear();
                    } else {
                        edit.text.pop();
                    }
                    edit.selected = false;
                    edit.error = None;
                }
                Key::Enter => {
                    let draft = &mut self.session.as_mut().unwrap().draft;
                    let result = match edit.field {
                        TextField::Label => draft.save_as_machine(edit.text.trim()),
                        TextField::Network(field) => field.apply(&mut draft.network, &edit.text),
                        TextField::Expert(field) => {
                            field.apply(&mut draft.advanced.expert, &edit.text)
                        }
                    };
                    match result {
                        Ok(()) => self.text_edit = None,
                        Err(error) => edit.error = Some(error),
                    }
                }
                Key::Quit => {
                    self.text_edit = None;
                    self.dialog = Some(Dialog::Quit { confirm: false });
                }
                _ => {}
            }
            return Effect::None;
        }
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
                (_, Key::Escape) => {
                    self.dialog = None;
                    self.action = None;
                }
                (Dialog::Action { operation, confirm }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Action {
                        operation,
                        confirm: !confirm,
                    })
                }
                (Dialog::Action { confirm: true, .. }, Key::Enter) => {
                    self.dialog = None;
                    if let Some(action) = self.action.take() {
                        return Effect::RunAction(action);
                    }
                }
                (Dialog::Action { .. }, Key::Enter) => {
                    self.dialog = None;
                    self.action = None;
                }
                (Dialog::Storage { slot, cd, choice }, Key::Left | Key::Right) => {
                    let count = if cd { 5 } else { 4 };
                    self.dialog = Some(Dialog::Storage {
                        slot,
                        cd,
                        choice: (choice + if key == Key::Left { count - 1 } else { 1 }) % count,
                    });
                }
                (Dialog::Storage { slot, cd, choice }, Key::Enter) => {
                    self.dialog = None;
                    let operations = if cd {
                        vec![
                            Operation::Info(slot),
                            Operation::LoadCd(slot),
                            Operation::NextCd(slot),
                            Operation::EmptyCd(slot),
                        ]
                    } else {
                        vec![
                            Operation::Info(slot),
                            Operation::Commit(slot),
                            Operation::Reset(slot),
                        ]
                    };
                    if let Some(&operation) = operations.get(choice) {
                        return Effect::Storage(operation);
                    }
                }
                (Dialog::Folder { folder, choice }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Folder {
                        folder,
                        choice: (choice + if key == Key::Left { 3 } else { 1 }) % 4,
                    });
                }
                (Dialog::Folder { folder, choice: 0 }, Key::Enter) => {
                    self.dialog = None;
                    self.busy = true;
                    return Effect::PickFolder(folder);
                }
                (Dialog::Folder { folder, choice: 1 }, Key::Enter) => {
                    self.dialog = None;
                    self.edit_network(Field::Directory(folder));
                }
                (Dialog::Folder { folder, choice: 2 }, Key::Enter) => {
                    folder.set(&mut self.session.as_mut().unwrap().draft.network, None);
                    self.dialog = None;
                }
                (Dialog::Folder { .. }, Key::Enter) => self.dialog = None,
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
                (Dialog::Reset { confirm }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Reset { confirm: !confirm });
                }
                (Dialog::Reset { confirm: true }, Key::Enter) => {
                    self.dialog = None;
                    return Effect::Reset;
                }
                (Dialog::Reset { confirm: false }, Key::Enter) => self.dialog = None,
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
                    let session = self.session.as_mut().unwrap();
                    session.draft.media.devices.remove(&id);
                    session.draft.media.cd_changers.remove(&id);
                    session.media_controller %= session.media_controllers();
                    session.row = session.row.min(session.rows().len().saturating_sub(1));
                    self.dialog = None;
                }
                (Dialog::Media { .. }, Key::Enter) => self.dialog = None,
                (Dialog::Firmware { choice }, Key::Left | Key::Right) => {
                    self.dialog = Some(Dialog::Firmware {
                        choice: (choice + if key == Key::Left { 2 } else { 1 }) % 3,
                    });
                }
                (Dialog::Firmware { choice: 0 }, Key::Enter) => {
                    self.dialog = None;
                    self.busy = true;
                    return Effect::PickProm;
                }
                (Dialog::Firmware { choice: 1 }, Key::Enter) => {
                    self.session.as_mut().unwrap().draft.machine.set_prom(None);
                    self.dialog = None;
                }
                (Dialog::Firmware { .. }, Key::Enter) => self.dialog = None,
                _ => {}
            }
            return Effect::None;
        }
        if key == Key::Quit {
            self.dialog = Some(Dialog::Quit { confirm: false });
            return Effect::None;
        }
        if key == Key::Reset && self.session.is_none() {
            self.dialog = Some(Dialog::Reset { confirm: false });
            return Effect::None;
        }
        let Some(session) = &mut self.session else {
            if matches!(key, Key::Options | Key::NextMachine) {
                self.session = Some(Session {
                    draft: live.clone(),
                    original: live.clone(),
                    tab: Tab::General,
                    row: 0,
                    media_controller: 0,
                    storage_index: 0,
                    advanced_page: Page::Core,
                });
                if key == Key::NextMachine {
                    let session = self.session.as_mut().unwrap();
                    session.row = 8;
                    if session.draft.next_machine() {
                        return Effect::Save(session.draft.clone());
                    }
                    session.row = 9;
                }
            }
            return Effect::None;
        };
        match key {
            Key::NextMachine => {
                session.draft.next_machine();
                session.media_controller = 0;
                session.row = 0;
            }
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
                (Tab::General, 8) => {
                    session.draft.next_machine();
                    session.media_controller = 0;
                }
                (Tab::General, 9) => {
                    self.text_edit = Some(TextEdit {
                        text: String::new(),
                        error: None,
                        field: TextField::Label,
                        selected: false,
                    })
                }
                (Tab::General, 0) => {
                    session.draft.machine.cycle_profile();
                    session.media_controller = 0;
                    session
                        .draft
                        .network
                        .reconcile_profile(session.draft.machine.profile);
                }
                (Tab::General, 1) => session.draft.machine.cycle_ram(),
                (Tab::General, 2) => session.draft.machine.cycle_cpu(),
                (Tab::General, 3) => session.draft.machine.cycle_keyboard(),
                (Tab::General, 4) => session.draft.scanlines = !session.draft.scanlines,
                (Tab::General, 5) => session.draft.show_status = !session.draft.show_status,
                (Tab::General, 6) => session.draft.tinker = !session.draft.tinker,
                (Tab::General, 7) if session.draft.machine.supports_prom_override() => {
                    self.dialog = Some(Dialog::Firmware { choice: 2 });
                }
                (Tab::General, 10) => session.draft.audio.muted = !session.draft.audio.muted,
                (Tab::General, 11) => session.draft.audio.cycle_volume(),
                (Tab::Advanced, row) if row + 1 == session.rows().len() => {
                    session.advanced_page = session.advanced_page.next();
                    session.row = session.rows().len() - 1;
                }
                (Tab::Advanced, row) if session.advanced_page != Page::Core => {
                    let settings = &mut session.draft.advanced.expert;
                    let field = match (session.advanced_page, row) {
                        (Page::Performance, 0) => Some(ExpertField::CountMhz),
                        (Page::Performance, 1) if iris::build_features::IDLE_PAUSE => {
                            settings.idle_parking = !settings.idle_parking;
                            None
                        }
                        (Page::Performance, 2) => {
                            settings.thread_affinity = !settings.thread_affinity;
                            None
                        }
                        (Page::Performance, 3) => Some(ExpertField::CpuCore),
                        (Page::Performance, 4) => Some(ExpertField::Rex3Core),
                        (Page::Performance, 5) => Some(ExpertField::RefreshCore),
                        (Page::Performance, 6) => {
                            if iris::build_features::JITV2 {
                                Some(ExpertField::JitThreads)
                            } else {
                                settings.jit_threads = 1;
                                None
                            }
                        }
                        (Page::Performance, 7) => {
                            settings.jit_cache = iris::build_features::JITV2 && !settings.jit_cache;
                            None
                        }
                        (Page::Connections, 0) => Some(ExpertField::Monitor),
                        (Page::Connections, 1) => Some(ExpertField::SerialA),
                        (Page::Connections, 2) => Some(ExpertField::SerialB),
                        (Page::Connections, 3) => Some(ExpertField::Gdb),
                        (Page::Connections, 4) => Some(ExpertField::Vnc),
                        (Page::Connections, 5) => Some(ExpertField::VncFps),
                        (Page::Connections, 6) | (Page::Developer, 4) => {
                            return Effect::RuntimeStatus;
                        }
                        (Page::Developer, 0) => Some(ExpertField::BootElf),
                        (Page::Developer, 1) => {
                            settings.trace_on_boot =
                                cfg!(feature = "perfetto") && !settings.trace_on_boot;
                            None
                        }
                        (Page::Developer, 2) if cfg!(feature = "perfetto") => {
                            Some(ExpertField::TraceCategories)
                        }
                        (Page::Developer, 3) if cfg!(feature = "perfetto") => {
                            settings.trace_guest_clock = !settings.trace_guest_clock;
                            None
                        }
                        _ => None,
                    };
                    if let Some(field) = field {
                        self.text_edit = Some(TextEdit {
                            text: field.value(settings),
                            field: TextField::Expert(field),
                            error: None,
                            selected: true,
                        });
                    }
                }
                (Tab::Advanced, 0) if session.draft.machine.newport() => {
                    session.draft.advanced.cycle_resolution()
                }
                (Tab::Advanced, 1) if session.draft.machine.newport() => {
                    session.draft.advanced.cycle_cache()
                }
                (Tab::Advanced, 2) if !session.draft.machine.supports_prom_override() => {
                    session.draft.advanced.cycle_clock_offset()
                }
                (Tab::Advanced, 3) if !session.draft.machine.supports_prom_override() => {
                    session.draft.advanced.scsi_deferred_interrupts =
                        !session.draft.advanced.scsi_deferred_interrupts
                }
                (Tab::Advanced, 4) => session.draft.advanced.cycle_serial_logging(),
                (Tab::Advanced, 5) => session.draft.smoothing = !session.draft.smoothing,
                (Tab::Advanced, 6) => session.draft.debug_output.cycle(),
                (Tab::Advanced, 7) => session.draft.gif.cycle_width(),
                (Tab::Advanced, 8) => session.draft.gif.cycle_fps(),
                (Tab::Advanced, 9) => session.draft.gif.cycle_encoder(),
                (Tab::Network, 0) => session
                    .draft
                    .network
                    .cycle_mode(session.draft.machine.profile),
                (Tab::Network, 1) if session.draft.network.mode == iris::config::NetMode::Nat => {
                    self.edit_network(Field::Subnet)
                }
                (Tab::Network, 2) if session.draft.network.mode == iris::config::NetMode::Pcap => {
                    self.busy = true;
                    return Effect::ListInterfaces;
                }
                (Tab::Network, 3) if session.draft.network.mode != iris::config::NetMode::Off => {
                    self.dialog = Some(Dialog::Folder {
                        folder: Folder::Nfs,
                        choice: 3,
                    });
                }
                (Tab::Network, 4) => session.draft.network.cycle_nfs_version(),
                (Tab::Network, 5) if session.draft.network.mode == iris::config::NetMode::Pcap => {
                    self.edit_network(Field::NfsIp)
                }
                (Tab::Network, 6) if session.draft.network.mode == iris::config::NetMode::Nat => {
                    self.dialog = Some(Dialog::Folder {
                        folder: Folder::Tftp,
                        choice: 3,
                    });
                }
                (Tab::Network, 7) if session.draft.network.mode == iris::config::NetMode::Nat => {
                    self.edit_network(Field::Forwards)
                }
                (Tab::Extensions, 0) => session.draft.machine.cycle_graphics(),
                (Tab::Extensions, 1) if session.draft.machine.newport() => {
                    session.draft.machine.heads = 3 - session.draft.machine.heads
                }
                (Tab::Extensions, 2)
                    if session.draft.machine.profile
                        == iris::config::MachineProfile::OctaneIp30 =>
                {
                    session.draft.machine.cpus = 3 - session.draft.machine.cpus
                }
                (Tab::Extensions, 3) => session.draft.media.scratch.cycle_slot(),
                (Tab::Extensions, 4) => session.draft.media.scratch.cycle_size(),
                (Tab::Extensions, 5) => {
                    session.storage_index = session.storage_index.wrapping_add(1)
                }
                (Tab::Extensions, 6) => {
                    if let Some(slot) = session.storage_slot() {
                        return Effect::Storage(Operation::QueueCd(slot));
                    }
                }
                (Tab::Extensions, 7) => {
                    if let Some(slot) = session.storage_slot() {
                        session.draft.media.cd_changers.remove(&slot);
                    }
                }
                (Tab::Extensions, 8) => {
                    if let Some(slot) = session.storage_slot() {
                        let cd = session.draft.media.devices[&slot].kind == MediaKind::Cdrom;
                        self.dialog = Some(Dialog::Storage {
                            slot,
                            cd,
                            choice: if cd { 4 } else { 3 },
                        });
                    }
                }
                (Tab::Extensions, 9) => return Effect::Storage(Operation::ImportScratch),
                (Tab::Extensions, 10) => return Effect::Storage(Operation::ExportScratch),
                (Tab::Network, 8) => {
                    session.draft.network.forward_lan = !session.draft.network.forward_lan
                }
                (Tab::Network, 9) => self.edit_network(Field::Mac),
                (Tab::Network, 10) if session.draft.network.mode == iris::config::NetMode::Nat => {
                    self.dialog = Some(Dialog::Folder {
                        folder: Folder::Rsh,
                        choice: 3,
                    });
                }
                (Tab::Media, _) => {
                    if let Some(id) = session.media_slot() {
                        self.dialog = Some(Dialog::Media { id, choice: 3 });
                    } else {
                        session.media_controller =
                            (session.media_controller + 1) % session.media_controllers();
                        session.row = session.media_targets() as usize;
                    }
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
    fn unavailable_saved_features_can_be_cleared_without_editing_the_file() {
        let mut live = Settings {
            tinker: true,
            ..Default::default()
        };
        live.advanced.expert.jit_threads = 4;
        live.advanced.expert.jit_cache = true;
        live.advanced.expert.trace_on_boot = true;
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        if !iris::build_features::JITV2 {
            let session = overlay.session.as_mut().unwrap();
            session.tab = Tab::Advanced;
            session.advanced_page = Page::Performance;
            session.row = 6;
            overlay.key(Key::Enter, &live);
            overlay.key(Key::Down, &live);
            overlay.key(Key::Enter, &live);
            let draft = &overlay.session.as_ref().unwrap().draft.advanced.expert;
            assert_eq!(draft.jit_threads, 1);
            assert!(!draft.jit_cache);
        }
        if !cfg!(feature = "perfetto") {
            let session = overlay.session.as_mut().unwrap();
            session.tab = Tab::Advanced;
            session.advanced_page = Page::Developer;
            session.row = 1;
            overlay.key(Key::Enter, &live);
            assert!(
                !overlay
                    .session
                    .as_ref()
                    .unwrap()
                    .draft
                    .advanced
                    .expert
                    .trace_on_boot
            );
        }
        assert!(live.advanced.expert.trace_on_boot);
        assert!(live.advanced.expert.jit_cache);
    }

    #[test]
    fn advanced_pages_edit_only_drafts_and_runtime_status_is_separate() {
        let live = Settings {
            tinker: true,
            ..Default::default()
        };
        let mut overlay = Overlay::default();
        for key in [
            Key::Options,
            Key::Right,
            Key::Right,
            Key::Up,
            Key::Enter,
            Key::Down,
            Key::Enter,
        ] {
            overlay.key(key, &live);
        }
        assert_eq!(
            overlay.session.as_ref().unwrap().advanced_page,
            Page::Performance
        );
        assert!(overlay.text_edit.is_some());
        overlay.text_input("97.5");
        overlay.key(Key::Enter, &live);
        assert_eq!(
            overlay
                .session
                .as_ref()
                .unwrap()
                .draft
                .advanced
                .expert
                .count_mhz
                .map(f64::from),
            Some(97.5)
        );
        assert_eq!(live.advanced.expert.count_mhz, None);
        for key in [
            Key::Escape,
            Key::Left,
            Key::Enter,
            Key::Options,
            Key::Right,
            Key::Right,
        ] {
            overlay.key(key, &live);
        }
        assert!(!overlay.session.as_ref().unwrap().dirty());
        assert_eq!(overlay.session.as_ref().unwrap().advanced_page, Page::Core);
        for key in [Key::Up, Key::Enter, Key::Enter, Key::Up] {
            overlay.key(key, &live);
        }
        assert_eq!(
            overlay.session.as_ref().unwrap().advanced_page,
            Page::Connections
        );
        assert_eq!(overlay.key(Key::Enter, &live), Effect::RuntimeStatus);
        assert!(!overlay.session.as_ref().unwrap().dirty());
    }

    #[test]
    fn storage_actions_confirm_separately_from_drafts_and_consume_busy_keys() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        let action = Action::new(Operation::NextCd(22));
        overlay.action_picked(action.clone());
        assert_eq!(overlay.key(Key::Options, &live), Effect::None);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::None); // Cancel by default.
        assert!(overlay.action.is_none());
        assert!(!overlay.session.as_ref().unwrap().dirty());
        overlay.action_picked(action.clone());
        overlay.key(Key::Left, &live);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::RunAction(action));
        overlay.action_busy = true;
        for key in [Key::Quit, Key::Escape, Key::Options, Key::Enter] {
            assert_eq!(overlay.key(key, &live), Effect::None);
        }
        overlay.show_report("Disc changed".into());
        assert!(!overlay.action_busy);
        assert_eq!(overlay.key(Key::Options, &live), Effect::None);
        overlay.key(Key::Enter, &live);
        assert!(overlay.report.is_none());
        assert!(!overlay.session.as_ref().unwrap().dirty());
    }

    #[test]
    fn changer_queue_is_a_saved_draft_and_discard_has_no_actions() {
        let mut live = Settings::default();
        live.media.devices.insert(
            6,
            Medium {
                kind: MediaKind::Cdrom,
                path: "/tmp/boot.iso".into(),
            },
        );
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        overlay.action_picked(Action {
            operation: Operation::QueueCd(6),
            path: Some("/tmp/second.iso".into()),
        });
        assert!(overlay.dialog.is_none());
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Save")
        };
        assert_eq!(draft.media.cd_changers[&6], ["/tmp/second.iso"]);
        assert!(draft.machine_differs_from(&live));
        overlay.save_failed();
        for key in [Key::Escape, Key::Left, Key::Enter] {
            assert_eq!(overlay.key(key, &live), Effect::None);
        }
        assert!(overlay.session.is_none());
        assert!(live.media.cd_changers.is_empty());
    }

    fn network_overlay(live: &Settings) -> Overlay {
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, live);
        assert!(
            !overlay
                .session
                .as_ref()
                .unwrap()
                .tabs()
                .contains(&Tab::Network)
        );
        for _ in 0..6 {
            overlay.key(Key::Down, live);
        }
        overlay.key(Key::Enter, live); // Tinker
        for _ in 0..3 {
            overlay.key(Key::Right, live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::Network);
        overlay
    }

    #[test]
    fn extensions_are_gated_drafted_and_require_restart() {
        let live = Settings::default();
        let mut overlay = network_overlay(&live);
        overlay.key(Key::Right, &live);
        assert_eq!(overlay.session.as_ref().unwrap().tab, Tab::Extensions);
        overlay.key(Key::Down, &live);
        overlay.key(Key::Enter, &live); // Two heads.
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Save")
        };
        assert_eq!(draft.machine.heads, 2);
        assert_eq!(live.machine.heads, 1);
        assert!(draft.machine_differs_from(&live));
        overlay.confirm_restart();
        overlay.key(Key::Enter, &live); // Cancel retains the draft.
        assert_eq!(overlay.session.as_ref().unwrap().draft.machine.heads, 2);
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
        assert!(
            !overlay
                .session
                .as_ref()
                .unwrap()
                .tabs()
                .contains(&Tab::Extensions)
        );
    }

    #[test]
    fn media_bus_selector_handles_wide_targets_and_unavailable_assignments() {
        let mut live = Settings::default();
        live.machine.profile = iris::config::MachineProfile::OctaneIp30;
        live.machine.cpu = iris::config::CpuModel::R10000;
        live.media.devices.insert(
            31,
            Medium {
                kind: MediaKind::Disk,
                path: "/tmp/last.raw".into(),
            },
        );
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        overlay.key(Key::Right, &live);
        overlay.key(Key::Up, &live); // Controller selector after target 15.
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Up, &live);
        assert_eq!(overlay.session.as_ref().unwrap().media_slot(), Some(31));
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Right, &live); // Cancel -> disk.
        assert_eq!(
            overlay.key(Key::Enter, &live),
            Effect::Pick {
                id: 31,
                kind: MediaKind::Disk
            }
        );
        overlay.key(Key::Escape, &live); // Cancel picker.
        overlay.key(Key::Left, &live);
        overlay.key(Key::Enter, &live); // Octane -> Indy; keep disk in the draft.
        let draft = &overlay.session.as_ref().unwrap().draft;
        assert!(draft.media.validate_for(draft.machine.profile).is_err());
        assert!(draft.media.devices.contains_key(&31));
        overlay.key(Key::Right, &live);
        overlay.key(Key::Up, &live); // Selector remains to allow disconnecting bus 1.
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Up, &live);
        let session = overlay.session.as_ref().unwrap();
        assert_eq!(session.media_slot(), Some(31));
        assert!(session.rows()[session.row].1.starts_with("Unavailable"));
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Left, &live); // Disconnect.
        overlay.key(Key::Enter, &live);
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
        assert!(live.media.devices.contains_key(&31));
    }

    #[test]
    fn network_editor_validates_and_discard_has_no_runtime_effects() {
        let live = Settings::default();
        let mut overlay = network_overlay(&live);
        overlay.key(Key::Down, &live);
        overlay.key(Key::Enter, &live);
        overlay.text_input("invalid");
        overlay.key(Key::Enter, &live);
        assert!(overlay.text_edit.as_ref().unwrap().error.is_some());
        assert_eq!(
            overlay.session.as_ref().unwrap().draft.network,
            live.network
        );
        overlay.key(Key::SelectAll, &live);
        overlay.text_input("10.23.0.0/24");
        overlay.key(Key::Enter, &live);
        assert!(overlay.text_edit.is_none());
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected Save");
        };
        assert!(!draft.machine_differs_from(&live));
        assert_eq!(draft.network.nat_subnet, "10.23.0.0/24");
        overlay.confirm_restart();
        overlay.key(Key::Enter, &live); // Cancel restart keeps draft.
        assert!(overlay.session.as_ref().unwrap().dirty());
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
    }

    #[test]
    fn network_pickers_only_edit_drafts_and_tinker_does_not_clear_settings() {
        let live = Settings::default();
        let mut overlay = network_overlay(&live);
        overlay.session.as_mut().unwrap().row = 3;
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Right, &live); // Cancel -> Choose
        assert_eq!(
            overlay.key(Key::Enter, &live),
            Effect::PickFolder(Folder::Nfs)
        );
        overlay.folder_picked(Folder::Nfs, "/tmp/share".into());
        assert!(live.network.nfs_dir.is_none());
        overlay.session.as_mut().unwrap().row = 0;
        overlay.key(Key::Enter, &live); // PCAP
        overlay.key(Key::Down, &live);
        overlay.key(Key::Down, &live);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::ListInterfaces);
        overlay.interfaces_listed(vec![Interface {
            name: "eth-test".into(),
            label: "Test interface".into(),
        }]);
        overlay.key(Key::Down, &live);
        overlay.key(Key::Enter, &live);
        assert_eq!(
            overlay
                .session
                .as_ref()
                .unwrap()
                .draft
                .network
                .pcap_interface
                .as_deref(),
            Some("eth-test")
        );
        for _ in 0..3 {
            overlay.key(Key::Left, &live);
        }
        for _ in 0..6 {
            overlay.key(Key::Down, &live);
        }
        overlay.key(Key::Enter, &live); // Hide Tinker.
        let session = overlay.session.as_ref().unwrap();
        assert!(!session.tabs().contains(&Tab::Network));
        assert_eq!(session.draft.network.nfs_dir.as_deref(), Some("/tmp/share"));
        assert!(session.draft.machine_differs_from(&live));
    }

    #[test]
    fn naming_and_selecting_machines_are_drafts_until_save() {
        let mut live = Settings::default();
        live.save_as_machine("Original").unwrap();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        overlay.session.as_mut().unwrap().row = 9;
        overlay.key(Key::Enter, &live);
        overlay.text_input("Original");
        overlay.key(Key::Enter, &live);
        assert!(overlay.text_edit.as_ref().unwrap().error.is_some());
        overlay.key(Key::Escape, &live);
        overlay.key(Key::Enter, &live);
        overlay.text_input("Second");
        overlay.key(Key::Enter, &live);
        assert!(
            overlay
                .session
                .as_ref()
                .unwrap()
                .draft
                .machines
                .contains_key("Second")
        );
        assert!(!live.machines.contains_key("Second"));
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
    }

    #[test]
    fn reset_requires_confirmation_and_does_not_save_or_discard_a_draft() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        assert_eq!(overlay.key(Key::Reset, &live), Effect::None);
        assert_eq!(overlay.dialog, Some(Dialog::Reset { confirm: false }));
        assert_eq!(overlay.key(Key::Enter, &live), Effect::None);
        overlay.key(Key::Reset, &live);
        overlay.key(Key::Left, &live);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::Reset);
        overlay.key(Key::Options, &live);
        overlay.key(Key::Enter, &live);
        assert_eq!(overlay.key(Key::Reset, &live), Effect::None);
        assert!(overlay.session.as_ref().unwrap().dirty());
        assert!(overlay.dialog.is_none());
    }

    #[test]
    fn firmware_selection_is_a_draft_and_survives_cancelled_restart() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        for _ in 0..3 {
            overlay.key(Key::Enter, &live);
        }
        let profile = iris::config::MachineProfile::O2Ip32;
        assert_eq!(
            overlay.session.as_ref().unwrap().draft.machine.profile,
            profile
        );
        for _ in 0..7 {
            overlay.key(Key::Down, &live);
        }
        overlay.key(Key::Enter, &live);
        assert_eq!(overlay.dialog, Some(Dialog::Firmware { choice: 2 }));
        overlay.key(Key::Right, &live);
        assert_eq!(overlay.key(Key::Enter, &live), Effect::PickProm);
        overlay.prom_picked(profile, "/tmp/ip32prom.bin".into());
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected Save")
        };
        assert_eq!(draft.machine.prom_path(), Some("/tmp/ip32prom.bin"));
        assert!(draft.machine_differs_from(&live));
        overlay.confirm_restart();
        overlay.key(Key::Enter, &live); // Cancel is the default.
        assert!(overlay.session.as_ref().unwrap().dirty());
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
    }

    #[test]
    fn audio_edits_are_drafts_and_do_not_require_a_machine_restart() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        for _ in 0..10 {
            overlay.key(Key::Down, &live);
        }
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Down, &live);
        overlay.key(Key::Enter, &live);
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected Save")
        };
        assert!(draft.audio.muted);
        assert_eq!(draft.audio.volume, 90);
        assert!(!draft.machine_differs_from(&live));
        assert!(!live.audio.muted);
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft.audio, live.audio);
    }

    #[test]
    fn bundled_firmware_selection_stays_in_the_draft_until_saved() {
        let mut live = Settings::default();
        live.machine.profile = iris::config::MachineProfile::OctaneIp30;
        live.machine.cpu = iris::config::CpuModel::R10000;
        live.machine.set_prom(Some("/missing/override.bin".into()));
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        for _ in 0..7 {
            overlay.key(Key::Down, &live);
        }
        overlay.key(Key::Enter, &live);
        overlay.key(Key::Left, &live); // Cancel -> Use bundled.
        overlay.key(Key::Enter, &live);
        let draft = &mut overlay.session.as_mut().unwrap().draft;
        assert_eq!(draft.machine.prom_path(), None);
        draft.machine.validate_prom().unwrap();
        assert!(draft.machine_differs_from(&live));
        assert_eq!(live.machine.prom_path(), Some("/missing/override.bin"));
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.as_ref().unwrap().draft, live);
    }

    #[test]
    fn unsupported_advanced_overrides_do_not_change_on_new_models() {
        let mut live = Settings {
            tinker: true,
            ..Default::default()
        };
        live.machine.profile = iris::config::MachineProfile::O2Ip32;
        live.machine.cpu = iris::config::CpuModel::R5000;
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Right, Key::Right] {
            overlay.key(key, &live);
        }
        for _ in 0..4 {
            overlay.key(Key::Enter, &live);
            overlay.key(Key::Down, &live);
        }
        assert!(!overlay.session.as_ref().unwrap().dirty());
    }

    #[test]
    fn saved_window_scale_preserves_clean_and_dirty_menu_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prismatic.cfg");
        let mut live = Settings::default();
        let mut overlay = Overlay::default();
        overlay.key(Key::Options, &live);
        let scale = live.window_scale.step(true);
        live.save_window_scale(&path, scale).unwrap();
        overlay.window_scale_saved(scale);
        assert!(!overlay.session.as_ref().unwrap().dirty());
        overlay.key(Key::Escape, &live);
        assert!(overlay.session.is_none());

        overlay.key(Key::Options, &live);
        overlay.key(Key::Down, &live);
        overlay.key(Key::Enter, &live);
        let draft_ram = overlay.session.as_ref().unwrap().draft.machine.ram_mb;
        let scale = scale.step(true);
        live.save_window_scale(&path, scale).unwrap();
        overlay.window_scale_saved(scale);
        assert!(overlay.session.as_ref().unwrap().dirty());
        assert_eq!(Settings::load(&path).unwrap(), live);
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected a Save draft");
        };
        assert_eq!(draft.window_scale, scale);
        assert_eq!(draft.machine.ram_mb, draft_ram);
        overlay.key(Key::Escape, &live);
        overlay.key(Key::Left, &live); // Discard.
        overlay.key(Key::Enter, &live);
        assert!(overlay.session.is_none());
        assert_eq!(Settings::load(&path).unwrap(), live);
        assert_eq!(live.window_scale, scale);
        assert_eq!(live.machine, Settings::default().machine);
    }

    #[test]
    fn debug_changes_are_drafts_until_saved_and_discard_restores_output() {
        use crate::settings::DebugOutput;
        let live = Settings {
            tinker: true,
            ..Default::default()
        };
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Right, Key::Right] {
            overlay.key(key, &live);
        }
        let session = overlay.session.as_mut().unwrap();
        session.row = session
            .rows()
            .iter()
            .position(|(label, _)| *label == "Debug output")
            .unwrap();
        overlay.key(Key::Enter, &live);
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
    fn gif_options_are_saved_as_a_draft_without_restarting() {
        let live = Settings {
            tinker: true,
            ..Default::default()
        };
        let mut overlay = Overlay::default();
        for key in [Key::Options, Key::Right, Key::Right] {
            overlay.key(key, &live);
        }
        for label in ["GIF resolution", "GIF frame rate", "GIF encoder"] {
            let session = overlay.session.as_mut().unwrap();
            session.row = session
                .rows()
                .iter()
                .position(|(name, _)| *name == label)
                .unwrap();
            overlay.key(Key::Enter, &live);
        }
        let Effect::Save(draft) = overlay.key(Key::Options, &live) else {
            panic!("Expected save");
        };
        assert_eq!(draft.gif.width, 320);
        assert_eq!(draft.gif.fps, 10);
        assert_eq!(draft.gif.encoder, crate::settings::GifEncoder::Ffmpeg);
        assert!(!draft.machine_differs_from(&live));
        assert_eq!(live.gif, Default::default());
        overlay.save_failed();
        for key in [Key::Escape, Key::Left, Key::Enter, Key::Options] {
            overlay.key(key, &live);
        }
        assert_eq!(overlay.session.unwrap().draft.gif, live.gif);
    }

    #[test]
    fn advanced_changes_remain_drafts_survive_hiding_and_can_be_discarded() {
        let live = Settings::default();
        let mut overlay = Overlay::default();
        for key in [
            Key::Options,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Enter,
            Key::Right,
            Key::Right,
        ] {
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
        for key in [
            Key::Left,
            Key::Left,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Enter,
        ] {
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
        for key in [
            Key::Right,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Enter,
            Key::Right,
            Key::Right,
        ] {
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
