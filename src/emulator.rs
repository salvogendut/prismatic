//! Iris owns the machine on a worker; SDL only sees status and the latest frame.
use crate::{
    activity::{self, Activity},
    display::Frame,
    media::{self, MediaSettings},
    settings::{AdvancedSettings, MachineSettings, SerialLogging},
};
use iris::{
    compositor::SwCompositor,
    config::{GraphicsBoard, MachineConfig, MachineProfile},
    debug_overlay::DebugOverlay,
    dev::ng1::rex3::Renderer,
    dev::ps2::Ps2Controller,
    disp::{BarStats, FB_MAX_H, FB_STRIDE, Rex3Screen, StatusBar, StatusBarTexture},
    workstation::{MachineSlot, Workstation},
};
use std::{
    any::Any,
    io::Write,
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Running,
    Halted,
    Stopping,
    Stopped,
    Failed(String),
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Starting => "Starting machine...",
            Self::Running => "Running",
            Self::Halted => "Machine halted",
            Self::Stopping => "Stopping machine...",
            Self::Stopped => "Machine stopped",
            Self::Failed(_) => "Machine error - see terminal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub monitor: Option<SocketAddr>,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            phase: Phase::Starting,
            monitor: None,
        }
    }
}

// One pending frame at most, replaced by the producer if the UI falls behind.
// Taking an Arc keeps the upload outside the lock and avoids cloning pixel data.
#[derive(Clone, Default)]
pub struct FrameSlot(Arc<Mutex<Option<Arc<Frame>>>>);

impl FrameSlot {
    fn publish(&self, frame: Frame) {
        *self.0.lock().unwrap() = Some(Arc::new(frame));
    }
    pub fn take(&self) -> Option<Arc<Frame>> {
        self.0.lock().unwrap().take()
    }
}

struct CaptureRenderer {
    frames: FrameSlot,
    compositor: SwCompositor,
    presented: bool,
    activity: Activity,
    cycles: u64,
}

impl CaptureRenderer {
    fn new(frames: FrameSlot, activity: Activity) -> Self {
        Self {
            frames,
            compositor: SwCompositor::new(),
            presented: false,
            activity,
            cycles: 0,
        }
    }
}

impl Renderer for CaptureRenderer {
    fn present(
        &mut self,
        screen: &mut Rex3Screen,
        _overlay: &mut DebugOverlay,
        _status: &mut StatusBar,
        _sbtex: &mut StatusBarTexture,
        stats: &BarStats,
        need_readback: bool,
        live_fb_rgb: Option<&[u32]>,
        live_fb_aux: Option<&[u32]>,
    ) {
        if stats.cycles != self.cycles {
            self.activity.signal(activity::CPU);
            self.cycles = stats.cycles;
        }
        if stats.hb & 3 != 0 {
            self.activity.signal(activity::NET);
        }
        let (width, height) = (screen.width, screen.height);
        if width == 0 || height == 0 || width > FB_STRIDE || height > FB_MAX_H {
            return;
        }
        if screen.status_bar_only && self.presented && !need_readback {
            return;
        }

        self.compositor
            .compose_pixels(&screen.compositor_source_from(live_fb_rgb, live_fb_aux));
        let pixels = self.compositor.pixels();
        if need_readback {
            screen.rgba.resize(pixels.len(), 0);
            screen.rgba.copy_from_slice(pixels);
            screen.frame_seq = screen.frame_seq.wrapping_add(1);
        }
        self.frames
            .publish(pack_frame(pixels, width, height, FB_STRIDE));
        self.presented = true;
    }
}

fn pack_frame(pixels: &[u32], width: usize, height: usize, stride: usize) -> Frame {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for &pixel in &pixels[y * stride..y * stride + width] {
            // Compositor output is 0xAABBGGRR. Extract channels independently
            // of host endianness; this is display output, not emulated memory.
            rgba.extend_from_slice(&[pixel as u8, (pixel >> 8) as u8, (pixel >> 16) as u8, 255]);
        }
    }
    Frame {
        width: width as u32,
        height: height as u32,
        rgba,
    }
}

pub fn machine_config(
    state_dir: &Path,
    hardware: &MachineSettings,
    media: &MediaSettings,
    advanced: &AdvancedSettings,
) -> Result<MachineConfig, String> {
    media.validate()?;
    advanced.validate()?;
    let path = |name: &str| {
        state_dir
            .join(name)
            .into_os_string()
            .into_string()
            .map_err(|_| "Iris requires a UTF-8 state directory".to_string())
    };
    let mut config = MachineConfig {
        // Indy uses the embedded fallback. Indigo2 gets an explicit temporary
        // copy of Iris's embedded PROM before construction (see spawn).
        prom: String::new(),
        nvram: path("nvram.bin")?,
        nveeprom: path("nveeprom.bin")?,
        banks: hardware.banks()?,
        scsi: media.iris_devices().into_iter().collect(),
        no_audio: true,
        headless: false,
        keyboard: hardware.keyboard,
        rtc_offset: advanced.rtc_offset(),
        scsi_deferred_int: advanced.scsi_deferred_interrupts,
        serial_log: if advanced.serial_logging == SerialLogging::Off {
            None
        } else {
            Some(path("serial.log")?)
        },
        serial_log_timestamps: if advanced.serial_logging == SerialLogging::Timestamped {
            iris::dev::serial_log::SerialLogTimestamps::Wall
        } else {
            iris::dev::serial_log::SerialLogTimestamps::Off
        },
        // Ephemeral loopback ports avoid collisions with another Iris instance.
        monitor_port: Some(0),
        serial_port_a: Some(0),
        serial_port_b: Some(0),
        ..MachineConfig::default()
    };
    config.machine.profile = hardware.profile;
    config.machine.cpu = hardware.cpu;
    config.machine.cache = advanced.cache;
    config.graphics.resolution = advanced.resolution;
    config.graphics.board = Some(GraphicsBoard::Newport);
    config.graphics.heads = 1;
    config.validate()?;
    Ok(config)
}

// Iris reports an unopenable log only on stderr. Check on the worker before
// building the machine so the frontend can report startup failure in its UI.
// This is never called while editing/validating a draft.
fn prepare_serial_log(config: &MachineConfig) -> Result<(), String> {
    if let Some(path) = &config.serial_log {
        if std::fs::metadata(path).is_ok_and(|m| !m.is_file()) {
            return Err(format!("Serial log must be a regular file: {path}"));
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Cannot open serial log {path}: {e}"))?;
    }
    Ok(())
}

// Serialize access with Iris's monitor/event controller. Dropping the machine
// must happen after releasing the slot lock, since Iris's Drop takes it too.
struct OwnedMachine {
    machine: Box<dyn Workstation>,
    slot: MachineSlot,
    stopped: bool,
    media_errors: media::MediaErrors,
}

impl OwnedMachine {
    fn new(mut machine: Box<dyn Workstation>) -> Self {
        let slot = machine.machine_slot();
        Self {
            machine,
            slot,
            stopped: false,
            media_errors: media::MediaErrors::default(),
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.stopped = true;
        let _guard = self.slot.lock();
        self.machine.stop();
        media::detach(self.machine.as_mut());
        self.media_errors.check()
    }
}

impl Drop for OwnedMachine {
    fn drop(&mut self) {
        if !self.stopped {
            // Also stop on a renderer-install or startup panic. Never double-panic.
            let _ = catch_unwind(AssertUnwindSafe(|| self.stop()));
        }
    }
}

pub struct Emulator {
    pub frames: FrameSlot,
    status: Arc<Mutex<Status>>,
    stop: mpsc::SyncSender<()>,
    thread: Option<JoinHandle<()>>,
    input: Arc<Mutex<Option<Arc<Ps2Controller>>>>,
    pub activity: Activity,
    has_media: bool,
}

impl Emulator {
    pub fn spawn(
        state_dir: &Path,
        hardware: &MachineSettings,
        media: &MediaSettings,
        advanced: &AdvancedSettings,
    ) -> Result<Self, String> {
        let mut config = machine_config(state_dir, hardware, media, advanced)?;
        // Attach explicitly before CPU start: Iris's CLI constructor exits the
        // process on media errors. The frontend must report them in the window.
        config.scsi.clear();
        let media = media.clone();
        let has_media = !media.devices.is_empty();
        iris::workstation::set_exit_on_guest_poweroff(false);
        let state_dir = state_dir.to_owned();
        let frames = FrameSlot::default();
        let status = Arc::new(Mutex::new(Status::default()));
        let (stop, receiver) = mpsc::sync_channel(1);
        let worker_frames = frames.clone();
        let worker_status = status.clone();
        let input = Arc::new(Mutex::new(None));
        let worker_input = input.clone();
        let activity = Activity::default();
        let worker_activity = activity.clone();
        let thread = thread::Builder::new()
            .name("prismatic-iris".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                    std::fs::create_dir_all(&state_dir)
                        .map_err(|e| format!("Cannot create machine state directory: {e}"))?;
                    prepare_serial_log(&config)?;
                    // Iris's Indigo2 fallback also searches the current directory.
                    // Give it the bundled image explicitly so a local ROM cannot
                    // silently change a configured machine's firmware.
                    let _prom = if config.machine.profile == MachineProfile::Indigo2Ip22 {
                        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                        file.write_all(&iris::prombini2::PROM0701367012)
                            .map_err(|e| e.to_string())?;
                        config.prom = file
                            .path()
                            .to_str()
                            .ok_or("Iris requires a UTF-8 PROM path")?
                            .to_owned();
                        Some(file)
                    } else {
                        None
                    };
                    let mut owned = OwnedMachine::new(iris::workstation::build(config)?);
                    media::attach(
                        owned.machine.as_mut(),
                        &media,
                        &worker_activity,
                        &mut owned.media_errors,
                    )?;
                    owned.machine.register_system_controller();
                    {
                        let _guard = owned.slot.lock();
                        let display = owned
                            .machine
                            .get_display()
                            .ok_or("Iris did not create a display")?;
                        *display.renderer_slot().lock() = Some(Box::new(CaptureRenderer::new(
                            worker_frames,
                            worker_activity,
                        )));
                        if matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                            owned.machine.start();
                            // Iris debug builds deliberately leave the CPU parked.
                            if !owned.machine.cpu_is_running() {
                                owned.machine.cpu_start();
                            }
                            *worker_input.lock().unwrap() = Some(owned.machine.get_ps2());
                        } else {
                            drop(_guard);
                            return owned.stop();
                        }
                    }
                    loop {
                        {
                            let _guard = owned.slot.lock();
                            let phase = if owned.machine.cpu_is_running() {
                                Phase::Running
                            } else {
                                Phase::Halted
                            };
                            *worker_status.lock().unwrap() = Status {
                                phase,
                                monitor: owned.machine.monitor_addr(),
                            };
                        }
                        match receiver.recv_timeout(Duration::from_millis(100)) {
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            _ => break,
                        }
                    }
                    worker_status.lock().unwrap().phase = Phase::Stopping;
                    *worker_input.lock().unwrap() = None;
                    owned.stop()?;
                    Ok(())
                }));
                *worker_input.lock().unwrap() = None;
                let phase = match result {
                    Ok(Ok(())) => Phase::Stopped,
                    Ok(Err(error)) => Phase::Failed(error),
                    Err(payload) => Phase::Failed(panic_message(payload.as_ref())),
                };
                worker_status.lock().unwrap().phase = phase;
            })
            .map_err(|e| format!("Cannot start Iris worker: {e}"))?;
        Ok(Self {
            frames,
            status,
            stop,
            thread: Some(thread),
            input,
            activity,
            has_media,
        })
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn input(&self) -> Option<Arc<Ps2Controller>> {
        self.input.lock().unwrap().clone()
    }

    pub fn request_stop(&self) {
        let _ = self.stop.try_send(());
    }

    pub fn finished(&mut self) -> bool {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            let _ = self.thread.take().unwrap().join();
        }
        self.thread.is_none()
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.request_stop();
        // Error/unwind fallback. The normal path keeps SDL responsive while
        // polling finished(). Attached disks must finish flushing before exit.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.finished() && (self.has_media || Instant::now() < deadline) {
            thread::sleep(Duration::from_millis(10));
        }
        if self.thread.is_some() {
            eprintln!("Prismatic: Iris worker did not stop within five seconds");
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "Iris worker panicked".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris::config::KeyboardLayout;

    #[test]
    fn advanced_options_map_to_iris_without_creating_log_during_validation() {
        use iris::{
            config::CacheModel, dev::ng1::vc2_timings::NewportResolution,
            dev::serial_log::SerialLogTimestamps,
        };
        let dir = tempfile::tempdir().unwrap();
        let defaults = machine_config(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &AdvancedSettings::default(),
        )
        .unwrap();
        assert_eq!(defaults.graphics.resolution, NewportResolution::Guest);
        assert_eq!(defaults.machine.cache, CacheModel::Auto);
        assert!(defaults.rtc_offset.is_zero());
        assert!(defaults.scsi_deferred_int);
        assert!(defaults.serial_log.is_none());
        let advanced = AdvancedSettings {
            resolution: NewportResolution::Res1024x768,
            cache: CacheModel::Shadow,
            clock_offset_years: -20,
            scsi_deferred_interrupts: false,
            serial_logging: SerialLogging::Timestamped,
        };
        let cfg = machine_config(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
        )
        .unwrap();
        assert_eq!(cfg.graphics.resolution, advanced.resolution);
        assert_eq!(cfg.machine.cache, advanced.cache);
        assert_eq!(cfg.rtc_offset.years, -20);
        assert!(!cfg.scsi_deferred_int);
        assert_eq!(
            cfg.serial_log.as_deref(),
            dir.path().join("serial.log").to_str()
        );
        assert_eq!(cfg.serial_log_timestamps, SerialLogTimestamps::Wall);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn serial_log_preparation_preserves_transcripts_and_reports_invalid_targets() {
        let dir = tempfile::tempdir().unwrap();
        let advanced = AdvancedSettings {
            serial_logging: SerialLogging::Raw,
            ..Default::default()
        };
        let config = machine_config(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
        )
        .unwrap();
        let path = dir.path().join("serial.log");
        std::fs::write(&path, "previous session\n").unwrap();
        prepare_serial_log(&config).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "previous session\n"
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            prepare_serial_log(&config)
                .unwrap_err()
                .contains("regular file")
        );
    }

    #[test]
    fn diskless_profile_never_uses_default_media_or_cwd_state() {
        let directory = tempfile::tempdir().unwrap();
        let hardware = MachineSettings {
            keyboard: KeyboardLayout::Iso,
            ..Default::default()
        };
        let config = machine_config(
            directory.path(),
            &hardware,
            &MediaSettings::default(),
            &AdvancedSettings::default(),
        )
        .unwrap();
        assert_eq!(config.keyboard, KeyboardLayout::Iso);
        assert!(config.scsi.is_empty());
        assert!(config.prom.is_empty());
        assert_eq!(
            config.nvram,
            directory.path().join("nvram.bin").to_str().unwrap()
        );
        assert_eq!(config.machine.profile, MachineProfile::IndyIp24);
        assert_eq!(config.graphics.board, Some(GraphicsBoard::Newport));
        assert!(!config.headless);
        assert!(config.no_audio);
        assert_eq!(config.monitor_port, Some(0));
        assert!(config.port_forward.is_empty());
        assert!(config.nfs.is_none());
    }

    #[test]
    fn frame_packing_respects_stride_and_channel_order() {
        let frame = pack_frame(
            &[
                0x00112233, 0x00445566, 0xdeadbeef, 0x00778899, 0x00aabbcc, 0xdeadbeef,
            ],
            2,
            2,
            3,
        );
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(
            frame.rgba,
            [
                0x33, 0x22, 0x11, 255, 0x66, 0x55, 0x44, 255, 0x99, 0x88, 0x77, 255, 0xcc, 0xbb,
                0xaa, 255
            ]
        );
    }

    #[test]
    fn slow_consumer_receives_only_latest_frame() {
        let frames = FrameSlot::default();
        frames.publish(pack_frame(&[0x11], 1, 1, 1));
        frames.publish(pack_frame(&[0x22], 1, 1, 1));
        assert_eq!(frames.take().unwrap().rgba[0], 0x22);
        assert!(frames.take().is_none());
    }

    #[test]
    fn startup_failure_is_reported_without_blocking_the_caller() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut emulator = Emulator::spawn(
            &file.path().join("invalid"),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &AdvancedSettings::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(matches!(emulator.status().phase, Phase::Failed(_)));
    }

    #[test]
    fn every_menu_hardware_combination_builds_a_valid_iris_config() {
        for profile in [MachineProfile::IndyIp24, MachineProfile::Indigo2Ip22] {
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            for &cpu in hardware.cpus() {
                for &ram_mb in hardware.ram_sizes() {
                    hardware.cpu = cpu;
                    hardware.ram_mb = ram_mb;
                    let config = machine_config(
                        Path::new("/tmp/prismatic-config-test"),
                        &hardware,
                        &MediaSettings::default(),
                        &AdvancedSettings::default(),
                    )
                    .unwrap();
                    assert_eq!(config.machine.profile, profile);
                    assert_eq!(config.machine.cpu, cpu);
                    assert_eq!(config.banks.iter().sum::<u32>(), ram_mb);
                    assert!(
                        config
                            .banks
                            .iter()
                            .all(|&bank| profile.bank_sizes().contains(&bank))
                    );
                    config.validate().unwrap();
                }
            }
        }
    }
}
