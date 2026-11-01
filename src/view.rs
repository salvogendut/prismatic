use crate::{
    activity,
    diagnostics::SCREEN_LINES,
    display::{Frame, fit},
    media::{MediaKind, MediaSettings},
    overlay::{Dialog, Overlay, Tab},
    settings::{DebugOutput, Settings},
};
use sdl3::{
    pixels::Color,
    rect::Rect,
    render::{BlendMode, Texture, WindowCanvas},
};

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 720;
// Match 1989/1986: compact shortcut band above the activity lamps.
const LED_HEIGHT: u32 = 22;
const HINT_HEIGHT: u32 = 16;
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
    pub debug_lines: &'a [String],
    pub capture_label: Option<&'a str>,
}

pub fn guest_rect(frame: &Frame, settings: &Settings) -> Rect {
    fit(
        frame.width,
        frame.height,
        Rect::new(
            0,
            0,
            WIDTH,
            HEIGHT - LED_HEIGHT - if settings.show_status { HINT_HEIGHT } else { 0 },
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
        HEIGHT - LED_HEIGHT - if settings.show_status { HINT_HEIGHT } else { 0 },
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
        shade(canvas, 100)?;
        let x = 256;
        let y = area.height() as i32 / 2 - 56;
        text(canvas, x, y, "P R I S M A T I C", TEXT)?;
        text(canvas, x, y + 28, "Happy Years / SGI", BORDER)?;
        text(canvas, x, y + 68, message, MUTED)?;
        text(
            canvas,
            x,
            y + 92,
            "Ctrl+P Boot/resume  F2 Machine  F5 Reset  F9 Options",
            MUTED,
        )?;
    }

    draw_debug(canvas, settings, presentation.debug_lines)?;
    draw_status(canvas, settings, &presentation)?;
    draw_leds(canvas, settings, &presentation)?;

    if let Some(session) = &overlay.session {
        draw_overlay(canvas, session, overlay.busy, overlay.action_busy)?;
    }
    if let Some(dialog) = overlay.dialog {
        draw_dialog(canvas, dialog, overlay.action.as_ref())?;
    }
    if let Some(lines) = &overlay.report {
        shade(canvas, 180)?;
        panel(canvas, Rect::new(60, 120, 840, 440))?;
        text(canvas, 80, 140, "Operation result", TEXT)?;
        for (i, line) in lines
            .iter()
            .skip(overlay.report_offset)
            .take(20)
            .enumerate()
        {
            text(
                canvas,
                80,
                170 + i as i32 * 16,
                &short_ascii(line, 96),
                TEXT,
            )?;
        }
        text(canvas, 80, 530, "Up/Down Scroll    Enter/Esc Close", MUTED)?;
    }
    if let Some(edit) = &overlay.text_edit {
        shade(canvas, 150)?;
        let x = 120;
        let y = HEIGHT as i32 / 2 - 80;
        panel(canvas, Rect::new(x, y, 720, 160))?;
        let (title, hint) = match edit.field {
            crate::overlay::TextField::Label => (
                "Save machine as",
                "Use up to 32 ASCII characters. Existing labels are kept.",
            ),
            crate::overlay::TextField::Network(field) => (field.title(), field.hint()),
            crate::overlay::TextField::Expert(field) => (field.title(), field.hint()),
        };
        text(canvas, x + 24, y + 20, title, TEXT)?;
        let visible: String = edit
            .text
            .chars()
            .rev()
            .take(80)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if edit.selected {
            fill(canvas, Rect::new(x + 24, y + 48, 672, 20), SELECTED)?;
        }
        text(
            canvas,
            x + 24,
            y + 52,
            &format!("{}|", short_ascii(&visible, 80)),
            TEXT,
        )?;
        text(
            canvas,
            x + 24,
            y + 84,
            edit.error
                .as_deref()
                .map(|s| short_ascii(s, 82))
                .as_deref()
                .unwrap_or("Enter accepts; F9 saves options. Ctrl+A selects all. Esc cancels."),
            MUTED,
        )?;
        for (line, hint) in wrap_ascii(hint, 82).iter().take(2).enumerate() {
            text(canvas, x + 24, y + 116 + line as i32 * 16, hint, MUTED)?;
        }
    }
    if let Some(picker) = &overlay.interfaces {
        shade(canvas, 150)?;
        let (x, y) = (100, HEIGHT as i32 / 2 - 150);
        panel(canvas, Rect::new(x, y, 760, 300))?;
        text(canvas, x + 24, y + 20, "PCAP host interface", TEXT)?;
        let labels: Vec<_> = std::iter::once("Automatic".to_owned())
            .chain(picker.choices.iter().map(|i| i.label.clone()))
            .chain(std::iter::once("Enter interface name...".to_owned()))
            .collect();
        let start = picker.row.saturating_sub(7);
        for (index, label) in labels.iter().enumerate().skip(start).take(8) {
            let row_y = y + 56 + (index - start) as i32 * 24;
            if index == picker.row {
                fill(canvas, Rect::new(x + 12, row_y - 4, 736, 20), SELECTED)?;
            }
            text(canvas, x + 24, row_y, &short_ascii(label, 88), TEXT)?;
        }
        text(
            canvas,
            x + 24,
            y + 264,
            "Up/Down Select   Enter Accept   Esc Cancel",
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
        let y = area.bottom() - 44;
        panel(canvas, Rect::new(24, y, 912, 36))?;
        text(canvas, 40, y + 14, &shown, TEXT)?;
    }
    Ok(())
}

fn wrap_ascii(value: &str, width: usize) -> Vec<String> {
    let value = short_ascii(value, usize::MAX);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
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

pub fn debug_rect(settings: &Settings) -> Option<Rect> {
    if settings.debug_output != DebugOutput::Screen {
        return None;
    }
    let bottom = HEIGHT - LED_HEIGHT - if settings.show_status { HINT_HEIGHT } else { 0 };
    let height = 28 + SCREEN_LINES as u32 * 12;
    // Leave the notification band and both bottom bars unobstructed.
    Some(Rect::new(24, (bottom - height - 52) as i32, 912, height))
}

fn draw_debug(
    canvas: &mut WindowCanvas,
    settings: &Settings,
    lines: &[String],
) -> Result<(), sdl3::Error> {
    let Some(rect) = debug_rect(settings) else {
        return Ok(());
    };
    fill(canvas, rect, Color::RGBA(8, 10, 18, 230))?;
    canvas.set_draw_color(BORDER);
    canvas.draw_rect(rect)?;
    text(
        canvas,
        rect.x() + 12,
        rect.y() + 8,
        "IRIS DEBUG - latest messages",
        BORDER,
    )?;
    if lines.is_empty() {
        text(
            canvas,
            rect.x() + 12,
            rect.y() + 24,
            "Waiting for new diagnostics...",
            MUTED,
        )?;
    }
    for (index, line) in lines.iter().enumerate() {
        text(
            canvas,
            rect.x() + 12,
            rect.y() + 24 + index as i32 * 12,
            line,
            TEXT,
        )?;
    }
    Ok(())
}

// The 1989 view uses an 8px inset, 20px rows and a content-sized panel.
// Scale inside the existing logical window so fullscreen letterboxing and
// guest hit testing keep using the same coordinate system.
fn draw_overlay(
    canvas: &mut WindowCanvas,
    session: &crate::overlay::Session,
    busy: bool,
    action_busy: bool,
) -> Result<(), sdl3::Error> {
    const PANEL_WIDTH: u32 = 840;
    const LINE_HEIGHT: i32 = 20;
    let rows = session.rows();
    let hints: Vec<String> = match session.tab {
        Tab::General => vec![
            if session.row == 7 {
                session.draft.machine.prom_path().map(|p| short_ascii(p, 100)).unwrap_or_else(||
                    "All models include bundled firmware. O2 and Octane also accept external PROMs.".into())
            } else if session.row >= 10 {
                "Audio and volume apply on Save without restarting. Enter cycles volume down by 10%.".into()
            } else { "Machine changes keep compatible CPU/RAM choices; other values are adjusted.".into() },
            "Hardware changes ask to restart when saved. Display and audio options apply on Save.".into(),
        ],
        Tab::Media => {
            let mut hints = vec!["Hard disks: persistent copy-on-write. CD-ROMs: read-only (ISO9660 or EFS).".into()];
            if let Some(medium) = session.media_slot().and_then(|slot| session.draft.media.devices.get(&slot)) {
                let path = short_ascii(&medium.path, 200);
                hints.extend(path.as_bytes().chunks(100).map(|chunk| std::str::from_utf8(chunk).unwrap().to_owned()));
            }
            hints.push("Media changes require restart. Shut down the guest OS before saving.".into());
            hints
        }
        Tab::Advanced if session.advanced_page != crate::expert::Page::Core => vec![crate::expert::hint(session.advanced_page, session.row).into(), "Saved per machine label. Setting changes take effect on the next boot/restart.".into()],
        Tab::Advanced => vec![match session.row {
            0 if !session.draft.machine.newport() => "GR2, IMPACT and GBE video modes are controlled by the PROM and guest OS.",
            0 => "Sets the initial Newport video mode; the guest may change it later.",
            1 if !session.draft.machine.newport() => "This machine uses Shadow cache. The Newport model preference is retained.",
            1 => "Auto selects the cache model for this build. Full/Shadow are expert overrides.",
            2 if session.draft.machine.supports_prom_override() => "This machine starts from host time; the saved clock offset applies to Indy/Indigo2.",
            2 => "Shifts the guest clock relative to host time at boot; it then runs normally.",
            3 if session.draft.machine.supports_prom_override() => "This machine's SCSI controller manages interrupt timing automatically.",
            3 => "Deferred is Iris's BSD-compatible default. Immediate changes SCSI timing.",
            4 => "Appends to serial.log in this machine's state directory. Timestamps use host UTC.",
            5 => "Bilinear filtering smooths the display when scaled; applies without restart.",
            6 => "Debug: Off, Screen (latest messages), Console (terminal). Applies on Save.",
            7 => "Native uses the guest screen size; scaled widths are optional. Applies to the next GIF.",
            8 => "GIF sampling rate. Changes apply to the next recording; F6 starts or stops.",
            10 => "Enter cycles Core, Performance, Connections and Developer pages.",
            _ => "FFmpeg optionally optimizes after recording; the built-in GIF is kept on failure.",
        }.into(), "Core options require restart. Turning Tinker off keeps the saved values active.".into()],
        Tab::Extensions => vec![match session.row {
            0 => "GR2/IMPACT compatibility depends on Iris and the guest OS; Newport is the default on Indy/IP22.",
            1 => "Two Newport heads appear side by side. Screenshots and GIFs include both heads.",
            2 => "Octane supports one or two CPUs and up to 8192 MB RAM (set RAM in General).",
            3 | 4 => "Scratch is a persistent raw transfer drive with an SGI volume header, in this label's state folder.",
            5 => "Enter cycles the configured disk/CD targets used by the rows below.",
            6 | 7 => "Changer queue edits are drafts and take effect after Save and restart.",
            8 => "Save or discard drafts before live actions. Commit/reset require no loaded guest and confirmation.",
            _ => "Scratch transfers briefly stop devices, then restore CPU state. They do not mount a guest filesystem.",
        }.into(), "Hardware changes require restart. Turning Tinker off keeps these settings active.".into()],
        Tab::About => vec![],
        Tab::Network => session.draft.network.hints(session.row),
    };
    let hints: Vec<String> = hints
        .iter()
        .flat_map(|hint| wrap_ascii(hint, 100))
        .collect();
    let row_count = if session.tab == Tab::About {
        4
    } else {
        rows.len()
    };
    let hint_y = 48 + row_count as i32 * LINE_HEIGHT + 8;
    let footer_y = hint_y + hints.len() as i32 * 16 + 12;
    let panel_height = (footer_y + 12) as u32;
    let scale = (WIDTH as f32 / 860.0)
        .min(HEIGHT as f32 / (panel_height + 8).max(510) as f32)
        .min(1.25);
    shade(canvas, 160)?;
    canvas.set_scale(scale, scale)?;
    panel(canvas, Rect::new(8, 8, PANEL_WIDTH, panel_height))?;
    fill(
        canvas,
        Rect::new(10, 10, PANEL_WIDTH - 4, 22),
        Color::RGB(0x30, 0x40, 0x60),
    )?;
    let mut tab_x = 20;
    for tab in session.tabs() {
        text(
            canvas,
            tab_x,
            14,
            tab.label(),
            if tab == session.tab { TEXT } else { MUTED },
        )?;
        tab_x += tab.label().len() as i32 * 8 + 20;
    }
    if session.dirty() {
        text(
            canvas,
            PANEL_WIDTH as i32 - 132,
            14,
            "Unsaved changes",
            Color::RGB(255, 208, 128),
        )?;
    }
    for (index, (label, value)) in rows.iter().enumerate() {
        let y = 48 + index as i32 * LINE_HEIGHT;
        if index == session.row {
            fill(canvas, Rect::new(10, y, PANEL_WIDTH - 4, 16), SELECTED)?;
        }
        let restart = (session.tab == Tab::General && (index < 4 || index == 7))
            || (session.tab == Tab::Advanced
                && match session.advanced_page {
                    crate::expert::Page::Core => index < 5,
                    crate::expert::Page::Performance => index < 8,
                    crate::expert::Page::Connections => index < 6,
                    crate::expert::Page::Developer => index < 4,
                })
            || (session.tab == Tab::Extensions && (index < 5 || index == 6 || index == 7));
        text(canvas, 20, y, label, TEXT)?;
        text(
            canvas,
            250,
            y,
            &short_ascii(value, if restart { 44 } else { 72 }),
            Color::RGB(255, 208, 128),
        )?;
        if restart {
            text(canvas, 620, y, "Requires restart", MUTED)?;
        }
    }
    if session.tab == Tab::About {
        for (index, line) in [
            "Prismatic - Happy Years interface for Iris",
            "Rust / SDL3",
            "Iris Indy / Indigo2 / Newport - SCSI disk and CD-ROM support.",
            "Click captures input; Ctrl+Enter releases. Shift+F4/F6/F9/F11/F12: guest key.",
        ]
        .iter()
        .enumerate()
        {
            text(canvas, 20, 48 + index as i32 * LINE_HEIGHT, line, TEXT)?;
        }
    }
    for (index, hint) in hints.iter().enumerate() {
        text(canvas, 20, hint_y + index as i32 * 16, hint, MUTED)?;
    }
    text(
        canvas,
        20,
        footer_y,
        if action_busy {
            "Storage operation in progress..."
        } else if busy {
            "Choosing files / checking options...   Esc Cancel"
        } else {
            "Left/Right Section  Up/Down Select  Enter Change  F9 Save  Esc Close"
        },
        MUTED,
    )?;
    canvas.set_scale(1.0, 1.0)?;
    Ok(())
}

fn shade(canvas: &mut WindowCanvas, alpha: u8) -> Result<(), sdl3::Error> {
    fill(
        canvas,
        Rect::new(0, 0, WIDTH, HEIGHT),
        Color::RGBA(0, 0, 0, alpha),
    )
}

fn draw_dialog(
    canvas: &mut WindowCanvas,
    dialog: Dialog,
    action: Option<&crate::storage::Action>,
) -> Result<(), sdl3::Error> {
    let (title, detail, buttons, selected) = match dialog {
        Dialog::Action { operation, confirm } => (
            operation.title(),
            operation.detail(),
            vec!["Apply now", "Cancel"],
            usize::from(!confirm),
        ),
        Dialog::Storage { cd, choice, .. } => (
            "Storage actions",
            "Live tray actions do not change saved boot media. Disk changes require no loaded guest.",
            if cd {
                vec!["Status", "Insert CD", "Next CD", "Empty tray", "Cancel"]
            } else {
                vec!["Status", "Commit", "Reset overlay", "Cancel"]
            },
            choice,
        ),
        Dialog::Folder { folder, choice } => (
            folder.title(),
            if folder == crate::network::Folder::Nfs {
                "The guest can read and write files in the selected folder."
            } else {
                "Serve files read-only through the NAT gateway."
            },
            vec!["Choose folder", "Enter path", "Disable", "Cancel"],
            choice,
        ),
        Dialog::Reset { confirm } => (
            "Reset this machine?",
            "The current guest session will be lost. Paused machines stay paused.",
            vec!["Reset", "Cancel"],
            usize::from(!confirm),
        ),
        Dialog::Firmware { choice } => (
            "Choose PROM firmware for this machine",
            "Choose an external ROM or use the firmware bundled with Iris. Extract ZIPs first.",
            vec!["Choose file", "Use bundled", "Cancel"],
            choice,
        ),
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
            "Save machine options and restart?",
            "Shut down the guest OS first. The current session will be lost.",
            vec!["Restart", "Cancel"],
            usize::from(!confirm),
        ),
    };
    let buttons_width = buttons.len() as i32 * 136 + (buttons.len() as i32 - 1) * 8;
    let width = (title.len().max(detail.len()) as i32 * 8).max(buttons_width) + 48;
    let height = 156;
    let scale = (WIDTH as f32 / (width + 16) as f32)
        .min(HEIGHT as f32 / (height + 16) as f32)
        .min(1.25);
    shade(canvas, 150)?;
    canvas.set_scale(scale, scale)?;
    let x = ((WIDTH as f32 / scale - width as f32) / 2.0) as i32;
    let y = ((HEIGHT as f32 / scale - height as f32) / 2.0) as i32;
    panel(canvas, Rect::new(x, y, width as u32, height as u32))?;
    text(canvas, x + 24, y + 20, title, TEXT)?;
    text(canvas, x + 24, y + 48, detail, MUTED)?;
    if let Some(action) = action {
        use crate::storage::Operation;
        let target = match action.operation {
            Operation::Info(slot)
            | Operation::LoadCd(slot)
            | Operation::NextCd(slot)
            | Operation::EmptyCd(slot)
            | Operation::QueueCd(slot)
            | Operation::Commit(slot)
            | Operation::Reset(slot) => format!("SCSI {}:{}", slot / 16, slot % 16),
            _ => "Scratch drive".into(),
        };
        let description = action.path.as_ref().map_or(target.clone(), |path| {
            format!("{target}: {}", path.display())
        });
        text(
            canvas,
            x + 24,
            y + 64,
            &short_ascii(&description, ((width - 48) / 8) as usize),
            TEXT,
        )?;
    }
    let start = x + (width - buttons_width) / 2;
    for (index, label) in buttons.iter().enumerate() {
        let button_x = start + index as i32 * 144;
        if index == selected {
            fill(canvas, Rect::new(button_x, y + 80, 136, 24), SELECTED)?;
        }
        canvas.set_draw_color(BORDER);
        canvas.draw_rect(Rect::new(button_x, y + 80, 136, 24))?;
        text(
            canvas,
            button_x + (136 - label.len() as i32 * 8) / 2,
            y + 88,
            label,
            TEXT,
        )?;
    }
    text(
        canvas,
        x + 24,
        y + 128,
        "Left/Right Select   Enter Confirm   Esc Cancel",
        MUTED,
    )?;
    canvas.set_scale(1.0, 1.0)?;
    Ok(())
}

fn draw_status(
    canvas: &mut WindowCanvas,
    settings: &Settings,
    presentation: &Presentation<'_>,
) -> Result<(), sdl3::Error> {
    if !settings.show_status {
        return Ok(());
    }
    let y = (HEIGHT - LED_HEIGHT - HINT_HEIGHT) as i32;
    fill(
        canvas,
        Rect::new(0, y, WIDTH, HINT_HEIGHT),
        Color::RGB(0x10, 0x10, 0x14),
    )?;
    let status = short_ascii(presentation.status, 48);
    let keys = if presentation.captured {
        "  Ctrl+Enter=release  Ctrl+P=pause  F2=machine  F4=shot  F5=reset  F6=gif  F9=options  F12=quit"
    } else {
        "  Ctrl+P=boot/pause  F2=machine  F4=shot  F5=reset  F6=gif  F9=options  F11=full  F12=quit"
    };
    let text_width = (status.len() + keys.len()) as f32 * 8.0;
    let scale = ((WIDTH - 16) as f32 / text_width).min(1.0);
    let x = ((WIDTH as f32 - text_width * scale) / 2.0 / scale) as i32;
    let text_y = (y as f32 + (HINT_HEIGHT as f32 - 8.0 * scale) / 2.0) / scale;
    canvas.set_scale(scale, scale)?;
    let red = Color::RGB(255, 64, 64);
    text(canvas, x, text_y as i32, &status, red)?;
    text(canvas, x + 1, text_y as i32, &status, red)?;
    text(
        canvas,
        x + status.len() as i32 * 8,
        text_y as i32,
        keys,
        Color::RGB(224, 224, 224),
    )?;
    canvas.set_scale(1.0, 1.0)?;
    Ok(())
}

#[derive(Clone, Copy)]
struct LedPalette {
    idle: Color,
    active: Color,
}
// CPU, storage and network follow 1989; RUN uses the siblings' blue power
// indicator, and CD uses the cyan optical-drive palette. Writes flash amber.
const RUN_LED: LedPalette = LedPalette {
    idle: Color::RGB(25, 50, 110),
    active: Color::RGB(90, 160, 255),
};
const CPU_LED: LedPalette = LedPalette {
    idle: Color::RGB(55, 55, 55),
    active: Color::RGB(255, 255, 255),
};
const NET_LED: LedPalette = LedPalette {
    idle: Color::RGB(80, 70, 18),
    active: Color::RGB(255, 224, 32),
};
const DISK_LED: LedPalette = LedPalette {
    idle: Color::RGB(18, 70, 18),
    active: Color::RGB(80, 255, 80),
};
const CD_LED: LedPalette = LedPalette {
    idle: Color::RGB(15, 60, 60),
    active: Color::RGB(80, 220, 220),
};
const WRITE_LED: Color = Color::RGB(255, 200, 90);

fn draw_leds(
    canvas: &mut WindowCanvas,
    settings: &Settings,
    presentation: &Presentation<'_>,
) -> Result<(), sdl3::Error> {
    let y = (HEIGHT - LED_HEIGHT) as i32;
    fill(
        canvas,
        Rect::new(0, y, WIDTH, LED_HEIGHT),
        Color::RGB(18, 18, 18),
    )?;
    fill(canvas, Rect::new(0, y, WIDTH, 1), Color::RGB(50, 50, 50))?;
    if let Some(label) = presentation.capture_label {
        text(canvas, 8, y + 7, label, Color::RGB(255, 96, 80))?;
    }
    let mut lamps = vec![("RUN".to_owned(), presentation.running, false, RUN_LED)];
    for index in 0..settings.machine.cpus as usize {
        lamps.push((
            format!("CPU{index}"),
            presentation.leds & activity::cpu(index) != 0,
            false,
            CPU_LED,
        ));
    }
    lamps.push((
        "NET".into(),
        presentation.leds & activity::NET != 0,
        false,
        NET_LED,
    ));
    if let Some(slot) = settings.media.scratch.slot {
        lamps.push((
            format!("XFR{slot}"),
            presentation.leds & (activity::disk_read(slot) | activity::disk_write(slot)) != 0,
            presentation.leds & activity::disk_write(slot) != 0,
            DISK_LED,
        ));
    }
    let non_storage_lamps = lamps.len();
    for (&id, medium) in &presentation.media.devices {
        let disk = medium.kind == MediaKind::Disk;
        lamps.push((
            format!(
                "{}{}",
                if disk { "HD" } else { "CD" },
                if id < 16 {
                    id.to_string()
                } else {
                    format!("{}:{}", id / 16, id % 16)
                }
            ),
            presentation.leds & (activity::disk_read(id) | activity::disk_write(id)) != 0,
            presentation.leds & activity::disk_write(id) != 0,
            if disk { DISK_LED } else { CD_LED },
        ));
    }
    // Many wide-bus targets cannot all fit in the strip. Group storage lamps
    // by controller and kind in that case, retaining separate read/write colors.
    let lamp_width = |lamps: &[(String, bool, bool, LedPalette)]| {
        lamps
            .iter()
            .map(|(label, ..)| 28 + label.len() as i32 * 8)
            .sum::<i32>()
    };
    if lamp_width(&lamps) > WIDTH as i32 - 200 {
        lamps.truncate(non_storage_lamps);
        for controller in 0..2 {
            for kind in [MediaKind::Disk, MediaKind::Cdrom] {
                let slots: Vec<_> = presentation
                    .media
                    .devices
                    .iter()
                    .filter(|(slot, medium)| **slot / 16 == controller && medium.kind == kind)
                    .map(|(&slot, _)| slot)
                    .collect();
                if slots.is_empty() {
                    continue;
                }
                let reads = slots
                    .iter()
                    .fold(0, |bits, &slot| bits | activity::disk_read(slot));
                let writes = slots
                    .iter()
                    .fold(0, |bits, &slot| bits | activity::disk_write(slot));
                let disk = kind == MediaKind::Disk;
                lamps.push((
                    format!("{}{}:*", if disk { "HD" } else { "CD" }, controller),
                    presentation.leds & (reads | writes) != 0,
                    presentation.leds & writes != 0,
                    if disk { DISK_LED } else { CD_LED },
                ));
            }
        }
    }
    let width: i32 = lamps
        .iter()
        .map(|(label, ..)| 20 + label.len() as i32 * 8)
        .sum::<i32>()
        + (lamps.len() as i32 - 1) * 8;
    let mut x = (WIDTH as i32 - width) / 2;
    let lamp_y = y + (LED_HEIGHT as i32 - 10) / 2;
    for (label, lit, write, palette) in lamps {
        let color = if !lit {
            palette.idle
        } else if write {
            WRITE_LED
        } else {
            palette.active
        };
        let rect = Rect::new(x, lamp_y, 14, 10);
        fill(canvas, rect, color)?;
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.draw_rect(rect)?;
        text(
            canvas,
            x + 20,
            lamp_y + 1,
            &label,
            Color::RGB(205, 205, 205),
        )?;
        x += 20 + label.len() as i32 * 8 + 8;
    }
    Ok(())
}
