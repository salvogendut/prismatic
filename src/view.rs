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
    pub startup_logo: Option<&'a Texture<'a>>,
    pub status: &'a str,
    pub placeholder: Option<&'a str>,
    pub captured: bool,
    pub leds: u128,
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
        if let Some(logo) = presentation.startup_logo {
            let info = logo.query();
            let top = area.height() as i32 / 2 - 250;
            canvas.copy(
                logo,
                None,
                fit(info.width, info.height, Rect::new(350, top, 260, 260)),
            )?;
            for (offset, value, color) in [
                (300, "P R I S M A T I C", TEXT),
                (328, "Happy Years / SGI", BORDER),
                (380, message, MUTED),
                (
                    440,
                    "Ctrl+P Boot/resume  F2 Machine  F5 Reset  F9 Options",
                    MUTED,
                ),
            ] {
                text(
                    canvas,
                    (WIDTH as i32 - value.len() as i32 * 8) / 2,
                    top + offset,
                    value,
                    color,
                )?;
            }
        } else {
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
    }

    draw_debug(canvas, settings, presentation.debug_lines)?;
    draw_status(canvas, settings, &presentation)?;
    draw_leds(canvas, settings, &presentation)?;

    if let Some(session) = &overlay.session {
        draw_overlay(canvas, session, overlay.busy, overlay.action_busy)?;
    }
    if let Some(form) = &overlay.disk_create {
        draw_disk_create(canvas, form, overlay.busy)?;
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
            crate::overlay::TextField::CdUrl(_) => (
                "Remote CD image",
                "HTTP(S) URL to cooked ISO9660/EFS data. Server must support byte ranges; CHDs must be downloaded first.",
            ),
            crate::overlay::TextField::Runtime(topic, row, index) => {
                let field = crate::runtime::fields(
                    topic,
                    index,
                    &overlay.session.as_ref().unwrap().draft.machine,
                )
                .remove(row);
                (field.label, field.hint)
            }
            crate::overlay::TextField::DiskSize => (
                "Disk capacity (MiB)",
                "1 to 1048576 MiB. 1024 MiB = 1 GiB. The guest must partition and format the disk.",
            ),
            crate::overlay::TextField::Label => (
                "Save machine as",
                "Use up to 32 ASCII characters. Existing labels are kept.",
            ),
            crate::overlay::TextField::Network(field) => (field.title(), field.hint()),
            crate::overlay::TextField::Expert(field) => (field.title(), field.hint()),
            crate::overlay::TextField::Extensions(field) => (field.title(), field.hint()),
            crate::overlay::TextField::CheckpointName => (
                "Checkpoint name",
                "1-48 letters, digits, hyphens or underscores. Save never overwrites a checkpoint.",
            ),
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
    if !crate::compat::DIAGNOSTICS || settings.debug_output != DebugOutput::Screen {
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
fn overlay_hints(session: &crate::overlay::Session) -> Vec<String> {
    let hints: Vec<String> = match session.tab {
        Tab::General => vec![
            if session.row == 2 && session.draft.machine.cpu.iris().is_err() {
                "The saved CPU is unavailable in this Iris build. Enter chooses a supported CPU; other labels are preserved.".into()
            } else if session.row == 12 {
                session.draft.machine.prom_boot.hint().into()
            } else if session.row == 13 {
                session.draft.machine.prom_console.hint(session.draft.machine.profile).into()
            } else if session.row == 7 {
                session.draft.machine.prom_path().map(|p| short_ascii(p, 100)).unwrap_or_else(||
                    if crate::compat::FIRMWARE { "The current core bundles firmware. O2, Octane and the newer platforms also accept external PROMs.".into() } else { "This Iris build needs external O2/Octane PROMs. Choose file to select one.".into() })
            } else if session.row >= 10 {
                "Audio and volume apply on Save without restarting. Enter cycles volume down by 10%.".into()
            } else { "Machine changes keep compatible CPU/RAM choices; other values are adjusted.".into() },
            if session.row == 12 {
                "Saved per machine label; applies on cold boot/restart. Boot disk and AutoLoad remain unchanged.".into()
            } else if session.row == 13 {
                "Saved per label; applies on cold boot/restart. Connection status shows active TCP ports and PTYs.".into()
            } else { "Hardware changes ask to restart when saved. Display and audio options apply on Save.".into() },
        ],
        Tab::Media => {
            let mut hints = vec![
                session.media_placement_hint().into(),
                "Suggested roles only; supported targets accept either media type. ID 0: onboard controller (reserved).".into(),
            ];
            if session.draft.machine.profile == iris::config::MachineProfile::IndyIp24 {
                hints.push("Indy internal and external devices share one SCSI bus; each device needs a unique ID.".into());
            }
            hints.push("Hard disks: persistent copy-on-write. CD-ROMs: read-only (ISO9660 or EFS).".into());
            if let Some(medium) = session.media_slot().and_then(|slot| session.draft.media.devices.get(&slot)) {
                let path = short_ascii(&medium.path, 200);
                hints.extend(path.as_bytes().chunks(100).map(|chunk| std::str::from_utf8(chunk).unwrap().to_owned()));
            }
            hints.push("Media changes require restart. Shut down the guest OS before saving.".into());
            hints
        }
        Tab::Advanced if session.advanced_page == crate::expert::Page::Capture => vec![match session.row {
            0 => "Native uses the guest screen size; scaled widths are optional.",
            1 => "GIF sampling rate. F6 starts or stops a recording.",
            _ => "FFmpeg optionally optimizes after recording; the built-in GIF is kept on failure.",
        }.into(), "Shared capture preferences. Save applies to the next recording without restarting.".into()],
        Tab::Advanced if session.advanced_page != crate::expert::Page::Core => vec![crate::expert::hint(session.advanced_page, session.row).into(), "Saved per machine label. Setting changes take effect on the next boot/restart.".into()],
        Tab::Advanced => vec![match session.row {
            0 if !session.draft.machine.newport() => "Non-Newport video modes are controlled by the PROM and guest OS.",
            0 => "Sets the initial Newport video mode; the guest may change it later.",
            1 if !session.draft.machine.newport() => "This machine uses Shadow cache. The Newport model preference is retained.",
            1 => "Auto selects the cache model for this build. Full/Shadow are expert overrides.",
            2 if session.draft.machine.supports_prom_override() => "This machine starts from host time; the saved clock offset applies to Indy/Indigo2.",
            2 => "Shifts the guest clock relative to host time at boot; it then runs normally.",
            3 if session.draft.machine.supports_prom_override() => "This machine's SCSI controller manages interrupt timing automatically.",
            3 => "Deferred is Iris's BSD-compatible default. Immediate changes SCSI timing.",
            4 => "Appends to serial.log in this machine's state directory. Timestamps use host UTC.",
            5 => "Bilinear filtering smooths the display when scaled; applies without restart.",
            _ => "Debug: Off, Screen (latest messages), Console (terminal). Applies on Save.",
        }.into(), "Core options require restart. Turning Tinker off keeps the saved values active.".into()],
        Tab::Extensions if matches!(session.extensions_page, crate::extensions::Page::Runtime(_)) => {
            if let crate::extensions::Page::Runtime(topic) = session.extensions_page {
                vec![crate::runtime::fields(topic, session.runtime_index, &session.draft.machine)[session.row].hint.to_owned()]
            } else { vec![] }
        },
        Tab::Extensions if session.extensions_page == crate::extensions::Page::Devices => vec![match session.row {
            0..=2 => "VINO is modeled on Indy/Indigo2. Host camera opens on boot; unavailable cameras supply black frames and retry.",
            3..=5 => "DaynaPort uses its own NAT subnet and inherits NFS/PCAP at boot. Network edits require restart while enabled; port forwards target onboard Ethernet.",
            6 => "Ultra64 is an N64 development bridge, requiring a compatible external gopher64. Only one bridge instance may run.",
            _ => "Reports active settings. Use the Iris monitor's vino status or scsi dayna commands for capture and packet diagnostics.",
        }.into(), "All device settings are saved per label and require restart.".into()],
        Tab::Extensions if session.extensions_page == crate::extensions::Page::Checkpoints => vec![
            "Experimental native state: single-head Newport Indy/IP22, raw COW disks or diskless. No CDs, scratch or optional devices.".into(),
            "Restores require the same executable, saved configuration and base disk contents. Audio/network sessions and external NFS files are not restored.".into(),
        ],
        Tab::Extensions if session.extensions_page == crate::extensions::Page::Hardware => vec![match session.row {
            0 => "GR2/IMPACT compatibility depends on Iris and the guest OS; Newport is the default on Indy/IP22.",
            1 => "Two Newport heads appear side by side. Screenshots and GIFs include both heads.",
            2 => "CPU count is per processor board. Octane permits 1/2; Tezro permits 1/2/4 depending on the selected part.",
            3 => "Select an exact processor clock/cache variant offered by Iris for this machine.",
            _ => "IP27/IP35 population: RAM in General is per node or C-brick; the status summary shows total RAM.",
        }.into(), "Hardware changes require restart. Turning Tinker off keeps these settings active.".into()],
        Tab::Extensions if session.row == 8 => vec!["Volatile write cache for the selected hard disk. Off syncs every write; On relies on guest flushes and periodic sync. Saved per target; restart required.".into()],
        Tab::Extensions => vec![match session.row {
            0 | 1 => "Scratch is a persistent raw transfer drive with an SGI volume header, in this label's state folder.",
            2 => "Enter cycles the configured disk/CD targets used by the rows below.",
            3 | 4 => "Changer queue edits are drafts and take effect after Save and restart.",
            5 => "Save or discard drafts before live actions. Commit/reset require no loaded guest and confirmation.",
            _ => "Scratch transfers briefly stop devices, then restore CPU state. They do not mount a guest filesystem.",
        }.into(), "Scratch and changer settings require restart. Live storage actions confirm separately.".into()],
        Tab::About => vec![],
        Tab::Network if session.network_page == crate::network::Page::Installation => session.draft.network.installation.hints(),
        Tab::Network => session.draft.network.hints(session.row),
    };
    let hints = session
        .unavailable(session.row)
        .map_or(hints, |reason| vec![reason.into()]);
    hints
        .iter()
        .flat_map(|hint| wrap_ascii(hint, 100))
        .collect()
}

struct OverlayLayout {
    rows_y: i32,
    hint_y: i32,
    footer_y: i32,
    panel_height: u32,
    scale: f32,
}

fn overlay_layout(subtabs: bool, rows: usize, hints: usize) -> OverlayLayout {
    let rows_y = if subtabs { 68 } else { 48 };
    let hint_y = rows_y + rows as i32 * 20 + 8;
    let footer_y = hint_y + hints as i32 * 16 + 12;
    let panel_height = (footer_y + 12) as u32;
    let scale = (WIDTH as f32 / 860.0)
        .min(HEIGHT as f32 / (panel_height + 8).max(510) as f32)
        .min(1.25);
    OverlayLayout {
        rows_y,
        hint_y,
        footer_y,
        panel_height,
        scale,
    }
}

fn subtab_rects(session: &crate::overlay::Session) -> Vec<(&'static str, bool, Rect)> {
    let mut x = 20;
    session
        .subtabs()
        .into_iter()
        .map(|(label, selected)| {
            let width = label.len() as u32 * 8 + 16;
            let rect = Rect::new(x, 36, width, 22);
            x += width as i32 + 4;
            (label, selected, rect)
        })
        .collect()
}

/// Coordinates are in the renderer's logical view, including its letterboxing.
pub fn subtab_at(session: &crate::overlay::Session, point: (f32, f32)) -> Option<usize> {
    let tabs = subtab_rects(session);
    if tabs.is_empty() {
        return None;
    }
    let layout = overlay_layout(true, session.rows().len(), overlay_hints(session).len());
    let (x, y) = (point.0 / layout.scale, point.1 / layout.scale);
    tabs.iter().position(|(_, _, rect)| {
        x >= rect.x() as f32
            && x < rect.right() as f32
            && y >= rect.y() as f32
            && y < rect.bottom() as f32
    })
}

fn draw_overlay(
    canvas: &mut WindowCanvas,
    session: &crate::overlay::Session,
    busy: bool,
    action_busy: bool,
) -> Result<(), sdl3::Error> {
    const PANEL_WIDTH: u32 = 840;
    const LINE_HEIGHT: i32 = 20;
    let rows = session.rows();
    let hints = overlay_hints(session);
    let subtabs = subtab_rects(session);
    let about_lines = [
        "Prismatic - Happy Years interface for Iris",
        "Rust / SDL3",
        concat!("Iris branch: ", env!("PRISMATIC_IRIS_BRANCH")),
        "Iris Indy / Indigo2 / Newport - SCSI disk and CD-ROM support.",
        "Click captures input; Ctrl+Enter releases. Shift+F4/F6/F9/F11/F12: guest key.",
    ];
    let row_count = if session.tab == Tab::About {
        about_lines.len()
    } else {
        rows.len()
    };
    let OverlayLayout {
        rows_y,
        hint_y,
        footer_y,
        panel_height,
        scale,
    } = overlay_layout(!subtabs.is_empty(), row_count, hints.len());
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
    for (label, selected, rect) in &subtabs {
        if *selected {
            fill(canvas, *rect, SELECTED)?;
        }
        text(
            canvas,
            rect.x() + 8,
            rect.y() + 4,
            label,
            if *selected { TEXT } else { MUTED },
        )?;
    }
    for (index, (label, value)) in rows.iter().enumerate() {
        let y = rows_y + index as i32 * LINE_HEIGHT;
        if index == session.row {
            fill(canvas, Rect::new(10, y, PANEL_WIDTH - 4, 16), SELECTED)?;
        }
        let restart = (session.tab == Tab::General && (index < 4 || index == 7 || index >= 12))
            || (session.tab == Tab::Advanced
                && match session.advanced_page {
                    crate::expert::Page::Core => index < 5,
                    crate::expert::Page::Performance => true,
                    crate::expert::Page::Connections => index < 8,
                    crate::expert::Page::Capture => false,
                    crate::expert::Page::Developer => index < 4,
                })
            || (session.tab == Tab::Extensions
                && match session.extensions_page {
                    crate::extensions::Page::Hardware => true,
                    crate::extensions::Page::Storage => {
                        index < 2 || index == 3 || index == 4 || index == 8
                    }
                    crate::extensions::Page::Devices => index < 7,
                    crate::extensions::Page::Checkpoints => false,
                    crate::extensions::Page::Runtime(_) => true,
                });
        let disabled = session.unavailable(index).is_some();
        let grey = Color::RGB(116, 125, 138);
        text(canvas, 20, y, label, if disabled { grey } else { TEXT })?;
        text(
            canvas,
            250,
            y,
            &short_ascii(value, if restart { 44 } else { 72 }),
            if disabled
                || (session.tab == Tab::General
                    && index == 2
                    && session.draft.machine.cpu.iris().is_err())
            {
                grey
            } else {
                Color::RGB(255, 208, 128)
            },
        )?;
        if disabled {
            text(canvas, 620, y, "Unavailable", grey)?;
        } else if restart {
            text(canvas, 620, y, "Requires restart", MUTED)?;
        }
    }
    if session.tab == Tab::About {
        for (index, line) in about_lines.iter().enumerate() {
            text(
                canvas,
                20,
                48 + index as i32 * LINE_HEIGHT,
                &short_ascii(line, 100),
                TEXT,
            )?;
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
            "Operation in progress..."
        } else if busy {
            "Choosing files / checking options...   Esc Cancel"
        } else if !subtabs.is_empty() {
            "Left/Right Section  Tab/Shift+Tab Topic  Up/Down Row  Enter Change  F9 Save  Esc Close"
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

fn draw_disk_create(
    canvas: &mut WindowCanvas,
    form: &crate::overlay::DiskCreate,
    busy: bool,
) -> Result<(), sdl3::Error> {
    shade(canvas, 180)?;
    let (x, y) = (80, 160);
    panel(canvas, Rect::new(x, y, 800, 380))?;
    text(
        canvas,
        x + 24,
        y + 20,
        &format!("New hard disk - SCSI {}:{}", form.id / 16, form.id % 16),
        TEXT,
    )?;
    let rows = [
        format!("Format       {}", form.format.label()),
        format!(
            "Capacity     {} MiB ({:.2} GiB)",
            form.size_mib,
            form.size_mib as f64 / 1024.0
        ),
        format!(
            "Destination  {}",
            form.path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Choose filename...".into())
        ),
        "Create disk and attach to draft".into(),
        "Cancel".into(),
    ];
    for (i, row) in rows.iter().enumerate() {
        let row_y = y + 60 + i as i32 * 28;
        if i == form.row && form.progress.is_none() && !busy {
            fill(canvas, Rect::new(x + 16, row_y - 4, 768, 24), SELECTED)?;
        }
        text(
            canvas,
            x + 24,
            row_y,
            &short_ascii(row, 94),
            if form.progress.is_some() || busy {
                MUTED
            } else {
                TEXT
            },
        )?;
    }
    let status = if busy {
        "Choose a filename in the system dialog. Esc cancels selection.".into()
    } else if let Some(percent) = form.progress {
        if form.cancelling {
            "Cancelling; removing unfinished image...".into()
        } else {
            format!("Creating disk: {percent}%    Esc cancels")
        }
    } else {
        form.error.clone().unwrap_or_else(|| {
            "Up/Down Select   Enter Edit/Apply   Left/Right Format   Esc Cancel".into()
        })
    };
    for (i, line) in wrap_ascii(&status, 94).iter().take(3).enumerate() {
        text(canvas, x + 24, y + 208 + i as i32 * 16, line, BORDER)?;
    }
    text(
        canvas,
        x + 24,
        y + 268,
        "Creates an unpartitioned, zero-filled hard disk with 512-byte sectors.",
        MUTED,
    )?;
    text(
        canvas,
        x + 24,
        y + 288,
        "Creates the host file now. Save options to apply the attachment.",
        MUTED,
    )?;
    text(
        canvas,
        x + 24,
        y + 308,
        "Discard keeps created files. Existing files are never replaced.",
        MUTED,
    )?;
    text(
        canvas,
        x + 24,
        y + 340,
        if cfg!(iris_chd_cow) {
            "Raw uses sparse storage where supported; CHD compresses the empty space."
        } else {
            "CHD: unavailable with this Iris build. Raw disk creation is available."
        },
        MUTED,
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
            if crate::compat::FIRMWARE {
                "Choose an external ROM or use the firmware bundled with Iris. Extract ZIPs first."
            } else {
                "This Iris build requires an external ROM. Bundled firmware is unavailable. Extract ZIPs first."
            },
            vec!["Choose file", "Use bundled", "Cancel"],
            choice,
        ),
        Dialog::Media { choice, .. } => (
            "Choose media for this SCSI target",
            "Select a raw/CHD disk, or ISO9660/EFS/CHD CD-ROM.",
            vec![
                "Hard disk",
                "CD-ROM",
                "Disconnect",
                "New disk...",
                if cfg!(iris_http_media) {
                    "CD URL..."
                } else {
                    "URL unavailable"
                },
                "Cancel",
            ],
            choice,
        ),
        Dialog::Checkpoint { operation, confirm } => (
            operation.title(),
            operation.detail(),
            vec!["Continue", "Cancel"],
            usize::from(!confirm),
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
            if matches!(dialog, Dialog::Firmware { .. }) && index == 1 && !crate::compat::FIRMWARE {
                Color::RGB(116, 125, 138)
            } else {
                TEXT
            },
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
    for index in 0..settings.machine.processor_count() as usize {
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
