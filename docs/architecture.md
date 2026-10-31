# Frontend and Iris integration

## Design baseline

Prismatic uses Rust and SDL3, following the native Happy Years desktop UI,
particularly 1989's draft settings and confirmation flow. The guest display
occupies the window; F9 opens the dark blue options panel with amber selection,
compact bitmap text and keyboard navigation. F11 toggles fullscreen; F12
confirms quit. Advanced controls are behind Tinker.

Rendering, textures, windows and host events belong to SDL on the main thread.
Settings policy is independent of drawing. The application polls machine
updates while emulation is active, and redraws only for new frames, UI events,
status changes and notification expiry. Guest proportions are preserved inside
the display area, leaving room for the optional status strip.

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | CLI, SDL lifecycle, events, texture uploads, asynchronous shutdown, smoke runs |
| `src/emulator.rs` | Diskless Iris configuration, worker ownership, software frame capture, latest-frame slot, machine status |
| `src/overlay.rs` | Draft settings, navigation, modal choices and effects |
| `src/settings.rs` | Desktop preferences, validation and atomic file replacement |
| `src/display.rs` | Packed RGBA frame type, startup/UI-preview pixels and aspect-preserving placement |
| `src/view.rs` | SDL drawing, status strip, options, confirmations and notifications |

## Core boundary

The root Cargo manifest depends directly on `../iris`, preserving that fork's
winit and block patches at the root where Cargo applies them. The nightly
Rust toolchain matches Iris. No emulator source is copied into this repository.
The first machine is fixed to Indy IP24, R4400, 64 MB RAM, one Newport head,
and no SCSI devices. An empty PROM path explicitly selects the embedded Indy
firmware; no working-directory ROM or Iris configuration is loaded.

Iris's current library still compiles winit/glutin, CPAL, and camera support.
Prismatic does not call `Ui::run`, so there is only the SDL window. Audio is
disabled in the machine config. Removing unused host dependencies and exposing
a PCM sink for SDL audio are later changes in the fork.

## Machine lifecycle

A worker with a 64 MB stack calls `iris::workstation::build(MachineConfig)`.
It owns the boxed `Workstation`, registers Iris's system/event controller and
installs the capture renderer before starting peripherals. It starts the CPU
explicitly when needed because Iris debug builds otherwise leave it parked.
Calls that access a running machine are serialized with Iris's `MachineSlot`
lock. The lock is released before dropping the machine: Iris's destructor
acquires it to invalidate the pointer held by its controller.

Shared status reports Starting, Running, Halted, Stopping, Stopped or Failed.
Running follows the core's CPU-running flag. The monitor and serial endpoints
bind ephemeral loopback ports; the monitor's actual address is printed in the
terminal. NVRAM lives in Prismatic's machine-state directory, separate from
frontend preferences and the Iris checkout. Use different `--state-dir` paths
for simultaneous instances. Iris also writes its shared draw-shape corpus
under `~/.iris/` unless `IRIS_REX_JIT_PROFILE` overrides that path; the documented
boot smoke command redirects this cache to `/tmp`.

Confirmed quit sends a stop request through a bounded channel. SDL continues
pumping events while the worker stops the CPU and devices, saves NVRAM through
Iris's normal stop path, and drops the machine. Startup/worker panics are
reported as failures. A guard also attempts stop during worker unwinding.
Normal shutdown has a ten-second UI deadline; the destructor has a bounded
five-second fallback for exceptional exits. If the core hangs, the program
reports failure rather than waiting indefinitely. These policies currently
apply only to diskless sessions; writable guests need a guest-aware shutdown
and disk synchronization flow before they are supported.

## Display handoff

`CaptureRenderer` implements `iris::dev::ng1::rex3::Renderer` and uses
`SwCompositor` on Iris's refresh thread. It packs the composited source rows
using Iris's stride, converts the `0xAABBGGRR` words to RGBA bytes and supplies
opaque alpha. Iris's old debug/status chrome is omitted. Core readback requests
also populate `Rex3Screen::rgba` and advance its frame sequence.

A single mutex-protected slot holds the newest pending `Arc<Frame>`. The
producer replaces an unread frame, so a slow UI never builds a video queue.
The SDL thread takes the frame, releases the lock, recreates the texture when
dimensions change, and uploads it. Heartbeat-only frames are skipped unless
Iris requests readback. No SDL renderer or texture crosses threads.

## Settings and verification

Desktop options are private drafts. The model emits a save request and the
application closes the session only after persistence succeeds. Discard needs
no runtime undo operation. These preferences do not alter Iris's hardware.

The UI smoke test never starts a machine. The PROM display smoke test creates
fresh temporary NVRAM, waits for a nonuniform guest image, captures the SDL
output after ten additional seconds and requests normal stop. It has a
120-second display timeout and returns failure for startup/shutdown errors.
Visual inspection of its screenshot establishes what PROM actually displayed;
a nonuniform-frame check alone does not identify a PROM menu or an OS boot.

## Next milestones

1. Connect SDL physical scancodes and relative mouse input to
   `Workstation::get_ps2()`. Release held guest keys/buttons on focus loss,
   capture release and overlay entry. Keep modal input and host shortcuts out
   of the guest, with an explicit way to send guest function keys.
2. Add SDL audio and feature boundaries for Iris's existing host backends.
   Validate Linux, Windows and macOS builds and device behavior.
3. Add General, Media, Extensions and Tinker-gated Advanced machine controls.
   Classify settings as live, next boot or restart-required. Validate before
   applying, confirm disruptive changes and use native media pickers.
4. Add F4 screenshots, F5 confirmed reset, F6 recording, F7 pause, activity
   indicators and window persistence as their backend behavior becomes ready.
