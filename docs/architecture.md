# Frontend and Iris integration

[Back to Prismatic](../README.md) · [User guides](../README.md#documentation) · [Testing](testing.md)

## Fetch-and-build workflow

`Makefile` exposes editable `IRIS_REPO` and `IRIS_BRANCH` defaults: the user's
Iris fork over HTTPS and `xiri`. `tools/build.py` also accepts these settings
directly. It creates a separate source cache for each repository/branch pair
under `target/bootstrap`, clones the requested branch on first use and uses
fetch plus fast-forward updates thereafter. Dirty checkouts, changed origins or
branches, and local/divergent commits stop the operation before compilation.
Failed clones stay in temporary directories and are removed. A process lock
serializes helper invocations; the developer's `../iris` remains independent.

Each cache has a generated frontend manifest and a copy of the root Cargo.lock,
with symlinks to the current frontend sources, assets and build inputs. This
preserves the manifest's `../iris` dependency and patched host libraries without
rewriting the checked-in files. The explicit workspace isolates Cargo discovery.
Cargo runs from the real Prismatic directory with an absolute manifest path and
`--locked`; runtime relative paths and screenshots therefore remain in the normal
working directory. Artifacts default to the root `target` directory. The source
revision, selected features/profile and command are recorded after success.
Checks write separate metadata so they do not replace the release build record.

Bootstrap features default to `pcap,idle-pause,jitv2` with no `developer` or
`perfetto`; these can be overridden in Make. `check` runs frontend-only fmt,
Clippy and tests, with test shader output isolated in the managed build directory.
The helper's Python tests exercise local Git clone/update behavior, preservation
of local work, generated inputs, Make overrides and failure propagation.

## Flatpak distribution

`make flatpak` uses `tools/flatpak.py` and the manifest in `packaging/flatpak/`.
It resolves the Makefile's Iris repository/branch (by default our fork's `xiri`)
to an exact commit before Flatpak Builder downloads sources. An explicit frontend
snapshot excludes personal configuration, media and captures. All package
compilation runs offline in the Freedesktop SDK, with checksum-pinned Rust and
Cargo inputs. libchdman builds from its bundled source with a portable CPU baseline.
The frontend/core boundary and native Cargo build remain unchanged.

The Flatpak launcher gives captures a writable per-application location and
redirects Iris's shader corpus into its private cache. Runtime permissions expose
display/audio, network, Downloads and Documents/Prismatic. Sidecar disk overlays
need directory access; portal permission to a single image may be insufficient.
Host PCAP privileges and PTY device namespaces remain sandbox limitations.

Tagged-release CI validates metadata, installs the bundle and runs UI/PROM smoke
checks before uploading artifacts. Published releases also receive the bundle,
checksums and a build provenance record. See [Linux Flatpak](flatpak.md) for the
manifest lifecycle, source pinning and installation instructions.

## Iris compatibility

`build.rs` embeds the linked Iris checkout's branch for the About overlay. It
watches Git HEAD, including linked-worktree metadata, so a branch switch at the
same commit updates the label on rebuild. Detached HEADs include a short commit
ID; absent Git metadata produces an explicit unknown label. This display value
does not control capability detection.

`build.rs` examines the sibling Iris sources for individual public embedding
APIs and emits checked `iris_*` cfgs. It watches each relevant source (or its
parent if a module is absent), so changing branches reruns detection. It does
not inspect branch names: merging performance/embedding work into `xiri`
automatically exposes the new capabilities on the next rebuild. Development
now uses `xiri`; the merged `perf/sgiemu-port` branch is retired. Compatibility
with the fork's `main` is retained. Older arbitrary Iris revisions are not a
supported API contract.

`compat` centralizes capability flags and small adapters. The native audio,
diagnostic and serial implementations are selected only when their APIs exist.
Legacy serial uses Iris's own TCP listeners and console port reporting; no PTY
or Off route is emulated. Legacy diagnostics stay on the console and guest
audio is disabled. Protected CHD disks fail validation without the COW API.
Older Iris needs external O2/Octane PROM files; the bundled choice is disabled.
A safe startup re-exec sets Iris's documented power-off environment override
when the embedding setter is absent. Such cores report guest shutdown as halted
rather than the fork's distinct powered-off state.

Overlay row availability is shared by rendering and keyboard dispatch. Disabled
rows stay selectable for their explanation but ignore Enter/Backspace. Numeric
editors and runtime actions cannot be opened through those rows. Saved settings
are not rewritten to defaults: expert/device settings have a separate runtime
projection. The frontend owns its saved CPU enum, so unavailable newer CPUs
round-trip across builds; boot refuses them and General cycles supported choices.
Safe paused checkpoints, owned remote services and per-machine affinity/JIT cache
management require their corresponding lifecycle APIs. Live network changes
fall back to restart confirmation on cores without persistent runtime overrides.

## Performance configuration

The SGIEmu work is integrated into Iris's `xiri` branch. The per-label
`ExpertSettings.sgiemu_optimizations` preference defaults to false and maps to
`MachineConfig.perf.sgiemu_optimizations` when the core exposes that field.
`build.rs` detects this API from source; unsupported cores retain the preference
but disable its Performance row. Their existing optimization policy is unchanged.

The toggle controls Cranelift SSA copies of guest integer registers and cached
DS1x86 calendar conversion. The register cache keeps architectural state current
and excludes FPU/fallback regions, developer and lockstep builds. O2 and Octane
use different RTCs. JIT compilation, persistent compiled-page caching, host
affinity and idle parking remain separate choices. Correctness fixes always apply.

Save uses the existing machine-difference and restart confirmation flow; Cancel
or Discard does not change the live machine. The worker constructs a replacement
with the selected policy, including all SMP CPUs. Iris keeps policy per JIT pool,
preserves it through worker/arena resets and includes it in persistent code keys.
RTC snapshots preserve guest state without overriding the host's policy.

Build with `pcap,idle-pause,perfetto,jitv2` using `nightly-2026-10-28`.
The interpreter build can use the RTC cache; the register cache needs `jitv2`.
Avoid `developer` when measuring the register optimization. PROM smoke checks
establish startup and restart behavior, not an OS boot speedup.

The former same-process JIT restart panic was traced to a global hot compilation
queue. Iris now gives each machine's JIT pool its own queue, so an old worker
cannot compile the replacement machine's request against old memory. This fix
remains active with SGIEmu optimizations either on or off.

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
| `build.rs`, `src/compat.rs` | Source API detection, compatibility adapters and runtime capability report |
| `src/main.rs` | CLI, SDL lifecycle, events, texture uploads, asynchronous shutdown, smoke runs |
| `src/emulator.rs` | Validated Iris configuration and media attachment, worker ownership, bundled firmware, frame capture, latest-frame slot, machine status |
| `src/input.rs` | Host/modal key routing, physical scancode mapping, held guest input, PS/2 delivery |
| `src/smoke.rs` | Scripted SDL input and General/Media/Advanced menu and restart smoke tests |
| `src/overlay.rs` | Draft settings, navigation, modal choices and effects |
| `src/expert.rs` | Advanced topics, validated performance/connection/developer settings, owned GDB/VNC and boot tracing |
| `src/serial.rs` | Per-channel TCP/PTY/Off routing, bounded nonblocking PTYs, actual endpoint reports and explicit transport cleanup |
| `src/settings.rs` | Desktop/hardware/media/Advanced settings, compatibility, RAM banks, state paths and atomic persistence |
| `src/profiles.rs` | Named machine stanzas, draft selection, state identity and concurrent catalog merging |
| `src/instance.rs` | Exclusive leases for machine state, writable disks and atomic configuration updates |
| `src/diagnostics.rs` | Live Off/Screen/Console routing of Iris diagnostics, bounded screen text and redraw updates |
| `src/capture.rs` | Asynchronous PPM screenshots, bounded GIF recording, atomic publication and optional FFmpeg optimization |
| `src/sound.rs` | Embedded Happy Years screenshot shutter through a main-thread SDL audio stream |
| `src/audio.rs` | Bounded per-machine guest PCM handoff and main-thread SDL3 playback |
| `src/network.rs` | Per-machine network settings, validation, Iris configuration mapping, Network rows and hints |
| `src/display.rs` | Packed RGBA frame type, startup/UI-preview pixels and aspect-preserving placement |
| `src/view.rs` | SDL drawing, status strip, LEDs, options, confirmations and notifications |
| `src/media.rs` | Native file/folder choosers, background validation and interface enumeration, media settings and Iris block-node adapters |
| `src/disk_image.rs` | Cancellable blank raw/CHD creation, capacity validation and publication without overwriting existing images |
| `tools/native.py`, `packaging/native/` | Revision-pinned native package builds, runtime bundling, platform metadata and installed-package smoke checks |
| `src/storage.rs` | CD tray actions, offline COW commit/reset, scratch drive and payload transfers, bounded action results |
| `src/activity.rs` | Atomic activity bits and visible LED pulse durations |

## Core boundary

The Odyssey integration incorporates Iris's existing `xiri` engine. Octane's
selector uses the IMPACT and VPro enum variants shared by all three supported
cores, without requiring the newer `GraphicsBoard::OCTANE` convenience constant. The existing `Workstation`
display/renderer interface carries Odyssey frames without a special frontend
rasterizer. Settings reject VPro on other profiles and reject a second head;
per-label persistence and restart confirmation use the existing machinery.
The core currently models one 1280x1024 Odyssey scanout and refuses VPro
snapshots. Hardware mode limits, DCD and physical-board validation are outside
this frontend integration.

The root Cargo manifest depends directly on `../iris`, developed on `xiri`,
preserving that fork's winit and block patches at the root where Cargo applies
them. It retains NetBSD and other guest OS compatibility work alongside IRIX. The nightly
Rust toolchain matches Iris. No emulator source is copied into this repository.
The default is Indy IP24, R4400, 64 MB RAM, one Newport head, and no SCSI
devices. General selects all five workstation profiles: Indy IP24, Indigo2 IP22,
Indigo2 IMPACT IP28, O2 IP32 and Octane IP30. Indy supports R4400/R5000, IP22
R4400, O2 R5000/R10000, IP28 R10000, and Octane R10000/R12000/R14000.
Octane supports one or two identical CPUs. Iris owns their identification,
cache geometry and default Count rates (97.5/150/300 MHz); Prismatic persists
the selected CPU and rejects the new models on other machines. RAM presets
fill up to four banks, largest first, with per-bank limits of 128 MB on Indy/IP22,
512 MB on IP28, 2048 MB on Octane, and 256 MB on O2. Total limits are 256 MB, 512 MB, 1 GiB,
1 GiB and 8 GiB respectively. Frontend validation checks menu choices and the
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

`MachineSettings.prom_boot` is saved per label and defaults to `keep_existing`
for old configurations. General cycles Keep existing, IRIX (`sash`, `/unix`,
`auto`) and NetBSD (`boot`, `netbsd.boot`, `auto`). It participates in the normal
machine-change restart comparison and checkpoint compatibility metadata.
After taking the machine-state lease, the worker calls Iris's
`prom_env::prepare_environment` before constructing the workstation.
The core updates the selected OSLoader, OSLoadFilename and OSLoadOptions in
the model's persistent storage: Indy DS1386 NVRAM, Indigo2 EEPROM, O2 environment
flash segment or Octane persistent flash records. Validation and draft edits do
not write firmware state. Existing non-boot settings survive; invalid formats,
checksums, insufficient space and I/O failures report `Phase::Failed`. Presets
are reapplied on cold boots, while Keep existing preserves the current values.

`MachineSettings.prom_console` independently defaults to `keep_existing` and
cycles through Graphics (`console=g`) and Serial (`console=d`) in General.
It shares the label persistence, restart confirmation, draft isolation and
checkpoint compatibility rules. Indy/Indigo2 store console in their fixed
environment; O2 stores it in its flash environment. Octane stores console in
DS1685 NVRAM, separate from the flash records holding boot variables. Iris
validates all affected images before writing and atomically replaces each file;
if a later write fails, it attempts to restore earlier files and reports any
restoration failure. The two Octane files are not a crash-atomic transaction.
Serial mode uses the selected TCP or PTY endpoint (B on Indy/Indigo2, A on
O2/Octane). It neither opens a terminal inside SDL nor changes routing preferences.

Extensions selects graphics boards, Newport head count and Octane CPU count.
`MachineSettings.graphics` is optional so old files retain Newport on Indy/IP22,
Solid IMPACT on IP28/Octane, and integrated GBE on O2. Indy offers Newport/XZ;
IP22 offers all six graphical boards; IP28/Octane offer the three IMPACT boards.
O2 has no board override. Non-Newport graphics pass guest resolution and Shadow
cache; saved Newport settings remain available when selecting Newport again.
Model/board changes reconcile incompatible graphics, head count and CPU count.
Validation rejects unsupported hand-edited configurations. These fields are in
the per-label hardware stanzas and follow the existing draft/save/restart flow.
`CaptureRenderer` uses Iris's `prebuilt` compositor path for GR2/IMPACT/GBE and the
existing Newport software compositor otherwise. Frame packing, readback, bounded
latest-frame delivery, capture and SDL rendering are shared. The worker sets the
common PS/2 layout before starting any CPU, including O2/Octane constructors that
do not yet apply `MachineConfig.keyboard` themselves.

Media attaches protected raw/CHD disks and read-only CDs to the common `ScsiBus`
on each supported controller: WD33C93A on Indy/Indigo2, AIC7880 on O2, ISP1040
on Octane. IP22/O2/Octane expose two buses; Indy/IP28 expose one in this core.
The activity/error adapter is retained on all models. CPU/device threads stop
before targets are detached and flushed; no frontend constructor uses Iris's
exit-on-media-error default attachment path.

Media's **New disk** form creates unformatted raw/CHD hard disks in a separate
worker. It uses a bounded result channel, atomic progress/cancellation and a
joined worker lifetime. A private temporary file in the destination directory
is inspected read-only and flushed before publication without replacement;
failed or cancelled creation removes the temporary file. Existing base images
and stale overlay sidecars are rejected. Raw images use sparse allocation;
libchdman's streaming hard-disk writer creates compressed CHDs with geometry
and hashes in a helper process. Cancellation kills and reaps that process
before deleting its temporary image, since the library's cancellation API
otherwise drains the entire compression job. The helper never opens SDL or
preferences; progress is bounded to one percentage value. CHD availability
follows the detected Iris storage API. Creation
is an explicit host-file action; attachment changes only the draft and follows
normal Save/restart validation. Discard keeps already-created files.

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

## Networking

The Tinker-gated Network tab edits per-machine NAT/PCAP/Off mode, subnet,
NFS folder/version/bridge IP, TFTP/RSH folders, MAC and TCP/UDP forwarding with
localhost or all-interface bind scope.
Settings live under `[network]` and each named machine's `.network` stanza.
Old files default to NAT with no shares or forwards. Hiding Tinker does not
disable saved networking. Changing from O2/Octane with Ethernet Off to an
Indy/Indigo2 selects NAT, matching the core's model restriction.

The frontend maps saved settings into `MachineConfig` before worker startup.
Backend, MAC, service folders and NFS options require restart confirmation.
Subnet, forwarding rules/bind scope and PCAP interface use the worker's published
`Arc<NatControl>`. The frontend validates and persists the complete draft before
requesting live changes; failed saves and Discard do not touch the backend.
Only changed controls are sent, so editing forwards does not reset the subnet
and its connections. These are asynchronous core requests; listener errors use
Iris diagnostics and interface errors use the PCAP status notification. Existing
Indy/Indigo2 NVRAM can retain its old MAC even after a configured override.
The fork retains explicit live network choices across peripheral restarts and
closes old listeners before rebinding or starting a renewed link.
CIDR, IP and forwarding checks are pure; folder checks and PCAP enumeration
use the bounded background job result channel. Selected service directories
must exist and be readable: validation never creates or exports a directory.
Paths resolve relative to the config file and normalize before catalog merging.
The worker checks folders again before constructing the core. NFS has Iris's
read/write semantics; TFTP and RSH are read-only. RSH exposes Iris's limited file
commands, without a host shell. No host service is opened by a draft.

The optional Cargo `pcap` feature forwards to `iris/pcap`. A non-PCAP build can
read a catalog containing PCAP labels, but refuses to save/start an active
PCAP configuration rather than accepting Iris's fallback to NAT. Interface
enumeration runs on a worker; SDL folder dialogs stay on the main thread.
The machine worker reports Iris's `PcapStatus`; capture failures produce a
notification independently of debug logging. PCAP NFS requires an explicit
virtual LAN IP; NAT-only services remain saved but are not passed to the PCAP
backend. Prismatic never changes host capture privileges.

## Machine lifecycle

A worker with a 64 MB stack calls `iris::workstation::build(MachineConfig)`.
It owns the boxed `Workstation`, registers Iris's system/event controller and
installs the capture renderer before starting peripherals. It starts the CPU
explicitly when needed because Iris debug builds otherwise leave it parked.
Calls that access a running machine are serialized with Iris's `MachineSlot`
lock. The lock is released before dropping the machine: Iris's destructor
acquires it to invalidate the pointer held by its controller.

Shared status reports Starting, Running, Paused, Halted, PoweredOff, Stopping,
Stopped or Failed, along with the monitor address and PCAP backend status.
Running follows the core's CPU-running flag. Monitor and serial TCP endpoints
default to ephemeral loopback ports. Connection status reports actual endpoints,
including Unix PTY paths. NVRAM lives in Prismatic's machine-state directory, separate from
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

A mutex-protected slot holds the latest `Arc<Frame>` for each of at most two
heads and a dirty flag. The producer replaces the previous frame, so a slow UI
never builds a video queue. The worker installs a separate `CaptureRenderer`
on `get_display()` and, when requested, `get_display_head1()` before starting
Iris. The SDL thread snapshots the frame references and releases the lock.
For two heads it combines them side by side at native resolution, padding a
shorter head with black; before both publish, the absent head reserves an area
matching the other. It recreates the SDL texture when dimensions change and
uploads the result. Screenshots/GIFs use the same combined guest frame. Input
still reaches the shared PS/2 controller through relative mouse motion. Heartbeat-only frames are skipped unless
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

Media uses `controller * 16 + target` keys, preserving legacy keys 1–7. It maps
both buses where supported and IDs 1–15 on Octane (1–7 elsewhere) to explicit
disk/CD types and paths. Core placement validation and frontend profile checks
agree. A per-session bus selector changes the visible rows without editing the
draft. Unsupported assignments remain reachable after a model change, and Save
rejects them instead of dropping devices. Duplicate writable images across buses
are rejected before any writable node opens.
Indy placement conventions use target 1 for the system disk and target 6 for
installation/recovery CDs; target 0 belongs to the host controller. All Indy
devices share one bus, including internal and external devices. These placement
conventions guide the frontend without reserving other targets or implying
support for historical tape/floptical devices.
Media row labels show the Indy-style role for primary-bus IDs 1–7; additional
buses and wider-bus targets show General use. Selection hints explain boot,
installation, historical device roles and the reserved controller ID 0. These
annotations do not change row indices, media types, validation or configuration.
Native SDL file-dialog callbacks hand results to a bounded channel; file probes
run on a separate worker. Escape abandons the result without changing the draft.
Save revalidates all images off the SDL thread, canonicalizes paths, rejects
duplicate hard disks and then follows the hardware restart confirmation flow.
Validation is read-only and never creates overlays. Writable backends open when
constructing a machine or executing an explicitly confirmed offline COW action.

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
or `ChdCdNode`. No filesystem decoding lives in the frontend. The checkpoint adapter supplies explicit disk-reference provenance after this
fallible attachment path; the empty constructor configuration is never used as
evidence of disk identity.

`MediaNode` delegates the disk block interface while recording actual reads/writes
and flushing on drop. CD and scratch targets use `ActivityTarget`, which observes
successful SCSI reads/writes outside the block node: CD changer replacement
therefore retains LED instrumentation. After CPU/device stop, the worker flushes
all targets, detaches them, checks retained errors and only then reports completion.
Iris retains its persistent COW sidecar behavior. Draft selection/discard never
opens a writable node. The probe checks format/readability; a later permission
or device failure is still reported by machine startup or shutdown.

Saved `media.cd_changers` lists accompany the boot CD on either SCSI bus.
Editing them follows normal draft validation/restart rules. Explicit live
insert/next/empty/status actions are serialized on the machine worker and select
the correct bus's synchronized target table. Insert probes before replacement;
Next requires two queued discs. The frontend does not unmount guest filesystems
or rewrite saved boot media after a live action. No guest-specific shell commands
are injected.

Extensions separates settings from immediate actions. Dirty drafts block live
actions and disk commit/reset. Confirmations default to Cancel. A single bounded
result channel returns a scrollable report while SDL continues handling events;
quit and additional actions wait for completion. Offline COW actions use a worker
and exclusive lease for only the selected disk, and require no loaded machine.
They call the existing backend commit/rollback and flush methods. Hashless CHD
commit stays unsupported, preserving the container; reset is supported.

The optional scratch drive has per-label target/size settings and a persistent
`scratch-SIZE.raw` in its state directory. Startup atomically creates the SGI
volume header/payload partition if absent, validates size and partition bounds,
and holds an exclusive disk lease through teardown. The target is directly
writable with no overlay. Import/export use `Workstation::with_paused` to stop
device I/O around bounded streaming file copies and restore CPU running state.
If Ctrl+P already paused the machine, transfers preserve its peripheral state.
Import validates the header and payload capacity, writes from byte 4096 and pads
the last partial sector; trailing data remains. Export publishes a complete
payload through a temporary file without overwriting existing destinations.
Guest filesystem handling and meaningful payload length remain the user's
responsibility. Scratch media participates in the normal shutdown flush wait.

`CaptureRenderer` records each CPU's cycle changes (`cycles` and `more_cpus`
from Iris's `BarStats`) and core network heartbeat bits. CPU0 and CPU1 use
distinct bits and independent pulse timers; the strip renders one white CPU LED
per configured processor.
Storage adapters publish distinct read/write bits for all 30 possible targets
in a 64-bit atomic word. Dense configurations group lamps by bus and media kind
to fit the strip; each group combines its members' activity. The main thread consumes
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
Hit testing converts window points through drawable window pixels to the letterboxed
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

Advanced has five visible topic sub-tabs: Core, Performance, Connections, Capture
and Developer. Core keeps its original machine and display controls; Capture owns
the shared GIF preferences. `Session.advanced_page` is presentation state, reset
when the overlay opens. Tab/Shift+Tab wraps between topics and Left/Right switches
main sections. Clicking a sub-tab uses the same layout and scaling as rendering.
Input mapping uses the window's drawable pixel size, preserving letterbox offsets
on resized and high-DPI windows.
Topic changes reset row selection without touching drafts or runtime state;
editors, dialogs, reports and pending work block both keyboard and mouse switching.
Numeric and path fields use the existing transactional text editor. `AdvancedSettings.expert`
is serialized under `[advanced.expert]` and copied into named machine stanzas.
Structural validation runs on load/save; capability, host affinity and ELF-file
checks run on the validation worker before persistence. Relative ELF paths resolve
against the config directory for both the current and inactive labels.

`expert::apply` maps Count MHz, thread affinity, JIT worker count and monitor/serial
ports into `MachineConfig`. Count MHz uses integer kHz internally so draft equality
is exact. Runtime setup happens only on the machine worker: probe fixed ports,
initialize affinity for every workstation, set idle parking through Iris's atomic
API, and configure the optional persistent JIT cache through its safe library API.
No environment variables are mutated after threads start. JIT cache destinations
are per-label; Iris's bounded cache writer captures each job's destination before
a later label can change the configuration. Feature-disabled controls remain
visible with capability hints. All saved expert settings participate in normal
restart detection, including while Tinker is hidden.

The worker loads an optional ELF before starting the CPU on supported Indy/Indigo2
models. It starts optional Perfetto recording before the first instruction and
remote services after peripherals start. `expert::Services` owns GDB and VNC
handles. Both disconnect clients and join their workers before the workstation is
stopped/dropped, including failures and unwinding. GDB binds synchronously and
exposes its actual loopback address; VNC uses Iris's primary-display `HostInput`.
Monitor, GDB and VNC endpoints and trace status are published through the existing
status mutex; the SDL thread only reads snapshots. `serial::Endpoints` creates
and holds serial transports before workstation construction, reports actual TCP
ports and PTY paths, and installs them through `MachineConfig.serial_backends`.
The core keeps log tees and CI fanout around supplied backends. Standalone Iris
continues using its configured TCP/Unix endpoints when no override is present.

Per-channel `serial_a`/`serial_b` routing defaults to `tcp` for old configurations;
`pty` (Unix only) and `off` preserve their inactive TCP port preferences. Only
active TCP ports participate in duplicate-port validation. Both transports must
open successfully before either is installed, and failure drops the first.
PTYs use safe nix APIs, raw slave settings and nonblocking, close-on-exec master
descriptors. A held slave descriptor permits reconnecting without EIO. UART
callbacks flush a 64 KiB bounded output queue; overflow is counted and displayed,
never allowed to block the UI or CPU. Kernel buffers provide input back-pressure.
Each boot allocates new PTYs, without persistent symlinks or saved device paths.
Pause retains their handles. A revocable transport wrapper explicitly closes
the underlying endpoint on unload or failed startup, even if Iris retains a
device/backend reference. Old callbacks then discard output and return WouldBlock
on input; clients on an old PTY receive hangup.

Focused checks cover raw bidirectional bytes, reconnect/hangup, bounded output,
automatic TCP ports, occupied-port failures, per-label persistence and draft
isolation. Live SDL checks boot all five profiles to serial PROM menus over PTYs,
verify console environment values and pause/quit cleanup, and exercise Indy with
minicom. The interpreter also passes a live PTY-to-fixed-TCP switch through the
restart confirmation flow. The integrated JIT core also passes same-process restarts; the former global
compilation-queue failure is described above.

Each boot trace gets a unique reserved file under the label's `traces/` directory;
it never truncates a previous capture. Stop flushes the recording and retains the
file. Cleanup also closes monitor-started recordings and tolerates a trace already
stopped through the monitor. Tracing uses bounded Iris event rings. Remote ports are loopback-only;
GDB/VNC default off. Fixed-port preflight failures, GDB/VNC bind failures, monitor bind failures and
invalid ELF files produce recoverable startup failures. Serial TCP endpoints bind
once and remain reserved through construction, so failures reach `Phase::Failed`
before firmware writes or CPU startup. The optional Cargo features forward
`idle-pause`, `jitv2`, `perfetto` and `developer` to the integrated `xiri` core.

Serial logging uses Iris's console sink (B on IP2x, A on IP30/IP32) and raw or wall-clock timestamps.
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

Network tests cover config migration, per-label isolation, folder normalization
and save conflicts, invalid CIDRs/IPs/forwards, missing directories, draft
discard and cancelled restarts, plus configuration mapping on all five models.
A packet-level test runs the real Iris NAT backend with frontend settings and
checks TFTP file contents and NFSv3 mount/lookup against separate temporary
folders on a custom subnet. A live-control test verifies forwarding listeners
appear only after successful persistence and close on a live update, without
recreating the backend. Both default and PCAP feature builds are checked.
An isolated desktop test exercises text editors, missing-folder rejection,
Save/Discard, restart confirmation, F2 switching, PCAP interface enumeration
and a missing-interface notification. It confirms that localhost forwarding
starts only after boot and stops with the old machine. Sustained IRIX network
use and bridging traffic onto a physical LAN remain separate validation work.

Extensions tests cover legacy defaults, per-label round trips, incompatible
CPU/board/head/media settings, all supported model/board/RAM mappings, Save
failure without file replacement, and controller selection/disconnection after
model changes. Frame tests cover independent head updates, different sizes,
black padding and latest-frame replacement. A real dual-Newport lifecycle test
seeds both video modes (the diskless PROM may leave head 1 disabled) and checks
both refresh threads publish before clean shutdown. CPU counter tests verify
independent activity and pulse expiry, with no overlap among two CPUs, network,
and all 30 SCSI targets.

Isolated SDL desktop checks exercise Extensions Save/Discard/restart cancellation,
F2 label isolation, dual-head native PPM/GIF dimensions, and two-CPU Octane PROM
startup. Pausing each Octane CPU separately confirms that only its own LED goes
idle; a 30-target fixture checks grouped storage lamps without hiding CPU LEDs. Diskless graphics checks include Indy XZ, IP22 Extreme/High IMPACT,
IP28 Maximum IMPACT and Octane Maximum IMPACT. Monitor inspection verifies
second-bus media on IP22/O2/Octane and target 15 on Octane, using disposable
read-only fixtures. These checks establish frontend integration and PROM output;
full guest-OS boots with each alternate board, dual-head guest desktops and
8 GiB Octane configurations remain separate validation work.

Storage tests cover saved changer/scratch migration and collisions, cancellation
and draft isolation, raw COW commit/reset and instance exclusion, failed CD loads,
post-swap LED activity, scratch header preservation, oversized imports and export
overwrite refusal. Real worker tests on IP22/O2/Octane swap CDs on the second bus
and import scratch payload while preserving a paused machine. These tests use
disposable fixtures and do not establish guest filesystem remount behavior.

An isolated SDL desktop check exercises offline Commit confirmation/cancellation,
scratch creation, CD Status/Next/Empty without replacing the worker or saved boot
media, and refusal of disk Commit while paused. Network checks cover live subnet,
forward rules and bind scope, listener retention across pause/resume, and cancelled
MAC restarts. Final UI, diskless PROM, Media and Advanced smokes and the optional
PCAP release build use the same `xiri` revision. Two-CPU Octane LED checks also
pass against the synced core's status source.

Step 3 checks cover the default and optional `pcap,idle-pause,jitv2,perfetto,developer`
builds. Real worker tests boot a static MIPS ELF, exchange GDB RSP, disconnect a
partial VNC handshake and reuse its ports, and flush unique boot traces even after
a monitor trace stop. An isolated SDL desktop check exercises all four Advanced
pages, draft discard, paused Save, restart cancellation, runtime reports, a full
VNC framebuffer, reset teardown and monitor trace stop. These establish host integration; remote clients on Windows/macOS and
OS-specific effects of nondefault clock/affinity/JIT settings need separate testing.

## Optional devices and checkpoints

Extensions has Hardware, Storage, Devices and Checkpoints topic sub-tabs.
Hardware contains graphics, display heads and CPU count; Storage contains scratch,
CD changer and disk actions. `Session.extensions_page` uses the same presentation
and navigation rules as Advanced. Device settings live in `AdvancedSettings.extensions`, so named
machine stanzas, transactional saves and restart comparison include them.
Validation runs without opening cameras, NAT engines or shared memory. It rejects
unsupported profiles, occupied DaynaPort SCSI slots, invalid MACs and overlapping
DaynaPort/onboard subnets. DaynaPort attaches fallibly after normal media, through
HPC3's native backend; it inherits network services at boot, so any Network edit
requires restart while it is configured. Media identifies its reserved target.

The `xiri` core starts/stops VINO DMA with machine peripherals. Drop clears the
VINO source and physical-memory back references, releasing the camera and breaking
its ownership cycle. Linux capture uses a bounded V4L2 dequeue timeout and
interruptible hotplug backoff. A paused machine retains its host capture source;
pre-boot PAUSED has no machine or camera. Ultra64 holds a host-wide file lease
before touching its fixed-name shared-memory region; the frontend checks actual
board creation and reports initialization failure. A weak bus adapter prevents
retained physical-memory graphs from keeping an old bridge alive; event objects
are destroyed before their shared-memory mapping, and the lease is released last. No external N64 emulator is
launched. These devices use unconditional native backends, not retired features.

`checkpoints.rs` owns a deliberately narrow capability gate: Indy/IP22, single
Newport head, raw COW disks or diskless, no CD/scratch/optional device, GDB or
active trace. It sends actions through the existing bounded machine-worker queue
and reports results through a bounded reply. UI rendering/input never performs
snapshot I/O. Actions require saved options, and Save/Restore require a default-
Cancel confirmation. Restore leaves CPU and peripherals stopped; the core's
paused load no longer briefly restarts peripherals after reading state.

Each bundle has a private native snapshot root containing `state/` and `.cas/`.
Save writes a temporary sibling directory and publishes it by rename only after
all state and provenance are present, without overwriting an existing checkpoint.
The manifest records serialized hardware/media/Advanced/Network configuration,
the executable's BLAKE3 hash, full base-disk hashes and an inventory of all state
files. Symlinks and nonregular files are rejected. Restore verifies those before
calling destructive native load, independently of Iris's diagnostic provenance
bypass. The adapter supplies externally attached `DiskRef`s and restores the
monitor's separate snapshot root after each operation. A failed native load ends
the worker; a failed preflight leaves the guest intact. Save resumes a previously
running guest even if saving fails. Live Network edits are included in checkpoint
provenance. Checkpoints neither roll back external NFS files nor reconstruct host
network connections or HAL2 transient audio state.

The tests exercise optional-device validation and label persistence, default-
Cancel action dialogs, VINO ownership release, DaynaPort attachment, native CPU
and raw COW overlay restoration, paused state, overwrite refusal and rejection of
modified bundles/base images. Host camera hardware, an external gopher64 peer,
and guest-specific DaynaPort drivers still require end-to-end testing, as do
Windows/macOS host backends.

Validation includes 136 default and 137 optional-feature frontend tests, strict
Clippy, dummy UI/PROM/Media/Advanced smokes and an isolated Xvfb desktop exercising
all Extensions pages, device status, draft discard, paused restart, checkpoint
confirmation/save/restore and explicit resume. Four focused Iris regressions cover
VINO shutdown and status locking, cancellable Linux camera retry, Ultra64 lease
exclusion, and repeated private-bridge start/stop/drop/reopen. The private IPC test
does not touch an active user bridge.

## Next milestones

1. Validate Windows/macOS builds, host camera hardware and sustained guest audio.
2. Exercise DaynaPort guest drivers and the external Ultra64/gopher64 bridge.
3. Extend native snapshot coverage before enabling more profiles/devices: CHD
   overlays, second-head framebuffers, GR2/IMPACT and transient audio state.
