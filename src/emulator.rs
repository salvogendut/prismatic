//! Iris owns the machine on a worker; SDL only sees status and the latest frame.
use crate::{
    activity::{self, Activity},
    display::Frame,
    media::{self, MediaSettings},
    settings::{AdvancedSettings, MachineSettings, SerialLogging},
};
use iris::{
    compositor::SwCompositor,
    config::{CacheModel, MachineConfig, MachineProfile},
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Running,
    Paused,
    Halted,
    PoweredOff,
    Stopping,
    Stopped,
    Failed(String),
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Starting => "Starting machine...",
            Self::Running => "Running",
            Self::Paused => "PAUSED",
            Self::Halted => "Machine halted",
            Self::PoweredOff => "Machine powered off",
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
    pub runtime: String,
    pub pcap: iris::net::PcapStatus,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            phase: Phase::Starting,
            monitor: None,
            runtime: String::new(),
            pcap: iris::net::PcapStatus::Inactive,
        }
    }
}

// One latest frame per head. Composing outside the lock keeps Iris refresh
// threads independent of SDL uploads and drops intermediate frames naturally.
#[derive(Default)]
struct FrameState {
    frames: [Option<Arc<Frame>>; 2],
    dirty: bool,
}

#[derive(Clone)]
pub struct FrameSlot {
    inner: Arc<Mutex<FrameState>>,
    heads: u8,
}

impl Default for FrameSlot {
    fn default() -> Self {
        Self::new(1)
    }
}

impl FrameSlot {
    fn new(heads: u8) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FrameState::default())),
            heads,
        }
    }
    fn publish(&self, head: usize, frame: Frame) {
        let mut state = self.inner.lock().unwrap();
        state.frames[head] = Some(Arc::new(frame));
        state.dirty = true;
    }
    pub fn take(&self) -> Option<Arc<Frame>> {
        let frames = {
            let mut state = self.inner.lock().unwrap();
            if !state.dirty {
                return None;
            }
            state.dirty = false;
            state.frames.clone()
        };
        if self.heads == 1 {
            frames[0].clone()
        } else {
            Some(Arc::new(Frame::side_by_side(
                frames[0].as_deref(),
                frames[1].as_deref(),
            )))
        }
    }
}

struct CaptureRenderer {
    frames: FrameSlot,
    head: usize,
    compositor: SwCompositor,
    presented: bool,
    activity: Activity,
    cpu_counters: activity::CpuCounters,
}

impl CaptureRenderer {
    fn new(frames: FrameSlot, activity: Activity, head: usize) -> Self {
        Self {
            frames,
            head,
            compositor: SwCompositor::new(),
            presented: false,
            activity,
            cpu_counters: activity::CpuCounters::default(),
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
        self.activity.signal(self.cpu_counters.update([
            Some(stats.cycles),
            stats.more_cpus[0].map(|(cycles, _)| cycles),
        ]));
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
            .publish(self.head, pack_frame(pixels, width, height, FB_STRIDE));
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
    network: &crate::network::NetworkSettings,
) -> Result<MachineConfig, String> {
    media.validate_for(hardware.profile)?;
    advanced.validate()?;
    let path = |name: &str| {
        state_dir
            .join(name)
            .into_os_string()
            .into_string()
            .map_err(|_| "Iris requires a UTF-8 state directory".to_string())
    };
    let mut config = MachineConfig {
        // Empty selects embedded firmware. Indigo2 needs a temporary file on
        // the worker to bypass its legacy working-directory fallback.
        prom: hardware.prom_path().unwrap_or_default().to_owned(),
        nvram: path("nvram.bin")?,
        nveeprom: path("nveeprom.bin")?,
        banks: hardware.banks()?,
        scsi: media.iris_devices().into_iter().collect(),
        no_audio: !crate::compat::AUDIO,
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
    config.machine.cpu = hardware.cpu.iris()?;
    config.machine.cpus = hardware.cpus;
    config.machine.cache = if hardware.newport() {
        advanced.cache
    } else {
        CacheModel::Shadow
    };
    config.graphics.resolution = if hardware.newport() {
        advanced.resolution
    } else {
        iris::dev::ng1::vc2_timings::NewportResolution::Guest
    };
    config.graphics.board = hardware.graphics_board();
    if hardware.profile == MachineProfile::OctaneIp30 {
        config.octane.flash = Some(path("flash.bin")?);
    }
    config.graphics.heads = hardware.heads;
    network.apply(&mut config)?;
    advanced.extensions.validate(hardware, media, network)?;
    advanced.extensions.apply(&mut config);
    advanced.expert.apply(&mut config)?;
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
        let flushed = self
            .machine
            .flush_disks()
            .map_err(|e| format!("Could not flush disks: {e}"));
        media::detach(self.machine.as_mut());
        flushed.and(self.media_errors.check())
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
    pub audio: Arc<crate::audio::Inbox>,
    status: Arc<Mutex<Status>>,
    commands: mpsc::SyncSender<Command>,
    stop_requested: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    input: Arc<Mutex<Option<Arc<Ps2Controller>>>>,
    pub activity: Activity,
    has_media: bool,
    network_control: Arc<Mutex<Option<Arc<iris::net::NatControl>>>>,
}

enum Command {
    Checkpoint(
        crate::checkpoints::Operation,
        String,
        Box<crate::network::NetworkSettings>,
        mpsc::SyncSender<Result<String, String>>,
    ),
    Stop,
    Pause,
    Resume,
    Storage(
        crate::storage::Action,
        mpsc::SyncSender<Result<String, String>>,
    ),
}

impl Emulator {
    pub fn spawn(
        state_dir: &Path,
        hardware: &MachineSettings,
        media: &MediaSettings,
        advanced: &AdvancedSettings,
        network: &crate::network::NetworkSettings,
    ) -> Result<Self, String> {
        let mut config = machine_config(state_dir, hardware, media, advanced, network)?;
        let mut checkpoint_context =
            crate::checkpoints::Context::new(hardware, media, advanced, network);
        let mut network = network.clone();
        let expert = advanced.expert.effective();
        let extensions = advanced.extensions.effective();
        let audio = Arc::new(crate::audio::Inbox::default());
        #[cfg(iris_audio)]
        {
            config.audio.output = Some(audio.clone());
        }
        // Attach explicitly before CPU start: Iris's CLI constructor exits the
        // process on media errors. The frontend must report them in the window.
        config.scsi.clear();
        let media = media.clone();
        let mut hardware = hardware.clone();
        let has_media = !media.devices.is_empty() || media.scratch.slot.is_some();
        #[cfg(iris_poweroff)]
        iris::workstation::set_exit_on_guest_poweroff(false);
        let state_dir = state_dir.to_owned();
        let frames = FrameSlot::new(hardware.heads);
        let network_control = Arc::new(Mutex::new(None));
        let worker_network = network_control.clone();
        let status = Arc::new(Mutex::new(Status::default()));
        let (commands, receiver) = mpsc::sync_channel(1);
        let stop_requested = Arc::new(AtomicBool::new(false));
        let worker_stop = stop_requested.clone();
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
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<Phase, String> {
                    hardware.validate_prom()?;
                    network.validate_files()?;
                    network.apply(&mut config)?;
                    std::fs::create_dir_all(&state_dir)
                        .map_err(|e| format!("Cannot create machine state directory: {e}"))?;
                    let _state_lease =
                        crate::instance::Lease::acquire(&state_dir.join("machine.lock"))?;
                    let _disk_leases = crate::instance::disks(&media)?;
                    prepare_serial_log(&config)?;
                    expert.prepare(&config, &state_dir)?;
                    let serial = crate::serial::Endpoints::prepare(&mut config, &expert)?;
                    // Iris's Indigo2 fallback also searches the current directory.
                    // Give it the bundled image explicitly so a local ROM cannot
                    // silently change a configured machine's firmware.
                    let bundled: Option<&[u8]> = match config.machine.profile {
                        MachineProfile::Indigo2Ip22 => Some(&iris::prombini2::PROM0701367012),
                        MachineProfile::Indigo2Ip28 => Some(&iris::prombinip28::PROM0701477002),
                        _ => None,
                    };
                    let _prom = if let Some(prom) = bundled {
                        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                        file.write_all(prom).map_err(|e| e.to_string())?;
                        config.prom = file
                            .path()
                            .to_str()
                            .ok_or("Iris requires a UTF-8 PROM path")?
                            .to_owned();
                        Some(file)
                    } else {
                        None
                    };
                    // Apply only saved settings, with the state lease held and
                    // the old workstation gone. Iris owns the firmware formats.
                    #[cfg(iris_prom)]
                    iris::prom_env::prepare_environment(
                        &config,
                        hardware.prom_boot.environment().map(|(loader, filename, options)| [loader, filename, options]),
                        hardware.prom_console.environment(),
                    )?;
                    let mut owned = OwnedMachine::new(iris::workstation::build(config)?);
                    // O2/Octane constructors currently use a default PS/2 layout.
                    // Set the shared controller before any CPU thread can query it.
                    owned.machine.get_ps2().set_layout(hardware.keyboard);
                    media::attach(
                        owned.machine.as_mut(),
                        &media,
                        &worker_activity,
                        &mut owned.media_errors,
                    )?;
                    let _scratch_lease = crate::storage::attach_scratch(
                        owned.machine.as_mut(),
                        &media,
                        &state_dir,
                        &worker_activity,
                    )?;
                    extensions.attach(owned.machine.as_mut())?;
                    *worker_network.lock().unwrap() = owned.machine.nat_control();
                    if let Some(path) = &expert.boot_elf {
                        owned.machine.load_elf(path)?;
                    }
                    owned.machine.register_system_controller();
                    // Start before CPUs, so boot traces include the first instruction.
                    let mut services = crate::expert::Services::start(&expert, &state_dir)?;
                    {
                        let _guard = owned.slot.lock();
                        let display = owned
                            .machine
                            .get_display()
                            .ok_or("Iris did not create a display")?;
                        *display.renderer_slot().lock() = Some(Box::new(CaptureRenderer::new(
                            worker_frames.clone(),
                            worker_activity.clone(),
                            0,
                        )));
                        if hardware.heads == 2 {
                            let second = owned
                                .machine
                                .get_display_head1()
                                .ok_or("Iris did not create the second Newport display")?;
                            *second.renderer_slot().lock() = Some(Box::new(CaptureRenderer::new(
                                worker_frames,
                                worker_activity,
                                1,
                            )));
                        }
                        if !worker_stop.load(Ordering::Acquire) {
                            owned.machine.start();
                            // Iris debug builds deliberately leave the CPU parked.
                            if !owned.machine.cpu_is_running() {
                                owned.machine.cpu_start();
                            }
                            *worker_input.lock().unwrap() = Some(owned.machine.get_ps2());
                        } else {
                            drop(_guard);
                            services.finish()?;
                            owned.stop()?;
                            return Ok(Phase::Stopped);
                        }
                    }
                    services.start_remote(&expert, owned.machine.as_ref())?;
                    if owned.machine.monitor_addr().is_none() {
                        return Err("Iris could not bind the configured monitor port".into());
                    }
                    let mut paused = false;
                    let final_phase = loop {
                        if worker_stop.load(Ordering::Acquire) {
                            break Phase::Stopped;
                        }
                        {
                            let _guard = owned.slot.lock();
                            // A monitor pause also stops the CPU. Only the core's
                            // completed power-off event may end the worker here.
                            if crate::compat::powered_off(owned.machine.as_ref()) {
                                break Phase::PoweredOff;
                            }
                            let phase = if owned.machine.cpu_is_running() {
                                Phase::Running
                            } else if paused {
                                Phase::Paused
                            } else {
                                Phase::Halted
                            };
                            *worker_status.lock().unwrap() = Status {
                                phase,
                                monitor: owned.machine.monitor_addr(),
                                runtime: format!("{}\nVINO: {} / {} / camera {}\nDaynaPort: {} / {}\nUltra64: {}",
                                    services.report(&serial, owned.machine.monitor_addr()), crate::compat::vino_status(owned.machine.as_mut()), if extensions.pal { "PAL" } else { "NTSC" }, extensions.camera_index,
                                    extensions.daynaport.map_or_else(|| "Off".into(), |s| format!("SCSI {}:{}", s / 16, s % 16)), extensions.dayna_subnet,
                                    if extensions.ultra64 { "Enabled (external bridge)" } else { "Off" }),
                                pcap: owned.machine.net_pcap_status(),
                            };
                        }
                        match receiver.recv_timeout(Duration::from_millis(100)) {
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Ok(Command::Checkpoint(operation, name, network, reply)) => {
                                let _guard = owned.slot.lock();
                                checkpoint_context.set_network(*network);
                                let (result, fatal) = if crate::compat::powered_off(owned.machine.as_ref()) {
                                    (Err("Machine powered off before the operation".into()), false)
                                } else {
                                    crate::checkpoints::execute(owned.machine.as_mut(), &checkpoint_context, &state_dir, &name, operation, &mut paused)
                                };
                                let failure = result.as_ref().err().cloned();
                                let _ = reply.try_send(result);
                                if fatal { break Phase::Failed(format!("Restore failed; machine stopped: {}", failure.unwrap_or_default())); }
                            }
                            Ok(Command::Storage(action, result)) => {
                                let _guard = owned.slot.lock();
                                let outcome =
                                    if crate::compat::powered_off(owned.machine.as_ref()) {
                                        Err("Machine powered off before the operation".into())
                                    } else {
                                        crate::storage::live(
                                            owned.machine.as_mut(),
                                            &action,
                                            &media,
                                            &state_dir,
                                            paused,
                                        )
                                    };
                                let _ = result.try_send(outcome);
                            }
                            Ok(Command::Pause) => {
                                // A second keypress can arrive before the UI sees
                                // Paused. Keep device stop/start pairs balanced.
                                if !paused {
                                    let _guard = owned.slot.lock();
                                    if !crate::compat::powered_off(owned.machine.as_ref()) {
                                        owned.machine.stop();
                                        paused = true;
                                    }
                                }
                            }
                            Ok(Command::Resume) => {
                                let _guard = owned.slot.lock();
                                // Never undo a guest power-off that arrived while
                                // waiting for a command. The next tick handles it.
                                if !crate::compat::powered_off(owned.machine.as_ref()) {
                                    owned.machine.start_guest()?;
                                    paused = false;
                                }
                            }
                            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                                break Phase::Stopped;
                            }
                        }
                    };
                    worker_status.lock().unwrap().phase = Phase::Stopping;
                    *worker_input.lock().unwrap() = None;
                    let service_result = services.finish();
                    owned.stop()?;
                    service_result?;
                    Ok(final_phase)
                }));
                *worker_input.lock().unwrap() = None;
                *worker_network.lock().unwrap() = None;
                let phase = match result {
                    Ok(Ok(phase)) => phase,
                    Ok(Err(error)) => Phase::Failed(error),
                    Err(payload) => Phase::Failed(panic_message(payload.as_ref())),
                };
                let mut status = worker_status.lock().unwrap();
                status.phase = phase;
                status.monitor = None;
                status.runtime.clear();
            })
            .map_err(|e| format!("Cannot start Iris worker: {e}"))?;
        Ok(Self {
            frames,
            audio,
            status,
            commands,
            stop_requested,
            thread: Some(thread),
            input,
            activity,
            has_media,
            network_control,
        })
    }

    pub fn checkpoint_action(
        &self,
        operation: crate::checkpoints::Operation,
        name: String,
        network: crate::network::NetworkSettings,
    ) -> Result<crate::storage::Job, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Checkpoint(operation, name, Box::new(network), tx))
            .map_err(|_| "Machine is busy; try again")?;
        Ok(crate::storage::Job::live(rx))
    }

    pub fn storage_action(
        &self,
        action: crate::storage::Action,
    ) -> Result<crate::storage::Job, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Storage(action, tx))
            .map_err(|_| "Machine is busy; try again")?;
        Ok(crate::storage::Job::live(rx))
    }

    pub fn network_control(&self) -> Option<Arc<iris::net::NatControl>> {
        self.network_control.lock().unwrap().clone()
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn input(&self) -> Option<Arc<Ps2Controller>> {
        self.input.lock().unwrap().clone()
    }

    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
        let _ = self.commands.try_send(Command::Stop);
    }

    pub fn request_pause(&self, pause: bool) -> Result<(), String> {
        self.commands
            .try_send(if pause {
                Command::Pause
            } else {
                Command::Resume
            })
            .map_err(|_| "Machine is busy changing state; try again".into())
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
    use iris::config::GraphicsBoard;
    use iris::config::KeyboardLayout;

    // Iris has process-wide display/CPU hooks; started machines must not overlap.
    static MACHINE_TEST: Mutex<()> = Mutex::new(());

    #[test]
    #[cfg(iris_prom)]
    fn console_preparation_is_optional_and_validates_all_octane_files_before_writing() {
        use iris::prom_env::{Console, prepare_environment};
        let dir = tempfile::tempdir().unwrap();
        for profile in MachineSettings::PROFILES {
            let state = dir.path().join(format!("{profile:?}"));
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            hardware.cpu = hardware.cpus()[0];
            let config = machine_config(
                &state,
                &hardware,
                &Default::default(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
            prepare_environment(&config, None, None).unwrap();
            assert!(!state.exists());
            prepare_environment(&config, None, Some(Console::Serial)).unwrap();
            let path = match profile {
                MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28 => &config.nveeprom,
                _ => &config.nvram,
            };
            let image = std::fs::read(path).unwrap();
            match profile {
                MachineProfile::IndyIp24 => assert_eq!(&image[66..68], b"d\0"),
                MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28 => {
                    assert_eq!(&image[2..4], b"d\0")
                }
                MachineProfile::O2Ip32 => assert!(image.windows(10).any(|b| b == b"console=d\0")),
                MachineProfile::OctaneIp30 => {
                    assert_eq!(&image[0x11..0x13], b"d\0");
                    assert!(!Path::new(config.octane.flash.as_ref().unwrap()).exists());
                    std::fs::write(config.octane.flash.as_ref().unwrap(), b"bad flash").unwrap();
                    assert!(
                        prepare_environment(
                            &config,
                            Some(["sash", "/unix", "auto"]),
                            Some(Console::Graphics)
                        )
                        .is_err()
                    );
                    assert_eq!(std::fs::read(path).unwrap(), image);
                }
            }
            prepare_environment(&config, None, Some(Console::Serial)).unwrap();
            assert_eq!(std::fs::read(path).unwrap(), image);
            prepare_environment(&config, None, None).unwrap();
            assert_eq!(std::fs::read(path).unwrap(), image);
        }
        let hardware = MachineSettings {
            prom_console: crate::settings::PromConsole::Serial,
            ..Default::default()
        };
        std::fs::write(dir.path().join("nvram.bin"), b"bad state").unwrap();
        let mut emulator = Emulator::spawn(
            dir.path(),
            &hardware,
            &Default::default(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(
            matches!(emulator.status().phase, Phase::Failed(error) if error.contains("PROM state"))
        );
        assert_eq!(
            std::fs::read(dir.path().join("nvram.bin")).unwrap(),
            b"bad state"
        );
    }

    #[test]
    #[cfg(iris_prom)]
    fn prom_boot_preparation_is_offline_atomic_and_errors_reach_the_ui() {
        use crate::settings::PromBoot;
        let dir = tempfile::tempdir().unwrap();
        for profile in MachineSettings::PROFILES {
            let state = dir.path().join(format!("{profile:?}"));
            std::fs::create_dir(&state).unwrap();
            let mut hardware = MachineSettings {
                profile,
                prom_boot: PromBoot::Netbsd,
                ..Default::default()
            };
            hardware.cpu = hardware.cpus()[0];
            let config = machine_config(
                &state,
                &hardware,
                &Default::default(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
            // Validation/config construction must not touch firmware files.
            assert_eq!(std::fs::read_dir(&state).unwrap().count(), 0);
            let (loader, filename, options) = hardware.prom_boot.environment().unwrap();
            iris::prom_env::prepare_boot_environment(&config, loader, filename, options).unwrap();
            let path = match profile {
                MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28 => &config.nveeprom,
                MachineProfile::OctaneIp30 => config.octane.flash.as_ref().unwrap(),
                _ => &config.nvram,
            };
            let saved = std::fs::read(path).unwrap();
            assert!(
                saved
                    .windows(b"netbsd.boot\0".len())
                    .any(|bytes| bytes == b"netbsd.boot\0"),
                "{profile:?}"
            );
            iris::prom_env::prepare_boot_environment(&config, loader, filename, options).unwrap();
            assert_eq!(std::fs::read(path).unwrap(), saved);
            assert!(
                iris::prom_env::prepare_boot_environment(&config, "sash", "bad\0name", "auto")
                    .is_err()
            );
            assert_eq!(std::fs::read(path).unwrap(), saved);
            std::fs::write(path, b"malformed firmware state").unwrap();
            assert!(
                iris::prom_env::prepare_boot_environment(&config, loader, filename, options)
                    .is_err()
            );
            assert_eq!(std::fs::read(path).unwrap(), b"malformed firmware state");
        }
        let hardware = MachineSettings {
            prom_boot: PromBoot::Netbsd,
            ..Default::default()
        };
        std::fs::write(dir.path().join("nvram.bin"), b"bad state").unwrap();
        let mut emulator = Emulator::spawn(
            dir.path(),
            &hardware,
            &Default::default(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(
            matches!(emulator.status().phase, Phase::Failed(error) if error.contains("PROM state"))
        );
        assert_eq!(
            std::fs::read(dir.path().join("nvram.bin")).unwrap(),
            b"bad state"
        );
    }

    #[test]
    #[cfg(iris_checkpoints)]
    fn native_checkpoint_restores_cpu_and_cow_disk_and_stays_paused() {
        use crate::checkpoints::{Context, Operation, execute};
        use iris::scsi::{ScsiDataLength, ScsiRequest, scsi_cmd};
        let _serial = MACHINE_TEST.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let disk = directory.path().join("disk.raw");
        std::fs::write(&disk, vec![0; 4096]).unwrap();
        let hardware = MachineSettings {
            ram_mb: 32,
            ..Default::default()
        };
        let mut media = MediaSettings::default();
        media.devices.insert(
            1,
            media::Medium {
                kind: media::MediaKind::Disk,
                path: disk.to_str().unwrap().into(),
            },
        );
        let advanced = AdvancedSettings::default();
        let network = Default::default();
        let context = Context::new(&hardware, &media, &advanced, &network);
        let mut config =
            machine_config(directory.path(), &hardware, &media, &advanced, &network).unwrap();
        config.scsi.clear();
        let native = thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || iris::workstation::build(config).unwrap())
            .unwrap()
            .join()
            .unwrap();
        let mut owned = OwnedMachine::new(native);
        media::attach(
            owned.machine.as_mut(),
            &media,
            &Activity::default(),
            &mut owned.media_errors,
        )
        .unwrap();
        let native = owned
            .machine
            .as_any_mut()
            .downcast_mut::<iris::machine::Machine>()
            .unwrap();
        native.cpu_step_n_inline(1).unwrap();
        let saved_pc = native.cpu_state_digest().unwrap().pc;
        let write = |machine: &mut dyn Workstation, value| {
            media::with_bus(machine, 0, |bus| {
                bus.request(
                    1,
                    &ScsiRequest {
                        cdb: vec![scsi_cmd::WRITE_10, 0, 0, 0, 0, 3, 0, 0, 1, 0],
                        data_len: ScsiDataLength::Unlimited,
                        data_in: Some(vec![value; 512]),
                    },
                )
                .unwrap()
                .unwrap();
            })
            .unwrap();
        };
        write(owned.machine.as_mut(), 0xA5);
        let mut paused = true;
        let (result, fatal) = execute(
            owned.machine.as_mut(),
            &context,
            directory.path(),
            "roundtrip",
            Operation::Save,
            &mut paused,
        );
        result.unwrap();
        assert!(!fatal);
        assert!(paused);
        assert!(!owned.machine.cpu_is_running());
        let (result, _) = execute(
            owned.machine.as_mut(),
            &context,
            directory.path(),
            "roundtrip",
            Operation::Save,
            &mut paused,
        );
        assert!(result.unwrap_err().contains("already exists"));
        write(owned.machine.as_mut(), 0x5A);
        let native = owned
            .machine
            .as_any_mut()
            .downcast_mut::<iris::machine::Machine>()
            .unwrap();
        native.cpu_step_n_inline(2).unwrap();
        assert_ne!(native.cpu_state_digest().unwrap().pc, saved_pc);
        let (result, fatal) = execute(
            owned.machine.as_mut(),
            &context,
            directory.path(),
            "roundtrip",
            Operation::Restore,
            &mut paused,
        );
        result.unwrap();
        assert!(!fatal);
        assert!(paused);
        assert!(!owned.machine.cpu_is_running());
        assert_eq!(
            owned
                .machine
                .as_any_mut()
                .downcast_mut::<iris::machine::Machine>()
                .unwrap()
                .cpu_state_digest()
                .unwrap()
                .pc,
            saved_pc
        );
        media::with_bus(owned.machine.as_mut(), 0, |bus| {
            let response = bus
                .request(
                    1,
                    &ScsiRequest {
                        cdb: vec![scsi_cmd::READ_10, 0, 0, 0, 0, 3, 0, 0, 1, 0],
                        data_len: ScsiDataLength::Unlimited,
                        data_in: None,
                    },
                )
                .unwrap()
                .unwrap();
            assert_eq!(response.data, vec![0xA5; 512]);
        })
        .unwrap();
        // A damaged bundle is rejected before touching the current CPU or disk.
        std::fs::write(
            directory.path().join("checkpoints/roundtrip/state/cpu.bin"),
            b"bad",
        )
        .unwrap();
        let (result, fatal) = execute(
            owned.machine.as_mut(),
            &context,
            directory.path(),
            "roundtrip",
            Operation::Restore,
            &mut paused,
        );
        assert!(result.is_err());
        assert!(!fatal);
        assert_eq!(
            owned
                .machine
                .as_any_mut()
                .downcast_mut::<iris::machine::Machine>()
                .unwrap()
                .cpu_state_digest()
                .unwrap()
                .pc,
            saved_pc
        );
        // A failed save must restart a guest that was running before the action.
        let invalid_state = directory.path().join("not-a-directory");
        std::fs::write(&invalid_state, b"file").unwrap();
        owned.machine.cpu_start();
        paused = false;
        let (result, fatal) = execute(
            owned.machine.as_mut(),
            &context,
            &invalid_state,
            "failed-save",
            Operation::Save,
            &mut paused,
        );
        assert!(result.is_err());
        assert!(!fatal);
        assert!(!paused);
        assert!(owned.machine.cpu_is_running());
        owned.machine.stop();
        // Exercise native deserialization failure after a successful preflight.
        let manifest_path = directory
            .path()
            .join("checkpoints/roundtrip/prismatic.toml");
        let mut manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["files"]["state/cpu.bin"] =
            toml::Value::String(blake3::hash(b"bad").to_hex().to_string());
        std::fs::write(&manifest_path, toml::to_string(&manifest).unwrap()).unwrap();
        let (result, fatal) = execute(
            owned.machine.as_mut(),
            &context,
            directory.path(),
            "roundtrip",
            Operation::Restore,
            &mut paused,
        );
        assert!(result.is_err());
        assert!(fatal);
        assert!(paused);
        assert!(!owned.machine.cpu_is_running());
        owned.stop().unwrap();
        assert_eq!(std::fs::read(&disk).unwrap(), vec![0; 4096]);
    }

    #[test]
    #[cfg(iris_devices)]
    fn optional_devices_attach_and_vino_releases_on_machine_drop() {
        let _serial = MACHINE_TEST.lock().unwrap();
        for _ in 0..2 {
            let dir = tempfile::tempdir().unwrap();
            let mut advanced = AdvancedSettings::default();
            advanced.extensions.vino = crate::extensions::Source::TestPattern;
            advanced.extensions.daynaport = Some(5);
            let config = machine_config(
                dir.path(),
                &Default::default(),
                &Default::default(),
                &advanced,
                &Default::default(),
            )
            .unwrap();
            let native = thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || iris::workstation::build(config).unwrap())
                .unwrap()
                .join()
                .unwrap();
            let mut owned = OwnedMachine::new(native);
            advanced.extensions.attach(owned.machine.as_mut()).unwrap();
            media::with_bus(owned.machine.as_mut(), 0, |bus| {
                bus.with_targets(|targets| {
                    assert!(targets[5].as_ref().unwrap().moves_packets());
                });
            })
            .unwrap();
            assert!(
                owned
                    .machine
                    .as_any_mut()
                    .downcast_mut::<iris::machine::Machine>()
                    .unwrap()
                    .vino_source_status()
                    .contains("cdmc")
            );
            owned.machine.restart_peripherals();
            owned.stop().unwrap();
        }
    }

    #[test]
    fn boot_elf_runs_at_entry_instead_of_prom() {
        use std::{io::Read, net::TcpStream};
        let _serial = MACHINE_TEST.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loop.elf");
        let mut elf = vec![0u8; 92];
        elf[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        for (at, value) in [(16, 2u16), (18, 8), (40, 52), (42, 32), (44, 1)] {
            elf[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        for (at, value) in [
            (20, 1u32),
            (24, 0x88001000),
            (28, 52),
            (52, 1),
            (56, 84),
            (60, 0x88001000),
            (64, 0x08001000),
            (68, 8),
            (72, 8),
            (76, 5),
            (80, 4),
            (84, 0x1000ffff),
        ] {
            elf[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
        std::fs::write(&path, elf).unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.boot_elf = Some(path.to_str().unwrap().into());
        let mut emulator = Emulator::spawn(
            &directory.path().join("state"),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(emulator.status().phase, Phase::Running);
        emulator.request_pause(true).unwrap();
        while emulator.status().phase != Phase::Paused && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(emulator.status().phase, Phase::Paused);
        let mut stream = TcpStream::connect(emulator.status().monitor.unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let prompt = |stream: &mut TcpStream| {
            let mut response = Vec::new();
            let mut byte = [0];
            while !response.ends_with(b"\n> ") && response != b"> " {
                stream.read_exact(&mut byte).unwrap();
                response.push(byte[0]);
            }
            String::from_utf8_lossy(&response).into_owned()
        };
        prompt(&mut stream);
        stream.write_all(b"r\n").unwrap();
        let registers = prompt(&mut stream);
        assert!(
            registers.contains("88001000") || registers.contains("88001004"),
            "{registers}"
        );
        emulator.request_stop();
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
    }

    #[cfg(feature = "perfetto")]
    #[test]
    #[cfg(iris_trace)]
    fn boot_traces_flush_and_each_boot_creates_a_separate_file() {
        let _serial = MACHINE_TEST.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.trace_on_boot = true;
        advanced.expert.trace_categories = "all,-prim".into();
        for index in 1..=2 {
            let mut emulator = Emulator::spawn(
                directory.path(),
                &MachineSettings::default(),
                &MediaSettings::default(),
                &advanced,
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Running);
            assert!(emulator.status().runtime.contains("recording to"));
            thread::sleep(Duration::from_millis(100));
            if index == 2 {
                // Same API used by `perfetto stop` in Iris's monitor.
                iris::perfetto::stop().unwrap();
            }
            emulator.request_stop();
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(emulator.finished());
            assert_eq!(emulator.status().phase, Phase::Stopped);
            assert_eq!(iris::perfetto::mask(), 0);
            let files: Vec<_> = std::fs::read_dir(directory.path().join("traces"))
                .unwrap()
                .collect();
            assert_eq!(files.len(), index);
            assert!(
                files
                    .iter()
                    .all(|file| file.as_ref().unwrap().metadata().unwrap().len() > 64)
            );
        }
    }

    #[test]
    #[cfg(all(iris_gdb, iris_vnc))]
    fn remote_services_disconnect_clients_and_release_ports_on_label_change() {
        use std::{
            io::Read,
            net::{TcpListener, TcpStream},
        };
        let _serial = MACHINE_TEST.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.gdb_port = Some(0);
        advanced.expert.vnc_port = Some(0);
        #[cfg(feature = "jitv2")]
        {
            advanced.expert.jit_threads = 2;
            advanced.expert.jit_cache = true;
        }
        for index in 0..2 {
            let mut emulator = Emulator::spawn(
                &root.path().join(index.to_string()),
                &MachineSettings::default(),
                &MediaSettings::default(),
                &advanced,
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(emulator.status().phase, Phase::Running);
            let report = emulator.status().runtime;
            let endpoint = |name: &str| -> SocketAddr {
                report
                    .lines()
                    .find_map(|s| s.strip_prefix(name))
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            let gdb_addr = endpoint("GDB: ");
            let vnc_addr = endpoint("VNC: ");
            let mut debugger = TcpStream::connect(gdb_addr).unwrap();
            debugger
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            debugger.write_all(b"$?#3f").unwrap();
            let mut byte = [0];
            debugger.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [b'+']); // Actual RSP, not just a listening socket.
            // Leave VNC mid-handshake; shutdown must still close/join it.
            let mut viewer = TcpStream::connect(vnc_addr).unwrap();
            viewer
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut banner = [0; 12];
            viewer.read_exact(&mut banner).unwrap();
            assert_eq!(&banner, b"RFB 003.008\n");
            emulator.request_stop();
            let deadline = Instant::now() + Duration::from_secs(8);
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(
                emulator.finished(),
                "Remote clients prevented worker shutdown"
            );
            assert_eq!(emulator.status().phase, Phase::Stopped);
            assert!(emulator.status().runtime.is_empty());
            assert_eq!(viewer.read(&mut byte).unwrap(), 0);
            let mut remaining = Vec::new();
            debugger.read_to_end(&mut remaining).unwrap();
            let gdb_probe = TcpListener::bind(gdb_addr).unwrap();
            let vnc_probe = TcpListener::bind(vnc_addr).unwrap();
            advanced.expert.gdb_port = Some(gdb_addr.port());
            advanced.expert.vnc_port = Some(vnc_addr.port());
            drop((gdb_probe, vnc_probe));
        }
    }

    #[test]
    fn occupied_connection_port_fails_before_constructing_machine() {
        let _serial = MACHINE_TEST.lock().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.monitor_port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let mut emulator = Emulator::spawn(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(emulator.status().phase, Phase::Failed(ref error) if error.contains("Monitor port"))
        );
        assert!(emulator.input().is_none());
        assert!(!dir.path().join("nvram.bin").exists());
    }

    #[test]
    fn occupied_serial_port_fails_before_constructing_or_writing_firmware() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.serial_port_b = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let mut emulator = Emulator::spawn(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(
            matches!(emulator.status().phase, Phase::Failed(ref error) if error.contains("Serial B"))
        );
        assert!(emulator.input().is_none());
        assert!(!dir.path().join("nvram.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    #[cfg(all(iris_serial, iris_firmware))]
    fn worker_ptys_survive_pause_and_close_when_each_model_is_unloaded() {
        let _serial = MACHINE_TEST.lock().unwrap();
        for profile in MachineSettings::PROFILES {
            let dir = tempfile::tempdir().unwrap();
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            hardware.cpu = hardware.cpus()[0];
            let mut advanced = AdvancedSettings::default();
            advanced.expert.serial_a = crate::serial::Route::Pty;
            advanced.expert.serial_b = crate::serial::Route::Pty;
            let mut emulator = Emulator::spawn(
                dir.path(),
                &hardware,
                &MediaSettings::default(),
                &advanced,
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Running, "{profile:?}");
            let runtime = emulator.status().runtime;
            let paths: Vec<_> = runtime
                .lines()
                .filter(|line| line.starts_with("Serial "))
                .map(|line| line.split_once(": PTY ").unwrap().1.to_owned())
                .collect();
            assert_eq!(paths.len(), 2);
            assert_ne!(paths[0], paths[1]);
            let clients: Vec<_> = paths
                .iter()
                .map(|p| {
                    std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(p)
                        .unwrap()
                })
                .collect();
            emulator.request_pause(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while emulator.status().phase != Phase::Paused && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Paused);
            for client in &clients {
                assert!(nix::sys::termios::tcgetattr(client).is_ok());
            }
            emulator.request_stop();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(emulator.finished());
            drop(emulator);
            for client in &clients {
                assert!(
                    nix::sys::termios::tcgetattr(client).is_err(),
                    "{profile:?}: PTY stayed open"
                );
            }
        }
    }

    #[test]
    fn expert_clock_and_connections_map_without_startup_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let mut advanced = AdvancedSettings::default();
        advanced.expert.count_mhz = Some(crate::expert::CountMhz::try_from(97.5).unwrap());
        advanced.expert.monitor_port = 12345;
        advanced.expert.serial_port_a = 12346;
        advanced.expert.serial_port_b = 12347;
        let config = machine_config(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(config.clock.fixed_mhz, Some(97.5));
        assert_eq!(
            (
                config.monitor_port,
                config.serial_port_a,
                config.serial_port_b
            ),
            (Some(12345), Some(12346), Some(12347))
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn worker_storage_actions_use_second_bus_and_preserve_paused_scratch_state() {
        use crate::storage::{Action, Operation};
        let _serial = MACHINE_TEST.lock().unwrap();
        for profile in [
            MachineProfile::Indigo2Ip22,
            MachineProfile::O2Ip32,
            MachineProfile::OctaneIp30,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let cd1 = dir.path().join("one.iso");
            let cd2 = dir.path().join("two.iso");
            std::fs::write(&cd1, vec![1; 65536]).unwrap();
            std::fs::write(&cd2, vec![2; 65536]).unwrap();
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            hardware.cpu = hardware.cpus()[0];
            if hardware.supports_prom_override() && !crate::compat::FIRMWARE {
                let rom = dir.path().join("loop.bin");
                let mut bytes = vec![0; 512 * 1024];
                bytes[..4].copy_from_slice(&0x1000ffff_u32.to_be_bytes());
                std::fs::write(&rom, bytes).unwrap();
                hardware.set_prom(Some(rom.to_str().unwrap().into()));
            }
            let mut media = MediaSettings::default();
            let slot = 16 + profile.max_scsi_id();
            media.devices.insert(
                slot,
                media::Medium::inspect(&cd1, media::MediaKind::Cdrom).unwrap(),
            );
            media
                .cd_changers
                .insert(slot, vec![cd2.to_str().unwrap().into()]);
            media.scratch.slot = Some(2);
            let mut emulator = Emulator::spawn(
                dir.path(),
                &hardware,
                &media,
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Running);
            let execute = |operation, path| {
                emulator
                    .storage_action(Action { operation, path })
                    .unwrap()
                    .0
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap()
            };
            assert!(
                execute(Operation::NextCd(slot), None)
                    .unwrap()
                    .contains("two.iso")
            );
            assert!(
                execute(
                    Operation::LoadCd(slot),
                    Some(dir.path().join("missing.iso"))
                )
                .is_err()
            );
            assert!(
                execute(Operation::Info(slot), None)
                    .unwrap()
                    .contains("two.iso")
            );
            assert!(
                execute(Operation::EmptyCd(slot), None)
                    .unwrap()
                    .contains("empty")
            );
            let payload = dir.path().join("input.bin");
            std::fs::write(&payload, b"scratch test").unwrap();
            execute(Operation::ImportScratch, Some(payload.clone())).unwrap();
            assert_eq!(emulator.status().phase, Phase::Running);
            emulator.request_pause(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while emulator.status().phase != Phase::Paused && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Paused);
            let ps2 = emulator.input().unwrap();
            let ps2_was_running = iris::traits::Device::is_running(ps2.as_ref());
            execute(Operation::ImportScratch, Some(payload)).unwrap();
            thread::sleep(Duration::from_millis(150));
            assert_eq!(emulator.status().phase, Phase::Paused);
            // IP22 stops PS/2 on pause; O2 keeps its controller active.
            // The transfer must preserve the model's peripheral state too.
            assert_eq!(
                iris::traits::Device::is_running(ps2.as_ref()),
                ps2_was_running
            );
            let scratch = media.scratch.path(dir.path());
            let bytes = std::fs::read(&scratch).unwrap();
            assert_eq!(&bytes[4096..4108], b"scratch test");
            emulator.request_stop();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(emulator.finished());
            assert_eq!(emulator.status().phase, Phase::Stopped);
        }
    }

    #[test]
    #[cfg(iris_poweroff)]
    fn guest_power_off_finishes_worker_but_monitor_halt_does_not() {
        use std::{io::Read, net::TcpStream};
        let _serial = MACHINE_TEST.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let hardware = MachineSettings {
            profile: MachineProfile::Indigo2Ip22,
            ..Default::default()
        };
        let mut emulator = Emulator::spawn(
            dir.path(),
            &hardware,
            &MediaSettings::default(),
            &AdvancedSettings::default(),
            &Default::default(),
        )
        .unwrap();
        let wait_for = |emulator: &Emulator, phase: Phase| {
            let deadline = Instant::now() + Duration::from_secs(15);
            while emulator.status().phase != phase && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, phase);
        };
        wait_for(&emulator, Phase::Running);
        emulator.request_pause(true).unwrap();
        wait_for(&emulator, Phase::Paused);
        assert!(!emulator.finished());
        emulator.request_pause(false).unwrap();
        wait_for(&emulator, Phase::Running);
        let mut stream = TcpStream::connect(emulator.status().monitor.unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let prompt = |stream: &mut TcpStream| {
            let mut response = Vec::new();
            let mut byte = [0];
            while !response.ends_with(b"\n> ") && response != b"> " {
                stream.read_exact(&mut byte).unwrap();
                response.push(byte[0]);
            }
            String::from_utf8_lossy(&response).into_owned()
        };
        prompt(&mut stream);
        stream.write_all(b"machine-stop\n").unwrap();
        assert!(prompt(&mut stream).contains("halted"));
        wait_for(&emulator, Phase::Halted);
        assert!(
            !emulator.finished(),
            "A monitor halt must keep the machine available"
        );

        // The real IP22 panel power latch: the same write used by guest power-off.
        stream.write_all(b"mw 0xbfbd9850 0 b\n").unwrap();
        wait_for(&emulator, Phase::PoweredOff);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(emulator.input().is_none());
        assert!(emulator.status().monitor.is_none());
        assert!(dir.path().join("nveeprom.bin").is_file());
        emulator.request_stop();
        assert_eq!(emulator.status().phase, Phase::PoweredOff);
    }

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
            &Default::default(),
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
            ..Default::default()
        };
        let cfg = machine_config(
            dir.path(),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &advanced,
            &Default::default(),
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
            &Default::default(),
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
            &Default::default(),
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
        assert_eq!(config.no_audio, !crate::compat::AUDIO);
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
    fn dual_heads_keep_latest_frames_and_handle_independent_mode_changes() {
        let frames = FrameSlot::new(2);
        assert!(frames.take().is_none());
        frames.publish(1, pack_frame(&[0x33], 1, 1, 1));
        let frame = frames.take().unwrap();
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(frame.rgba, [0, 0, 0, 255, 0x33, 0, 0, 255]);
        frames.publish(0, pack_frame(&[0x11, 0x22], 1, 2, 1));
        frames.publish(1, pack_frame(&[0x44, 0x55], 2, 1, 2));
        frames.publish(1, pack_frame(&[0x66, 0x77], 2, 1, 2));
        let frame = frames.take().unwrap();
        assert_eq!((frame.width, frame.height), (3, 2));
        assert_eq!(
            frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [0x11, 0x66, 0x77, 0x22, 0, 0]
        );
        assert!(frames.take().is_none());
        frames.publish(0, pack_frame(&[0x88], 1, 1, 1));
        let frame = frames.take().unwrap();
        assert_eq!((frame.width, frame.height), (3, 1));
        assert_eq!(
            frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [0x88, 0x66, 0x77]
        );
    }

    #[test]
    fn dual_newport_worker_publishes_both_heads_and_stops_cleanly() {
        let _serial = MACHINE_TEST.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let hardware = MachineSettings {
            heads: 2,
            ..Default::default()
        };
        let mut emulator = Emulator::spawn(
            directory.path(),
            &hardware,
            &MediaSettings::default(),
            // The diskless PROM leaves head 1 disabled. Seed both controllers
            // with a video mode so both refresh threads can present immediately.
            &AdvancedSettings {
                resolution: iris::dev::ng1::vc2_timings::NewportResolution::Res1024x768,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let both = loop {
            let ready = emulator
                .frames
                .inner
                .lock()
                .unwrap()
                .frames
                .iter()
                .all(Option::is_some);
            if ready || emulator.finished() || Instant::now() >= deadline {
                break ready;
            }
            thread::sleep(Duration::from_millis(10));
        };
        emulator.request_stop();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert_eq!(emulator.status().phase, Phase::Stopped);
        assert!(
            both,
            "Both actual Iris refresh threads must reach their renderers"
        );
    }

    #[test]
    fn all_extensions_map_to_valid_core_configuration() {
        for profile in MachineSettings::PROFILES {
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            hardware.cpu = hardware.cpus()[0];
            for &board in hardware.graphics_choices() {
                hardware.graphics = Some(board);
                hardware.heads = if hardware.newport() { 2 } else { 1 };
                hardware.cpus = if profile == MachineProfile::OctaneIp30 {
                    2
                } else {
                    1
                };
                hardware.ram_mb = *hardware.ram_sizes().last().unwrap();
                let mut media = MediaSettings::default();
                for controller in 0..profile.scsi_controllers() {
                    let slot = controller * 16 + profile.max_scsi_id();
                    media.devices.insert(
                        slot,
                        media::Medium {
                            kind: media::MediaKind::Disk,
                            path: format!("disk-{slot}.raw"),
                        },
                    );
                }
                let config = machine_config(
                    Path::new("/tmp/unused-prismatic-config"),
                    &hardware,
                    &media,
                    &AdvancedSettings::default(),
                    &Default::default(),
                )
                .unwrap();
                assert_eq!(config.graphics.board, Some(board));
                assert_eq!(config.graphics.heads, hardware.heads);
                assert_eq!(config.machine.cpus, hardware.cpus);
                assert_eq!(config.banks.iter().sum::<u32>(), hardware.ram_mb);
                for (&slot, device) in &config.scsi {
                    assert_eq!(device.placement(slot), (slot / 16, slot % 16));
                }
            }
        }
    }

    #[test]
    fn slow_consumer_receives_only_latest_frame() {
        let frames = FrameSlot::default();
        frames.publish(0, pack_frame(&[0x11], 1, 1, 1));
        frames.publish(0, pack_frame(&[0x22], 1, 1, 1));
        assert_eq!(frames.take().unwrap().rgba[0], 0x22);
        assert!(frames.take().is_none());
    }

    #[test]
    fn impact_and_gbe_frames_keep_native_pixels_and_monitor_readback() {
        let frames = FrameSlot::default();
        let mut renderer = CaptureRenderer::new(frames.clone(), Activity::default(), 0);
        let mut screen = Rex3Screen::new();
        screen.prebuilt = true;
        screen.width = 1280;
        screen.height = 1024;
        screen.fb_rgb[0] = 0xff332211;
        screen.fb_rgb[1023 * FB_STRIDE + 1279] = 0xff665544;
        let stats = BarStats {
            now: Instant::now(),
            cycles: 1,
            fasttick: 0,
            more_cpus: [None; iris::disp::MORE_CPUS],
            decoded_delta: 0,
            l1i_hits: 0,
            l1i_fetches: 0,
            uncached: 0,
            hb: 0,
            count_hz: 1,
            gfifo_pending: 0,
        };
        renderer.present(
            &mut screen,
            &mut DebugOverlay::new(),
            &mut StatusBar::new(),
            &mut StatusBarTexture::new(),
            &stats,
            true,
            None,
            None,
        );
        let frame = frames.take().unwrap();
        assert_eq!((frame.width, frame.height), (1280, 1024));
        assert_eq!(&frame.rgba[..4], &[0x11, 0x22, 0x33, 255]);
        assert_eq!(
            &frame.rgba[frame.rgba.len() - 4..],
            &[0x44, 0x55, 0x66, 255]
        );
        assert_eq!(screen.rgba[1023 * FB_STRIDE + 1279], 0xff665544);
        assert_eq!(screen.frame_seq, 1);
    }

    #[test]
    fn missing_external_prom_is_a_recoverable_worker_failure() {
        for profile in [MachineProfile::O2Ip32, MachineProfile::OctaneIp30] {
            let dir = tempfile::tempdir().unwrap();
            let mut hardware = MachineSettings {
                profile,
                cpu: crate::compat::CpuModel::R10000,
                ..Default::default()
            };
            hardware.set_prom(Some(
                dir.path().join("missing.bin").to_str().unwrap().into(),
            ));
            let mut emulator = Emulator::spawn(
                dir.path(),
                &hardware,
                &MediaSettings::default(),
                &AdvancedSettings::default(),
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(emulator.finished());
            let Phase::Failed(error) = emulator.status().phase else {
                panic!("Expected a PROM error")
            };
            assert!(error.contains("Cannot open PROM"));
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn o2_and_octane_start_attach_media_and_stop_with_synthetic_firmware() {
        let _serial = MACHINE_TEST.lock().unwrap();
        // A MIPS branch-to-self checks construction, SCSI attachment, PS/2 and
        // teardown with a deterministic guest program instead of waiting for PROM boot.
        for (profile, cpu) in [
            (MachineProfile::O2Ip32, crate::compat::CpuModel::R5000),
            (MachineProfile::O2Ip32, crate::compat::CpuModel::R10000),
            (MachineProfile::OctaneIp30, crate::compat::CpuModel::R10000),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut prom = vec![0; 512 * 1024];
            prom[..4].copy_from_slice(&0x1000ffff_u32.to_be_bytes());
            let rom = dir.path().join("loop.bin");
            std::fs::write(&rom, &prom).unwrap();
            let disk = dir.path().join("scratch.raw");
            let cd = dir.path().join("cd.iso");
            std::fs::write(&disk, vec![0x5a; 65536]).unwrap();
            std::fs::write(&cd, vec![0x6b; 65536]).unwrap();
            let mut media = MediaSettings::default();
            for (id, path, kind) in [
                (16 + profile.max_scsi_id(), &disk, media::MediaKind::Disk),
                (profile.max_scsi_id(), &cd, media::MediaKind::Cdrom),
            ] {
                media
                    .devices
                    .insert(id, media::Medium::inspect(path, kind).unwrap());
            }
            let mut hardware = MachineSettings {
                profile,
                cpu,
                ..Default::default()
            };
            hardware.cpus = if profile == MachineProfile::OctaneIp30 {
                2
            } else {
                1
            };
            hardware.set_prom(Some(rom.to_str().unwrap().into()));
            let mut emulator = Emulator::spawn(
                &dir.path().join("state"),
                &hardware,
                &media,
                &AdvancedSettings::default(),
                &Default::default(),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while emulator.status().phase == Phase::Starting && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(emulator.status().phase, Phase::Running);
            assert!(emulator.input.lock().unwrap().is_some());
            emulator.request_stop();
            while !emulator.finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(emulator.finished(), "Machine did not stop");
            assert_eq!(emulator.status().phase, Phase::Stopped);
            assert_eq!(std::fs::read(&disk).unwrap(), vec![0x5a; 65536]);
            assert_eq!(std::fs::read(&cd).unwrap(), vec![0x6b; 65536]);
            assert_eq!(std::fs::read(&rom).unwrap(), prom);
            assert!(dir.path().join("scratch.raw.overlay").is_file());
        }
    }

    #[test]
    fn startup_failure_is_reported_without_blocking_the_caller() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut emulator = Emulator::spawn(
            &file.path().join("invalid"),
            &MachineSettings::default(),
            &MediaSettings::default(),
            &AdvancedSettings::default(),
            &Default::default(),
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
        for profile in MachineSettings::PROFILES {
            let mut hardware = MachineSettings {
                profile,
                ..Default::default()
            };
            for &cpu in hardware.cpus().iter().filter(|cpu| cpu.iris().is_ok()) {
                for &ram_mb in hardware.ram_sizes() {
                    hardware.cpu = cpu;
                    hardware.ram_mb = ram_mb;
                    let config = machine_config(
                        Path::new("/tmp/prismatic-config-test"),
                        &hardware,
                        &MediaSettings::default(),
                        &AdvancedSettings::default(),
                        &Default::default(),
                    )
                    .unwrap();
                    assert_eq!(config.machine.profile, profile);
                    assert_eq!(config.machine.cpu, cpu.iris().unwrap());
                    if !hardware.newport() {
                        assert_eq!(
                            config.graphics.resolution,
                            iris::dev::ng1::vc2_timings::NewportResolution::Guest
                        );
                        assert_eq!(config.machine.cache, CacheModel::Shadow);
                    }
                    if matches!(
                        profile,
                        MachineProfile::Indigo2Ip28 | MachineProfile::OctaneIp30
                    ) {
                        assert_eq!(config.graphics.board, Some(GraphicsBoard::SolidImpact));
                    }
                    assert_eq!(
                        config.octane.flash.is_some(),
                        profile == MachineProfile::OctaneIp30
                    );
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
