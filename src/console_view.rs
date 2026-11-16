//! SDL terminal view and input focus; adapted from Heliacal's serial console.
use crate::{
    headless::{Console, Snapshot},
    serial_screen::Scroll,
    serial_selection::Selection,
};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Mod, Scancode},
    mouse::{MouseButton, MouseWheelDirection},
    pixels::Color,
    rect::Rect,
    render::WindowCanvas,
};
use std::sync::Arc;
const BORDER: Color = Color::RGB(137, 163, 203);
const TEXT: Color = Color::RGB(240, 240, 240);
const MUTED: Color = Color::RGB(170, 190, 215);
const PANEL: Color = Color::RGB(25, 32, 52);
const ACCENT: Color = Color::RGB(255, 200, 90);
fn fill(c: &mut WindowCanvas, rect: Rect, color: Color) -> Result<(), sdl3::Error> {
    c.set_draw_color(color);
    c.fill_rect(rect)
}
fn text(
    c: &mut WindowCanvas,
    x: i32,
    y: i32,
    value: &str,
    color: Color,
) -> Result<(), sdl3::Error> {
    c.set_draw_color(color);
    c.draw_debug_text(value, (x as f32, y as f32))
}
#[derive(Default)]
pub struct ConsoleView {
    console: Option<Arc<Console>>,
    state: Option<Snapshot>,
    pub focused: bool,
    running: bool,
    selection: Option<Selection>,
    drag: Option<i32>,
}
impl ConsoleView {
    pub fn has_output(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            (0..24).any(|r| {
                (0..80).any(|c| s.screen.cell(r, c).is_some_and(|cell| cell.has_contents()))
            })
        })
    }
    pub fn visible(&self) -> bool {
        self.state.is_some()
    }
    pub fn sync(&mut self, console: Option<Arc<Console>>, running: bool, modal: bool) -> bool {
        let changed = match (&self.console, &console) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.release();
            self.selection = None;
            self.state = None;
            self.console = console;
        }
        self.running = running;
        if modal || (!running && self.focused) {
            self.release();
        }
        let Some(console) = &self.console else {
            return changed;
        };
        let state = console.snapshot();
        let redraw = self.state.as_ref().is_none_or(|old| {
            !Arc::ptr_eq(&old.screen, &state.screen)
                || old.endpoint != state.endpoint
                || old.dropped != state.dropped
                || old.attached != state.attached
        });
        self.state = Some(state);
        redraw || changed
    }
    pub fn release(&mut self) {
        if let Some(selection) = &mut self.selection {
            selection.dragging = false;
        }
        self.focused = false;
        self.drag = None;
        if let Some(c) = &self.console {
            c.focus(false);
        }
    }
    fn scroll(&mut self, request: Scroll) {
        self.selection = None;
        if let Some(c) = &self.console {
            c.scroll(request);
        }
    }
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.selection = None;
        self.console.as_ref().unwrap().send(bytes)
    }
    pub fn handle(
        &mut self,
        event: &Event,
        point: Option<(i32, i32)>,
        clipboard: &sdl3::clipboard::ClipboardUtil,
    ) -> Result<bool, String> {
        let Some(state) = self.state.as_ref() else {
            return Ok(false);
        };
        match event {
            Event::Window {
                win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized,
                ..
            } => {
                self.release();
                return Ok(false);
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                ..
            } => {
                let Some((x, y)) = point else {
                    return Ok(false);
                };
                if copy_rect().contains_point((x, y)) {
                    if !state.attached {
                        return Err("Start the headless guest to create its PTY".into());
                    }
                    clipboard
                        .set_clipboard_text(&crate::headless::minicom(&state.endpoint))
                        .map_err(|e| e.to_string())?;
                } else if serial_scroll_rect().contains_point((x, y)) {
                    let thumb = serial_scroll_thumb(state.screen.history, state.screen.offset);
                    if thumb.contains_point((x, y)) {
                        self.drag = Some(y - thumb.y());
                    } else {
                        self.scroll(Scroll::By(if y < thumb.y() { 24 } else { -24 }));
                    }
                } else if serial_console_rect().contains_point((x, y)) {
                    self.focused = self.running && state.attached;
                    self.console.as_ref().unwrap().focus(self.focused);
                    self.selection =
                        Some(Selection::new(state.screen.clone(), serial_cell_at(x, y)));
                } else {
                    self.release();
                    return Ok(false);
                }
            }
            Event::MouseMotion { .. }
                if self.drag.is_some() || self.selection.as_ref().is_some_and(|s| s.dragging) =>
            {
                if let Some((x, y)) = point {
                    if let Some(grab) = self.drag {
                        self.scroll(Scroll::To(serial_scroll_offset(
                            state.screen.history,
                            y,
                            grab,
                        )));
                    } else if let Some(s) = &mut self.selection {
                        s.end = serial_cell_at(x, y);
                    }
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                ..
            } => {
                self.drag = None;
                if let Some(s) = &mut self.selection {
                    s.dragging = false;
                }
                if self.selection.as_ref().is_some_and(|s| !s.nonempty()) {
                    self.selection = None;
                }
            }
            Event::MouseWheel { y, direction, .. } => self.scroll(Scroll::By(
                (if *direction == MouseWheelDirection::Flipped {
                    -*y
                } else {
                    *y
                }) as i32
                    * 3,
            )),
            Event::KeyDown {
                scancode: Some(key),
                keycode,
                keymod,
                repeat,
                ..
            } => {
                let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
                    && !keymod.intersects(Mod::RALTMOD | Mod::MODEMOD | Mod::LEVEL5MOD);
                let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                if ctrl && *key == Scancode::Return {
                    if !repeat {
                        self.focused = !self.focused && self.running && state.attached;
                        self.console.as_ref().unwrap().focus(self.focused);
                    }
                } else if ctrl && shift && *key == Scancode::C {
                    let value = self
                        .selection
                        .as_ref()
                        .filter(|s| s.nonempty())
                        .map_or_else(
                            || crate::headless::minicom(&state.endpoint),
                            Selection::text,
                        );
                    if !value.is_empty() {
                        clipboard
                            .set_clipboard_text(&value)
                            .map_err(|e| e.to_string())?;
                    }
                } else if ctrl && shift && *key == Scancode::V && self.focused {
                    let bytes = crate::serial_input::paste_bytes(
                        &clipboard.clipboard_text().map_err(|e| e.to_string())?,
                        state.screen.bracketed_paste,
                    )?;
                    self.send(&bytes)?;
                } else if shift
                    && matches!(
                        key,
                        Scancode::PageUp | Scancode::PageDown | Scancode::Home | Scancode::End
                    )
                {
                    self.scroll(match key {
                        Scancode::PageUp => Scroll::By(24),
                        Scancode::PageDown => Scroll::By(-24),
                        Scancode::Home => Scroll::To(state.screen.history),
                        _ => Scroll::To(0),
                    });
                } else if *key == Scancode::Escape && self.selection.is_some() {
                    self.selection = None;
                } else if crate::serial_input::reserved(*key, *keymod)
                    || crate::input::scale_shortcut(*key, *keycode, *keymod, *repeat).is_some()
                {
                    return Ok(false);
                } else if self.focused {
                    if let Some(bytes) = crate::serial_input::key_bytes(
                        *key,
                        *keycode,
                        *keymod,
                        state.screen.application_cursor(),
                    ) {
                        self.send(&bytes)?;
                    }
                } else {
                    return Ok(false);
                }
            }
            Event::TextInput { text, .. } if self.focused => self.send(text.as_bytes())?,
            Event::KeyUp { .. } if self.focused => {}
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub fn draw(&self, c: &mut WindowCanvas) -> Result<(), sdl3::Error> {
        let Some(state) = &self.state else {
            return Ok(());
        };
        let focused = self.focused;
        let selection = self.selection.as_ref();
        fill(c, Rect::new(0, 0, 960, 680), Color::RGB(5, 8, 14))?;
        text(c, 80, 48, "HEADLESS / SERIAL CONSOLE", ACCENT)?;
        let endpoint = if state.attached {
            format!("{} -> {}", state.endpoint, state.path)
        } else {
            "Serial endpoint closed / history retained".into()
        };
        text(
            c,
            80,
            76,
            &endpoint.chars().take(100).collect::<String>(),
            TEXT,
        )?;
        fill(c, serial_console_rect(), CONSOLE_BG)?;
        c.set_draw_color(if focused { ACCENT } else { BORDER });
        c.draw_rect(serial_console_rect())?;
        // 80x24 at 10x18 pixels. Group equal attributes to avoid one SDL text
        // operation per character. The UI uses its existing ASCII glyph font.
        let screen = selection.map_or(state.screen.as_ref(), |s| s.screen.as_ref());
        let style_at = |row, col| {
            let mut style = cell_style(screen.cell(row, col).unwrap());
            if selection.is_some_and(|s| s.contains(row, col)) {
                style.0 = Color::RGB(255, 255, 255);
                style.1 = Color::RGB(49, 82, 130);
            }
            style
        };
        for row in 0..24 {
            let mut col = 0;
            while col < 80 {
                let start = col;
                let style = style_at(row, col);
                let mut run = String::new();
                while col < 80 {
                    let cell = screen.cell(row, col).unwrap();
                    if style_at(row, col) != style {
                        break;
                    }
                    run.push(if !cell.has_contents() || cell.is_wide_continuation() {
                        ' '
                    } else {
                        cell.contents()
                            .chars()
                            .next()
                            .filter(|ch| ch.is_ascii() && !ch.is_ascii_control())
                            .unwrap_or('?')
                    });
                    col += 1;
                }
                let x = 80 + i32::from(start) * 10;
                let y = 128 + i32::from(row) * 18;
                let width = u32::from(col - start) * 10;
                if style.1 != CONSOLE_BG {
                    fill(c, Rect::new(x, y, width, 18), style.1)?;
                }
                let scale = c.scale();
                c.set_scale(scale.0 * 1.25, scale.1 * 1.25)?;
                c.set_draw_color(style.0);
                let drawn = c.draw_debug_text(&run, (x as f32 / 1.25, (y + 3) as f32 / 1.25));
                let restored = c.set_scale(scale.0, scale.1);
                drawn.and(restored)?;
                if style.2 {
                    fill(c, Rect::new(x, y + 15, width, 1), style.0)?;
                }
            }
        }
        if !screen.hide_cursor() && !selection.is_some_and(|s| s.nonempty()) {
            let (row, col) = screen.cursor_position();
            fill(
                c,
                Rect::new(
                    80 + i32::from(col.min(79)) * 10,
                    128 + i32::from(row.min(23)) * 18 + 16,
                    10,
                    2,
                ),
                cell_style(screen.cell(row.min(23), col.min(79)).unwrap()).0,
            )?;
        }
        fill(c, serial_scroll_rect(), PANEL)?;
        fill(
            c,
            serial_scroll_thumb(screen.history, screen.offset),
            if screen.history == 0 { MUTED } else { BORDER },
        )?;
        let position = if selection.is_some_and(|s| s.nonempty()) {
            "Selection held / Ctrl+Shift+C copies / Esc clears".to_string()
        } else if screen.offset == 0 {
            format!(
                "LIVE / {} saved lines / wheel or Shift+PageUp to scroll",
                screen.history
            )
        } else {
            format!(
                "{} lines above live / Shift+End returns to latest output",
                screen.offset
            )
        };
        text(c, 80, 100, &position, MUTED)?;

        fill(c, copy_rect(), PANEL)?;
        c.set_draw_color(BORDER);
        c.draw_rect(copy_rect())?;
        text(c, 752, 592, "Copy command", TEXT)?;
        let command = crate::headless::minicom(&state.endpoint);
        text(
            c,
            80,
            594,
            &command.chars().take(78).collect::<String>(),
            MUTED,
        )?;
        text(
            c,
            80,
            624,
            if focused {
                "Keyboard focused / Ctrl+Enter releases / Ctrl+Shift+C/V copies and pastes"
            } else {
                "Click console or Ctrl+Enter to type / Ctrl+Shift+C copies selected text"
            },
            MUTED,
        )?;
        if state.dropped > 0 {
            text(
                c,
                80,
                646,
                &format!("Slow serial reader: {} output bytes dropped", state.dropped),
                ACCENT,
            )?;
        }
        Ok(())
    }
}
fn copy_rect() -> Rect {
    Rect::new(736, 582, 144, 28)
}
const CONSOLE_BG: Color = Color::RGB(8, 12, 18);
const CONSOLE_FG: Color = Color::RGB(215, 224, 232);

fn terminal_color(color: vt100::Color, default: Color, bright: bool) -> Color {
    match color {
        vt100::Color::Default if bright => Color::RGB(255, 255, 255),
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color::RGB(r, g, b),
        vt100::Color::Idx(mut index) => {
            if bright && index < 8 {
                index += 8;
            }
            const ANSI: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (170, 0, 0),
                (0, 170, 0),
                (170, 85, 0),
                (0, 0, 170),
                (170, 0, 170),
                (0, 170, 170),
                (170, 170, 170),
                (85, 85, 85),
                (255, 85, 85),
                (85, 255, 85),
                (255, 255, 85),
                (85, 85, 255),
                (255, 85, 255),
                (85, 255, 255),
                (255, 255, 255),
            ];
            if index < 16 {
                let (r, g, b) = ANSI[index as usize];
                Color::RGB(r, g, b)
            } else if index < 232 {
                let cube = index - 16;
                let level = |n| if n == 0 { 0 } else { 55 + n * 40 };
                Color::RGB(level(cube / 36), level((cube / 6) % 6), level(cube % 6))
            } else {
                let gray = 8 + (index - 232) * 10;
                Color::RGB(gray, gray, gray)
            }
        }
    }
}

fn cell_style(cell: &vt100::Cell) -> (Color, Color, bool) {
    let mut fg = terminal_color(cell.fgcolor(), CONSOLE_FG, cell.bold());
    let mut bg = terminal_color(cell.bgcolor(), CONSOLE_BG, false);
    if cell.dim() {
        let dim = |a, b| ((u16::from(a) + u16::from(b)) / 2) as u8;
        fg = Color::RGB(dim(fg.r, bg.r), dim(fg.g, bg.g), dim(fg.b, bg.b));
    }
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    (fg, bg, cell.underline())
}

pub fn serial_console_rect() -> Rect {
    Rect::new(64, 112, 832, 460)
}

pub fn serial_scroll_rect() -> Rect {
    Rect::new(900, 112, 16, 460)
}
pub fn serial_scroll_thumb(history: usize, offset: usize) -> Rect {
    let track = serial_scroll_rect();
    let height = ((track.height() as usize * 24) / (history + 24)).max(24) as u32;
    let travel = (track.height() - height) as usize;
    let y = (travel * (history - offset.min(history)))
        .checked_div(history)
        .unwrap_or(0);
    Rect::new(track.x(), track.y() + y as i32, track.width(), height)
}
pub fn serial_scroll_offset(history: usize, y: i32, grab: i32) -> usize {
    let track = serial_scroll_rect();
    let travel = (track.height() - serial_scroll_thumb(history, 0).height()) as i32;
    if travel == 0 {
        return 0;
    }
    let top = (y - grab - track.y()).clamp(0, travel) as usize;
    history - (history * top / travel as usize)
}
pub fn serial_cell_at(x: i32, y: i32) -> (u16, u16) {
    (
        ((y - 128).div_euclid(18)).clamp(0, 23) as u16,
        ((x - 80).div_euclid(10)).clamp(0, 79) as u16,
    )
}

#[cfg(all(test, unix, iris_serial, iris_headless_devices))]
mod tests {
    use super::*;
    use iris::dev::serial_host::SerialBackend;
    use sdl3::keyboard::Keycode;

    #[test]
    #[ignore = "requires SDL video; run separately with SDL_VIDEODRIVER=dummy"]
    fn clipboard_selection_paste_command_and_paused_history() {
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let clipboard = video.clipboard();
        let console = Arc::new(Console::default());
        let mut bridge = crate::headless::Bridge::open(console.clone(), "").unwrap();
        for byte in b"hello world\r\n" {
            console.send_byte(*byte);
        }
        bridge.poll(true).unwrap();
        let mut view = ConsoleView::default();
        view.sync(Some(console.clone()), true, false);
        let mut selection = Selection::new(console.snapshot().screen, (0, 0));
        selection.end = (0, 4);
        view.selection = Some(selection);
        let key = |scan, code, mods| Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(code),
            scancode: Some(scan),
            keymod: mods,
            repeat: false,
            which: 0,
            raw: 0,
        };
        let copy = key(Scancode::C, Keycode::C, Mod::LCTRLMOD | Mod::LSHIFTMOD);
        assert!(view.handle(&copy, None, &clipboard).unwrap());
        assert_eq!(clipboard.clipboard_text().unwrap(), "hello");
        let point = copy_rect().center();
        let click = Event::MouseButtonDown {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: point.x() as f32,
            y: point.y() as f32,
        };
        assert!(
            view.handle(&click, Some((point.x(), point.y())), &clipboard)
                .unwrap()
        );
        assert_eq!(
            clipboard.clipboard_text().unwrap(),
            crate::headless::minicom(&console.snapshot().endpoint)
        );
        view.focused = true;
        console.focus(true);
        clipboard.set_clipboard_text("one\ntwo\r\n").unwrap();
        assert!(
            view.handle(
                &key(Scancode::V, Keycode::V, Mod::LCTRLMOD | Mod::LSHIFTMOD),
                None,
                &clipboard
            )
            .unwrap()
        );
        let bytes = (0..8)
            .map(|_| console.recv_byte().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(bytes, b"one\rtwo\r");
        assert!(
            view.handle(
                &key(Scancode::C, Keycode::C, Mod::LCTRLMOD),
                None,
                &clipboard
            )
            .unwrap()
        );
        assert_eq!(console.recv_byte().unwrap(), 3);
        view.release();
        view.selection = Some(Selection::new(console.snapshot().screen, (0, 0)));
        view.sync(Some(console.clone()), false, false);
        assert!(view.selection.as_ref().unwrap().dragging);
        view.release();
        assert!(!view.selection.as_ref().unwrap().dragging);
        console.send_byte(b'!');
        bridge.poll(false).unwrap();
        assert!(console.send(b"paused").is_err());
    }
}
