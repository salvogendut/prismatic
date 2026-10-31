mod activity;
mod capture;
mod diagnostics;
mod display;
mod emulator;
mod input;
mod media;
mod overlay;
mod settings;
mod smoke;
mod sound;
mod view;

use emulator::{Emulator, Phase, Status};
use input::{GuestInput, KeyRoute};
use iris::config::KeyboardLayout;
use media::{Completion, MediaJobs};
use overlay::{Effect, Key, Overlay};
use sdl3::{
    event::{Event, WindowEvent},
    mouse::{MouseButton, MouseWheelDirection},
    pixels::PixelFormat,
    render::ScaleMode,
};
use settings::{MachineSettings, Settings};
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
    settings_smoke_test: bool,
    media_smoke_test: bool,
    advanced_smoke_test: bool,
    state_dir: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    keyboard: Option<KeyboardLayout>,
}

impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => {
                    println!(
                        "Prismatic - Happy Years interface for Iris

Usage: prismatic [--config PATH] [--state-dir PATH] [--keyboard ansi|iso|jis]
                 [--smoke-test | --boot-smoke-test | --input-smoke-test |
                  --settings-smoke-test | --media-smoke-test | --advanced-smoke-test]
                 [--screenshot PATH.bmp]

Click display: capture input  Ctrl+Enter: release
F4 Screenshot  F6 Start/stop GIF  F9 Options  F11 Fullscreen  F12 Quit
Shift+F4/F6/F9/F11/F12: send a bare guest function-key tap while captured
Ctrl+Plus/Minus (also Ctrl+= and keypad +/-): scale window by 25% and save

General: machine, RAM, CPU and keyboard. Core changes require restart.
Tinker > Advanced: resolution, cache, clock offset, SCSI timing, serial and debug output.
Advanced sets GIF resolution (Native/320/480/640), frame rate (10/20/25) and encoder.
Captures save the guest display as timestamped PPM/GIF files in the working directory.
Media: raw/CHD hard disks and ISO9660/EFS/CHD CD-ROMs.
Boots diskless by default with bundled firmware. Guest audio is not connected yet.
--keyboard overrides the saved keyboard type for this session.
--smoke-test renders the UI without starting Iris.
--boot-smoke-test waits for guest display output, captures ten seconds later, and stops.
--input-smoke-test drives the PROM through SDL events and checks capture.
--settings-smoke-test edits General options and tests a confirmed machine restart.
--media-smoke-test attaches temporary disk/CD fixtures through Media and restarts.
--advanced-smoke-test tests Tinker, Advanced options and confirmed restart.
--state-dir overrides Indy NVRAM storage; Indigo2 uses its indigo2 subdirectory.
--config overrides prismatic.cfg in the per-user application directory.
Menu Save persists General, Media, Advanced and display settings to this file."
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
                Some("--settings-smoke-test") => options.settings_smoke_test = true,
                Some("--media-smoke-test") => options.media_smoke_test = true,
                Some("--advanced-smoke-test") => options.advanced_smoke_test = true,
                Some("--keyboard") => {
                    options.keyboard =
                        Some(match args.next().as_deref().and_then(|arg| arg.to_str()) {
                            Some("ansi") => KeyboardLayout::Ansi,
                            Some("iso") => KeyboardLayout::Iso,
                            Some("jis") => KeyboardLayout::Jis,
                            _ => return Err("--keyboard requires ansi, iso or jis".into()),
                        });
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
            options.settings_smoke_test,
            options.media_smoke_test,
            options.advanced_smoke_test,
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
            && !options.settings_smoke_test
            && !options.media_smoke_test
            && !options.advanced_smoke_test
        {
            return Err("--screenshot requires a smoke test".into());
        }
        if (options.boot_smoke_test
            || options.input_smoke_test
            || options.settings_smoke_test
            || options.media_smoke_test
            || options.advanced_smoke_test)
            && options.state_dir.is_some()
        {
            return Err(
                "Machine smoke tests always use temporary machine state; omit --state-dir".into(),
            );
        }
        if (options.settings_smoke_test || options.media_smoke_test || options.advanced_smoke_test)
            && options.config.is_some()
        {
            return Err("Menu smoke tests save temporary preferences; omit --config".into());
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
    let machine_smoke = options.boot_smoke_test
        || options.input_smoke_test
        || options.settings_smoke_test
        || options.media_smoke_test
        || options.advanced_smoke_test;
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
    let legacy = options
        .config
        .is_none()
        .then(|| preferences.join("prismatic.toml"));
    let path = options
        .config
        .unwrap_or_else(|| preferences.join("prismatic.cfg"));
    let mut settings = Settings::open(&path, legacy.as_deref())?;
    let debug_log = Arc::new(diagnostics::DebugLog::new(settings.debug_output));
    iris::diagnostics::set_sink(Some(debug_log.clone()));
    let mut debug_lines = Vec::new();
    iris::diagnostics::stderr(format_args!("Prismatic: configuration {}", path.display()));
    if let Some(keyboard) = options.keyboard {
        settings.machine.keyboard = keyboard;
    }
    let state_dir =
        |hardware: &MachineSettings| hardware.state_dir(&preferences, options.state_dir.as_deref());
    let mut active = settings.clone();
    if machine_smoke && !active.media.devices.is_empty() {
        return Err("Machine smoke tests start with disconnected media".into());
    }
    let (width, height) = settings.window_scale.dimensions(view::WIDTH, view::HEIGHT);
    let mut window = video
        .window("Prismatic - Happy Years", width, height)
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
        Some(Emulator::spawn(
            &state_dir(&active.machine),
            &active.machine,
            &active.media,
            &active.advanced,
        )?)
    };
    let mouse = sdl.mouse();
    let mut guest = GuestInput::default();
    let mut media_jobs = MediaJobs::default();
    let mut leds = activity::Leds::default();
    let mut status = Status::default();
    let mut events = sdl.event_pump()?;
    let event_sender = sdl.event()?;
    let mut input_smoke = smoke::InputSmoke::default();
    let mut settings_smoke = smoke::SettingsSmoke::default();
    let mut advanced_smoke = smoke::AdvancedSmoke::default();
    let mut media_smoke = if options.media_smoke_test {
        Some(smoke::MediaSmoke::new(&preferences)?)
    } else {
        None
    };
    let mut generation = 0;
    let mut fullscreen = false;
    let mut pending_window_scale = false;
    let mut title_captured = false;
    let mut toast: Option<(String, Instant)> = None;
    let mut redraw = true;
    let mut frames = 0;
    let mut guest_frame_received = false;
    let mut first_content: Option<Instant> = None;
    let mut captures = capture::Captures::new(std::env::current_dir()?);
    let mut shutter = sound::Shutter::default();
    let started = Instant::now();
    let mut stopping: Option<Instant> = None;
    // A new worker is constructed only after the previous worker has joined.
    let mut restart: Option<(Settings, bool)> = None;
    let mut failure: Option<String> = None;

    loop {
        for result in captures.poll() {
            let message = match result {
                Ok(report) => {
                    if report.kind == capture::Kind::Screenshot
                        && let Err(error) = shutter.play(&sdl)
                    {
                        eprintln!(
                            "Prismatic: screenshot saved, but shutter sound unavailable: {error}"
                        );
                    }
                    report.message()
                }
                Err(error) => error,
            };
            eprintln!("Prismatic: {message}");
            toast = Some((message, Instant::now()));
            redraw = true;
        }
        let mut ready_save = None;
        if let Some(result) = media_jobs.poll() {
            redraw = true;
            match result {
                Ok(Completion::Picked { id, medium }) => overlay.picked(id, medium),
                Ok(Completion::PromPicked { profile, path }) => overlay.prom_picked(profile, path),
                Ok(Completion::Validated {
                    settings,
                    confirmed,
                }) => ready_save = Some((settings, confirmed)),
                Ok(Completion::Cancelled) => overlay.busy = false,
                Err(error) => {
                    eprintln!("Prismatic: {error}");
                    overlay.save_failed();
                    toast = Some((error, Instant::now()));
                }
            }
        }
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
        let first = events.wait_event_timeout(
            captures
                .wait_time(Duration::from_millis(timeout))
                .max(Duration::from_millis(1)),
        );
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
                    keycode,
                    keymod,
                    repeat,
                    ..
                } => {
                    let modal = overlay.session.is_some() || overlay.dialog.is_some();
                    let route = input::scale_shortcut(scan, keycode, keymod, repeat)
                        .unwrap_or_else(|| {
                            input::route_key(scan, keymod, repeat, guest.captured, modal)
                        });
                    match route {
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
                        KeyRoute::WindowScale(increase) => {
                            let scale = settings.window_scale.step(increase);
                            match settings.save_window_scale(&path, scale) {
                                Ok(()) => {
                                    overlay.window_scale_saved(scale);
                                    pending_window_scale = true;
                                    let suffix = if fullscreen {
                                        " (after fullscreen)"
                                    } else {
                                        ""
                                    };
                                    toast = Some((
                                        format!("Window scale: {}%{suffix}", scale.percent()),
                                        Instant::now(),
                                    ));
                                }
                                Err(error) => {
                                    eprintln!("Prismatic: {error}");
                                    toast = Some((
                                        "Could not save window scale; see terminal".into(),
                                        Instant::now(),
                                    ));
                                }
                            }
                            None
                        }
                        KeyRoute::Screenshot => {
                            let result = if guest_frame_received {
                                captures.screenshot(frame.clone())
                            } else {
                                Err("Screenshot unavailable - waiting for the guest display".into())
                            };
                            let message = match result {
                                Ok(()) => "Saving screenshot...".into(),
                                Err(error) => {
                                    eprintln!("Prismatic: {error}");
                                    error
                                }
                            };
                            toast = Some((message, Instant::now()));
                            None
                        }
                        KeyRoute::Gif => {
                            let result = if guest_frame_received || captures.is_recording() {
                                captures.toggle_gif(frame.clone(), &settings.gif)
                            } else {
                                Err("GIF unavailable - waiting for the guest display".into())
                            };
                            let message = match result {
                                Ok(message) => message.to_owned(),
                                Err(error) => {
                                    eprintln!("Prismatic: {error}");
                                    error
                                }
                            };
                            toast = Some((message, Instant::now()));
                            None
                        }
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
                                && !view::debug_rect(&settings)
                                    .is_some_and(|rect| rect.contains_point((x as i32, y as i32)))
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
                let effect = overlay.key(key, &settings);
                let confirmed_restart = matches!(effect, Effect::Restart(_));
                match effect {
                    Effect::None => {}
                    Effect::PickProm => {
                        let hardware = &overlay.session.as_ref().unwrap().draft.machine;
                        if let Err(error) = media_jobs.pick_prom(
                            canvas.window(),
                            hardware.profile,
                            hardware.prom_path(),
                        ) {
                            overlay.save_failed();
                            toast = Some((error, Instant::now()));
                        }
                    }
                    Effect::CancelMedia => {
                        media_jobs.cancel();
                        ready_save = None;
                    }
                    Effect::Pick { id, kind } => {
                        let current = overlay
                            .session
                            .as_ref()
                            .and_then(|s| s.draft.media.devices.get(&id))
                            .map(|m| m.path.as_str());
                        if let Some(smoke) = &media_smoke {
                            let medium = smoke
                                .expected
                                .devices
                                .get(&id)
                                .ok_or("Unexpected smoke SCSI target")?;
                            if medium.kind != kind {
                                return Err("Unexpected smoke media kind".into());
                            }
                            media_jobs.pick_path(id, kind, medium.path.clone().into());
                        } else if let Err(error) =
                            media_jobs.pick(canvas.window(), id, kind, current)
                        {
                            overlay.save_failed();
                            toast = Some((error, Instant::now()));
                        }
                    }
                    Effect::Quit => {
                        stopping = Some(Instant::now());
                        overlay.saved();
                    }
                    Effect::Save(draft) | Effect::Restart(draft) => {
                        overlay.busy = true;
                        media_jobs.validate(draft, confirmed_restart);
                    }
                }
            }
        }
        // Fullscreen/maximize transitions can be asynchronous. Keep pumping SDL
        // until the window is floating before requesting its saved dimensions.
        if pending_window_scale
            && !fullscreen
            && canvas.window().fullscreen_state() == sdl3::video::FullscreenType::Off
            && !canvas.window().is_minimized()
        {
            let result = if canvas.window().is_maximized() {
                if canvas.window_mut().restore() {
                    Ok(())
                } else {
                    Err(sdl3::get_error().to_string())
                }
            } else {
                pending_window_scale = false;
                let (width, height) = settings.window_scale.dimensions(view::WIDTH, view::HEIGHT);
                canvas
                    .window_mut()
                    .set_size(width, height)
                    .map_err(|e| e.to_string())
            };
            if let Err(error) = result {
                pending_window_scale = false;
                eprintln!("Prismatic: window resize failed: {error}");
                toast = Some((
                    "Scale saved; window resize failed. See terminal".into(),
                    Instant::now(),
                ));
            }
            redraw = true;
        }
        if let Some((mut draft, confirmed)) = ready_save {
            // File validation runs off-thread and may return a snapshot from
            // before a scaling shortcut. Retain the separately saved scale.
            draft.window_scale = settings.window_scale;
            overlay.busy = false;
            let needs_restart = !options.smoke_test
                && (draft.machine_differs_from(&active)
                    || matches!(status.phase, Phase::Failed(_) | Phase::PoweredOff));
            if let Err(error) = emulator::machine_config(
                &state_dir(&draft.machine),
                &draft.machine,
                &draft.media,
                &draft.advanced,
            ) {
                eprintln!("Prismatic: {error}");
                toast = Some((error, Instant::now()));
                overlay.save_failed();
            } else if needs_restart
                && !confirmed
                && matches!(
                    status.phase,
                    Phase::Starting | Phase::Running | Phase::Halted
                )
            {
                overlay.confirm_restart();
            } else {
                match draft.save(&path) {
                    Ok(()) => {
                        debug_log.set_output(draft.debug_output);
                        settings = draft;
                        overlay.saved();
                        if needs_restart {
                            release_input(&mut guest, &mouse, canvas.window());
                            restart =
                                Some((settings.clone(), matches!(status.phase, Phase::Failed(_))));
                            stopping = Some(Instant::now());
                            toast =
                                Some(("Options saved - restarting machine".into(), Instant::now()));
                        } else {
                            toast = Some(("Options saved".into(), Instant::now()));
                        }
                    }
                    Err(error) => {
                        eprintln!("Prismatic: {error}");
                        overlay.save_failed();
                        toast = Some((
                            "Could not save options. Draft retained; see terminal for details."
                                .into(),
                            Instant::now(),
                        ));
                    }
                }
            }
            redraw = true;
        }
        let pulses = emulator
            .as_ref()
            .map_or(0, |machine| machine.activity.take());
        redraw |= leds.update(pulses, Instant::now());
        if let Some(machine) = &mut emulator {
            let next = machine.status();
            if next != status {
                if next.monitor != status.monitor
                    && let Some(address) = next.monitor
                {
                    iris::diagnostics::stderr(format_args!("Prismatic: Iris monitor at {address}"));
                }
                if let Phase::Failed(error) = &next.phase {
                    eprintln!("Prismatic: {error}");
                    failure = Some(error.clone());
                    toast = Some((
                        "Iris failed. See terminal for details; F12 closes the window.".into(),
                        Instant::now(),
                    ));
                    if machine_smoke && restart.is_none() {
                        stopping = Some(Instant::now());
                    }
                }
                status = next;
                if status.phase != Phase::Running {
                    release_input(&mut guest, &mouse, canvas.window());
                }
                if status.phase == Phase::PoweredOff {
                    guest.attach(None);
                    guest_frame_received = false;
                    leds = activity::Leds::default();
                    frame = Arc::new(display::Frame::blank(frame.width, frame.height));
                    texture.update(None, &frame.rgba, frame.width as usize * 4)?;
                }
                redraw = true;
            }
            if let Some(next) = machine.frames.take()
                && status.phase != Phase::PoweredOff
            {
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
                    iris::diagnostics::stderr(format_args!(
                        "Prismatic: guest display content received ({}x{})",
                        next.width, next.height
                    ));
                }
                frame = next;
                guest_frame_received = true;
                redraw = true;
            }
        }
        if stopping.is_none() && status.phase != Phase::PoweredOff {
            captures.tick(&frame);
        } else {
            captures.stop();
        }
        let smoke_timeout = if options.settings_smoke_test
            || options.media_smoke_test
            || options.advanced_smoke_test
        {
            240
        } else {
            120
        };
        if machine_smoke
            && stopping.is_none()
            && started.elapsed() >= Duration::from_secs(smoke_timeout)
        {
            failure = Some(format!(
                "PROM smoke test timed out after {smoke_timeout} seconds"
            ));
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
        let settings_done = if options.settings_smoke_test && stopping.is_none() {
            match settings_smoke.advance(
                smoke::SettingsSmokeState {
                    generation,
                    elapsed: first_content.map(|time| time.elapsed()),
                    settings: &settings,
                    overlay: &overlay,
                    captured: guest.captured,
                },
                &event_sender,
                canvas.window(),
            ) {
                Ok(done) => {
                    if done && Settings::load(&path)? != settings {
                        failure =
                            Some("Settings smoke preferences do not match applied settings".into());
                        stopping = Some(Instant::now());
                    }
                    done
                }
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
        let media_done = if let Some(smoke) = &mut media_smoke
            && stopping.is_none()
        {
            match smoke.advance(
                smoke::SettingsSmokeState {
                    generation,
                    elapsed: first_content.map(|time| time.elapsed()),
                    settings: &settings,
                    overlay: &overlay,
                    captured: guest.captured,
                },
                &event_sender,
                canvas.window(),
            ) {
                Ok(done) => {
                    if done && Settings::load(&path)? != settings {
                        return Err("Media preferences did not persist".into());
                    }
                    done
                }
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
        let advanced_done = if options.advanced_smoke_test && stopping.is_none() {
            match advanced_smoke.advance(
                smoke::SettingsSmokeState {
                    generation,
                    elapsed: first_content.map(|time| time.elapsed()),
                    settings: &settings,
                    overlay: &overlay,
                    captured: guest.captured,
                },
                &event_sender,
                canvas.window(),
            ) {
                Ok(done) => {
                    if done
                        && (Settings::load(&path)? != settings
                            || !state_dir(&settings.machine).join("serial.log").is_file())
                    {
                        failure =
                            Some("Advanced configuration or serial log was not persisted".into());
                        stopping = Some(Instant::now());
                    }
                    done
                }
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
            || input_done
            || settings_done
            || media_done
            || advanced_done;
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
        if let Some(lines) = debug_log.take_update() {
            debug_lines = lines;
            redraw = true;
        }
        if redraw || options.smoke_test || capture_boot {
            texture.set_scale_mode(if settings.smoothing {
                ScaleMode::Linear
            } else {
                ScaleMode::Nearest
            });
            let running_label = active.machine.summary();
            let label = if stopping.is_some() {
                if restart.is_some() {
                    "Restarting machine..."
                } else {
                    "Stopping machine..."
                }
            } else if options.smoke_test {
                "PRISMATIC / UI PREVIEW"
            } else if status.phase == Phase::Running {
                &running_label
            } else {
                status.phase.label()
            };
            let placeholder = if options.smoke_test {
                Some("Display preview - no machine connected")
            } else if status.phase == Phase::PoweredOff {
                Some("Machine powered off - F9 then Save to start")
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
                    leds: leds.lit,
                    media: &active.media,
                    running: status.phase == Phase::Running && stopping.is_none(),
                    debug_lines: &debug_lines,
                    capture_label: captures.label(),
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
            captures.stop();
            release_input(&mut guest, &mouse, canvas.window());
            let finished = if let Some(machine) = &mut emulator {
                machine.request_stop();
                machine.finished()
            } else {
                true
            };
            if finished && captures.idle() {
                let stop_error =
                    emulator
                        .as_ref()
                        .and_then(|machine| match machine.status().phase {
                            Phase::Failed(error) => Some(error),
                            _ => None,
                        });
                if let Some((next_settings, was_failed)) = restart.take() {
                    // Drop old handles and pixels before creating the replacement.
                    guest.attach(None);
                    emulator = None;
                    guest_frame_received = false;
                    first_content = None;
                    leds = activity::Leds::default();
                    frame = Arc::new(display::Frame::preview());
                    texture = creator.create_texture_streaming(
                        PixelFormat::RGBA32,
                        frame.width,
                        frame.height,
                    )?;
                    texture.update(None, &frame.rgba, frame.width as usize * 4)?;
                    let result = if let Some(error) = stop_error.filter(|_| !was_failed) {
                        Err(format!("Restart aborted: {error}"))
                    } else {
                        Emulator::spawn(
                            &state_dir(&next_settings.machine),
                            &next_settings.machine,
                            &next_settings.media,
                            &next_settings.advanced,
                        )
                    };
                    match result {
                        Ok(machine) => {
                            emulator = Some(machine);
                            active = next_settings;
                            generation += 1;
                            status = Status::default();
                            failure = None;
                            toast = None;
                        }
                        Err(error) => {
                            eprintln!("Prismatic: {error}");
                            status.phase = Phase::Failed(error.clone());
                            failure = Some(error);
                            toast = Some(("Restart failed. Open F9 to revise the settings; see terminal for details.".into(), Instant::now()));
                        }
                    }
                    stopping = if machine_smoke && emulator.is_none() {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    redraw = true;
                } else {
                    if let Some(error) = stop_error {
                        failure = Some(error);
                    }
                    break;
                }
            } else if !finished
                && active.media.devices.is_empty()
                && time.elapsed() >= Duration::from_secs(10)
            {
                failure = Some("Iris did not finish stopping within ten seconds".into());
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
