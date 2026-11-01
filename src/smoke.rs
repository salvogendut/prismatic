//! A repeatable SDL-event route through the embedded diskless PROM.
//! The resulting screenshot still needs visual inspection; this is not OCR.
use crate::{
    overlay::{Dialog, Overlay},
    settings::Settings,
};
use iris::config::MachineProfile;
use sdl3::{
    EventSubsystem,
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod, Scancode},
    mouse::{MouseButton, MouseState},
    video::Window,
};
use std::time::Duration;

#[derive(Default)]
pub struct InputSmoke {
    step: usize,
}

pub struct SettingsSmokeState<'a> {
    pub generation: u32,
    pub elapsed: Option<Duration>,
    pub settings: &'a Settings,
    pub overlay: &'a Overlay,
    pub captured: bool,
}

#[derive(Default)]
pub struct SettingsSmoke {
    step: usize,
    input: InputSmoke,
}

impl SettingsSmoke {
    pub fn advance(
        &mut self,
        state: SettingsSmokeState<'_>,
        events: &EventSubsystem,
        window: &Window,
    ) -> Result<bool, String> {
        let Some(elapsed) = state.elapsed else {
            return Ok(false);
        };
        if state.generation == 1 {
            if self.step != 4
                || state.settings.machine.profile != MachineProfile::Indigo2Ip22
                || state.settings.machine.ram_mb != 128
            {
                return Err("Settings smoke restarted with unexpected hardware".into());
            }
            return self.input.advance(
                elapsed,
                events,
                window,
                state.captured,
                state.overlay.session.is_some() || state.overlay.dialog.is_some(),
            );
        }
        if state.generation != 0 {
            return Err("Settings smoke restarted more than once".into());
        }
        if self.step >= 4 || elapsed < Duration::from_secs(10 + self.step as u64) {
            return Ok(false);
        }
        if state.settings.machine.ram_mb != 64
            || state.settings.machine.profile != MachineProfile::IndyIp24
        {
            return Err("Settings smoke changed live hardware before confirmation".into());
        }
        use Scancode as S;
        let keys: &[Scancode] = match self.step {
            // Try a RAM change, cancel restart, then discard the draft.
            0 => &[S::F9, S::Down, S::Return, S::F9],
            1 => {
                if state.overlay.dialog != Some(Dialog::Restart { confirm: false }) {
                    return Err(
                        "Settings smoke expected restart confirmation with Cancel selected".into(),
                    );
                }
                &[S::Return, S::Escape, S::Left, S::Return]
            }
            2 => {
                if state.overlay.session.is_some() || state.overlay.dialog.is_some() {
                    return Err("Settings smoke did not discard the hardware draft".into());
                }
                // Change model and RAM together, then save and confirm restart.
                &[S::F9, S::Return, S::Down, S::Return, S::F9]
            }
            _ => {
                if state.overlay.dialog != Some(Dialog::Restart { confirm: false }) {
                    return Err(
                        "Settings smoke expected hardware confirmation before restart".into(),
                    );
                }
                &[S::Left, S::Return]
            }
        };
        for &scan in keys {
            tap(events, window, scan)?;
        }
        eprintln!("Prismatic: settings smoke step {} complete", self.step);
        self.step += 1;
        Ok(false)
    }
}

fn tap(events: &EventSubsystem, window: &Window, scan: Scancode) -> Result<(), String> {
    for down in [true, false] {
        let event = if down {
            Event::KeyDown {
                timestamp: 0,
                window_id: window.id(),
                keycode: Keycode::from_scancode(scan, Default::default(), false),
                scancode: Some(scan),
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: window.id(),
                keycode: Keycode::from_scancode(scan, Default::default(), false),
                scancode: Some(scan),
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            }
        };
        events.push_event(event).map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl InputSmoke {
    pub fn advance(
        &mut self,
        elapsed: Duration,
        events: &EventSubsystem,
        window: &Window,
        captured: bool,
        modal: bool,
    ) -> Result<bool, String> {
        // Allow firmware transitions to finish between input bursts.
        let times = [10, 11, 14, 15, 18, 21, 22, 23, 24, 25, 26];
        if self.step >= times.len() {
            return Ok(true);
        }
        if elapsed < Duration::from_secs(times[self.step]) {
            return Ok(false);
        }
        let id = window.id();
        let push = |event| events.push_event(event).map_err(|e| e.to_string());
        let key = |scan, down, modifiers| {
            push(if down {
                Event::KeyDown {
                    timestamp: 0,
                    window_id: id,
                    keycode: Keycode::from_scancode(scan, Default::default(), false),
                    scancode: Some(scan),
                    keymod: modifiers,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }
            } else {
                Event::KeyUp {
                    timestamp: 0,
                    window_id: id,
                    keycode: Keycode::from_scancode(scan, Default::default(), false),
                    scancode: Some(scan),
                    keymod: modifiers,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }
            })
        };
        let tap = |scan, modifiers| -> Result<(), String> {
            key(scan, true, modifiers)?;
            key(scan, false, modifiers)
        };
        let click = || -> Result<(), String> {
            let (w, h) = window.size();
            let (x, y) = (w as f32 / 2.0, h as f32 / 2.0);
            push(Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            })?;
            push(Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            })
        };
        if matches!(self.step, 1 | 2 | 3 | 4 | 5 | 7 | 9) && !captured {
            return Err(format!(
                "Input smoke step {} requires mouse capture",
                self.step
            ));
        }
        if matches!(self.step, 6 | 8 | 10) && captured {
            return Err(format!(
                "Input smoke step {} failed to release capture",
                self.step
            ));
        }
        use Scancode as S;
        match self.step {
            0 => click()?,
            1 => tap(S::Return, Mod::NOMOD)?, // Dismiss the diskless boot error.
            2 => push(Event::MouseMotion {
                timestamp: 0,
                window_id: id,
                which: 0,
                mousestate: MouseState::from_sdl_state(0),
                x: 0.0,
                y: 0.0,
                xrel: 100.0,
                yrel: 50.0,
            })?,
            3 => {
                tap(S::_5, Mod::NOMOD)?;
                tap(S::Return, Mod::NOMOD)?;
            }
            4 => {
                // Enter Command Monitor, then query the emulated hardware.
                for scan in [S::H, S::I, S::N, S::V, S::Return] {
                    tap(scan, Mod::NOMOD)?;
                }
            }
            5 => {
                key(S::LCtrl, true, Mod::LCTRLMOD)?;
                tap(S::Return, Mod::LCTRLMOD)?;
                key(S::LCtrl, false, Mod::NOMOD)?;
            }
            6 => click()?,
            7 => {
                key(S::LCtrl, true, Mod::LCTRLMOD)?;
                tap(S::F9, Mod::LCTRLMOD)?;
            }
            8 => {
                if !modal {
                    return Err("Input smoke did not open F9 options".into());
                }
                key(S::LCtrl, false, Mod::NOMOD)?;
                tap(S::Escape, Mod::NOMOD)?;
                click()?;
            }
            9 => {
                key(S::LShift, true, Mod::LSHIFTMOD)?;
                push(Event::Window {
                    timestamp: 0,
                    window_id: id,
                    win_event: WindowEvent::FocusLost,
                })?;
                key(S::LShift, false, Mod::NOMOD)?;
            }
            _ => {}
        }
        eprintln!("Prismatic: SDL input smoke step {} complete", self.step);
        self.step += 1;
        Ok(self.step == times.len())
    }
}

/// Real menu/restart/SCSI integration with disposable sector fixtures. Native
/// dialogs are replaced with known paths; image inspection still runs normally.
pub struct MediaSmoke {
    pub expected: crate::media::MediaSettings,
    step: usize,
    input: InputSmoke,
}

impl MediaSmoke {
    pub fn new(directory: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        use crate::media::{MediaKind, MediaSettings, Medium};
        let mut expected = MediaSettings::default();
        for (id, kind, name) in [
            (1, MediaKind::Disk, "disk.raw"),
            (4, MediaKind::Cdrom, "iso9660.iso"),
            (5, MediaKind::Cdrom, "efs.img"),
        ] {
            let path = directory.join(name);
            std::fs::write(&path, vec![0; 65536])?;
            expected.devices.insert(id, Medium::inspect(&path, kind)?);
        }
        let path = directory.join("disk.chd");
        libchdman_rs::hd::create_from_reader(
            &[0; 65536][..],
            &path,
            libchdman_rs::hd::HdCreateOptions {
                logical_size: 65536,
                ..Default::default()
            },
            &mut |_| {},
            &|| false,
        )
        .map_err(|e| format!("CHD fixture: {e:?}"))?;
        expected
            .devices
            .insert(2, Medium::inspect(&path, MediaKind::Disk)?);
        let path = directory.join("cd.chd");
        libchdman_rs::cd::create_from_iso(
            &directory.join("iso9660.iso"),
            &path,
            Default::default(),
            &mut |_| {},
            &|| false,
        )
        .map_err(|e| format!("CD fixture: {e:?}"))?;
        expected
            .devices
            .insert(6, Medium::inspect(&path, MediaKind::Cdrom)?);
        Ok(Self {
            expected,
            step: 0,
            input: InputSmoke::default(),
        })
    }

    pub fn advance(
        &mut self,
        state: SettingsSmokeState<'_>,
        events: &EventSubsystem,
        window: &Window,
    ) -> Result<bool, String> {
        let Some(elapsed) = state.elapsed else {
            return Ok(false);
        };
        if state.generation == 1 {
            if self.step != 7 || state.settings.media != self.expected {
                return Err("Media smoke restarted with unexpected devices".into());
            }
            return self.input.advance(
                elapsed,
                events,
                window,
                state.captured,
                state.overlay.session.is_some() || state.overlay.dialog.is_some(),
            );
        }
        if state.generation != 0 {
            return Err("Media smoke restarted more than once".into());
        }
        if state.overlay.busy
            || self.step >= 7
            || elapsed < Duration::from_secs(10 + self.step as u64)
        {
            return Ok(false);
        }
        if !state.settings.media.devices.is_empty() {
            return Err("Media applied before confirmation".into());
        }
        if self.step > 0 && self.step <= 5 {
            let id = [1, 2, 4, 5, 6][self.step - 1];
            let draft = &state
                .overlay
                .session
                .as_ref()
                .ok_or("Media editor closed unexpectedly")?
                .draft
                .media;
            if draft.devices.get(&id) != self.expected.devices.get(&id) {
                return Err(format!("Media smoke: missing selection for SCSI {id}"));
            }
        }
        use Scancode as S;
        let keys: &[Scancode] = match self.step {
            0 => &[S::F9, S::Right, S::Return, S::Right, S::Return],
            1 => &[S::Down, S::Return, S::Right, S::Return],
            2 => &[S::Down, S::Down, S::Return, S::Right, S::Right, S::Return],
            3 | 4 => &[S::Down, S::Return, S::Right, S::Right, S::Return],
            5 => &[S::F9],
            _ => {
                if state.overlay.dialog != Some(Dialog::Restart { confirm: false }) {
                    return Err("Media save did not ask to restart".into());
                }
                &[S::Left, S::Return]
            }
        };
        for &scan in keys {
            tap(events, window, scan)?;
        }
        eprintln!("Prismatic: media smoke step {} complete", self.step);
        self.step += 1;
        Ok(false)
    }
}

#[derive(Default)]
pub struct AdvancedSmoke {
    step: usize,
    input: InputSmoke,
    showing_menu: bool,
}

impl AdvancedSmoke {
    fn expected() -> crate::settings::AdvancedSettings {
        crate::settings::AdvancedSettings {
            resolution: iris::dev::ng1::vc2_timings::NewportResolution::Res1024x768,
            cache: iris::config::CacheModel::Shadow,
            clock_offset_years: -5,
            scsi_deferred_interrupts: false,
            serial_logging: crate::settings::SerialLogging::Timestamped,
        }
    }

    pub fn advance(
        &mut self,
        state: SettingsSmokeState<'_>,
        events: &EventSubsystem,
        window: &Window,
    ) -> Result<bool, String> {
        use Scancode as S;
        let Some(elapsed) = state.elapsed else {
            return Ok(false);
        };
        if state.generation == 1 {
            if self.step != 4
                || state.settings.advanced != Self::expected()
                || state.settings.tinker
            {
                return Err("Advanced smoke restarted with unexpected options".into());
            }
            if !self.input.advance(
                elapsed,
                events,
                window,
                state.captured,
                state.overlay.session.is_some() || state.overlay.dialog.is_some(),
            )? {
                return Ok(false);
            }
            // Reopen Advanced for the screenshot. Showing it is a draft change;
            // persisted Tinker remains off and the core settings remain active.
            if !self.showing_menu {
                for scan in [
                    S::F9,
                    S::Down,
                    S::Down,
                    S::Down,
                    S::Down,
                    S::Down,
                    S::Down,
                    S::Return,
                    S::Right,
                    S::Right,
                ] {
                    tap(events, window, scan)?;
                }
                self.showing_menu = true;
                return Ok(false);
            }
            let session = state
                .overlay
                .session
                .as_ref()
                .ok_or("Advanced smoke did not reopen options")?;
            if session.tab != crate::overlay::Tab::Advanced
                || session.draft.advanced != Self::expected()
            {
                return Err("Advanced smoke did not retain hidden options".into());
            }
            return Ok(elapsed >= Duration::from_secs(28));
        }
        if state.generation != 0 {
            return Err("Advanced smoke restarted more than once".into());
        }
        if state.overlay.busy
            || self.step >= 4
            || elapsed < Duration::from_secs(10 + self.step as u64)
        {
            return Ok(false);
        }
        if state.settings.advanced != Default::default() || state.settings.tinker {
            return Err("Advanced options applied before confirmation".into());
        }
        let edit = [
            S::F9,
            S::Down,
            S::Down,
            S::Down,
            S::Down,
            S::Down,
            S::Down,
            S::Return,
            S::Right,
            S::Right,
            S::Return,
            S::Down,
            S::Return,
            S::Return,
            S::Down,
            S::Return,
            S::Down,
            S::Return,
            S::Down,
            S::Return,
            S::Return,
        ];
        match self.step {
            0 | 2 => {
                if self.step == 2
                    && (state.overlay.session.is_some() || state.overlay.dialog.is_some())
                {
                    return Err("Advanced smoke failed to discard options".into());
                }
                for scan in edit {
                    tap(events, window, scan)?;
                }
                // Second attempt: hide Tinker before saving all five options.
                if self.step == 2 {
                    for scan in [
                        S::Left,
                        S::Left,
                        S::Down,
                        S::Down,
                        S::Down,
                        S::Down,
                        S::Down,
                        S::Down,
                        S::Return,
                    ] {
                        tap(events, window, scan)?;
                    }
                }
                tap(events, window, S::F9)?;
            }
            1 | 3 => {
                if state.overlay.dialog != Some(Dialog::Restart { confirm: false }) {
                    return Err("Advanced smoke expected restart confirmation".into());
                }
                let draft = &state.overlay.session.as_ref().unwrap().draft;
                if draft.advanced != Self::expected() {
                    return Err("Advanced smoke edits did not reach the draft".into());
                }
                let keys: &[Scancode] = if self.step == 1 {
                    &[S::Return, S::Escape, S::Left, S::Return] // Cancel, discard.
                } else {
                    &[S::Left, S::Return]
                };
                for &scan in keys {
                    tap(events, window, scan)?;
                }
            }
            _ => unreachable!(),
        }
        eprintln!("Prismatic: advanced smoke step {} complete", self.step);
        self.step += 1;
        Ok(false)
    }
}
