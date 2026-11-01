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
| `src/profiles.rs` | Named machine stanzas, draft selection, state identity and concurrent catalog merging |
| `src/instance.rs` | Exclusive leases for machine state, writable disks and atomic configuration updates |
| `src/diagnostics.rs` | Live Off/Screen/Console routing of Iris diagnostics, bounded screen text and redraw updates |
| `src/capture.rs` | Asynchronous PPM screenshots, bounded GIF recording, atomic publication and optional FFmpeg optimization |
| `src/sound.rs` | Embedded Happy Years screenshot shutter through a main-thread SDL audio stream |
| `src/audio.rs` | Bounded per-machine guest PCM handoff and main-thread SDL3 playback |
| `src/display.rs` | Packed RGBA frame type, startup/UI-preview pixels and aspect-preserving placement |
| `src/view.rs` | SDL drawing, status strip, LEDs, options, confirmations and notifications |
| `src/media.rs` | Native file chooser, asynchronous read-only probes, media settings and Iris block-node adapters |
| `src/activity.rs` | Atomic activity bits and visible LED pulse durations |

## Core boundary

The root Cargo manifest depends directly on `../iris`, preserving that fork's
winit and block patches at the root where Cargo applies them. The nightly
Rust toolchain matches Iris. No emulator source is copied into this repository.
The default is Indy IP24, R4400, 64 MB RAM, one Newport head, and no SCSI
devices. General selects all five workstation profiles: Indy IP24, Indigo2 IP22,
Indigo2 IMPACT IP28, O2 IP32 and Octane IP30. Indy supports R4400/R5000, IP22
R4400, O2 R5000/R10000, and IP28/Octane R10000. Octane uses one CPU. RAM presets
fill up to four banks, largest first, with per-bank limits of 128 MB on Indy/IP22,
512 MB on IP28/Octane, and 256 MB on O2. Total limits are 256 MB, 512 MB, 1 GiB,
1 GiB and 2 GiB respectively. Frontend validation checks menu choices and the
core validates each generated `MachineConfig`. IP28 avoids 32/128 MB single
banks, which fail the current core's PROM sizing: 32 MB uses 16+16, 128 MB uses
64+64, and 384 MB uses 256+64+64. This preserves menu totals without changing
CPU/device emulation. The 1 GiB preset remains 512+512.

Indy uses its embedded PROM. For IP22/IP28 the worker writes Iris's bundled
`prombini2`/`prombinip28` bytes to a temporary file kept through teardown.
O2 and Octane use the fork's embedded 4.18/4.9 PROMs by default. Prismatic passes
an empty `prom` path to select those bytes directly, bypassing Iris's standalone
working-directory search. Optional external overrides are remembered independently
in `machine.o2_prom`/`machine.octane_prom`; Use bundled clears the draft override.
The native picker and asynchronous Save
validation check a readable regular file and size (O2: 512 KiB; Octane: 512 KiB
through 1 MiB), without writing flash/NVRAM. Content compatibility is the caller's
responsibility. The worker rechecks before construction; a missing external override is a
recoverable `Phase::Failed`, so F9 remains available. Relative firmware paths
resolve against the configuration's directory, as media paths do.

IP28 and Octane explicitly select Solid IMPACT. O2 uses its integrated CRIME/GBE
path (no graphics-board override). All three pass guest resolution and Shadow
cache; saved Newport settings remain available on returning to Indy/IP22.
`CaptureRenderer` uses Iris's `prebuilt` compositor path for IMPACT/GBE and the
existing Newport software compositor otherwise. Frame packing, readback, bounded
latest-frame delivery, capture and SDL rendering are shared. The worker sets the
common PS/2 layout before starting any CPU, including O2/Octane constructors that
do not yet apply `MachineConfig.keyboard` themselves.

Media attaches protected raw/CHD disks and read-only CDs to the common `ScsiBus`
on controller 0: WD33C93A on Indy/Indigo2, AIC7880 on O2, ISP1040 on Octane.
The activity/error adapter is retained on all models. CPU/device threads stop
before targets are detached and flushed; no frontend constructor uses Iris's
exit-on-media-error default attachment path.

Iris's current library still compiles winit/glutin, CPAL, and camera support.
Prismatic does not call `Ui::run`, so there is only the SDL window. Guest audio
is enabled, with a runtime-only `AudioConfig.output` factory per worker. The
shared HAL2/MACE/RAD output path sends resampled 48 kHz stereo i16 PCM into a
lock-free ring of at most 100 ms and hands the consumer to Prismatic's single
pending-stream inbox. It never opens CPAL for a Prismatic machine. The normal
standalone Iris configuration retains its CPAL backend.

SDL audio opening, gain, queueing and teardown all run on the main thread.
The event loop drains at most 40 ms at once and caps SDL's input queue at 60 ms,
discarding older samples after a stall. Reads/discards commit stereo pairs so
ring overflow does not swap channels. Host mute/volume live under `[audio]`,
outside named machine stanzas, and apply only after successful Save. They do
not alter guest devices or require a restart. Inactive/paused states discard
incoming samples and clear SDL's queue; a closed producer or replacement
machine drops the playback stream. A newly resumed HAL2 output replaces the
pending consumer; O2/Octane streams can remain allocated while paused.

Device errors clear playback, report once and leave emulation running. Saving
mute then unmute retries; a new machine gets a fresh audio inbox and retry
state. The screenshot shutter uses an independent stream. Host recording,
device-selection controls and removal of unused host dependencies remain future work.

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
the Iris checkout. Each model has a distinct state directory (`indy`, `indigo2`, `indigo2-ip28`,
`o2`, `octane`); custom `--state-dir` preserves the existing Indy path and nests
other models under it. O2's `nvram.bin` is its persisted flash. Octane uses
`nvram.bin` plus `flash.bin`; neither writes the PROM source.
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
The worker observes `GuestState::PoweredOff` under Iris's machine-slot lock,
then clears input, detaches and flushes media, and drops the core before publishing
`Phase::PoweredOff`. Cleanup errors still produce `Failed`. CPU inactivity alone
remains `Halted`, so monitor stops and breakpoints cannot trigger teardown.
SDL clears the final framebuffer and LEDs, finishes GIF capture and shows a
powered-off placeholder while continuing to service menus. Ctrl+P can boot again;
saving options prepares the configuration in PAUSED. Discard has no effect.

Normal startup has `Phase::Paused` and no worker. Options can be edited and saved
without constructing Iris or opening writable media. Ctrl+P constructs the worker
and boots. Once booted, pause/resume use the bounded worker command channel: pause stops
the CPU and peripherals, and resume restarts them at the retained PC. An atomic
stop flag takes priority even when that channel is full. F5 performs a cold reset
through normal stop, media flushing and complete worker reconstruction. It boots
a replacement immediately only if the old machine was running/starting; otherwise
it leaves the selected configuration PAUSED. This also recreates renderer and
CPU/device state instead of relying on an in-place reset. F5 confirmation does
not save or apply an options draft. Smoke runs retain automatic boot.

`machine_label` selects a `[machines."label"]` stanza. Each entry stores hardware,
media and Advanced machine options; presentation/capture preferences stay global.
Save As creates a distinct entry without overwriting its source. Draft selection
retains draft edits until Save; Discard affects neither files nor running state.
The first named entry adopts the old model state directory. Later entries use the
complete hex encoding of the label's ASCII bytes under `machines/`, then the model
directory, preventing path traversal and naming collisions. No state is copied
or created while editing. Writable media retain their existing overlay paths.

Each worker holds OS file locks for its state directory and writable disk images
through teardown. Read-only media need no exclusive lease. Catalog saves lock a
sidecar, compare each changed entry with its loaded baseline, merge unrelated
on-disk entries, then publish atomically. Conflicting edits fail with the draft
retained. Baselines are save bookkeeping, excluded from dirty-state comparisons.

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

Ctrl+Plus/Minus requests a window size relative to the 960×720 logical view,
in quarter steps from 0.75 to 4.0. `WindowScale` stores exact integer quarter
steps and serializes as the `window_scale` TOML factor; older files default to
1.0. Startup uses the saved factor before creating the SDL window. Resizing
does not change the logical view, guest framebuffer, hit-testing transform or
capture resolution. Window managers may constrain requested dimensions.

Scaling shortcuts save live preferences atomically before requesting a resize.
A failed save leaves both the live factor and window size unchanged. The saved
factor is copied into both the original and draft menu snapshots, preserving
unrelated edits and the session's dirty state. Completed asynchronous media
validation also merges the latest live factor before saving its older snapshot.
Thus menu Save, Discard and delayed validation cannot overwrite a newer scale.
Fullscreen defers the resize until windowed mode; maximized windows first
request restore. The main loop waits for the relevant SDL state without blocking
on a window synchronization call, then requests the saved dimensions.

## Media and activity

Media maps controller 0 targets 1–7 to an explicit disk/CD type and image path.
Indy placement conventions use target 1 for the system disk and target 6 for
installation/recovery CDs; target 0 belongs to the host controller. All Indy
devices share one bus, including internal and external devices. These placement
conventions guide the frontend without reserving other targets or implying
support for historical tape/floptical devices.
Native SDL file-dialog callbacks hand results to a bounded channel; file probes
run on a separate worker. Escape abandons the result without changing the draft.
Save revalidates all images off the SDL thread, canonicalizes paths, rejects
duplicate hard disks and then follows the hardware restart confirmation flow.
Validation is read-only and never creates overlays. Opening the writable
backend happens only when constructing the selected machine.

The Iris CLI constructor exits the process on some media errors. Prismatic
therefore constructs an empty SCSI configuration, opens the selected block
nodes fallibly, and attaches boxed `ScsiDevice`s through `ScsiBus::attach`
before starting any devices or the CPU. Raw disks use `CowNode` over a read-only
`RawNode`; CHD disks use Iris's `open_chd_cow`. Hashed bases retain MAME diffs;
hashless uncompressed bases use a read-only CHD node under a sparse sector
overlay. This avoids MAME's requirement for a nonzero parent SHA-1 without
modifying or extracting the base. Only an empty, structurally valid legacy
diff can be bypassed, and it is left intact. The sparse adapter refuses commit
so raw sector writes can never reach a CHD container. CDs use a read-only raw node
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

## Screenshot and GIF capture

F4 and F6 are host shortcuts, including when menus are open; repeats are ignored.
Shift plus either key sends a bare guest tap while input is captured, like the
other reserved function keys. Capture takes the latest immutable Iris `Frame`,
including its composed guest cursor. It does not read SDL pixels, so window size,
overlays, debug text, scanlines and the bottom bars do not enter captures.

There is at most one screenshot worker and one GIF worker. Recording starts
with a guest frame and fixed output dimensions. A two-entry synchronous channel
receives timed frame samples through nonblocking sends. The SDL event loop wakes
at the selected sampling interval even if Iris has not published a new image.
Full queues skip samples; timestamps preserve elapsed time. On the worker,
aspect-preserving scaling, color quantization and GIF/LZW encoding operate with
bounded frame storage. Identical images coalesce and long holds split at GIF's
16-bit delay limit. Closing the channel requests stop without competing for a
queue slot; the final delay includes time through the stop request.

PPM/GIF files use UTC timestamped names and unique suffixes in the working
directory. Workers flush temporary files and publish without replacing existing
captures. Failures remove incomplete files and report back to the UI. Quit and
restart stop sampling and wait asynchronously for capture completion while SDL
keeps pumping events. Fatal UI error cleanup also joins the writers.

Successful screenshot completion queues the shared 1984/1985/1986 shutter sample
through a dedicated SDL3 audio stream on the main thread. The embedded WAV is
decoded and the default playback device opened lazily. Repeated shots clear the
stream before replay, keeping queued audio bounded. Missing host audio is
reported once and never turns a successful file save into a capture failure.
This UI sound is independent of the guest's SDL audio stream and mute setting.

GIF width, fps and encoder persist in `[gif]`, independently of restart-required
Iris settings. Width 0 means native guest resolution and is the default; saved
320/480/640-pixel widths retain their meaning. Native capture uses the first
guest frame's exact dimensions and skips scaling when subsequent frames match.
A recording snapshots settings at start, so editing or saving a draft
cannot change the current recording. Optional FFmpeg optimization uses two
bounded-time passes (palette generation, then encoding) to avoid buffering the
whole video. Only a smaller, decodable result with matching dimensions and
duration replaces the built-in GIF. Missing FFmpeg or optimization failure keeps
the original and reports a warning.

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

Ctrl+Plus/Minus are host shortcuts even with menus open or input captured.
Host key symbols recognize non-US layouts, Ctrl+= and keypad plus/minus;
Alt/GUI combinations and autorepeats do not trigger scaling. Guest key mapping
continues to use physical scancodes.

F2/F4/F5/F6/F9/F11/F12 are host shortcuts. Ctrl+P boots, pauses or resumes outside
modal editing. F2 selects the next label and F5 opens reset confirmation.
While captured, Shift plus a reserved function
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

Menu desktop, hardware and media options are private drafts; window scale is an
immediate, separately saved shortcut preference. `prismatic.cfg` uses
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
machine. Changes to core options during Starting/Running/Halted or a previously
booted Paused session open a
Restart/Cancel dialog, initially on Cancel. Both direct F9 save and Save from the unsaved-changes dialog follow
this path. Cancel retains the draft, Discard has no runtime or file side effect,
and a failed validation/write keeps the editor open. Display-only saves do not
restart. The session closes only after atomic persistence succeeds.

A confirmed core-options save releases capture and requests normal worker stop.
SDL keeps pumping events while the worker stops and joins; no second machine
is constructed concurrently. After a successful stop, the frontend drops old
input/frame handles, clears the old image and prepares the selected configuration
in PAUSED. Ctrl+P boots with its label/model's state directory. Status labels show
the active machine label and state. F2 uses the same validation, persistence and
restart confirmation path; an initial unbooted machine needs no restart.
Stop failures/timeouts prevent replacement; construction/startup failures are
reported with the saved configuration retained for correction in F9.

Advanced stores Newport resolution, CPU cache model, RTC year offset, SCSI
interrupt timing and serial logging independently of the Tinker visibility
flag. Default values match Iris. All five participate in machine restart
detection; hiding Tinker or changing filtering/debug output alone does not restart.
The frontend maps these values into `MachineConfig` before construction. Newport
resolution is an initial mode, not an override of later guest programming.
Clock offsets seed the Indy/Indigo2 RTC from host time on construction/NVRAM
load. O2/Octane do not apply that override or WD33C93A deferred interrupts;
those controls show fixed values and ignore Enter on these models. Their saved
values are retained for switching back. Non-Newport profiles similarly show
fixed Shadow cache and guest-controlled resolution.

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
IP28 was also checked with its bundled PROM and Solid IMPACT at 32 MB, 128 MB
and 1 GiB. Its 128 MB input smoke screenshot shows the R10000/195 MHz CPU,
128 MB memory and IMPACT graphics in `hinv`. O2/Octane have synthetic looping-ROM
startup/media/PS2/shutdown tests and prebuilt-frame tests. The bundled O2 4.18
PROM reaches its diskless boot prompt with R5000 and R10000 at 128 MB. Octane
4.9 reaches the firmware menu at 128 MB after acknowledging its internal and
external SCSI device/cable diagnostic failures. These real-firmware checks use
fresh temporary state and no guest disks; they do not verify IRIX boot.
Visual inspection of a screenshot establishes what PROM actually displayed;
a nonuniform-frame check alone does not identify a PROM menu or an OS boot.

Guest audio has unit coverage for bounded queues, stereo-frame alignment,
closed producers, mute/pause flushing, device-open failures and retry. An
isolated desktop check programs real HAL2 DMA on Indy and IP22, capturing SDL's
output: stereo peaks of 12000/6000 become 6000/3000 at saved 50% volume and zero
while muted. It also checks draft isolation, pause/resume, cold reset, F2 label
switching and clean teardown. IP28 and O2 PROM playback produced non-silent SDL
captures. These checks do not establish sustained IRIX playback quality or
host microphone support; Octane's shared RAD output route still needs an
end-to-end guest playback recording.

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

1. Add device-selection controls and feature boundaries for Iris's existing host backends.
   Validate Linux, Windows and macOS builds and device behavior.
2. Extend General/Media/Advanced and add Extensions and Network controls.
   Classify settings as live, next boot or restart-required. Validate before
   applying and confirm disruptive changes. Add explicit COW management and
   guest-aware media swapping when their core interfaces are ready.
3. Extend audio validation to sustained IRIX playback and host-device changes on each platform.
