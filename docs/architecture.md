# Frontend and Iris integration

## Design baseline

Prismatic uses Rust and SDL3, following the native Happy Years desktop UI,
particularly 1989's draft settings and confirmation flow. The guest display
occupies the window; F9 opens the dark blue options panel with amber selection,
compact bitmap text and keyboard navigation. The view follows 1989's 8-pixel
panel inset, 20-pixel rows and content-dependent height, with a capped UI scale
inside the letterboxed logical window. Dialogs and their button groups are centered.
F11 toggles fullscreen; F12 confirms quit. Advanced controls are behind Tinker.

Rendering, textures, windows and host events belong to SDL on the main thread.
Settings policy is independent of drawing. The application polls machine
updates while emulation is active, and redraws only for new frames, UI events,
status changes, LED pulse expiry and notification expiry. Guest proportions are
preserved inside the display area, leaving room for the optional 16-pixel hint
band and persistent 22-pixel LED strip. Guest hit testing uses that same area.
The centered hint line puts the red machine description before grey shortcuts.
Both bars render before modal shading. Toasts anchor above the reserved bars,
including when the hints are hidden.

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | CLI, SDL lifecycle, events, texture uploads, asynchronous shutdown, smoke runs |
| `src/emulator.rs` | Validated Iris configuration and media attachment, worker ownership, bundled firmware, frame capture, latest-frame slot, machine status |
| `src/input.rs` | Host/modal key routing, physical scancode mapping, held guest input, PS/2 delivery |
| `src/smoke.rs` | Scripted SDL input and General/Media/Advanced menu and restart smoke tests |
| `src/overlay.rs` | Draft settings, navigation, modal choices and effects |
| `src/settings.rs` | Desktop/hardware/media/Advanced settings, compatibility, RAM banks, state paths and atomic persistence |
| `src/diagnostics.rs` | Live Off/Screen/Console routing of Iris diagnostics, bounded screen text and redraw updates |
| `src/display.rs` | Packed RGBA frame type, startup/UI-preview pixels and aspect-preserving placement |
| `src/view.rs` | SDL drawing, status strip, LEDs, options, confirmations and notifications |
| `src/media.rs` | Native file chooser, asynchronous read-only probes, media settings and Iris block-node adapters |
| `src/activity.rs` | Atomic activity bits and visible LED pulse durations |

## Core boundary

The root Cargo manifest depends directly on `../iris`, preserving that fork's
winit and block patches at the root where Cargo applies them. The nightly
Rust toolchain matches Iris. No emulator source is copied into this repository.
The default is Indy IP24, R4400, 64 MB RAM, one Newport head, and no SCSI
devices. General also selects Indigo2 IP22, RAM presets, a compatible CPU and
keyboard type. Indy supports R4400/R5000 and 32–256 MB; Indigo2 exposes R4400
and 32–512 MB. RAM presets fill up to four MC banks, at most 128 MB per bank.
Frontend validation restricts model/CPU/RAM choices; `MachineConfig::validate`
also checks the generated Iris configuration before it is saved or started.

An empty PROM path selects the embedded Indy firmware. Iris's Indigo2 fallback
also searches the working directory, so the worker writes the public bundled
`prombini2` bytes to a temporary file and passes that exact path to construction.
The temporary file lives through machine teardown. No working-directory ROM,
default SCSI image or Iris configuration file is selected. Other profiles need
display paths beyond this Newport renderer and are rejected by validation.

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
the Iris checkout. Indy and Indigo2 have distinct state directories; custom
`--state-dir` preserves the existing Indy path and nests Indigo2 under it.
Use different `--state-dir` paths
for simultaneous instances. Iris also writes its shared draw-shape corpus
under `~/.iris/` unless `IRIS_REX_JIT_PROFILE` overrides that path; the documented
boot smoke command redirects this cache to `/tmp`.

Confirmed quit sends a stop request through a bounded channel. SDL continues
pumping events while the worker stops the CPU and devices, saves NVRAM through
Iris's normal stop path, and drops the machine. Startup/worker panics are
reported as failures. A guard also attempts stop during worker unwinding.
Diskless shutdown has a ten-second UI deadline and a five-second destructor
fallback. With media attached, teardown waits for storage flushing to finish;
the normal event loop remains responsive. Flush failures surface as Failed and
prevent a replacement machine. Restart/quit dialogs remind the user to shut
down the guest OS first. Iris's process-wide `set_exit_on_guest_poweroff(false)`
keeps guest power-off from terminating the frontend before its cleanup runs.

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

## Media and activity

Media maps controller 0 targets 1–7 to an explicit disk/CD type and image path.
Native SDL file-dialog callbacks hand results to a bounded channel; file probes
run on a separate worker. Escape abandons the result without changing the draft.
Save revalidates all images off the SDL thread, canonicalizes paths, rejects
duplicate hard disks and then follows the hardware restart confirmation flow.
Validation is read-only and never creates overlays. Opening the writable
backend happens only when constructing the selected machine.

The Iris CLI constructor exits the process on some media errors. Prismatic
therefore constructs an empty SCSI configuration, opens the selected block
nodes fallibly, and attaches `ScsiDevice`s through `ScsiTargets::with_targets`
before starting any devices or the CPU. Raw disks use `CowNode` over a read-only
`RawNode`; CHD disks use `ChdNode` with COW enabled. CDs use a read-only raw node
or `ChdCdNode`. No filesystem decoding lives in the frontend. Snapshot loading
and media hot swapping are not exposed; the constructor's empty disk-reference
provenance must be addressed before adding snapshot support.

`MediaNode` delegates the block interface while recording actual reads/writes
and flushing on drop. After CPU/device stop, the worker detaches targets to
force these flushes, checks retained errors, and only then reports completion.
Iris retains its persistent COW sidecar behavior. Draft selection/discard never
opens a writable node. The probe checks format/readability; a later permission
or device failure is still reported by machine startup or shutdown.

`CaptureRenderer` records CPU cycle changes and core network heartbeat bits.
Storage adapters publish per-target read/write bits. The main thread consumes
the atomic bitset and stretches pulses to 150 ms, redrawing when they expire.
LED timers do not depend on new guest framebuffer output. Function palettes
follow the siblings: CPU white, network yellow, hard disks green and optical
media cyan; RUN uses blue, and writes flash amber. Idle colors preserve each
function's hue. Lamps have black borders, a shared top separator and spacing
computed from the actual labels so the entire group is centered.

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

Desktop, hardware and media options are private drafts. `prismatic.cfg` uses
TOML and persists `[machine]`, `[advanced]` and `[media.devices.ID]` alongside
display preferences. First launch creates the file in SDL's preference directory;
`--config` selects another path. An existing default `prismatic.toml` migrates
only when the new file is absent. Invalid files remain untouched. Relative
image paths resolve beside the configuration file.
Serde defaults migrate older preference files without a separate rewrite.
Model changes retain compatible CPU/RAM selections and normalize incompatible
choices in the draft. `--keyboard` overrides the loaded value; a later Save
persists the override along with the other settings.

The model emits a save request; the application validates the proposed Iris
configuration and compares hardware/media/Advanced options against the active
machine. Changes to core options during Starting/Running/Halted open a
Restart/Cancel dialog, initially on Cancel. Both direct F9 save and Save from the unsaved-changes dialog follow
this path. Cancel retains the draft, Discard has no runtime or file side effect,
and a failed validation/write keeps the editor open. Display-only saves do not
restart. The session closes only after atomic persistence succeeds.

A confirmed core-options save releases capture and requests normal worker stop.
SDL keeps pumping events while the worker stops and joins; no second machine
is constructed concurrently. After a successful stop, the frontend drops old
input/frame handles, clears the old image and starts the selected configuration
with its model's NVRAM directory. Status labels describe the active hardware.
Stop failures/timeouts prevent replacement; construction/startup failures are
reported with the saved configuration retained for correction in F9.

Advanced stores Newport resolution, CPU cache model, RTC year offset, SCSI
interrupt timing and serial logging independently of the Tinker visibility
flag. Default values match Iris. All five participate in machine restart
detection; hiding Tinker or changing filtering/debug output alone does not restart.
The frontend maps these values into `MachineConfig` before construction. Newport
resolution is an initial mode, not an override of later guest programming.
Clock offsets seed the RTC from host time on construction/NVRAM load.

Serial logging uses Iris's channel-B sink and raw or wall-clock timestamps.
The path is `serial.log` within the selected machine's state directory. The
worker checks that this is an appendable regular file before construction,
because Iris's constructor otherwise logs an open failure only to stderr.
No log is opened during draft editing, validation or Discard. The log follows
Iris's existing write/flush behavior and never truncates an existing transcript.

Debug output is a frontend preference exposed in Advanced, stored as top-level
`debug_output = "off" | "screen" | "console"`. A process-wide Iris diagnostic
sink is installed before machine construction. Device threads emit routine
startup, bus-exception, Newport and draw-cache messages to it; host failures
remain on stderr, and serial/monitor/file tracing keep their own destinations.
Screen mode bounds formatting to 1024 ASCII characters per message, wraps at
110 columns and keeps only 12 lines. The SDL loop takes changed snapshots even
without a new guest frame. It draws the log above notifications and bottom bars,
before menu shading; clicking the log cannot capture guest input. Console
preserves the original text and stream, and Off skips formatting. Successful
Save changes the destination live; changing modes clears screen history.

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

The settings smoke test drives the actual menu through cancel, discard and a
confirmed hardware save, replacing a 64 MB Indy with a 128 MB Indigo2 in the
same process. It then runs the input script and checks the saved preferences.
It has a 240-second total deadline and always uses temporary preferences and
NVRAM. Visual inspection of `hinv` checks the new memory size and XL graphics.

The media smoke test supplies temporary image paths in place of the native
chooser, drives the real Media menu and confirms a restart with two hard disks
(raw and CHD) and three CDs (raw ISO/EFS sector fixtures and a Mode 1 CHD).
It verifies saved `prismatic.cfg` and then runs the input script. Its `hinv`
screenshot verifies SCSI target enumeration; synthetic fixtures do not test
an operating-system installation or the native dialog UI.

The Advanced smoke test changes all five core options, tests cancel/discard,
hides Tinker before saving, then confirms a restart and checks persisted
settings and log creation. It runs the input script and reopens Advanced for
visual inspection while keeping the saved Tinker flag off. Unit tests also
verify old-file defaults, invalid values, the Iris config mapping, append
preservation, invalid log targets and restart detection independent of UI flags.

## Next milestones

1. Add SDL audio and feature boundaries for Iris's existing host backends.
   Validate Linux, Windows and macOS builds and device behavior.
2. Extend General/Media/Advanced and add Extensions and Network controls.
   Classify settings as live, next boot or restart-required. Validate before
   applying and confirm disruptive changes. Add explicit COW management and
   guest-aware media swapping when their core interfaces are ready.
3. Add F4 screenshots, F5 confirmed reset, F6 recording, F7 pause
   controls and window persistence as their backend behavior becomes ready.
