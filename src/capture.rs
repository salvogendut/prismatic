//! Guest-only captures. File I/O, scaling, quantization and optional FFmpeg run
//! on workers; the UI shares immutable frames through a two-frame queue.
use crate::{
    display::Frame,
    settings::{GifEncoder, GifSettings},
};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

type Job = JoinHandle<Result<Report, String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Screenshot,
    Gif,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Screenshot => "Screenshot",
            Self::Gif => "GIF",
        }
    }
}

#[derive(Debug)]
pub struct Report {
    pub path: PathBuf,
    pub kind: Kind,
    pub warning: Option<String>,
}

impl Report {
    pub fn message(&self) -> String {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(warning) = &self.warning {
            format!("{} saved: {name} ({warning})", self.kind.label())
        } else {
            format!("{} saved: {name}", self.kind.label())
        }
    }
}

struct Sample {
    frame: Arc<Frame>,
    at: Duration,
}

struct Recording {
    sender: Option<SyncSender<Sample>>,
    job: Job,
    started: Instant,
    end: Arc<Mutex<Duration>>,
    period: Duration,
    next: Duration,
    skipped: u64,
}

pub struct Captures {
    directory: PathBuf,
    screenshot: Option<Job>,
    recording: Option<Recording>,
}

impl Captures {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            screenshot: None,
            recording: None,
        }
    }

    pub fn screenshot(&mut self, frame: Arc<Frame>) -> Result<(), String> {
        validate_frame(&frame)?;
        if self.screenshot.is_some() {
            return Err("The previous screenshot is still being saved".into());
        }
        let directory = self.directory.clone();
        self.screenshot = Some(
            thread::Builder::new()
                .name("screenshot".into())
                .spawn(move || {
                    save_ppm(&directory, &frame).map_err(|e| format!("Screenshot failed: {e}"))
                })
                .map_err(|e| format!("Cannot start screenshot writer: {e}"))?,
        );
        Ok(())
    }

    pub fn toggle_gif(
        &mut self,
        frame: Arc<Frame>,
        settings: &GifSettings,
    ) -> Result<&'static str, String> {
        if self.is_recording() {
            self.stop();
            return Ok("Finishing GIF capture...");
        }
        if self.recording.is_some() {
            return Err("The previous GIF is still being saved".into());
        }
        settings.validate()?;
        let size = gif_size(&frame, settings.width)?;
        let settings = settings.clone();
        let period = Duration::from_millis(1000 / settings.fps as u64);
        let directory = self.directory.clone();
        let (sender, receiver) = mpsc::sync_channel(2);
        let end = Arc::new(Mutex::new(Duration::ZERO));
        let worker_end = end.clone();
        let started = Instant::now();
        let job = thread::Builder::new()
            .name("gif-capture".into())
            .spawn(move || {
                save_gif(&directory, frame, size, &settings, receiver, worker_end)
                    .map_err(|e| format!("GIF capture failed: {e}"))
            })
            .map_err(|e| format!("Cannot start GIF encoder: {e}"))?;
        self.recording = Some(Recording {
            sender: Some(sender),
            job,
            started,
            end,
            period,
            next: period,
            skipped: 0,
        });
        Ok("GIF recording - press F6 to stop")
    }

    pub fn is_recording(&self) -> bool {
        self.recording.as_ref().is_some_and(|r| r.sender.is_some())
    }

    pub fn label(&self) -> Option<&'static str> {
        self.recording.as_ref().map(|r| {
            if r.sender.is_some() {
                "GIF REC - F6 STOP"
            } else {
                "GIF SAVING..."
            }
        })
    }

    pub fn idle(&self) -> bool {
        self.recording.is_none() && self.screenshot.is_none()
    }

    /// Wake in time to sample even when the guest image has not changed.
    pub fn wait_time(&self, maximum: Duration) -> Duration {
        self.recording
            .as_ref()
            .filter(|r| r.sender.is_some())
            .map_or(maximum, |r| {
                r.next.saturating_sub(r.started.elapsed()).min(maximum)
            })
    }

    pub fn tick(&mut self, frame: &Arc<Frame>) {
        let Some(r) = &mut self.recording else {
            return;
        };
        let Some(sender) = &r.sender else {
            return;
        };
        let at = r.started.elapsed();
        if at < r.next {
            return;
        }
        // No catch-up burst. Timestamps retain elapsed time if samples are lost.
        r.next = at + r.period;
        match sender.try_send(Sample {
            frame: frame.clone(),
            at,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => r.skipped += 1,
            Err(TrySendError::Disconnected(_)) => {
                r.sender = None;
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(r) = &mut self.recording
            && r.sender.is_some()
        {
            *r.end.lock().unwrap() = r.started.elapsed();
            // EOF is the stop request, so a full frame queue cannot lose it.
            r.sender = None;
        }
    }

    pub fn poll(&mut self) -> Vec<Result<Report, String>> {
        let mut results = Vec::new();
        if self
            .screenshot
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            results.push(finish(self.screenshot.take().unwrap()));
        }
        if self.recording.as_ref().is_some_and(|r| r.job.is_finished()) {
            let r = self.recording.take().unwrap();
            if r.skipped != 0 {
                eprintln!(
                    "Prismatic: GIF encoder skipped {} samples; elapsed timing retained",
                    r.skipped
                );
            }
            results.push(finish(r.job));
        }
        results
    }
}

impl Drop for Captures {
    fn drop(&mut self) {
        // Normal quit/restart drains through poll while SDL remains responsive.
        // Also finish files if a fatal SDL error unwinds the application early.
        self.stop();
        let jobs = self
            .screenshot
            .take()
            .into_iter()
            .chain(self.recording.take().map(|r| r.job));
        for job in jobs {
            match finish(job) {
                Ok(report) => eprintln!("Prismatic: {}", report.message()),
                Err(error) => eprintln!("Prismatic: {error}"),
            }
        }
    }
}

fn finish(job: Job) -> Result<Report, String> {
    job.join()
        .unwrap_or_else(|_| Err("Capture worker panicked".into()))
}

fn validate_frame(frame: &Frame) -> Result<(), String> {
    if frame.width == 0
        || frame.height == 0
        || frame.width > 4096
        || frame.height > 4096
        || frame.rgba.len() != frame.width as usize * frame.height as usize * 4
    {
        return Err("No valid guest framebuffer to capture".into());
    }
    Ok(())
}

fn capture_file(directory: &Path, extension: &str) -> io::Result<(NamedTempFile, PathBuf)> {
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%SZ");
    let file = tempfile::Builder::new()
        .prefix(&format!("prismatic-{stamp}-"))
        .suffix(&format!(".{extension}.part"))
        .tempfile_in(directory)?;
    let path = file.path().with_extension("");
    Ok((file, path))
}

fn publish(file: NamedTempFile, path: &Path) -> io::Result<()> {
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}

fn save_ppm(directory: &Path, frame: &Frame) -> io::Result<Report> {
    let (mut file, path) = capture_file(directory, "ppm")?;
    {
        let mut writer = BufWriter::new(&mut file);
        write!(writer, "P6\n{} {}\n255\n", frame.width, frame.height)?;
        for pixel in frame.rgba.as_chunks::<4>().0 {
            writer.write_all(&pixel[..3])?;
        }
        writer.flush()?;
    }
    publish(file, &path)?;
    Ok(Report {
        path,
        kind: Kind::Screenshot,
        warning: None,
    })
}

fn gif_size(frame: &Frame, width: u16) -> Result<(u16, u16), String> {
    validate_frame(frame)?;
    let height = (width as u64 * frame.height as u64 / frame.width as u64).max(1);
    if height > 2048 {
        return Err("Guest aspect ratio is too tall for GIF capture".into());
    }
    Ok((width, height as u16))
}

// Fix the canvas at recording start. A later guest mode change is fitted with
// black borders, preserving aspect ratio without changing the GIF dimensions.
fn scale_rgb(frame: &Frame, (width, height): (u16, u16)) -> Result<Vec<u8>, String> {
    validate_frame(frame)?;
    let (width, height) = (width as usize, height as usize);
    let scale = (width as f64 / frame.width as f64).min(height as f64 / frame.height as f64);
    let w = (frame.width as f64 * scale).round().max(1.0) as usize;
    let h = (frame.height as f64 * scale).round().max(1.0) as usize;
    let (left, top) = ((width - w) / 2, (height - h) / 2);
    let mut rgb = vec![0; width * height * 3];
    for y in 0..h {
        for x in 0..w {
            let source = ((y * frame.height as usize / h) * frame.width as usize
                + x * frame.width as usize / w)
                * 4;
            let dest = ((y + top) * width + x + left) * 3;
            rgb[dest..dest + 3].copy_from_slice(&frame.rgba[source..source + 3]);
        }
    }
    Ok(rgb)
}

fn write_delayed<W: Write>(
    encoder: &mut gif::Encoder<W>,
    frame: &mut gif::Frame<'_>,
    mut delay: u64,
) -> Result<(), gif::EncodingError> {
    // Long static scenes may exceed GIF's 16-bit centisecond delay field.
    delay = delay.max(1);
    while delay > 0 {
        frame.delay = delay.min(u16::MAX as u64) as u16;
        encoder.write_frame(frame)?;
        delay -= frame.delay as u64;
    }
    Ok(())
}

fn save_gif(
    directory: &Path,
    first: Arc<Frame>,
    size: (u16, u16),
    settings: &GifSettings,
    receiver: Receiver<Sample>,
    end: Arc<Mutex<Duration>>,
) -> Result<Report, Box<dyn std::error::Error>> {
    let (mut file, path) = capture_file(directory, "gif")?;
    {
        let writer = BufWriter::new(&mut file);
        let mut encoder = gif::Encoder::new(writer, size.0, size.1, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        let mut rgb = scale_rgb(&first, size)?;
        let mut pending = gif::Frame::from_rgb_speed(size.0, size.1, &rgb, 10);
        pending.dispose = gif::DisposalMethod::Keep;
        let mut since = 0_u64;
        for sample in receiver {
            let next = scale_rgb(&sample.frame, size)?;
            if next == rgb {
                continue;
            }
            let at = (sample.at.as_millis() / 10) as u64;
            write_delayed(&mut encoder, &mut pending, at.saturating_sub(since))?;
            since = at;
            rgb = next;
            pending = gif::Frame::from_rgb_speed(size.0, size.1, &rgb, 10);
            pending.dispose = gif::DisposalMethod::Keep;
        }
        let end = (end.lock().unwrap().as_millis() / 10) as u64;
        write_delayed(
            &mut encoder,
            &mut pending,
            end.saturating_sub(since).max(100 / settings.fps as u64),
        )?;
        encoder.into_inner()?.flush()?;
    }
    publish(file, &path)?;
    let warning = if settings.encoder == GifEncoder::Ffmpeg {
        optimize_gif(&path)
            .err()
            .map(|e| format!("FFmpeg: {e}; built-in GIF kept"))
    } else {
        None
    };
    Ok(Report {
        path,
        kind: Kind::Gif,
        warning,
    })
}

#[derive(Debug, PartialEq, Eq)]
struct GifInfo {
    width: u16,
    height: u16,
    duration: u64,
    last_delay: u16,
}

fn gif_info(path: &Path) -> Result<GifInfo, Box<dyn std::error::Error>> {
    let mut decoder = gif::DecodeOptions::new().read_info(BufReader::new(File::open(path)?))?;
    let mut info = GifInfo {
        width: decoder.width(),
        height: decoder.height(),
        duration: 0,
        last_delay: 0,
    };
    while let Some(frame) = decoder.read_next_frame()? {
        info.duration += frame.delay as u64;
        info.last_delay = frame.delay;
    }
    if info.duration == 0 {
        return Err("empty or zero-duration GIF".into());
    }
    Ok(info)
}

fn optimize_gif(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let before = gif_info(path)?;
    let optimized = tempfile::Builder::new()
        .suffix(".gif")
        .tempfile_in(path.parent().unwrap())?;
    let palette = tempfile::Builder::new()
        .suffix(".png")
        .tempfile_in(path.parent().unwrap())?;
    // Two passes avoid buffering the entire video while a global palette is
    // generated. Keep the already-published built-in GIF on every failure.
    run_ffmpeg(
        Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-loglevel",
                "error",
                "-filter_threads",
                "1",
                "-i",
            ])
            .arg(path)
            .args([
                "-vf",
                "palettegen=max_colors=256:reserve_transparent=0",
                "-frames:v",
                "1",
                "-update",
                "1",
            ])
            .arg(palette.path()),
    )?;
    run_ffmpeg(
        Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-loglevel",
                "error",
                "-filter_complex_threads",
                "1",
                "-i",
            ])
            .arg(path)
            .arg("-i")
            .arg(palette.path())
            .args([
                "-lavfi",
                "paletteuse=dither=none",
                "-fps_mode",
                "passthrough",
                "-gifflags",
                "+offsetting+transdiff",
                "-loop",
                "0",
                "-final_delay",
            ])
            .arg(before.last_delay.to_string())
            .arg(optimized.path()),
    )?;
    let after = gif_info(optimized.path())?;
    if (before.width, before.height, before.duration) != (after.width, after.height, after.duration)
    {
        return Err("optimization changed dimensions or timing".into());
    }
    if optimized.as_file().metadata()?.len() < path.metadata()?.len() {
        optimized.as_file().sync_all()?;
        optimized.persist(path)?;
    }
    Ok(())
}

fn run_ffmpeg(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    let mut errors = tempfile::tempfile()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(errors.try_clone()?)
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("optimization timed out".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        errors.seek(SeekFrom::Start(0))?;
        let mut detail = String::new();
        errors.take(1024).read_to_string(&mut detail)?;
        return Err(format!("{status}: {}", detail.trim()).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, color: [u8; 4]) -> Arc<Frame> {
        Arc::new(Frame {
            width,
            height,
            rgba: color.repeat((width * height) as usize),
        })
    }

    #[test]
    fn ppm_preserves_dimensions_colors_and_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let frame = Frame {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
        };
        let one = save_ppm(dir.path(), &frame).unwrap();
        let two = save_ppm(dir.path(), &frame).unwrap();
        assert_ne!(one.path, two.path);
        assert_eq!(
            std::fs::read(&one.path).unwrap(),
            b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06"
        );
        let (mut temp, _) = capture_file(dir.path(), "ppm").unwrap();
        temp.write_all(b"replacement").unwrap();
        assert!(publish(temp, &one.path).is_err());
        assert_eq!(
            std::fs::read(&one.path).unwrap(),
            std::fs::read(&two.path).unwrap()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        assert!(save_ppm(&one.path, &frame).is_err());
    }

    #[test]
    fn gif_roundtrip_retains_pixels_looping_and_elapsed_time_between_samples() {
        let dir = tempfile::tempdir().unwrap();
        let red = solid(4, 2, [255, 0, 0, 255]);
        let blue = solid(4, 2, [0, 0, 255, 255]);
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(Sample {
            frame: blue,
            at: Duration::from_millis(120),
        })
        .unwrap();
        tx.send(Sample {
            frame: red.clone(),
            at: Duration::from_millis(180),
        })
        .unwrap();
        drop(tx);
        let report = save_gif(
            dir.path(),
            red,
            (4, 2),
            &GifSettings::default(),
            rx,
            Arc::new(Mutex::new(Duration::from_millis(250))),
        )
        .unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options
            .read_info(File::open(&report.path).unwrap())
            .unwrap();
        assert_eq!((decoder.width(), decoder.height()), (4, 2));
        for (delay, color) in [
            (12, [255, 0, 0, 255]),
            (6, [0, 0, 255, 255]),
            (7, [255, 0, 0, 255]),
        ] {
            let frame = decoder.read_next_frame().unwrap().unwrap();
            assert_eq!(frame.delay, delay);
            assert_eq!(&*frame.buffer, color.repeat(8));
        }
        assert!(decoder.read_next_frame().unwrap().is_none());
        assert_eq!(decoder.repeat(), gif::Repeat::Infinite);
        assert_eq!(std::fs::read(&report.path).unwrap().last(), Some(&0x3b));
        assert_eq!(gif_info(&report.path).unwrap().duration, 25);
    }

    #[test]
    fn static_frames_coalesce_and_long_delays_are_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let first = solid(2, 2, [30, 40, 50, 255]);
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(Sample {
            frame: first.clone(),
            at: Duration::from_secs(1),
        })
        .unwrap();
        drop(tx);
        let report = save_gif(
            dir.path(),
            first,
            (2, 2),
            &GifSettings::default(),
            rx,
            Arc::new(Mutex::new(Duration::from_secs(700))),
        )
        .unwrap();
        let mut decoder = gif::DecodeOptions::new()
            .read_info(File::open(&report.path).unwrap())
            .unwrap();
        assert_eq!(decoder.read_next_frame().unwrap().unwrap().delay, 65535);
        assert_eq!(decoder.read_next_frame().unwrap().unwrap().delay, 4465);
        assert!(decoder.read_next_frame().unwrap().is_none());
    }

    #[test]
    fn resolution_changes_fit_the_original_canvas_and_invalid_frames_fail() {
        let wide = solid(4, 2, [255, 0, 0, 255]);
        assert_eq!(gif_size(&wide, 480).unwrap(), (480, 240));
        let tall = solid(2, 4, [255, 0, 0, 255]);
        let rgb = scale_rgb(&tall, (4, 2)).unwrap();
        assert_eq!(rgb, [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0].repeat(2));
        let invalid = Frame {
            width: 4,
            height: 2,
            rgba: vec![],
        };
        assert!(validate_frame(&invalid).is_err());
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(Sample {
            frame: Arc::new(invalid),
            at: Duration::from_millis(40),
        })
        .unwrap();
        drop(tx);
        assert!(
            save_gif(
                dir.path(),
                wide,
                (4, 2),
                &GifSettings::default(),
                rx,
                Arc::new(Mutex::new(Duration::from_secs(1)))
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn full_frame_queue_drops_samples_and_stop_still_delivers_eof() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::sync_channel(2);
        let mut captures = Captures::new(dir.path().into());
        captures.recording = Some(Recording {
            sender: Some(tx),
            job: thread::spawn(|| Err("test worker".into())),
            started: Instant::now() - Duration::from_secs(1),
            end: Arc::new(Mutex::new(Duration::ZERO)),
            period: Duration::from_millis(40),
            next: Duration::ZERO,
            skipped: 0,
        });
        let frame = solid(2, 2, [255, 0, 0, 255]);
        for _ in 0..3 {
            captures.recording.as_mut().unwrap().next = Duration::ZERO;
            captures.tick(&frame);
        }
        assert_eq!(captures.recording.as_ref().unwrap().skipped, 1);
        captures.stop();
        assert!(!captures.is_recording());
        assert_eq!(rx.into_iter().count(), 2);
        assert!(
            *captures.recording.as_ref().unwrap().end.lock().unwrap() >= Duration::from_secs(1)
        );
    }

    #[test]
    fn immediate_stop_still_finishes_a_valid_gif_and_worker_failures_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut captures = Captures::new(dir.path().into());
        let frame = solid(4, 2, [10, 20, 30, 255]);
        captures
            .toggle_gif(frame.clone(), &GifSettings::default())
            .unwrap();
        assert!(captures.is_recording());
        captures
            .toggle_gif(frame.clone(), &GifSettings::default())
            .unwrap();
        assert!(!captures.is_recording());
        assert!(
            captures
                .toggle_gif(frame.clone(), &GifSettings::default())
                .is_err()
        );
        let r = captures.recording.take().unwrap();
        let report = finish(r.job).unwrap();
        assert!(gif_info(&report.path).unwrap().duration >= 4);
        let mut invalid = Captures::new(report.path.clone()); // A file, not a directory.
        invalid.screenshot(frame).unwrap();
        assert!(
            finish(invalid.screenshot.take().unwrap())
                .unwrap_err()
                .contains("Screenshot failed")
        );
    }
}
