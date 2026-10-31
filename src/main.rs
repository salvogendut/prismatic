mod display;
mod emulator;
mod overlay;
mod settings;
mod view;

use emulator::{Emulator, Phase, Status};
use overlay::{Effect, Key, Overlay};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
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
    state_dir: Option<PathBuf>,
    screenshot: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => {
                    println!(
                        "Prismatic - Happy Years interface for Iris\n\nUsage: prismatic [--config PATH] [--state-dir PATH] [--smoke-test | --boot-smoke-test] [--screenshot PATH.bmp]\n\nF9 Options  F11 Fullscreen  F12 Quit\n\nBoots a diskless Indy using the embedded PROM. Guest input and audio are not connected yet.\n--smoke-test renders the UI without starting Iris.\n--boot-smoke-test waits for guest display output, captures after ten more seconds, and stops Iris.\n--state-dir overrides the directory for machine NVRAM.\n--config overrides the desktop preferences path."
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
        if options.smoke_test && options.boot_smoke_test {
            return Err("Choose either --smoke-test or --boot-smoke-test".into());
        }
        if options.screenshot.is_some() && !options.smoke_test && !options.boot_smoke_test {
            return Err("--screenshot requires --smoke-test or --boot-smoke-test".into());
        }
        if options.boot_smoke_test && options.state_dir.is_some() {
            return Err(
                "--boot-smoke-test always uses temporary machine state; omit --state-dir".into(),
            );
        }
        Ok(Some(options))
    }
}

fn keycode(key: Keycode) -> Option<Key> {
    Some(match key {
        Keycode::F9 => Key::Options,
        Keycode::Escape => Key::Escape,
        Keycode::Return | Keycode::KpEnter => Key::Enter,
        Keycode::Left => Key::Left,
        Keycode::Right => Key::Right,
        Keycode::Up => Key::Up,
        Keycode::Down => Key::Down,
        Keycode::F12 => Key::Quit,
        _ => return None,
    })
}

fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let testing = options.smoke_test || options.boot_smoke_test;
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
        Some(Emulator::spawn(&state_dir)?)
    };
    let mut status = Status::default();
    let mut events = sdl.event_pump()?;
    let mut fullscreen = false;
    let mut toast: Option<(String, Instant)> = None;
    let mut redraw = true;
    let mut frames = 0;
    let mut guest_frame_received = false;
    let mut first_content: Option<Instant> = None;
    let started = Instant::now();
    let mut stopping: Option<Instant> = None;
    let mut failure: Option<String> = None;

    loop {
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
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if repeat
                        && !matches!(
                            key,
                            Keycode::Left | Keycode::Right | Keycode::Up | Keycode::Down
                        )
                    {
                        continue;
                    }
                    if key == Keycode::F11 {
                        match canvas.window_mut().set_fullscreen(!fullscreen) {
                            Ok(()) => fullscreen = !fullscreen,
                            Err(e) => {
                                eprintln!("{e}");
                                toast = Some((format!("Fullscreen failed: {e}"), Instant::now()));
                            }
                        }
                        None
                    } else {
                        keycode(key)
                    }
                }
                _ => None,
            };
            if let Some(key) = key {
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
                    if options.boot_smoke_test {
                        stopping = Some(Instant::now());
                    }
                }
                status = next;
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
                if options.boot_smoke_test && first_content.is_none() && has_picture(&next) {
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
        if options.boot_smoke_test
            && stopping.is_none()
            && started.elapsed() >= Duration::from_secs(120)
        {
            failure = Some("PROM display smoke test timed out after 120 seconds".into());
            stopping = Some(Instant::now());
            redraw = true;
        }
        let capture_boot = options.boot_smoke_test
            && stopping.is_none()
            && first_content.is_some_and(|time| time.elapsed() >= Duration::from_secs(10));
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
            eprintln!("Prismatic: PROM display smoke test captured; stopping Iris");
            stopping = Some(Instant::now());
            redraw = true;
        }
        if options.smoke_test && frames >= 3 {
            break;
        }
        if let Some(time) = stopping {
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
