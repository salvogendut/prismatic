//! Iris owns the machine on a worker; SDL only sees status and the latest frame.
use crate::display::Frame;
use iris::{
    compositor::SwCompositor,
    config::{CpuModel, GraphicsBoard, KeyboardLayout, MachineConfig, MachineProfile, banks_from4},
    debug_overlay::DebugOverlay,
    dev::ng1::rex3::Renderer,
    dev::ps2::Ps2Controller,
    disp::{BarStats, FB_MAX_H, FB_STRIDE, Rex3Screen, StatusBar, StatusBarTexture},
    workstation::{MachineSlot, Workstation},
};
use std::{
    any::Any,
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
            Self::Starting => "Starting Indy...",
            Self::Running => "Indy / R4400 / 64 MB",
            Self::Halted => "Indy halted",
            Self::Stopping => "Stopping Indy...",
            Self::Stopped => "Indy stopped",
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
}

impl CaptureRenderer {
    fn new(frames: FrameSlot) -> Self {
        Self {
            frames,
            compositor: SwCompositor::new(),
            presented: false,
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
        _stats: &BarStats,
        need_readback: bool,
        live_fb_rgb: Option<&[u32]>,
        live_fb_aux: Option<&[u32]>,
    ) {
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

fn diskless_config(state_dir: &Path, keyboard: KeyboardLayout) -> Result<MachineConfig, String> {
    let path = |name: &str| {
        state_dir
            .join(name)
            .into_os_string()
            .into_string()
            .map_err(|_| "Iris requires a UTF-8 state directory".to_string())
    };
    let mut config = MachineConfig {
        // Empty path always selects Iris's embedded Indy PROM, regardless of cwd.
        prom: String::new(),
        nvram: path("nvram.bin")?,
        nveeprom: path("nveeprom.bin")?,
        banks: banks_from4([64, 0, 0, 0]),
        scsi: Default::default(),
        no_audio: true,
        headless: false,
        keyboard,
        // Ephemeral loopback ports avoid collisions with another Iris instance.
        monitor_port: Some(0),
        serial_port_a: Some(0),
        serial_port_b: Some(0),
        ..MachineConfig::default()
    };
    config.machine.profile = MachineProfile::IndyIp24;
    config.machine.cpu = CpuModel::R4400;
    config.graphics.board = Some(GraphicsBoard::Newport);
    config.graphics.heads = 1;
    Ok(config)
}

// Serialize access with Iris's monitor/event controller. Dropping the machine
// must happen after releasing the slot lock, since Iris's Drop takes it too.
struct OwnedMachine {
    machine: Box<dyn Workstation>,
    slot: MachineSlot,
    stopped: bool,
}

impl OwnedMachine {
    fn new(mut machine: Box<dyn Workstation>) -> Self {
        let slot = machine.machine_slot();
        Self {
            machine,
            slot,
            stopped: false,
        }
    }

    fn stop(&mut self) {
        self.stopped = true;
        let _guard = self.slot.lock();
        self.machine.stop();
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
}

impl Emulator {
    pub fn spawn(state_dir: &Path, keyboard: KeyboardLayout) -> Result<Self, String> {
        let config = diskless_config(state_dir, keyboard)?;
        let state_dir = state_dir.to_owned();
        let frames = FrameSlot::default();
        let status = Arc::new(Mutex::new(Status::default()));
        let (stop, receiver) = mpsc::sync_channel(1);
        let worker_frames = frames.clone();
        let worker_status = status.clone();
        let input = Arc::new(Mutex::new(None));
        let worker_input = input.clone();
        let thread = thread::Builder::new()
            .name("prismatic-iris".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                    std::fs::create_dir_all(&state_dir)
                        .map_err(|e| format!("Cannot create machine state directory: {e}"))?;
                    let mut owned = OwnedMachine::new(iris::workstation::build(config)?);
                    owned.machine.register_system_controller();
                    {
                        let _guard = owned.slot.lock();
                        let display = owned
                            .machine
                            .get_display()
                            .ok_or("Iris did not create a display")?;
                        *display.renderer_slot().lock() =
                            Some(Box::new(CaptureRenderer::new(worker_frames)));
                        if matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                            owned.machine.start();
                            // Iris debug builds deliberately leave the CPU parked.
                            if !owned.machine.cpu_is_running() {
                                owned.machine.cpu_start();
                            }
                            *worker_input.lock().unwrap() = Some(owned.machine.get_ps2());
                        } else {
                            return Ok(());
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
                    owned.stop();
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
        // polling finished(). A wedged core must not hang process teardown.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.finished() && Instant::now() < deadline {
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

    #[test]
    fn diskless_profile_never_uses_default_media_or_cwd_state() {
        let directory = tempfile::tempdir().unwrap();
        let config = diskless_config(directory.path(), KeyboardLayout::Iso).unwrap();
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
        let mut emulator =
            Emulator::spawn(&file.path().join("invalid"), KeyboardLayout::Ansi).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(emulator.finished());
        assert!(matches!(emulator.status().phase, Phase::Failed(_)));
    }
}
