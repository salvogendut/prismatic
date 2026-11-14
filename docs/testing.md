# Testing and smoke checks

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Checks](#checks)
- [Smoke test behavior](#smoke-test-behavior)
- [Model coverage](#model-coverage)
- [Input and menu checks](#input-and-menu-checks)
- [State isolation and test coverage](#state-isolation-and-test-coverage)
- [Installation helper checks](#installation-helper-checks)

## Checks

Run these commands from the repository root. See [building](building.md) for
toolchain requirements and [Iris feature compatibility](building.md#iris-compatibility).

For the managed Iris checkout selected in the Makefile:

```sh
make test-bootstrap
make check FEATURES=
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software make run ARGS=--smoke-test
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software make run ARGS='--startup-smoke-test --screenshot /tmp/prismatic-startup.bmp'
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software make run ARGS='--boot-smoke-test --screenshot /tmp/prismatic-prom.bmp'
```

Repository, branch and feature overrides also apply to these targets. Use
`make check` to check the selected optional features as well. For direct Cargo
development against your maintained sibling `../iris` checkout:

```sh
cargo fmt -p prismatic --check
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo test --locked
# Omit perfetto on older Iris main revisions with the Odyssey compile issue:
cargo test --locked --features pcap,idle-pause,jitv2
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --smoke-test
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --boot-smoke-test --screenshot /tmp/prismatic-prom.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --input-smoke-test --screenshot /tmp/prismatic-input.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --settings-smoke-test --screenshot /tmp/prismatic-settings.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --media-smoke-test --screenshot /tmp/prismatic-media.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --advanced-smoke-test --screenshot /tmp/prismatic-advanced.bmp
```

On cores with working Perfetto, also run
`cargo test --locked --features pcap,idle-pause,jitv2,perfetto,developer` for
tracing/developer coverage. Keep `developer` off in performance builds.

## Smoke test behavior

`--smoke-test` renders three UI frames with the options open and exits without
starting Iris. `--startup-smoke-test` renders the initial paused screen with its
embedded logo and boot instructions, using the same temporary-state safeguards.
Both accept `--screenshot` for visual inspection. `--boot-smoke-test` uses fresh
temporary NVRAM, waits for a nonuniform guest framebuffer, renders for ten more seconds, optionally captures
the SDL output, and stops Iris. It fails if startup, display output (120-second
timeout), or shutdown fails. This checks the display path; it does not identify
PROM text or prove that a complete OS boot works.

`cargo test --locked` also checks named-label persistence, concurrent catalog
saves, instance locks, draft isolation and pause/reset/resume. The IP22 power-off
regression writes its panel register separately from a monitor halt, using
temporary state and no guest disks. Advanced tests also cover per-label
expert settings, invalid draft edits, port collisions, real GDB protocol exchange,
VNC teardown during a partial handshake, immediate port reuse and direct ELF boot.
The optional-feature suite checks that each boot trace is distinct and flushed.

## Model coverage

For a diskless IP28 check, create a separate configuration and use the same
smoke path (the input script also works with its PROM):

```sh
cat > /tmp/prismatic-ip28.cfg <<'EOF'
[machine]
profile = "indigo2_ip28"
cpu = "r10000"
ram_mb = 128
EOF
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --release --locked -- --config /tmp/prismatic-ip28.cfg --input-smoke-test --screenshot /tmp/prismatic-ip28.bmp
```

IP28 has passed PROM display checks at 32 MB, 128 MB and 1 GiB, with keyboard,
mouse and hardware inventory checked at 128 MB. Bundled O2 4.18 reaches the
diskless boot prompt with both R5000 and R10000 at 128 MB. Bundled Octane 4.9
reaches the PROM menu at 128 MB: its diskless POST reports internal/external
SCSI device/cable diagnostic failures, and Enter continues to the menu.
O2/Octane also have synthetic-ROM startup, external-override, media attachment
and shutdown coverage. These checks do not establish
IRIX boot compatibility across all models or RAM configurations.

## Input and menu checks

`--input-smoke-test` also boots with fresh NVRAM. It queues SDL events to capture
input, dismiss the boot error, move the pointer, enter the command monitor and
type `hinv`. It checks release/recapture through Ctrl+Enter, F9 and focus loss,
then captures the final display and stops. Inspect the screenshot for the
hardware listing: the test checks routing and capture state but does not read
PROM text. Omit `SDL_VIDEODRIVER` and `SDL_RENDER_DRIVER` to exercise the native
desktop window; leave it focused while the scripted test runs.

`--settings-smoke-test` exercises the actual General menu: change RAM, cancel
restart, discard the draft, then save and confirm a change from a 64 MB Indy to
a 128 MB Indigo2. After restart it runs the input smoke script and checks saved
preferences. Inspect its screenshot for `Memory size: 128 Mbytes` and
`Graphics: XL`. It has a 240-second overall timeout and rejects `--config`
because it automatically saves preferences in its temporary directory.

`--media-smoke-test` selects temporary raw/CHD disks and raw/CHD CDs through
Media, saves `prismatic.cfg`, confirms restart and runs the input script.
Inspect `hinv` for disks 1/2 and CD-ROMs 4/5/6. It substitutes known temporary
paths for the OS file chooser, but exercises normal asynchronous validation,
attachment and shutdown. These are synthetic sector fixtures, not bootable
IRIX installation media; this does not establish a full OS installation.

`--advanced-smoke-test` enables Tinker and edits all five core options. It
cancels restart and discards once, then hides Tinker and confirms the changes.
After restarting with 1024×768 initial resolution, Shadow cache, −5 years,
immediate SCSI interrupts and timestamped logging, it runs the input script,
checks saved preferences and log creation, and reopens Advanced for the
screenshot. This test uses temporary state and a 240-second total timeout;
it checks integration and PROM operation, not guest OS compatibility for
every combination of expert options.

## State isolation and test coverage

The machine smoke commands redirect Iris's draw-shape cache to `/tmp` as well.
All tests use temporary preferences unless `--config` is supplied. General,
Media and Advanced menu tests automatically save options and reject `--config`.
Missing configuration files are initialized on startup. Machine smoke tests always use
temporary NVRAM and reject `--state-dir`; diskless tests reject configurations
with attached media. A screenshot path is accepted only with a smoke test.
Unit tests cover draft settings, persistence and failures,
hardware compatibility/bank mapping, state-directory isolation, confirmation
behavior, framebuffer packing/stride, frame replacement, startup failure,
physical keys, host/modal routing, PS/2 releases, mouse input, high-DPI hit
testing, configuration migration, COW persistence, CD read-only access,
invalid media, flush failures, LED pulses, Advanced option mapping, restart
detection and retained settings when Tinker is hidden. Storage/network tests cover
CD changer actions on both buses, COW Commit/Reset, scratch import/export bounds,
paused transfers and live forwarding after successful Save. Linux is validated;
other platforms still need integration checks.

Topic navigation tests cover forward/backward wrapping, mouse selection,
draft retention and modal/busy guards. For a desktop check, enable Tinker and
visit every Advanced and Extensions sub-tab using clicks and Tab/Shift+Tab.
Change Capture and Storage options, switch topics, and exercise Save and Discard.
Repeat mouse selection at 75% scale and with a wide, letterboxed window to verify
that the visible sub-tabs and their click targets remain aligned.

Disk-creation tests check zero sectors, exact raw/CHD capacity and geometry,
CHD hashes, guest writes through Iris's overlays, invalid sizes/extensions,
existing files and stale sidecars, cancellation cleanup and draft isolation.
For a desktop check, use Media > a SCSI target > New disk, create a small raw
image and a CHD, then Save and reopen Media. Repeat with an existing destination,
cancel a large CHD while it is being created, and Discard an attachment after
creation: the completed host image must remain and live devices must not change.

Related: [building](building.md), [integration architecture](architecture.md).

## Installation helper checks

The normal suite checks stream endpoint validation, per-label persistence,
draft isolation, disabled controls, real Iris frame transport, helper error
reporting/cancellation and bounded diagnostic output. It needs no container
runtime or IRIX media.

Use the bundled helper from `make`, or build it alone with `make install-server`.
These ignored tests use synthetic files, then exercise BOOTP and TFTP over Unix
and loopback TCP through Iris's actual stream backend and the managed server
lifecycle:

```sh
PRISMATIC_INSTALL_HELPER="$PWD/target/release/instigator" cargo test --locked native_instigator_answers -- --ignored --nocapture
cargo test --locked podman_instigator_answers -- --ignored --nocapture
```

For the standalone build, use `target/install-server/instigator` instead.
Native package CI runs these tests on each host; Flatpak runs them inside its
offline builder. Packaging tests also cover helper discovery, preserved overrides,
source pinning, archive checksums and failed-build preservation.

Run the Podman test where rootless Podman and the built image are available.
When compiling in distrobox without Podman inside it, run the compiled test
executable on the Linux host if its shared libraries are compatible. Each test
uses a private setup folder, socket and container; none accesses IRIX images.

For a desktop check, open Network > Installation, edit a helper and folder,
then Discard and confirm Connection settings were preserved. Save a setup with
a missing `instigator.yaml` and confirm the overlay stays open. A valid setup
must remain pre-boot PAUSED until Ctrl+P; changing an active installation must
offer restart confirmation. Check recent logs in Installation status, reset,
and quit during helper startup. No `prismatic-install-*` container should remain.

Full real-media installation is a separate check: use a new blank disk and
fresh label, verify PROM network boot and miniroot, complete `inst`, then boot
the resulting disk with the helper disabled. A BOOTP/TFTP packet test or an
installer return marker alone does not establish a successful OS installation.

The [recorded installation tests](installation.md#validation-status) completed
Indy R4400 / IRIX 6.5.22f, O2 R5000 / IRIX 6.5.30f and Octane R10000 /
IRIX 6.5.30f installations and graphical disk boots with the helper disabled.
The Octane used 512 MB and Solid IMPACT. O2 used a network-only bootstrap;
Octane used network media with the documented cold-PROM kernel-load and
helper-restart recovery. Indy also has a separately verified network-only
miniroot test. See [installation errors and workarounds](irix-installation-troubleshooting.md).
Repeat this real-media check when changing the helper, boot environment or
stream lifecycle; keep the new disk and its overlay sidecars separate from
existing machines.

## New platform and runtime checks

The runtime schema suite exercises all current processor parts and graphics
choices, protected NVMe/FC configuration, inactive-preference preservation,
virtio slot conflicts, invalid settings and path resolution. The remote-CD
regression serves a local HTTP byte-range fixture and verifies read-only access.

For the seven newer platforms, run the opt-in diskless lifecycle test:

```sh
IRIS_REX_JIT_PROFILE=/tmp/prismatic-platform-rex.bin IRIS_NO_EXIT_ON_POWEROFF=1 \
cargo test --locked --features pcap,idle-pause,jitv2 new_platforms_start -- --ignored --nocapture
```

It uses temporary state and bundled firmware for Origin 200/2000, Onyx2, Fuel,
Tezro, Origin 3200 and Onyx 3000. It checks startup, CPU execution when counters
are exposed, pause, stop and immediate serial-port reuse. It is **not an OS boot
or graphics correctness test**. The current xiri integration supplies independent
CPU counters for headless platforms; an upstream main lacking those hooks can
only be checked for lifecycle/port handling on those machines.

The existing Indy `--boot-smoke-test` still verifies real PROM framebuffer
output. Existing O2/Octane IRIX installation results are separate, stronger
coverage for those particular configurations.

Validation of this integration on 2026-11-14: 185 default-feature frontend tests
and 186 with `pcap,idle-pause,jitv2` passed against the xiri integration; 157
passed against isolated upstream main. Both cores passed the seven-platform
lifecycle check. The dummy-video UI and diskless Indy PROM framebuffer checks
passed. The core's injected platform-serial regression passed, and
`wgpu,gpujit,hostgl` passed a compilation check on Linux (runtime GPU validation
is not claimed).
