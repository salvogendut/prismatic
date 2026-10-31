use crate::{
    activity,
    display::{Frame, fit},
    media::{MediaKind, MediaSettings},
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
const LED_HEIGHT: u32 = 24;
const PANEL: Color = Color::RGB(0x19, 0x20, 0x34);
const BORDER: Color = Color::RGB(0x89, 0xa3, 0xcb);
const SELECTED: Color = Color::RGB(0x80, 0x60, 0x20);
const TEXT: Color = Color::RGB(240, 240, 240);
const MUTED: Color = Color::RGB(170, 190, 215);

pub struct Presentation<'a> {
    pub status: &'a str,
    pub placeholder: Option<&'a str>,
    pub captured: bool,
    pub leds: u64,
    pub media: &'a MediaSettings,
    pub running: bool,
}

pub fn guest_rect(frame: &Frame, settings: &Settings) -> Rect {
    fit(
        frame.width,
        frame.height,
        Rect::new(
            0,
            0,
            WIDTH,
            HEIGHT - LED_HEIGHT - if settings.show_status { 36 } else { 0 },
        ),
    )
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
        HEIGHT - LED_HEIGHT - if settings.show_status { 36 } else { 0 },
    );
    let destination = guest_rect(frame, settings);
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
        fill(
            canvas,
            Rect::new(0, (HEIGHT - LED_HEIGHT - 36) as i32, WIDTH, 36),
            PANEL,
        )?;
        text(
            canvas,
            16,
            (HEIGHT - LED_HEIGHT) as i32 - 22,
            if presentation.captured {
                "Ctrl+Enter Release | F9 Options | F11 Full | F12 Quit"
            } else {
                "Click display to capture | F9 Options | F11 Full | F12 Quit"
            },
            TEXT,
        )?;
        text(
            canvas,
            704,
            (HEIGHT - LED_HEIGHT) as i32 - 22,
            presentation.status,
            MUTED,
        )?;
    }

    if let Some(session) = &overlay.session {
        fill(
            canvas,
            Rect::new(0, 0, WIDTH, HEIGHT),
            Color::RGBA(0, 0, 0, 160),
        )?;
        let media_tab = session.tab == Tab::Media;
        panel(
            canvas,
            Rect::new(16, 16, 928, if media_tab { 466 } else { 402 }),
        )?;
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
        for (index, (label, value)) in session.rows().iter().enumerate() {
            let row_y = 68 + index as i32 * 28;
            if index == session.row {
                fill(canvas, Rect::new(24, row_y - 6, 912, 24), SELECTED)?;
            }
            text(canvas, 32, row_y, label, TEXT)?;
            text(
                canvas,
                352,
                row_y,
                &short_ascii(value, 70),
                Color::RGB(255, 208, 128),
            )?;
            if session.tab == Tab::General && index < 4 {
                text(canvas, 704, row_y, "Requires restart", MUTED)?;
            }
        }
        match session.tab {
            Tab::General => {
                text(
                    canvas,
                    32,
                    284,
                    "Machine changes keep compatible CPU/RAM choices; other values are adjusted.",
                    MUTED,
                )?;
                text(
                    canvas,
                    32,
                    312,
                    "Hardware changes ask to restart when saved. Display options apply on Save.",
                    MUTED,
                )?;
            }
            Tab::Media => {
                text(
                    canvas,
                    32,
                    284,
                    "Hard disks: persistent copy-on-write. CD-ROMs: read-only (ISO9660 or EFS).",
                    MUTED,
                )?;
                if let Some(medium) = session.draft.media.devices.get(&(session.row as u8 + 1)) {
                    let path = short_ascii(&medium.path, 220);
                    for (i, chunk) in path.as_bytes().chunks(110).enumerate() {
                        text(
                            canvas,
                            32,
                            312 + i as i32 * 24,
                            std::str::from_utf8(chunk).unwrap(),
                            TEXT,
                        )?;
                    }
                }
                text(
                    canvas,
                    32,
                    372,
                    "Media changes require restart. Shut down the guest OS before saving.",
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
                    "Iris Indy / Indigo2 / Newport - SCSI disk and CD-ROM support.",
                    "Click captures input; Ctrl+Enter releases. Shift+F9/F11/F12: guest key.",
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
            if media_tab { 428 } else { 364 },
            "Left/Right Section   Up/Down Select   Enter Change",
            MUTED,
        )?;
        text(
            canvas,
            32,
            if media_tab { 456 } else { 392 },
            if overlay.busy {
                "Choosing/validating image...   Esc Cancel"
            } else {
                "F9 Save and close   Esc Close"
            },
            TEXT,
        )?;
    }

    if let Some(dialog) = overlay.dialog {
        fill(
            canvas,
            Rect::new(0, 0, WIDTH, HEIGHT),
            Color::RGBA(0, 0, 0, 150),
        )?;
        panel(canvas, Rect::new(176, 260, 608, 180))?;
        let (title, detail, buttons, selected) = match dialog {
            Dialog::Media { choice, .. } => (
                "Choose media for this SCSI target",
                "Select a raw/CHD disk, or ISO9660/EFS/CHD CD-ROM.",
                vec!["Hard disk", "CD-ROM", "Disconnect", "Cancel"],
                choice,
            ),
            Dialog::Unsaved { choice } => (
                "Save changes?",
                "Options have changed. Choose what to do before closing.",
                vec!["Save", "Discard", "Cancel"],
                choice,
            ),
            Dialog::Quit { confirm } => (
                "Quit Prismatic?",
                "Shut down the guest OS first. Stop and discard unsaved options?",
                vec!["Quit", "Cancel"],
                usize::from(!confirm),
            ),
            Dialog::Restart { confirm } => (
                "Save hardware/media changes and restart?",
                "Shut down the guest OS first. The current session will be lost.",
                vec!["Restart", "Cancel"],
                usize::from(!confirm),
            ),
        };
        text(canvas, 200, 284, title, TEXT)?;
        text(canvas, 200, 316, detail, MUTED)?;
        for (index, label) in buttons.iter().enumerate() {
            let button_x = 200 + index as i32 * 144;
            if index == selected {
                fill(canvas, Rect::new(button_x, 352, 136, 30), SELECTED)?;
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
    draw_leds(canvas, &presentation)?;
    Ok(())
}

fn short_ascii(value: &str, max: usize) -> String {
    let mut text: String = value
        .chars()
        .take(max)
        .map(|c| {
            if c.is_ascii() && !c.is_control() {
                c
            } else {
                '?'
            }
        })
        .collect();
    if value.chars().count() > max && max >= 3 {
        text.truncate(max - 3);
        text.push_str("...");
    }
    text
}

fn draw_leds(
    canvas: &mut WindowCanvas,
    presentation: &Presentation<'_>,
) -> Result<(), sdl3::Error> {
    let y = (HEIGHT - LED_HEIGHT) as i32;
    fill(
        canvas,
        Rect::new(0, y, WIDTH, LED_HEIGHT),
        Color::RGB(8, 10, 12),
    )?;
    let mut lamps = vec![
        ("RUN".to_owned(), presentation.running, false),
        ("CPU".into(), presentation.leds & activity::CPU != 0, false),
        ("NET".into(), presentation.leds & activity::NET != 0, false),
    ];
    for (&id, medium) in &presentation.media.devices {
        lamps.push((
            format!(
                "{}{id}",
                if medium.kind == MediaKind::Disk {
                    "HD"
                } else {
                    "CD"
                }
            ),
            presentation.leds & (activity::disk_read(id) | activity::disk_write(id)) != 0,
            presentation.leds & activity::disk_write(id) != 0,
        ));
    }
    let start = (WIDTH as i32 - lamps.len() as i32 * 72) / 2;
    for (index, (label, lit, write)) in lamps.iter().enumerate() {
        let x = start + index as i32 * 72;
        let color = if !lit {
            Color::RGB(25, 45, 30)
        } else if *write {
            Color::RGB(255, 180, 55)
        } else {
            Color::RGB(70, 230, 100)
        };
        fill(canvas, Rect::new(x, y + 7, 14, 10), color)?;
        text(canvas, x + 20, y + 8, label, MUTED)?;
    }
    Ok(())
}
