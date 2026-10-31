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
