//! A repeatable SDL-event route through the embedded diskless PROM.
//! The resulting screenshot still needs visual inspection; this is not OCR.
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
