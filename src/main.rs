mod display;
mod emulator;
mod input;
mod overlay;
mod settings;
mod smoke;
mod view;

use emulator::{Emulator, Phase, Status};
use input::{GuestInput, KeyRoute};
use iris::config::KeyboardLayout;
use overlay::{Effect, Key, Overlay};
use sdl3::{
    event::{Event, WindowEvent},
    mouse::{MouseButton, MouseWheelDirection},
    pixels::PixelFormat,
    render::ScaleMode,
};
use settings::Settings;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Options {
    config: Option<PathBuf>,
    smoke_test: bool,
    boot_smoke_test: bool,
    input_smoke_test: bool,
    state_dir: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    keyboard: KeyboardLayout,
}

impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => {
                    println!(
                        "Prismatic - Happy Years interface for Iris\n\nUsage: prismatic [--config PATH] [--state-dir PATH] [--keyboard ansi|iso|jis] [--smoke-test | --boot-smoke-test | --input-smoke-test] [--screenshot PATH.bmp]\n\nClick display: capture input  Ctrl+Enter: release\nF9 Options  F11 Fullscreen  F12 Quit\nShift+F9/F11/F12: send a bare guest function-key tap while captured\n\nBoots a diskless Indy using the embedded PROM. Audio is not connected yet.\n--keyboard selects the physical keyboard type (default: ansi).\n--smoke-test renders the UI without starting Iris.\n--boot-smoke-test waits for guest display output, captures after ten more seconds, and stops Iris.\n--input-smoke-test drives the PROM through SDL events, checks capture, and stops Iris.\n--state-dir overrides the directory for machine NVRAM.\n--config overrides the desktop preferences path."
                    );
                    return Ok(None);
                }
                Some("--version" | "-V") => {
                    println!("prismatic {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                Some("--config") => {
                    options.config = Some(args.next().ok_or("--config requires a path")?.into())
                }
                Some("--smoke-test") => options.smoke_test = true,
                Some("--boot-smoke-test") => options.boot_smoke_test = true,
                Some("--input-smoke-test") => options.input_smoke_test = true,
                Some("--keyboard") => {
                    options.keyboard = match args.next().as_deref().and_then(|arg| arg.to_str()) {
                        Some("ansi") => KeyboardLayout::Ansi,
                        Some("iso") => KeyboardLayout::Iso,
                        Some("jis") => KeyboardLayout::Jis,
                        _ => return Err("--keyboard requires ansi, iso or jis".into()),
                    };
                }
                Some("--state-dir") => {
                    options.state_dir =
                        Some(args.next().ok_or("--state-dir requires a path")?.into());
                }
                Some("--screenshot") => {
                    options.screenshot =
                        Some(args.next().ok_or("--screenshot requires a path")?.into())
                }
                _ => {
                    return Err(format!(
                        "Unknown argument: {} (use --help)",
                        arg.to_string_lossy()
                    ));
                }
            }
        }
        if [
            options.smoke_test,
            options.boot_smoke_test,
            options.input_smoke_test,
        ]
        .into_iter()
        .filter(|&set| set)
        .count()
            > 1
        {
            return Err("Choose only one smoke test".into());
        }
        if options.screenshot.is_some()
            && !options.smoke_test
            && !options.boot_smoke_test
            && !options.input_smoke_test
        {
            return Err("--screenshot requires a smoke test".into());
        }
        if (options.boot_smoke_test || options.input_smoke_test) && options.state_dir.is_some() {
            return Err(
                "Boot/input smoke tests always use temporary machine state; omit --state-dir"
                    .into(),
            );
        }
        Ok(Some(options))
    }
}

fn release_input(
    guest: &mut GuestInput,
    mouse: &sdl3::mouse::MouseUtil,
    window: &sdl3::video::Window,
) {
    guest.release();
    mouse.set_relative_mouse_mode(window, false);
}

fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let machine_smoke = options.boot_smoke_test || options.input_smoke_test;
    let testing = options.smoke_test || machine_smoke;
    // Keep test preferences and NVRAM away from the normal machine state.
    let temporary = if testing {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let preferences = match &temporary {
        Some(directory) => directory.path().to_owned(),
        None => sdl3::filesystem::get_pref_path("", "prismatic")?,
    };
    let path = options
        .config
        .unwrap_or_else(|| preferences.join("prismatic.toml"));
    let state_dir = options
        .state_dir
        .unwrap_or_else(|| preferences.join("indy"));
    let mut settings = Settings::load(&path)?;
    let mut window = video
        .window("Prismatic - Happy Years", view::WIDTH, view::HEIGHT)
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()?;
    window.set_minimum_size(720, 540)?;
    let mut canvas = window.into_canvas();
    canvas.set_logical_size(
        view::WIDTH,
        view::HEIGHT,
        sdl3::sys::render::SDL_RendererLogicalPresentation::LETTERBOX,
    )?;
    let creator = canvas.texture_creator();
    let mut frame = Arc::new(display::Frame::preview());
    let mut texture =
        creator.create_texture_streaming(PixelFormat::RGBA32, frame.width, frame.height)?;
    texture.update(None, &frame.rgba, frame.width as usize * 4)?;
    let mut overlay = Overlay::default();
    if options.smoke_test {
        overlay.key(Key::Options, &settings);
    }
    let mut emulator = if options.smoke_test {
        None
    } else {
        Some(Emulator::spawn(&state_dir, options.keyboard)?)
    };
    let mouse = sdl.mouse();
    let mut guest = GuestInput::default();
    let mut status = Status::default();
    let mut events = sdl.event_pump()?;
    let event_sender = sdl.event()?;
    let mut input_smoke = smoke::InputSmoke::default();
    let mut fullscreen = false;
    let mut title_captured = false;
    let mut toast: Option<(String, Instant)> = None;
    let mut redraw = true;
    let mut frames = 0;
    let mut guest_frame_received = false;
    let mut first_content: Option<Instant> = None;
    let started = Instant::now();
    let mut stopping: Option<Instant> = None;
    let mut failure: Option<String> = None;

    loop {
        guest.attach(emulator.as_ref().and_then(Emulator::input));
        if !guest.captured && mouse.relative_mouse_mode(canvas.window()) {
            mouse.set_relative_mouse_mode(canvas.window(), false);
        }
        let timeout = if options.smoke_test {
            1
        } else if emulator.is_some() {
            16
        } else {
            100
        };
        let first = events.wait_event_timeout(Duration::from_millis(timeout));
        for event in first.into_iter().chain(events.poll_iter()) {
            redraw = true;
            if stopping.is_some() {
                continue;
            }
            let key = match event {
                Event::Quit { .. }
                | Event::Window {
                    win_event: WindowEvent::CloseRequested,
                    ..
                } => Some(Key::Quit),
                Event::Window {
                    win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized,
                    ..
                } => {
                    release_input(&mut guest, &mouse, canvas.window());
                    None
                }
                Event::KeyDown {
                    scancode: Some(scan),
                    keymod,
                    repeat,
                    ..
                } => {
                    let modal = overlay.session.is_some() || overlay.dialog.is_some();
                    match input::route_key(scan, keymod, repeat, guest.captured, modal) {
                        KeyRoute::Guest => {
                            guest.key(scan, true, repeat);
                            None
                        }
                        KeyRoute::FunctionTap => {
                            guest.function_tap(scan);
                            None
                        }
                        KeyRoute::Release => {
                            release_input(&mut guest, &mouse, canvas.window());
                            None
                        }
                        KeyRoute::Ignore => None,
                        KeyRoute::Host(key) => Some(key),
                        KeyRoute::Fullscreen => {
                            match canvas.window_mut().set_fullscreen(!fullscreen) {
                                Ok(()) => fullscreen = !fullscreen,
                                Err(e) => {
                                    eprintln!("{e}");
                                    toast =
                                        Some((format!("Fullscreen failed: {e}"), Instant::now()));
                                }
                            }
                            None
                        }
                    }
                }
                Event::KeyUp {
                    scancode: Some(scan),
                    ..
                } => {
                    guest.key(scan, false, false);
                    None
                }
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if guest.captured {
                        guest.button(mouse_btn, true);
                    } else if mouse_btn == MouseButton::Left
                        && guest.available()
                        && guest_frame_received
                        && status.phase == Phase::Running
                        && overlay.session.is_none()
                        && overlay.dialog.is_none()
                    {
                        let point = display::window_to_view(
                            (x, y),
                            canvas.window().size(),
                            canvas.output_size()?,
                            (view::WIDTH, view::HEIGHT),
                        );
                        let area = view::guest_rect(&frame, &settings);
                        if point.is_some_and(|(x, y)| {
                            x >= area.x() as f32
                                && y >= area.y() as f32
                                && x < area.right() as f32
                                && y < area.bottom() as f32
                        }) {
                            mouse.set_relative_mouse_mode(canvas.window(), true);
                            if mouse.relative_mouse_mode(canvas.window()) {
                                guest.capture();
                            } else {
                                toast = Some((
                                    format!("Mouse capture failed: {}", sdl3::get_error()),
                                    Instant::now(),
                                ));
                            }
                        }
                    }
                    None
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    guest.button(mouse_btn, false);
                    None
                }
                Event::MouseMotion { xrel, yrel, .. } => {
                    guest.motion(xrel, yrel);
                    None
                }
                Event::MouseWheel { y, direction, .. } => {
                    guest.scroll(if direction == MouseWheelDirection::Flipped {
                        -y
                    } else {
                        y
                    });
                    None
                }
                _ => None,
            };
            if let Some(key) = key {
                if matches!(key, Key::Options | Key::Quit) {
                    release_input(&mut guest, &mouse, canvas.window());
                }
                match overlay.key(key, &settings) {
                    Effect::None => {}
                    Effect::Quit => {
                        stopping = Some(Instant::now());
                        overlay.saved();
                    }
                    Effect::Save(draft) => {
                        match draft.save(&path) {
                            Ok(()) => {
                                settings = draft;
                                overlay.saved();
                                toast = Some(("Options saved".into(), Instant::now()));
                            }
                            Err(error) => {
                                eprintln!("{error}");
                                toast = Some(("Could not save options. Draft retained; see terminal for details.".into(), Instant::now()));
                            }
                        }
                    }
                }
            }
        }
        if let Some(machine) = &mut emulator {
            let next = machine.status();
            if next != status {
                if next.monitor != status.monitor
                    && let Some(address) = next.monitor
                {
                    eprintln!("Prismatic: Iris monitor at {address}");
                }
                if let Phase::Failed(error) = &next.phase {
                    eprintln!("Prismatic: {error}");
                    failure = Some(error.clone());
                    toast = Some((
                        "Iris failed. See terminal for details; F12 closes the window.".into(),
                        Instant::now(),
                    ));
                    if machine_smoke {
                        stopping = Some(Instant::now());
                    }
                }
                status = next;
                if status.phase != Phase::Running {
                    release_input(&mut guest, &mouse, canvas.window());
                }
                redraw = true;
            }
            if let Some(next) = machine.frames.take() {
                if (next.width, next.height) != (frame.width, frame.height) {
                    texture = creator.create_texture_streaming(
                        PixelFormat::RGBA32,
                        next.width,
                        next.height,
                    )?;
                }
                texture.update(None, &next.rgba, next.width as usize * 4)?;
                if machine_smoke && first_content.is_none() && has_picture(&next) {
                    first_content = Some(Instant::now());
                    eprintln!(
                        "Prismatic: guest display content received ({}x{})",
                        next.width, next.height
                    );
                }
                frame = next;
                guest_frame_received = true;
                redraw = true;
            }
        }
        if machine_smoke && stopping.is_none() && started.elapsed() >= Duration::from_secs(120) {
            failure = Some("PROM smoke test timed out after 120 seconds".into());
            stopping = Some(Instant::now());
            redraw = true;
        }
        let input_done = if options.input_smoke_test
            && stopping.is_none()
            && let Some(time) = first_content
        {
            match input_smoke.advance(
                time.elapsed(),
                &event_sender,
                canvas.window(),
                guest.captured,
                overlay.session.is_some() || overlay.dialog.is_some(),
            ) {
                Ok(done) => done,
                Err(error) => {
                    failure = Some(error);
                    stopping = Some(Instant::now());
                    redraw = true;
                    false
                }
            }
        } else {
            false
        };
        let capture_boot = (options.boot_smoke_test
            && stopping.is_none()
            && first_content.is_some_and(|time| time.elapsed() >= Duration::from_secs(10)))
            || input_done;
        if guest.captured != title_captured {
            canvas.window_mut().set_title(if guest.captured {
                "Prismatic - Input captured (Ctrl+Enter releases)"
            } else {
                "Prismatic - Happy Years"
            })?;
            title_captured = guest.captured;
            redraw = true;
        }
        if toast
            .as_ref()
            .is_some_and(|(_, time)| time.elapsed() >= Duration::from_secs(4))
        {
            toast = None;
            redraw = true;
        }
        if redraw || options.smoke_test || capture_boot {
            texture.set_scale_mode(if settings.smoothing {
                ScaleMode::Linear
            } else {
                ScaleMode::Nearest
            });
            let label = if stopping.is_some() {
                "Stopping Indy..."
            } else if options.smoke_test {
                "PRISMATIC / UI PREVIEW"
            } else {
                status.phase.label()
            };
            let placeholder = if options.smoke_test {
                Some("Display preview - no machine connected")
            } else if !guest_frame_received || matches!(status.phase, Phase::Failed(_)) {
                Some(label)
            } else {
                None
            };
            view::draw(
                &mut canvas,
                &texture,
                &frame,
                &settings,
                &overlay,
                toast.as_ref().map(|(text, _)| text.as_str()),
                view::Presentation {
                    status: label,
                    placeholder,
                    captured: guest.captured,
                },
            )?;
            frames += 1;
            if (capture_boot || (options.smoke_test && frames == 3))
                && let Some(path) = &options.screenshot
            {
                canvas.read_pixels(None)?.save_bmp(path)?;
            }
            canvas.present();
            redraw = false;
        }
        if capture_boot {
            eprintln!("Prismatic: PROM smoke test captured; stopping Iris");
            stopping = Some(Instant::now());
            redraw = true;
        }
        if options.smoke_test && frames >= 3 {
            break;
        }
        if let Some(time) = stopping {
            release_input(&mut guest, &mouse, canvas.window());
            if let Some(machine) = &mut emulator {
                machine.request_stop();
                if machine.finished() {
                    if let Phase::Failed(error) = machine.status().phase {
                        failure = Some(error);
                    }
                    break;
                }
                if time.elapsed() >= Duration::from_secs(10) {
                    failure = Some("Iris did not finish stopping within ten seconds".into());
                    break;
                }
            } else {
                break;
            }
        }
    }
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}

fn has_picture(frame: &display::Frame) -> bool {
    frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel != &frame.rgba[..4])
}

fn main() -> std::process::ExitCode {
    let result = Options::parse()
        .map_err(Into::into)
        .and_then(|options| match options {
            Some(options) => run(options),
            None => Ok(()),
        });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Prismatic: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
