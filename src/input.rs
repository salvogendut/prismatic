//! Physical host input and guest-held state, independent of window capture APIs.
use crate::overlay::Key;
use iris::dev::ps2::Ps2Controller;
use sdl3::{
    keyboard::{Keycode, Mod, Scancode},
    mouse::MouseButton,
};
use std::{collections::HashSet, sync::Arc};
use winit::keyboard::KeyCode;

#[derive(Debug, PartialEq, Eq)]
pub enum KeyRoute {
    Guest,
    FunctionTap,
    Host(Key),
    Fullscreen,
    Screenshot,
    Gif,
    Pause,
    WindowScale(bool),
    Release,
    Ignore,
}

/// Use host key symbols so plus/minus also work on non-US layouts. Accept
/// Ctrl+= as the unshifted shortcut, and keep AltGr / desktop chords separate.
pub fn scale_shortcut(
    scan: Scancode,
    key: Option<Keycode>,
    modifiers: Mod,
    repeat: bool,
) -> Option<KeyRoute> {
    if !modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
        || modifiers.intersects(Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD)
    {
        return None;
    }
    let increase = match key {
        Some(Keycode::Plus | Keycode::Equals | Keycode::KpPlus) => true,
        Some(Keycode::Minus | Keycode::Underscore | Keycode::KpMinus) => false,
        None => match scan {
            Scancode::Equals | Scancode::KpPlus => true,
            Scancode::Minus | Scancode::KpMinus => false,
            _ => return None,
        },
        _ => return None,
    };
    Some(if repeat {
        KeyRoute::Ignore
    } else {
        KeyRoute::WindowScale(increase)
    })
}

pub fn route_key(
    scan: Scancode,
    modifiers: Mod,
    repeat: bool,
    captured: bool,
    modal: bool,
) -> KeyRoute {
    use Scancode as S;
    if scan == S::P
        && modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
        && !modifiers.intersects(Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD)
    {
        return if repeat || modal {
            KeyRoute::Ignore
        } else {
            KeyRoute::Pause
        };
    }
    let reserved = matches!(
        scan,
        S::F2 | S::F4 | S::F5 | S::F6 | S::F9 | S::F11 | S::F12
    );
    if captured
        && !modal
        && modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
        && matches!(scan, S::Return | S::KpEnter)
    {
        return if repeat {
            KeyRoute::Ignore
        } else {
            KeyRoute::Release
        };
    }
    if captured && !modal && reserved && modifiers.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
        return if repeat {
            KeyRoute::Ignore
        } else {
            KeyRoute::FunctionTap
        };
    }
    if reserved {
        return if repeat {
            KeyRoute::Ignore
        } else {
            match scan {
                S::F2 => KeyRoute::Host(Key::NextMachine),
                S::F4 => KeyRoute::Screenshot,
                S::F5 => KeyRoute::Host(Key::Reset),
                S::F6 => KeyRoute::Gif,
                S::F9 => KeyRoute::Host(Key::Options),
                S::F12 => KeyRoute::Host(Key::Quit),
                _ => KeyRoute::Fullscreen,
            }
        };
    }
    if modal {
        if scan == S::A && modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
            return KeyRoute::Host(Key::SelectAll);
        }
        if repeat && !matches!(scan, S::Left | S::Right | S::Up | S::Down) {
            return KeyRoute::Ignore;
        }
        return match scan {
            S::Escape => KeyRoute::Host(Key::Escape),
            S::Backspace => KeyRoute::Host(Key::Backspace),
            S::Return | S::KpEnter => KeyRoute::Host(Key::Enter),
            S::Left => KeyRoute::Host(Key::Left),
            S::Right => KeyRoute::Host(Key::Right),
            S::Up => KeyRoute::Host(Key::Up),
            S::Down => KeyRoute::Host(Key::Down),
            _ => KeyRoute::Ignore,
        };
    }
    if captured {
        KeyRoute::Guest
    } else {
        KeyRoute::Ignore
    }
}

#[derive(Default)]
pub struct GuestInput {
    controller: Option<Arc<Ps2Controller>>,
    pub captured: bool,
    held: HashSet<Scancode>,
    buttons: u8,
    motion: (f32, f32),
    wheel: f32,
}

impl GuestInput {
    pub fn attach(&mut self, controller: Option<Arc<Ps2Controller>>) {
        let unchanged = match (&self.controller, &controller) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.release();
            self.controller = controller;
        }
    }

    pub fn available(&self) -> bool {
        self.controller.is_some()
    }

    pub fn capture(&mut self) {
        if self.available() {
            self.captured = true;
        }
    }

    pub fn release(&mut self) {
        if let Some(controller) = &self.controller {
            for scan in self.held.drain() {
                if let Some(key) = physical_key(scan) {
                    controller.push_kb(key, false);
                }
            }
            if self.buttons != 0 {
                controller.push_mouse_input(0, 0, 0, 0);
            }
        }
        self.held.clear();
        self.buttons = 0;
        self.motion = (0.0, 0.0);
        self.wheel = 0.0;
        self.captured = false;
    }

    pub fn key(&mut self, scan: Scancode, pressed: bool, repeat: bool) {
        if !self.captured {
            return;
        }
        let Some(key) = physical_key(scan) else {
            return;
        };
        let Some(controller) = &self.controller else {
            return;
        };
        if pressed {
            // A repeat from a key held before capture must not create a phantom
            // press. Iris handles the guest's typematic policy for held keys.
            if repeat && !self.held.contains(&scan) {
                return;
            }
            if !repeat && !self.held.insert(scan) {
                return;
            }
            controller.push_kb(key, true);
        } else if self.held.remove(&scan) {
            controller.push_kb(key, false);
        }
    }

    /// Shift + a host-reserved function key sends one bare tap. Keep physical
    /// held state unchanged so releasing Shift before/after Fn stays balanced.
    pub fn function_tap(&self, scan: Scancode) {
        if !self.captured {
            return;
        }
        let (Some(controller), Some(key)) = (&self.controller, physical_key(scan)) else {
            return;
        };
        let modifiers: Vec<_> = self
            .held
            .iter()
            .filter_map(|&scan| {
                matches!(
                    scan,
                    Scancode::LShift
                        | Scancode::RShift
                        | Scancode::LCtrl
                        | Scancode::RCtrl
                        | Scancode::LAlt
                        | Scancode::RAlt
                        | Scancode::LGui
                        | Scancode::RGui
                )
                .then(|| physical_key(scan))
                .flatten()
            })
            .collect();
        for &key in &modifiers {
            controller.push_kb(key, false);
        }
        controller.push_kb(key, true);
        controller.push_kb(key, false);
        for &key in &modifiers {
            controller.push_kb(key, true);
        }
    }

    pub fn button(&mut self, button: MouseButton, pressed: bool) {
        if !self.captured {
            return;
        }
        let bit = match button {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 4,
            MouseButton::X1 => 8,
            MouseButton::X2 => 16,
            _ => return,
        };
        let buttons = if pressed {
            self.buttons | bit
        } else {
            self.buttons & !bit
        };
        if buttons != self.buttons {
            self.buttons = buttons;
            self.send_mouse(0, 0, 0);
        }
    }

    pub fn motion(&mut self, dx: f32, dy: f32) {
        if !self.captured || !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.motion.0 += dx.clamp(-4096.0, 4096.0);
        self.motion.1 += dy.clamp(-4096.0, 4096.0);
        let (mut x, mut y) = (self.motion.0.trunc() as i32, self.motion.1.trunc() as i32);
        self.motion.0 -= x as f32;
        self.motion.1 -= y as f32;
        // Iris accepts screen-down-positive Y and does the PS/2 inversion.
        // Split large moves before Iris's per-packet clamp can lose distance.
        while x != 0 || y != 0 {
            let (dx, dy) = (x.clamp(-255, 255), y.clamp(-255, 255));
            self.send_mouse(dx, dy, 0);
            x -= dx;
            y -= dy;
        }
    }

    pub fn scroll(&mut self, lines: f32) {
        if !self.captured || !lines.is_finite() {
            return;
        }
        self.wheel += lines.clamp(-64.0, 64.0);
        let mut steps = self.wheel.trunc() as i32;
        self.wheel -= steps as f32;
        while steps != 0 {
            let dz = steps.clamp(-7, 7);
            self.send_mouse(0, 0, dz);
            steps -= dz;
        }
    }

    fn send_mouse(&self, dx: i32, dy: i32, dz: i32) {
        if let Some(controller) = &self.controller {
            controller.push_mouse_input(self.buttons, dx, dy, dz);
        }
    }
}

impl Drop for GuestInput {
    fn drop(&mut self) {
        self.release();
    }
}

pub fn physical_key(scan: Scancode) -> Option<KeyCode> {
    use KeyCode as K;
    use Scancode as S;
    Some(match scan {
        S::A => K::KeyA,
        S::B => K::KeyB,
        S::C => K::KeyC,
        S::D => K::KeyD,
        S::E => K::KeyE,
        S::F => K::KeyF,
        S::G => K::KeyG,
        S::H => K::KeyH,
        S::I => K::KeyI,
        S::J => K::KeyJ,
        S::K => K::KeyK,
        S::L => K::KeyL,
        S::M => K::KeyM,
        S::N => K::KeyN,
        S::O => K::KeyO,
        S::P => K::KeyP,
        S::Q => K::KeyQ,
        S::R => K::KeyR,
        S::S => K::KeyS,
        S::T => K::KeyT,
        S::U => K::KeyU,
        S::V => K::KeyV,
        S::W => K::KeyW,
        S::X => K::KeyX,
        S::Y => K::KeyY,
        S::Z => K::KeyZ,
        S::_0 => K::Digit0,
        S::_1 => K::Digit1,
        S::_2 => K::Digit2,
        S::_3 => K::Digit3,
        S::_4 => K::Digit4,
        S::_5 => K::Digit5,
        S::_6 => K::Digit6,
        S::_7 => K::Digit7,
        S::_8 => K::Digit8,
        S::_9 => K::Digit9,
        S::Return => K::Enter,
        S::Escape => K::Escape,
        S::Backspace => K::Backspace,
        S::Tab => K::Tab,
        S::Space => K::Space,
        S::Minus => K::Minus,
        S::Equals => K::Equal,
        S::LeftBracket => K::BracketLeft,
        S::RightBracket => K::BracketRight,
        S::Backslash | S::NonUsHash => K::Backslash,
        S::NonUsBackslash => K::IntlBackslash,
        S::Semicolon => K::Semicolon,
        S::Apostrophe => K::Quote,
        S::Grave => K::Backquote,
        S::Comma => K::Comma,
        S::Period => K::Period,
        S::Slash => K::Slash,
        S::CapsLock => K::CapsLock,
        S::F1 => K::F1,
        S::F2 => K::F2,
        S::F3 => K::F3,
        S::F4 => K::F4,
        S::F5 => K::F5,
        S::F6 => K::F6,
        S::F7 => K::F7,
        S::F8 => K::F8,
        S::F9 => K::F9,
        S::F10 => K::F10,
        S::F11 => K::F11,
        S::F12 => K::F12,
        S::PrintScreen => K::PrintScreen,
        S::ScrollLock => K::ScrollLock,
        S::Pause => K::Pause,
        S::Insert => K::Insert,
        S::Delete => K::Delete,
        S::Home => K::Home,
        S::End => K::End,
        S::PageUp => K::PageUp,
        S::PageDown => K::PageDown,
        S::Up => K::ArrowUp,
        S::Down => K::ArrowDown,
        S::Left => K::ArrowLeft,
        S::Right => K::ArrowRight,
        S::NumLockClear => K::NumLock,
        S::Kp0 => K::Numpad0,
        S::Kp1 => K::Numpad1,
        S::Kp2 => K::Numpad2,
        S::Kp3 => K::Numpad3,
        S::Kp4 => K::Numpad4,
        S::Kp5 => K::Numpad5,
        S::Kp6 => K::Numpad6,
        S::Kp7 => K::Numpad7,
        S::Kp8 => K::Numpad8,
        S::Kp9 => K::Numpad9,
        S::KpDivide => K::NumpadDivide,
        S::KpMultiply => K::NumpadMultiply,
        S::KpMinus => K::NumpadSubtract,
        S::KpPlus => K::NumpadAdd,
        S::KpEnter => K::NumpadEnter,
        S::KpPeriod => K::NumpadDecimal,
        S::KpEquals => K::NumpadEqual,
        S::LCtrl => K::ControlLeft,
        S::RCtrl => K::ControlRight,
        S::LShift => K::ShiftLeft,
        S::RShift => K::ShiftRight,
        S::LAlt => K::AltLeft,
        S::RAlt => K::AltRight,
        S::LGui => K::SuperLeft,
        S::RGui => K::SuperRight,
        S::Application => K::ContextMenu,
        S::International1 => K::IntlRo,
        S::International2 => K::KanaMode,
        S::International3 => K::IntlYen,
        S::International4 => K::Convert,
        S::International5 => K::NonConvert,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris::{config::KeyboardLayout, traits::Device};

    #[test]
    fn scaling_uses_host_symbols_and_excludes_repeats_and_altgr() {
        for (scan, key, increase) in [
            (Scancode::Equals, Keycode::Equals, true),
            (Scancode::Equals, Keycode::Plus, true),
            (Scancode::RightBracket, Keycode::Plus, true),
            (Scancode::KpPlus, Keycode::KpPlus, true),
            (Scancode::Minus, Keycode::Minus, false),
            (Scancode::Minus, Keycode::Underscore, false),
            (Scancode::KpMinus, Keycode::KpMinus, false),
        ] {
            for modifiers in [Mod::LCTRLMOD, Mod::RCTRLMOD | Mod::LSHIFTMOD] {
                assert_eq!(
                    scale_shortcut(scan, Some(key), modifiers, false),
                    Some(KeyRoute::WindowScale(increase))
                );
                assert_eq!(
                    scale_shortcut(scan, Some(key), modifiers, true),
                    Some(KeyRoute::Ignore)
                );
            }
            for modifiers in [
                Mod::NOMOD,
                Mod::LCTRLMOD | Mod::RALTMOD,
                Mod::RCTRLMOD | Mod::LGUIMOD,
            ] {
                assert_eq!(scale_shortcut(scan, Some(key), modifiers, false), None);
            }
        }
        assert_eq!(
            scale_shortcut(Scancode::Equals, Some(Keycode::A), Mod::LCTRLMOD, false),
            None
        );
        assert_eq!(
            scale_shortcut(Scancode::KpPlus, None, Mod::RCTRLMOD, false),
            Some(KeyRoute::WindowScale(true))
        );
    }

    fn drain(controller: &Ps2Controller, mouse: bool) -> Vec<u8> {
        std::iter::from_fn(|| controller.pop_from(mouse)).collect()
    }

    fn connected() -> (GuestInput, Arc<Ps2Controller>) {
        let controller = Arc::new(Ps2Controller::new(None));
        controller.start();
        // Direct device ports, set 3, typematic and break codes enabled.
        for byte in [0xf0, 3, 0xfa, 0xf4] {
            controller.write_to(byte, false);
        }
        controller.write_to(0xf4, true);
        drain(&controller, false);
        drain(&controller, true);
        let mut input = GuestInput::default();
        input.attach(Some(controller.clone()));
        input.capture();
        (input, controller)
    }

    #[test]
    fn host_shortcuts_and_modal_navigation_do_not_reach_guest() {
        use KeyRoute as R;
        use Scancode as S;
        assert_eq!(route_key(S::A, Mod::NOMOD, false, false, false), R::Ignore);
        assert_eq!(
            route_key(S::Escape, Mod::NOMOD, false, true, false),
            R::Guest
        );
        assert_eq!(
            route_key(S::F9, Mod::NOMOD, false, true, false),
            R::Host(Key::Options)
        );
        assert_eq!(
            route_key(S::F12, Mod::LSHIFTMOD, false, true, false),
            R::FunctionTap
        );
        assert_eq!(
            route_key(S::F12, Mod::LSHIFTMOD, true, true, false),
            R::Ignore
        );
        assert_eq!(
            route_key(S::F12, Mod::LSHIFTMOD, false, true, true),
            R::Host(Key::Quit)
        );
        assert_eq!(
            route_key(S::Return, Mod::RCTRLMOD, false, true, false),
            R::Release
        );
        assert_eq!(
            route_key(S::Return, Mod::RCTRLMOD, false, false, true),
            R::Host(Key::Enter)
        );
        assert_eq!(
            route_key(S::Down, Mod::NOMOD, true, false, true),
            R::Host(Key::Down)
        );
        assert_eq!(
            route_key(S::Return, Mod::NOMOD, true, false, true),
            R::Ignore
        );
        assert_eq!(route_key(S::A, Mod::NOMOD, false, true, true), R::Ignore);
    }

    #[test]
    fn capture_keys_work_in_menus_ignore_repeats_and_allow_shift_guest_taps() {
        for (scan, expected) in [
            (Scancode::F2, KeyRoute::Host(Key::NextMachine)),
            (Scancode::F5, KeyRoute::Host(Key::Reset)),
            (Scancode::F4, KeyRoute::Screenshot),
            (Scancode::F6, KeyRoute::Gif),
        ] {
            for captured in [false, true] {
                for modal in [false, true] {
                    assert_eq!(
                        route_key(scan, Mod::NOMOD, false, captured, modal),
                        expected
                    );
                    assert_eq!(
                        route_key(scan, Mod::NOMOD, true, captured, modal),
                        KeyRoute::Ignore
                    );
                }
            }
            assert_eq!(
                route_key(scan, Mod::LSHIFTMOD, false, true, false),
                KeyRoute::FunctionTap
            );
            assert_eq!(
                route_key(scan, Mod::LSHIFTMOD, true, true, false),
                KeyRoute::Ignore
            );
        }
    }

    #[test]
    fn control_p_is_a_host_toggle_but_never_boots_from_an_open_draft() {
        for captured in [false, true] {
            assert_eq!(
                route_key(Scancode::P, Mod::LCTRLMOD, false, captured, false),
                KeyRoute::Pause
            );
            assert_eq!(
                route_key(Scancode::P, Mod::LCTRLMOD, false, captured, true),
                KeyRoute::Ignore
            );
            assert_eq!(
                route_key(Scancode::P, Mod::LCTRLMOD, true, captured, false),
                KeyRoute::Ignore
            );
        }
        assert_eq!(
            route_key(Scancode::P, Mod::NOMOD, false, true, false),
            KeyRoute::Guest
        );
    }

    #[test]
    fn release_balances_keys_and_buttons_and_ignores_stale_repeats() {
        let (mut input, ps2) = connected();
        input.key(Scancode::A, true, true); // Already held before capture.
        assert!(drain(&ps2, false).is_empty());
        input.key(Scancode::A, true, false);
        input.key(Scancode::A, true, true);
        input.button(MouseButton::Middle, true);
        input.release();
        assert_eq!(drain(&ps2, false), [0x1c, 0x1c, 0xf0, 0x1c]);
        assert_eq!(drain(&ps2, true), [0x0c, 0, 0, 0x08, 0, 0]);
        input.capture();
        input.key(Scancode::A, false, false);
        input.key(Scancode::A, true, true);
        input.button(MouseButton::Middle, false);
        assert!(drain(&ps2, false).is_empty());
        assert!(drain(&ps2, true).is_empty());
    }

    #[test]
    fn detaching_releases_previous_controller_and_disables_capture() {
        let (mut input, ps2) = connected();
        input.key(Scancode::A, true, false);
        input.attach(None);
        assert!(!input.captured);
        input.capture();
        assert!(!input.captured);
        assert_eq!(drain(&ps2, false), [0x1c, 0xf0, 0x1c]);
    }

    #[test]
    fn function_tap_temporarily_lifts_shift_and_restores_held_state() {
        let (mut input, ps2) = connected();
        input.key(Scancode::LShift, true, false);
        input.function_tap(Scancode::F9);
        input.key(Scancode::F9, false, false);
        input.key(Scancode::LShift, false, false);
        assert_eq!(
            drain(&ps2, false),
            [0x12, 0xf0, 0x12, 0x47, 0xf0, 0x47, 0x12, 0xf0, 0x12]
        );
    }

    #[test]
    fn physical_mapping_preserves_keypad_modifiers_and_iso_key() {
        let (mut input, ps2) = connected();
        ps2.set_layout(KeyboardLayout::Iso);
        for scan in [
            Scancode::NonUsBackslash,
            Scancode::KpEnter,
            Scancode::Return,
            Scancode::LCtrl,
            Scancode::RCtrl,
        ] {
            input.key(scan, true, false);
            input.key(scan, false, false);
        }
        assert_eq!(
            drain(&ps2, false),
            [
                0x13, 0xf0, 0x13, 0x79, 0xf0, 0x79, 0x5a, 0xf0, 0x5a, 0x11, 0xf0, 0x11, 0x58, 0xf0,
                0x58
            ]
        );
        assert_eq!(
            physical_key(Scancode::International3),
            Some(KeyCode::IntlYen)
        );
        assert_eq!(physical_key(Scancode::Unknown), None);
    }

    #[test]
    fn mouse_preserves_fractional_motion_direction_and_packet_bounds() {
        let (mut input, ps2) = connected();
        input.motion(0.5, 0.5);
        assert!(drain(&ps2, true).is_empty());
        input.motion(0.5, 0.5);
        assert_eq!(drain(&ps2, true), [0x28, 1, 255]); // Screen down => PS/2 negative Y.
        input.motion(300.0, -300.0);
        let packets = drain(&ps2, true);
        assert_eq!(packets.len() % 3, 0);
        let mut total = (0, 0);
        for packet in packets.as_chunks::<3>().0 {
            assert_eq!(packet[0], 0x08); // Positive X/Y, no overflow or buttons.
            assert!(packet[1] <= 127 && packet[2] <= 127);
            total.0 += u32::from(packet[1]);
            total.1 += u32::from(packet[2]);
        }
        assert_eq!(total, (300, 300)); // Iris splits large moves without losing motion.
        input.motion(f32::NAN, 1.0);
        assert!(drain(&ps2, true).is_empty());
        input.motion(0.5, 0.5);
        input.release();
        input.capture();
        input.motion(0.5, 0.5);
        assert!(
            drain(&ps2, true).is_empty(),
            "Capture must discard old fractions"
        );
    }

    #[test]
    fn wheel_preserves_fractional_steps_and_sends_ps2_up_direction() {
        let (mut input, ps2) = connected();
        for byte in [0xf3, 200, 0xf3, 100, 0xf3, 80] {
            ps2.write_to(byte, true);
        }
        drain(&ps2, true); // Enable IntelliMouse packets, discard ACKs.
        input.scroll(0.5);
        assert!(drain(&ps2, true).is_empty());
        input.scroll(0.5);
        assert_eq!(drain(&ps2, true), [0x08, 0, 0, 0xff]);
        input.scroll(-9.0);
        assert_eq!(drain(&ps2, true), [0x08, 0, 0, 7, 0x08, 0, 0, 2]);
    }
}
