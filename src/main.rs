mod display;
mod overlay;
mod settings;
mod view;

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
    time::{Duration, Instant},
};

#[derive(Default)]
struct Options {
    config: Option<PathBuf>,
    smoke_test: bool,
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
                        "Prismatic - Happy Years interface for Iris\n\nUsage: prismatic [--config PATH] [--smoke-test [--screenshot PATH.bmp]]\n\nF9 Options  F11 Fullscreen  F12 Quit\n\nThis foundation build displays a preview; Iris is not connected yet.\n--smoke-test renders three frames with the options open, then exits.\n--config overrides the desktop preferences path."
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
        if options.screenshot.is_some() && !options.smoke_test {
            return Err("--screenshot requires --smoke-test".into());
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
    // Smoke tests use temporary preferences even without --config.
    let temporary = if options.smoke_test && options.config.is_none() {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let path = match options.config {
        Some(path) => path,
        None if temporary.is_some() => temporary.as_ref().unwrap().path().join("prismatic.toml"),
        None => sdl3::filesystem::get_pref_path("", "prismatic")?.join("prismatic.toml"),
    };
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
    let frame = display::Frame::preview();
    let mut texture =
        creator.create_texture_streaming(PixelFormat::RGBA32, frame.width, frame.height)?;
    texture.update(None, &frame.rgba, frame.width as usize * 4)?;
    let mut overlay = Overlay::default();
    if options.smoke_test {
        overlay.key(Key::Options, &settings);
    }
    let mut events = sdl.event_pump()?;
    let mut fullscreen = false;
    let mut toast: Option<(String, Instant)> = None;
    let mut redraw = true;
    let mut frames = 0;

    'running: loop {
        let first = events.wait_event_timeout(Duration::from_millis(if options.smoke_test {
            1
        } else {
            100
        }));
        for event in first.into_iter().chain(events.poll_iter()) {
            redraw = true;
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
                    Effect::Quit => break 'running,
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
        if toast
            .as_ref()
            .is_some_and(|(_, time)| time.elapsed() >= Duration::from_secs(4))
        {
            toast = None;
            redraw = true;
        }
        if redraw || options.smoke_test {
            texture.set_scale_mode(if settings.smoothing {
                ScaleMode::Linear
            } else {
                ScaleMode::Nearest
            });
            view::draw(
                &mut canvas,
                &texture,
                &frame,
                &settings,
                &overlay,
                toast.as_ref().map(|(text, _)| text.as_str()),
            )?;
            frames += 1;
            if options.smoke_test
                && frames == 3
                && let Some(path) = &options.screenshot
            {
                canvas.read_pixels(None)?.save_bmp(path)?;
            }
            canvas.present();
            redraw = false;
        }
        if options.smoke_test && frames >= 3 {
            break;
        }
    }
    Ok(())
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
