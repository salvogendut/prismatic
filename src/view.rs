use crate::{
    display::{Frame, fit},
    overlay::{Dialog, Overlay, Tab},
    settings::Settings,
};
use sdl3::{
    pixels::Color,
    rect::Rect,
    render::{BlendMode, Texture, WindowCanvas},
};

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 720;
const PANEL: Color = Color::RGB(0x19, 0x20, 0x34);
const BORDER: Color = Color::RGB(0x89, 0xa3, 0xcb);
const SELECTED: Color = Color::RGB(0x80, 0x60, 0x20);
const TEXT: Color = Color::RGB(240, 240, 240);
const MUTED: Color = Color::RGB(170, 190, 215);

pub struct Presentation<'a> {
    pub status: &'a str,
    pub placeholder: Option<&'a str>,
}

fn text(
    canvas: &mut WindowCanvas,
    x: i32,
    y: i32,
    value: &str,
    color: Color,
) -> Result<(), sdl3::Error> {
    canvas.set_draw_color(color);
    canvas.draw_debug_text(value, (x as f32, y as f32))
}

fn fill(canvas: &mut WindowCanvas, rect: Rect, color: Color) -> Result<(), sdl3::Error> {
    canvas.set_draw_color(color);
    canvas.fill_rect(rect)
}

fn panel(canvas: &mut WindowCanvas, rect: Rect) -> Result<(), sdl3::Error> {
    fill(canvas, rect, PANEL)?;
    canvas.set_draw_color(BORDER);
    canvas.draw_rect(rect)
}

pub fn draw(
    canvas: &mut WindowCanvas,
    texture: &Texture,
    frame: &Frame,
    settings: &Settings,
    overlay: &Overlay,
    notification: Option<&str>,
    presentation: Presentation<'_>,
) -> Result<(), sdl3::Error> {
    canvas.set_draw_color(Color::RGB(5, 8, 14));
    canvas.clear();
    let area = Rect::new(
        0,
        0,
        WIDTH,
        HEIGHT - if settings.show_status { 36 } else { 0 },
    );
    let destination = fit(frame.width, frame.height, area);
    canvas.copy(texture, None, destination)?;
    canvas.set_blend_mode(BlendMode::Blend);
    if settings.scanlines {
        for y in (destination.y()..destination.bottom()).step_by(3) {
            fill(
                canvas,
                Rect::new(destination.x(), y, destination.width(), 1),
                Color::RGBA(0, 0, 0, 65),
            )?;
        }
    }
    if let Some(message) = presentation.placeholder {
        let x = 256;
        let y = area.height() as i32 / 2 - 56;
        text(canvas, x, y, "P R I S M A T I C", TEXT)?;
        text(canvas, x, y + 28, "Happy Years / SGI", BORDER)?;
        text(canvas, x, y + 68, message, MUTED)?;
        text(
            canvas,
            x,
            y + 92,
            "F9 Options     F11 Fullscreen     F12 Quit",
            MUTED,
        )?;
    }

    if settings.show_status {
        fill(canvas, Rect::new(0, (HEIGHT - 36) as i32, WIDTH, 36), PANEL)?;
        text(
            canvas,
            16,
            HEIGHT as i32 - 22,
            "F9 Options   F11 Fullscreen   F12 Quit",
            TEXT,
        )?;
        text(canvas, 640, HEIGHT as i32 - 22, presentation.status, MUTED)?;
    }

    if let Some(session) = &overlay.session {
        fill(
            canvas,
            Rect::new(0, 0, WIDTH, HEIGHT),
            Color::RGBA(0, 0, 0, 160),
        )?;
        panel(canvas, Rect::new(16, 16, 928, 290))?;
        fill(
            canvas,
            Rect::new(18, 18, 924, 28),
            Color::RGB(0x30, 0x40, 0x60),
        )?;
        let mut tab_x = 32;
        for tab in session.tabs() {
            text(
                canvas,
                tab_x,
                28,
                tab.label(),
                if tab == session.tab { TEXT } else { MUTED },
            )?;
            tab_x += tab.label().len() as i32 * 8 + 32;
        }
        if session.dirty() {
            text(
                canvas,
                792,
                28,
                "Unsaved changes",
                Color::RGB(255, 208, 128),
            )?;
        }
        for (index, (label, enabled)) in session.rows().iter().enumerate() {
            let row_y = 68 + index as i32 * 28;
            if index == session.row {
                fill(canvas, Rect::new(24, row_y - 6, 912, 24), SELECTED)?;
            }
            text(canvas, 32, row_y, label, TEXT)?;
            text(
                canvas,
                352,
                row_y,
                if *enabled { "On" } else { "Off" },
                Color::RGB(255, 208, 128),
            )?;
        }
        match session.tab {
            Tab::General => {
                text(
                    canvas,
                    32,
                    192,
                    "Display preferences apply when saved. Tinker reveals advanced options.",
                    MUTED,
                )?;
            }
            Tab::Advanced => {
                text(
                    canvas,
                    32,
                    192,
                    "Bilinear filtering smooths the display when scaled.",
                    MUTED,
                )?;
            }
            Tab::About => {
                for (index, line) in [
                    "Prismatic - Happy Years interface for Iris",
                    "Rust / SDL3",
                    "Iris Indy / R4400 / Newport - diskless embedded PROM boot.",
                    "Guest input, audio and media configuration are not connected yet.",
                ]
                .iter()
                .enumerate()
                {
                    text(canvas, 32, 68 + index as i32 * 28, line, TEXT)?;
                }
            }
        }
        text(
            canvas,
            32,
            252,
            "Left/Right Section   Up/Down Select   Enter Toggle",
            MUTED,
        )?;
        text(canvas, 32, 280, "F9 Save and close   Esc Close", TEXT)?;
    }

    if let Some(dialog) = overlay.dialog {
        fill(
            canvas,
            Rect::new(0, 0, WIDTH, HEIGHT),
            Color::RGBA(0, 0, 0, 150),
        )?;
        panel(canvas, Rect::new(176, 260, 608, 180))?;
        let (title, detail, buttons, selected) = match dialog {
            Dialog::Unsaved { choice } => (
                "Save changes?",
                "Options have changed. Choose what to do before closing.",
                vec!["Save", "Discard", "Cancel"],
                choice,
            ),
            Dialog::Quit { confirm } => (
                "Quit Prismatic?",
                "Stop the machine and discard any unsaved options?",
                vec!["Quit", "Cancel"],
                usize::from(!confirm),
            ),
        };
        text(canvas, 200, 284, title, TEXT)?;
        text(canvas, 200, 316, detail, MUTED)?;
        for (index, label) in buttons.iter().enumerate() {
            let button_x = 200 + index as i32 * 176;
            if index == selected {
                fill(canvas, Rect::new(button_x, 352, 160, 30), SELECTED)?;
            }
            text(canvas, button_x + 12, 364, label, TEXT)?;
        }
        text(
            canvas,
            200,
            412,
            "Left/Right Select   Enter Confirm   Esc Cancel",
            MUTED,
        )?;
    }

    if let Some(message) = notification {
        // SDL's built-in font is ASCII. Detailed errors remain available on stderr.
        let shown: String = message
            .chars()
            .take(110)
            .map(|c| {
                if c.is_ascii() && !c.is_control() {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        panel(canvas, Rect::new(24, 620, 912, 36))?;
        text(canvas, 40, 634, &shown, TEXT)?;
    }
    Ok(())
}
