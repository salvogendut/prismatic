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
| `src/input.rs` | Host/modal key routing, physical scancode mapping, held guest input, PS/2 delivery |
| `src/smoke.rs` | Scripted SDL input smoke test through the diskless PROM |
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

## Input handoff

After starting the devices, the worker publishes an `Arc<Ps2Controller>` from
`Workstation::get_ps2()`. SDL events use its internally synchronized input API
directly, without a second event queue or the machine-slot lock. The worker
withdraws the handle before stop and on failure; the frontend also releases
capture whenever the machine leaves Running. The retained Arc keeps the
controller alive during handoff, and Iris ignores events after device stop.

SDL physical scancodes map explicitly to the `winit::keyboard::KeyCode` enum
currently required by Iris. Prismatic depends on winit only for this API type;
SDL remains the host event owner. `--keyboard ansi|iso|jis` selects the core's
physical keyboard type. The guest chooses its character layout. SDL text/IME
events are not interpreted as key presses.

A left click inside the guest rectangle enables SDL relative mouse mode.
Hit testing converts window points through renderer pixels to the letterboxed
logical view, including high-DPI scaling. The capture click is consumed. Only
captured input reaches the guest. Ctrl+Enter, F9, the quit dialog, focus loss,
window hiding/minimizing and machine stop release all tracked keys/buttons and
disable relative mode. Closing an overlay or regaining focus does not recapture.

F9/F11/F12 are host shortcuts. While captured, Shift plus a reserved function
key sends a bare guest tap by temporarily lifting held modifiers and restoring
them afterward. Modal navigation consumes keys before guest delivery. Repeats
are forwarded only for keys first pressed during capture; Iris applies the
guest's typematic policy. Unmatched key/button releases are ignored.

Relative motion retains fractional counts and splits large deltas into PS/2
packet-sized steps. Screen-down-positive Y is passed to Iris, which performs
the PS/2 inversion. Vertical wheel input retains fractional steps and respects
SDL's flipped direction flag. The core controls mouse protocol negotiation and
queue coalescing; scrolling/extra buttons require guest support. Horizontal
scroll, text paste and IME composition remain outside this milestone.

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

The input smoke test uses the same temporary-state lifecycle and deadline. It
queues SDL events through the normal application loop to enter the command
monitor and type `hinv`, then checks capture release/recapture and modal entry.
Its screenshot needs visual inspection to establish the firmware response.
Unit tests also verify the resulting bytes using a real Iris PS/2 controller,
including held-state cleanup, modifier restoration and fractional mouse input.

## Next milestones

1. Add SDL audio and feature boundaries for Iris's existing host backends.
   Validate Linux, Windows and macOS builds and device behavior.
2. Add General, Media, Extensions and Tinker-gated Advanced machine controls.
   Classify settings as live, next boot or restart-required. Validate before
   applying, confirm disruptive changes and use native media pickers.
3. Add F4 screenshots, F5 confirmed reset, F6 recording, F7 pause, activity
   indicators and window persistence as their backend behavior becomes ready.
