# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for [our](https://github.com/salvogendut/iris) fork of [Iris](https://github.com/techomancer/iris),
following the desktop conventions of the Happy Years emulators
([1984](https://github.com/salvogendut/1984), [1985](https://github.com/salvogendut/1985),
[1986](https://github.com/salvogendut/1986), and [1989](https://github.com/salvogendut/1989)).

Prismatic embeds Iris and opens **PAUSED** with a **diskless SGI Indy**
configuration by default: R4400, 64 MB RAM, Newport graphics and Iris's built-in PROM. **F9 > General**
configures Indy IP24, Indigo2 IP22, Indigo2 IMPACT IP28, O2 IP32 and Octane IP30,
with compatible RAM/CPU choices, keyboard layout, PROM selection and saved
machine labels.
Configure it first, then press **Ctrl+P** to boot. Every normal launch and label
switch starts paused; smoke tests start automatically. The guest framebuffer
appears in the SDL3 window, with the Happy Years F9 overlay, fullscreen,
scanlines, filtering and saved desktop preferences. SDL keyboard and relative
mouse input now reach the guest through Iris's PS/2 controller.

**F9 > Media** attaches hard disks and CD-ROMs; bottom LEDs show machine, CPU,
network and disk activity, with a separate white LED for each CPU (`CPU0`,
and `CPU1` on dual-CPU Octane). **Guest audio plays through SDL3**, with saved mute
and volume controls in General. Screenshots play the shared Happy Years shutter
sound independently. With fresh NVRAM and
no disk, the PROM normally displays "Unable to boot; press any key to continue".
Click the display to capture input, then press Enter to reach the PROM menu.
Choose **Enter Command Monitor** (5) and try `hinv` to inspect the emulated
hardware. Startup errors appear in the window
and terminal; confirmed quit stops the worker and saves the machine's NVRAM before closing.

## Screenshots

IRIX 6.5 running in Prismatic, captured at the guest's native 1282×1024 resolution.

![IRIX 6.5 desktop with the console and Icon Catalog](images/irix65-desktop.png)

![Xmahjongg running on the IRIX 6.5 desktop](images/irix65-xmahjongg.png)

## Build and run

Run `make` in a Rust/SDL3 build environment:

```sh
make                          # Fetch/update Iris, then build a release
./target/release/prismatic
# Or build and launch in one step:
make run
```

The first settings in [Makefile](Makefile) choose the source:

```make
IRIS_REPO ?= https://github.com/salvogendut/iris.git
IRIS_BRANCH ?= xiri
```

Edit these lines to change the defaults, or override them for one invocation:

```sh
make IRIS_BRANCH=main
make IRIS_REPO=https://github.com/YOUR-NAME/iris.git IRIS_BRANCH=your-branch
make FEATURES=pcap,idle-pause,perfetto,jitv2
```

The helper clones each repository/branch combination into its own ignored
`target/bootstrap/<source-key>/iris` directory. Later invocations fetch and
fast-forward that managed checkout before compiling. It refuses local changes,
changed remotes/branches, and local or divergent commits instead of resetting
them. An existing sibling `../iris` checkout is not modified.

A generated Cargo workspace beside the managed Iris checkout links to this
repository's current `src`, assets and build script. This builds your latest
Prismatic edits together with the fetched core without rewriting the root Cargo
manifest or lockfile. Cargo builds use `--locked`; if a newer Iris dependency
requires a lockfile update, the build fails rather than silently resolving a new
dependency set. The successful build's Iris revision and settings are recorded
in `target/bootstrap/<source-key>/build-info.json`.
Check results use a separate `check-info.json`, preserving the build record.

| Command / setting | Purpose |
| --- | --- |
| `make fetch` | Fetch Iris without requiring Rust or compiling. |
| `make run ARGS='--config "my machine.cfg"'` | Build and run from this repository, so relative config/capture paths behave normally. |
| `make PROFILE=dev` | Build into `target/debug` instead of `target/release`. |
| `make FEATURES=` | Use the interpreter with optional Cargo features disabled. |
| `make check FEATURES=` | Fetch, then run frontend formatting, Clippy and tests with default Cargo features. |
| `make test-bootstrap` | Test the build helper using disposable local Git repositories; no network or Rust required. |
| `CARGO=...`, `PYTHON=...` | Select the Cargo command or Python interpreter. |
| `CARGO_TARGET_DIR=...` | Choose the Cargo artifact directory; default is this repository's `target`. |

`make help` lists the helper options. Without Make, run `python3 tools/build.py`;
its `--repo` and `--branch` options select other sources. The helper requires
Git and Python 3 on Unix, plus the Rust/native build dependencies below for
compilation. Use your Distrobox or other development environment with Cargo on
`PATH` and the native libraries installed. It does not install system packages
or change your Rust environment.

For direct Cargo builds that use a checkout you maintain yourself, keep the
checkouts alongside one another:

```text
Dev/
  iris/        # our Iris fork
  prismatic/
```

Prismatic development uses our Iris fork's **`xiri`** branch, which includes the
merged performance work. **`perf/sgiemu-port`** is retired. Automated builds
default to **`xiri`**, retaining NetBSD and other guest OS compatibility work
alongside IRIX. Compatibility with the fork's **`main`** remains available. Direct `cargo`
commands use `../iris`; `make` uses its managed source checkout. Each binary
links the Iris checkout present when built; rebuild after changing sources.

`build.rs` detects optional embedding APIs from the Iris sources, independently
of branch names. Missing features are greyed out in the overlays, with a reason
in the hint strip. Saved preferences remain in `prismatic.cfg` and machine
stanzas; rebuilding against a core that provides the APIs enables them again.
Rebuilding against the integrated `xiri` automatically enables its APIs,
including the optional SGIEmu performance control.

The current `main` core lacks several frontend hooks: SDL guest audio,
diagnostic routing, PROM boot/console presets, PTY/Off serial routing, owned
GDB/VNC services, per-machine affinity/cache controls, optional-device lifecycle
support and paused checkpoint saving. Saved PTY/Off routes fall back to TCP at their saved port (0 for automatic);
choose distinct ports if previously inactive ports conflict. TCP serial ports, normal graphics,
raw disks, CDs, network services and supported CPU/JIT options remain usable.
Protected CHD hard disks require the fork's CHD COW API; unavailable builds
reject them before startup and accept raw images instead. `main` also requires
external O2/Octane PROMs selected in General. Diagnostics stay on the console;
guest audio is disabled when no SDL PCM hook exists. Network changes require
restart on cores without persistent live-network overrides.

The integrated `xiri` includes PROM presets, PTY/Off routing and the newer
Octane CPU selections. General cycles only CPUs available in the linked core. A label saved
with an unavailable CPU still loads, but boot asks you to select a supported
CPU rather than silently replacing the hardware. **Connection status** lists
which optional capabilities are unavailable in the current build.

The root Cargo manifest preserves the fork's patched winit
and block dependencies. Install Rust through rustup; `rust-toolchain.toml`
selects `nightly-2026-10-28`, the toolchain used for the Iris performance work.
On a Fedora development system, install the native build dependencies:

```sh
sudo dnf install git python3 gcc gcc-c++ make pkgconf-pkg-config sdl3-devel alsa-lib-devel clang-devel libpcap-devel
# Recommended fetch-and-build workflow (defaults to our fork's xiri):
make
# Alternatively, with a manually maintained sibling checkout:
# Iris xiri:
cargo run --release --locked --features pcap,idle-pause,perfetto,jitv2
# Older Iris main (or omit --features for the interpreter):
cargo run --release --locked --features pcap,idle-pause,jitv2
```

Older `main` Odyssey revisions fail to compile with Iris's `perfetto` feature
(missing `events::cfifo`/`events::swap` references); the integrated `xiri` carries
the correction. Omit `perfetto` on affected checkouts. Tracing controls are grey
when not built or when the owned tracing API is absent.

The performance build enables Iris's experimental CPU JIT and its new integer
register cache, alongside the RTC calendar cache. Keep `developer` disabled:
developer/lockstep instrumentation disables the register cache. The existing
PCAP, idle-pause and Perfetto controls remain available. These optimizations
are workload-dependent; the branch's bare-metal results do not establish a
Prismatic desktop or OS boot speedup.

The current Iris library still compiles its winit/glutin, CPAL and camera
modules. Prismatic uses its own SDL window. Guest audio uses a per-machine PCM handoff
when that API is available; otherwise it is disabled without opening CPAL. libclang is needed to build the
Linux camera bindings.
SDL is linked dynamically through the [`sdl3` Rust bindings](https://github.com/vhspace/sdl3-rs).
SDL_ttf and a separate widget toolkit are not required. Linux is the first
validated platform; Windows and macOS integration builds remain to be checked.

With Iris bundled-firmware support, all five models include PROMs; otherwise
O2 and Octane require external PROM files. General accepts these files or
overrides (see below). The default is diskless; only images
selected in Prismatic are attached. It does not read `iris.toml`, `prom.bin`,
or default SCSI image paths from either checkout. Normal machine state is kept under
SDL's per-user application directory, normally
`~/.local/share/prismatic/` on Linux. Each model has its own directory: `indy/`,
`indigo2/`, `indigo2-ip28/`, `o2/` or `octane/`, containing its NVRAM/EEPROM or
persistent flash. `--state-dir PATH` keeps the existing Indy layout at `PATH`;
other models use their named subdirectory under `PATH`. Firmware source files
are read-only; O2 saves flash in `nvram.bin`, Octane in `flash.bin` alongside its
`nvram.bin`. Existing flash preserves the guest's firmware and environment.
The Iris monitor and serial ports default to ephemeral loopback ports.
Advanced > Connections selects serial TCP ports or pseudoterminals, plus optional
GDB/VNC servers. Connection status shows all active addresses and PTY paths.
Iris also maintains its shared draw-shape cache at
`~/.iris/rex-jit-profile.bin`; `IRIS_REX_JIT_PROFILE` overrides that cache path.

See [the integration design](docs/architecture.md) for module boundaries and
remaining work.

## Controls

| Key | Action |
| --- | --- |
| Click guest display | Capture keyboard and mouse; the capture click is consumed |
| Ctrl+Enter | Release input and return the pointer to the desktop |
| Ctrl+P | Boot the selected machine; pause or resume an existing session |
| F2 | Cycle saved machine labels; a running session requires confirmation before switching |
| Ctrl+Plus / Ctrl+Minus | Increase / decrease window scale by 25%; also accepts Ctrl+= and keypad +/− |
| F4 | Save a guest-display screenshot (PPM) |
| F5 | Confirm a cold reset; a paused session remains paused |
| F6 | Start or stop an animated GIF capture |
| F9 | Open options; save and close an open options session |
| Left / Right (in options/dialogs) | Change section or confirmation choice |
| Up / Down (in options) | Select a row |
| Enter (in options/dialogs) | Cycle a value, toggle an option or accept a confirmation |
| Escape (in options/dialogs) | Close; offer Save / Discard / Cancel if changed |
| F11 | Toggle fullscreen |
| F12 / window close | Confirm quit |
| Shift+F2 / Shift+F4 / Shift+F5 / Shift+F6 / Shift+F9 / Shift+F11 / Shift+F12 (captured) | Send a bare guest function-key tap |

While captured, other keys, including Escape and the remaining function keys,
go to the guest. Opening options or a quit confirmation, losing focus, or
stopping the machine releases capture and any held guest keys/buttons. Click
the display again to resume input. The host shortcuts remain available when
input is released. Shift+reserved-function-key sends one unmodified tap, not
a held key or a Shift combination.

Window scaling ranges from **75% to 400%**, relative to the default 960×720
window (100%). Each shortcut saves `window_scale` immediately in `prismatic.cfg`,
and the next launch restores it. A notification shows the selected percentage.
The shortcuts also work with guest input captured and menus open; unfinished
menu edits remain separate, and Save/Discard preserves the saved scale.
In fullscreen, scaling selects the size used after F11 returns to windowed mode.
A maximized window is restored before resizing. The desktop may constrain
large requested sizes. Guest resolution and screenshot/GIF dimensions remain
independent of the window scale.

Keyboard input follows **physical key positions**, preserving left/right
modifiers and the numeric keypad. General > Keyboard selects Iris's physical
keyboard type (default: ANSI). `--keyboard ansi|iso|jis` overrides the saved
choice for the session; saving options also persists that choice. Use ISO for
a European keyboard. Character layout is determined by the guest. Text paste and IME
input are not implemented. Left, middle and right mouse buttons are forwarded;
wheel and extra buttons work when the guest enables the corresponding PS/2
mouse protocol.

Enable **General > Tinker** to expose Advanced, Network and Extensions.
Advanced includes Iris core options, filtering, debug output and GIF capture
settings; Extensions selects graphics hardware, CPU count, storage services,
optional devices and experimental checkpoints.
Hiding these tabs retains their saved settings. Options
are edited in a private draft and applied only after a successful save. Cancelling or discarding leaves the live display
unchanged. Confirmation dialogs initially select Cancel.
Extensions also has explicit storage actions: these have their own confirmation
and result screens and run independently of settings Save/Discard.

## Screenshots and GIF capture

**F4** saves the guest framebuffer at its native resolution as a PPM image,
matching 1989. **F6** starts recording; press it again to finish the GIF. These
shortcuts work while input is captured and while menus are open. Captures contain
the guest display and guest cursor, without the frontend menus, debug panel,
scanlines, notifications or bottom bars. With two Newport heads, captures include
both displays side by side at their combined native size. A guest frame must have arrived first.

Successful screenshots play the same embedded shutter sample as 1984/1985/1986.
If host audio is unavailable, the image still saves and the terminal reports it.

Files appear in the **current working directory**, with names such as
`prismatic-20261031-142530Z-ABC123.gif` (UTC timestamp and a unique suffix).
Completed files never replace an existing capture. Notifications report saves
and failures; the LED bar shows **GIF REC - F6 STOP** while recording and
**GIF SAVING...** while finishing. Confirmed quit or machine restart finishes
the active recording before proceeding. Cancelling quit/restart keeps it running.

Use **General > Tinker > Advanced** for GIF resolution, frame rate and encoder.
The default is **Native resolution, 25 fps, built-in encoder**. Native captures
the guest's exact pixel dimensions at recording start (for example, 1282×1024),
independently of the window size. Toggle **GIF resolution** between Native and
320/480/640-pixel widths with Enter, then F9 to save. Existing saved widths stay
selected until changed. Saved changes apply to the next recording without
restarting Iris. Scaled widths preserve the guest aspect ratio; later guest
resolution changes fit inside the recording's initial canvas with black borders.
Identical frames share a longer delay.
If encoding falls behind, samples may be skipped while elapsed timing is retained.

**FFmpeg optimize** optionally runs `ffmpeg` from PATH after recording. The
built-in GIF is kept if FFmpeg is unavailable, fails, changes dimensions/timing
or produces a larger file. Screenshot and GIF writing, encoding and optimization
run on workers while the window continues handling input.

## Machine settings

In **F9 > General**, select a row with Up/Down and press Enter to cycle its
value. F9 saves. Hardware changes ask for **Restart / Cancel** before they are
saved and applied; choose Restart with Left/Right, then Enter. Cancel or Escape
returns to the draft. Escape from the editor offers Save / Discard / Cancel.

**General > PROM boot** selects a boot preset for the current machine label:

| Choice | OSLoader | OSLoadFilename | OSLoadOptions |
| --- | --- | --- | --- |
| Keep existing (default) | Unchanged | Unchanged | Unchanged |
| IRIX | `sash` | `/unix` | `auto` |
| NetBSD | `boot` | `netbsd.boot` | `auto` |

F9 saves the choice in that label's `[machines."Label".machine]` stanza as
`prom_boot = "keep_existing"`, `"irix"` or `"netbsd"`. The preset applies before
each cold boot or confirmed restart. When the machine is still waiting in its
initial PAUSED state, Save leaves it ready for **Ctrl+P**. Discard and failed
configuration saves do not modify firmware state.

Boot partitions, AutoLoad, network addresses and other PROM settings are preserved.
Keep existing stops applying a preset; it retains the values already stored in
the PROM, including an earlier preset. These choices set boot variables, so the
selected disk must already contain the matching loader and kernel. The NetBSD
preset uses your `netbsd.boot` filename; other installations may use a different
name (see the [NetBSD boot manual](https://man.netbsd.org/sgimips/boot.8)).
All five machine profiles have firmware-storage support; this does not imply
that either guest OS boots on every model. Unrecognized or damaged firmware
state produces a startup error; Keep existing allows normal PROM initialization.

**General > PROM console** independently selects the console for each label:

| Choice | PROM setting | Console |
| --- | --- | --- |
| Keep existing (default) | Unchanged | Retains the stored console choice |
| Graphics | `console=g` | Guest display and keyboard in the Prismatic window |
| Serial | `console=d` | First guest serial console using the selected TCP or PTY route |

These are the PROM's [graphics and serial console settings](https://www.netbsd.org/~sborrill/htdocs/sgimips/faq.html).
F9 saves `prom_console = "keep_existing"`, `"graphics"` or `"serial"` in the
label's machine stanza. It follows the same cold boot/restart and initial
**Ctrl+P** behavior as PROM boot. Keep existing retains any previously applied
choice. Changing the console preserves the OS boot preset, baud rate and other
existing firmware settings.

For Serial, enable **Tinker**, then select routing in **Advanced > Connections**:
use **Serial B** on Indy/Indigo2, or **Serial A** on O2/Octane. After booting with
Ctrl+P, open **Connection status** to find the assigned TCP port or PTY path.
TCP defaults to automatic allocation; connect with `telnet 127.0.0.1 PORT`, or
use minicom with the PTY instructions below. Use distinct fixed TCP ports for
simultaneously running labels. Serial output appears in that terminal; the
Prismatic guest display may remain blank. The overlay remains available to
switch back to Graphics.

| Machine | CPUs | RAM presets (MB) | Graphics | PROM |
| --- | --- | --- | --- | --- |
| Indy IP24 | R4400, R5000 | 32, 64, 128, 256 | Newport, XZ | Bundled |
| Indigo2 IP22 | R4400 | 32, 64, 128, 256, 384, 512 | Newport, XZ, Extreme, Solid/High/Maximum IMPACT | Bundled |
| Indigo2 IMPACT IP28 | R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | Solid/High/Maximum IMPACT | Bundled |
| O2 IP32 | R5000, R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | CRIME/GBE | Bundled 4.18; optional external IP32 PROM |
| Octane IP30 | R10000, R12000, R14000 (one or two CPUs) | 64, 128, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 6144, 8192 | Solid/High/Maximum IMPACT; VPro V6/V8/V10/V12 | Bundled 4.9; optional external IP30 PROM |

For Odyssey, choose **Octane IP30** in General, enable **Tinker**, then select
**Extensions > Graphics board > VPro V6, V8, V10 or V12**, save, and boot. The selected card is saved per machine label and
changing it follows the usual restart confirmation. V6/V10 have 32 MB of
graphics memory; V8/V12 have 128 MB. Iris presents the newer Crossbow revision
for V10/V12. The bundled PROM 4.9 meets all four cards' firmware requirement.
Existing persistent flash can retain an older PROM; V6/V8 require 4.0 or later,
and V10/V12 require 4.5 or later. The guest OS also needs an Odyssey driver.
The current core models one 1280x1024 display; this selector does not enable
DCD or dual-head VPro. VPro machine snapshots remain unsupported by Iris.

All models offer ANSI, ISO and JIS keyboard layouts. Graphics choices and
Octane CPU count are in **Extensions**, under Tinker. Newport is the default
on Indy/IP22; Solid IMPACT is the default on IP28/Octane.

Changing models keeps compatible CPU/RAM choices, selects R4400 when changing
from an R5000 Indy to Indigo2, and caps RAM at 256 MB when returning to Indy.
These hardware settings take effect by stopping the old machine and preparing a
fresh, paused configuration, so the current guest session is lost. Ctrl+P boots it.
Before the first boot, options can change without restarting a machine. The
window stays responsive
during restart. Display and audio options apply on Save without restarting. Startup or
restart failures remain visible in the window and terminal; F9 lets you revise
the saved settings. A new worker is never started after a stop timeout.

When IRIX requests power-off, Prismatic finishes storage cleanup, finalizes any
GIF recording, clears the guest display and shows **Machine powered off** with
the RUN light off. The window stays open: press **Ctrl+P** to boot again, or use
F9 to revise its options and leave it ready in PAUSED. Discard leaves it powered
off. A monitor halt
or breakpoint keeps the last display and reports **Machine halted** instead.
In 4Dwm's System Shutdown dialog, select **Power Off the System** to request
power-off; a shutdown that returns to firmware does not power off the machine.

### Named machines

In **General > Save machine as...**, type a label and press Enter, then **F9** to
save the complete options draft. Labels accept 1–32 printable ASCII characters.
Save As keeps the source label; an existing name is rejected so it cannot be
overwritten accidentally. Select an existing label with **General > Machine
label** or **F2**, then edit its options and Save to update it. Discard changes
neither the saved labels nor the active machine.

Each label remembers its machine model, CPU model/count, RAM, graphics board,
display heads, keyboard, firmware, media,
Advanced machine options and Network settings. Desktop preferences, capture
options and Tinker remain shared. F2 cycles labels alphabetically, and the
status strip shows the active label and its state. Close Options before using
Ctrl+P. F5 asks before cold-resetting
an existing guest. It stops and recreates the machine, finalizes any GIF capture,
and boots again if it was running. A paused machine remains ready for Ctrl+P.
Pause/resume preserves memory; cold reset discards the guest session.

One machine runs per window. Switching a running or previously booted paused
session stops it after confirmation, then leaves the selected label **PAUSED**.
Each launch also starts PAUSED. To open a particular saved label directly:

```sh
cargo run --release --locked -- --machine-label "Indy IRIX"
```

Labels are TOML stanzas in `prismatic.cfg`; `machine_label` selects the default.
The menus write complete settings. This abbreviated example uses defaults for
omitted fields:

```toml
machine_label = "Indy IRIX"

[machines."Indy IRIX"]
legacy_state = true
[machines."Indy IRIX".machine]
profile = "indy_ip24"
cpu = "r4400"
ram_mb = 64
[machines."Indy IRIX".media.devices.1]
kind = "disk"
path = "/path/to/irix.chd"

[machines."Indigo2 Test".machine]
profile = "indigo2_ip22"
cpu = "r4400"
ram_mb = 128
```

The first label adopts the unnamed machine's firmware state (`legacy_state =
true`). Additional labels have separate NVRAM/EEPROM/flash directories under
`machines/<hex-encoded-label>/<model>/`. Disk paths still identify the existing
images and their overlays; Save As copies the configuration. Independent windows
can run different labels with different writable disks. Instance locks prevent
two Prismatic windows from writing the same firmware state or disk overlay;
read-only CDs can be shared. Configuration saves preserve unrelated labels from
other windows and reject conflicting edits to the same label. Reopen the config
to pick up labels added in another window.

Prismatic passes explicit temporary copies of Iris's bundled Indigo2 PROMs to
avoid the core's alternate ROM search in the working directory. IMPACT and GBE
use Iris's software-composed framebuffer through the same SDL display and capture
path; F4/F6 retain the guest's native dimensions.

The feature descriptions below apply when the linked Iris core exposes the
required APIs. Greyed controls show their availability reason in the hint strip.

### O2 and Octane firmware

The `xiri` core embeds O2 4.18 and Octane 4.9 as Rust byte arrays,
just like their Indy and Indigo2 PROMs. With that capability, leave **PROM
firmware** at **Bundled with Iris**. On cores without it (currently `main`),
**Use bundled** is greyed out and an external PROM is required before boot.

To override the bundled image, use **General > PROM firmware > Choose file**.
Choose the extracted `.bin` image, press F9, and confirm any requested restart.
**Use bundled** clears the override in the draft; Save applies it and Discard
leaves the previous selection intact. Paths are remembered
separately as `o2_prom` and `octane_prom` under `[machine]` in `prismatic.cfg`.
Relative paths in a hand-written config resolve beside that config. File selection
and Save check readability and size on a worker without creating machine state;
Save rejects a missing or invalid-sized external PROM and keeps the draft open;
it does not silently substitute the bundled image. Cancelling
the picker or discarding the draft leaves the running machine untouched. These
checks do not authenticate firmware contents or guarantee a successful guest boot.

The bundled O2 4.18 PROM is 512 KiB; Octane 4.9 is 1 MiB. Octane accepts an external IP30 flash image up to
1 MiB, including the 879,312-byte IRIX `IP30prom.bin`; Prismatic accepts images
from 512 KiB to 1 MiB. MAME documents these full flash dumps:

| Model | File | SHA-1 |
| --- | --- | --- |
| O2 | `ip32prom.rev4.18.bin` | `f2cfa7246d67f88fe5490e40dac6c04b1deb4d28` |
| Octane | `ip30prom.rev4.9.bin` | `de250875c608add63749d3f9fb81a82cb58c3586` |

References: [MAME O2 firmware definitions](https://github.com/mamedev/mame/blob/master/src/mame/sgi/o2.cpp),
[MAME Octane firmware definition](https://github.com/mamedev/mame/blob/master/src/mame/sgi/octane.cpp).
The matching arrays live in `src/prombinip32.rs` and `src/prombinip30.rs` in
[our Iris fork](https://github.com/salvogendut/iris/tree/xiri),
and are compiled into Prismatic. The fork's `tools/embed-prom.py` regenerates
them from the original binaries without altering their bytes.

Existing machine flash remains authoritative: switching firmware does not erase
O2's `nvram.bin` or Octane's `flash.bin`. Use a new machine label for a fresh
firmware boot while retaining the existing machine's state.

## Advanced settings

Enable **F9 > General > Tinker**, then use Left/Right to open **Advanced**.
Up/Down selects a row and Enter cycles its value. F9 saves; core changes use
the same **Restart / Cancel** confirmation as General and Media. Cancel keeps
the draft, and Discard leaves the running machine and saved file unchanged.
Turning Tinker off hides the controls without resetting their values.

The **Advanced page** row cycles **Core**, **Performance**, **Connections** and
**Developer**. Press Up from the first row to reach it quickly. Enter opens a
text editor for numeric values and paths; Ctrl+A replaces the value, Enter
accepts the draft, and Escape cancels. The existing controls below are on Core.

| Setting | Choices | Default |
| --- | --- | --- |
| Guest resolution | Guest-controlled, 1024×768, 1280×960, 1280×1024 | Guest-controlled |
| CPU cache model | Auto, Full, Shadow | Auto |
| Guest clock offset | Host time or −5/−10/−15/−20/−25/−30 years | Host time |
| SCSI interrupt timing | Deferred, Immediate | Deferred |
| Serial console log | Off, Raw, Timestamped (UTC) | Off |
| Bilinear filtering | Off, On; applies without restarting | Off |
| Debug output | Off, Screen, Console; applies without restarting | Off |
| GIF resolution | Native guest resolution, or 320/480/640 pixels wide | Native |
| GIF frame rate | 10, 20, 25 fps | 25 |
| GIF encoder | Built-in, FFmpeg optimize (if available) | Built-in |

Performance exposes the following settings, all saved per machine label:

| Setting | Behavior |
| --- | --- |
| Count clock | CP0 Count rate in MHz; `auto` uses the model default. This controls guest timer/clock reporting, independently of host emulation throughput. |
| Idle parking | Enables Iris's known-idle-loop parking in `idle-pause` builds. |
| Host thread affinity | Enables optional host core IDs for CPU, REX3 processing and Newport refresh threads. `auto` leaves a role to the host scheduler. CPU IDs must be available to the process; both Octane CPUs use the CPU role. |
| JIT compiler workers | 1–64 workers for the experimental `jitv2` build. The execution engine is selected at build time. |
| JIT disk cache | Opt-in compiled-page reuse, stored under this label's state directory in `jit-cache/`. |
| SGIEmu optimizations | **Off by default.** Enables the JIT integer register cache and Indy/Indigo2 clock calendar cache. Changing a running machine requires a confirmed restart. |

Enable **Tinker**, then open **F9 → Advanced → Performance** to change
**SGIEmu optimizations**. The choice persists per machine label as
`advanced.expert.sgiemu_optimizations`. Ordinary JIT compilation and correctness
fixes, including the JIT restart and Newport rendering fixes, remain enabled
independently. The register cache requires `jitv2` without `developer`; O2 and
Octane have different RTCs. On older cores the control is disabled and the saved
preference is retained; it cannot change those cores' optimization policy.

Connections provides **Serial A routing** and **Serial B routing**, plus editable
**Monitor**, **Serial A/B TCP**, **GDB** and **VNC** ports. Each serial route
cycles **TCP** (default), **Pseudoterminal** (Unix hosts), and **Off**. Off
disconnects the host endpoint while leaving the guest UART emulated. Monitor
and serial TCP ports accept `0`/`auto` for automatic allocation;
GDB and VNC additionally accept blank/`off` to disable them (the default).
All bind to **127.0.0.1 without authentication**. Configured fixed ports must
be distinct and free when booting; use different ports or automatic allocation
when running labels in separate processes. GDB targets CPU 0; attaching stops
that CPU. VNC shares the primary guest display and input, with a saved 1–60 fps
limit (default 30). Connection status describes the loaded machine, independently
of menu drafts, including the actual automatically allocated serial ports.
Services start on Ctrl+P boot and close on reset, label switch, power-off or quit.

For minicom, choose **Pseudoterminal** on the desired channel, save with F9,
and boot with Ctrl+P. For the PROM console, also select **General > PROM console >
Serial**: use channel B on Indy/Indigo2 and A on O2/Octane. Open
**Advanced > Connections > Connection status** and copy its PTY path, for example:

```sh
minicom -o -D /dev/pts/7 -b 9600
```

Replace `/dev/pts/7` with the displayed path. In minicom, set hardware and software
flow control to **No**; `-o` skips modem initialization. See the
[minicom manual](https://man7.org/linux/man-pages/man1/minicom.1.html).
Use one terminal reader per PTY, running as the same user in the same host/container
as Prismatic. The PTY remains available while paused and supports disconnecting
and reconnecting. Its path can change after each boot; only the routing choice
and TCP port preference are saved, not the temporary device path.

The PTY uses raw bytes with a bounded 64 KiB output queue in addition to the
kernel buffer. An absent or slow reader can eventually lose output; Connection
status reports dropped bytes. Serial logging remains available for a transcript.
Selecting PTY or Off retains the TCP port preference but does not bind it. All
connection changes are saved per label and require a restart when running:

```toml
[machines."Serial Indy".advanced.expert]
serial_a = "off"
serial_port_a = 0
serial_b = "pty"
serial_port_b = 8881
```

The current performance build can reach a PROM panic when restarting an Indy
inside the same Prismatic process after changing routing. Configure the channels
before the first Ctrl+P boot, or quit and relaunch after saving changes to a
running machine. The saved routes work on a fresh launch; the equivalent
in-process PTY-to-TCP switch passes with the interpreter build.

Developer provides **Boot ELF**, **Trace on boot**, trace categories and the
trace timeline. Boot ELF accepts a static, big-endian MIPS ELF32 executable on
Indy/IP22/IP28; it loads before the CPU starts and bypasses PROM execution.
Relative paths resolve beside `prismatic.cfg`; clear the field to restore PROM
boot. Iris does not currently support direct ELF loading on O2/Octane.
With `perfetto`, Trace on boot writes a new `traces/boot-*.pftrace` in the label's
state directory for every boot. It flushes on stop without replacing earlier
traces. Categories accept `all` or a comma-separated list such as
`cpu,irq,gfx,scsi`; prefix a category with `-` to exclude it (`all,-prim`).
The timeline uses host time or retired guest instructions (CPU 0 on Octane).
Runtime status shows the active trace path and build features. Open the trace
with the Perfetto UI. The `developer` feature also enables Iris's existing
monitor device/GPU tracing commands; see the fork's `debug.md` for those commands.

Unavailable controls are grey and retain their saved values. Enter and Backspace
have no effect on those rows; the runtime uses supported defaults without rewriting
the configuration. Build flags remain explicit (the Perfetto examples require a
core with working Perfetto support):

```sh
cargo run --release --locked --features pcap,idle-pause,perfetto
# Performance build: enable the CPU JIT with its GPR cache active:
cargo run --release --locked --features pcap,idle-pause,perfetto,jitv2
# Also enable the experimental CPU JIT and developer monitor instrumentation:
# This diagnostic build disables the GPR cache.
cargo run --release --locked --features pcap,idle-pause,perfetto,jitv2,developer
```

Performance, connection and developer setting changes use the normal Save and
restart confirmation. Draft validation checks ELF format, host core IDs and
build support without starting servers or creating cache/trace files. Discard
has no runtime effects. Settings persist in `[advanced.expert]` and each named
machine's `[machines."Label".advanced.expert]` stanza. Existing configs keep their
previous behavior; optional remote services, boot ELF and tracing default off.

Debug output routes Iris's routine startup, bus-probe, Newport register/display
and draw-cache diagnostics. **Screen** shows the latest 12 wrapped lines above
the bottom strips; **Console** writes to the launching terminal. Changes apply
on Save, without restarting; Discard preserves the active destination. Screen
starts with new messages when enabled, and Off clears the screen log. To capture
startup diagnostics, save Screen or Console before the next launch.
Startup/storage failures still reach the terminal with debug output Off.
This setting is separate from guest serial logging and Iris monitor tracing.

With GR2, IMPACT or O2 integrated graphics, video mode is guest-controlled and
the cache is fixed to Shadow. O2 and Octane also use their controllers' SCSI timing and host RTC time;
those Advanced rows cannot be changed. Saved Newport/cache/clock/SCSI preferences
are retained for models that support them.

Resolution selects the **initial Newport video mode**; the PROM or guest may
program another mode later. Cache Auto selects Iris's implementation for the
build (Full in the current interpreter build). This is a cache model control,
not a JIT switch. Deferred SCSI interrupts are Iris's BSD-compatible default.

The clock offset shifts the RTC's host-time seed at each boot; time then runs
normally and the guest can still set its clock. It is a relative offset, not a
fixed date. The menu uses five-year presets; `clock_offset_years` in the file
also accepts individual whole years from −30 through 0. Iris clamps RTC dates
to the emulated chip's supported range (1970–2039).

Serial logging appends console output (B on Indy/Indigo2, A on O2/Octane) to **`serial.log`** beside the
selected machine's NVRAM: normally `~/.local/share/prismatic/indy/serial.log` or
`~/.local/share/prismatic/indigo2/serial.log`. `--state-dir` moves the log with the
machine state. Raw preserves serial bytes; Timestamped prefixes lines with
host UTC timestamps. The graphical display is not transcribed. Existing logs
are preserved, and no log is created merely by editing or discarding a draft.
An unopenable log is reported as a startup error.

## Extensions

Enable **General > Tinker** and open **Extensions** with Left/Right. The last row,
**Extensions page**, cycles **Hardware**, **Devices** and **Checkpoints**. On the
Hardware page, Enter cycles **Graphics board**, **Display heads** and **CPU count** through the options
supported for the selected model. These settings belong to the selected machine
label. Save validates and persists the draft, and asks to restart a running
machine. Discard has no runtime effect; turning Tinker off keeps the hardware
settings active.

- Newport on Indy/IP22 supports one or two heads. Two displays appear side by
  side in the same SDL window, using the guest's shared keyboard and mouse.
  Each keeps its native resolution; a shorter head has black padding, and an
  uninitialized head is black.
  The guest OS must support and enable the second display. The diskless PROM
  can leave it blank in Guest-controlled mode; a Newport resolution preset
  initializes both heads.
- GR2 XZ is selectable on Indy/IP22, and Extreme on IP22. Solid, High and Maximum
  IMPACT are selectable on IP22, IP28 and Octane. O2 keeps its integrated GBE.
  These alternate boards use Iris's current emulation; availability in the menu
  does not guarantee a particular guest OS or graphics workload works.
- Octane offers one or two R10000, R12000 or R14000 CPUs in General. Default
  nominal clocks are 195, 300 and 600 MHz respectively; R12000/R14000 have
  2 MB secondary caches. These are functional models; pipeline timing is not
  simulated. General's RAM choices extend to 8192 MB,
  using up to four 2048 MB banks. Large settings need corresponding host memory;
  full 8 GiB guest boots are not part of the smoke checks.

Changing models keeps compatible graphics choices and resets incompatible ones
to the model default. Selecting a board other than Newport resets the head count
to one; leaving Octane resets the CPU count to one. Existing configurations with
no new keys retain their previous hardware defaults. The optional `graphics`
key and `heads`/`cpus` keys live in `[machine]` and each label's machine stanza:

```toml
[machines."Dual Octane".machine]
profile = "octane_ip30"
cpu = "r10000"
cpus = 2
ram_mb = 4096
graphics = "maximpact"
heads = 1
```

### Optional devices

The **Devices** page exposes Iris's VINO, DaynaPort and Ultra64 backends on the
Indy/Indigo2 family. O2 and Octane do not support these backends. All controls are
per-label drafts, saved under `[advanced.extensions]` (or
`[machines."Label".advanced.extensions]`), and require a restart. Disable optional
devices before selecting an unsupported model. No extra Cargo features are needed.

- **VINO source**: Off, Test pattern, Black or Host camera. Choose NTSC
  (640×486) or PAL (768×576), and a zero-based host camera index. Camera access
  begins when you boot with Ctrl+P. Iris retries unavailable cameras and supplies
  black frames until capture works. Pausing stops guest DMA; closing/resetting
  the machine releases the camera. **Device status** reports the active source;
  the Iris monitor's `vino status` gives detailed DMA/capture diagnostics.
- **DaynaPort target**: Off or an unused SCSI target on either supported bus.
  Media shows that slot as Ethernet. Set a MAC (blank uses Iris's default) and a
  separate, non-overlapping NAT subnet; the default is `192.168.10.0/24`. It
  inherits NFS/PCAP settings at boot. Network changes therefore require restart
  while DaynaPort is enabled. Port forwarding still targets onboard Ethernet.
  The guest needs a DaynaPort-compatible driver; `scsi dayna` in the monitor
  reports its network activity.
- **Ultra64 development board**: enables Iris's N64 RAMROM/IPC bridge. This needs
  a compatible external gopher64 emulator; Prismatic does not launch it. A host
  lock permits only one active Ultra64 bridge across compatible Iris/Prismatic
  instances. Startup reports failure if the bridge is unavailable.

For example:

```toml
[advanced.extensions]
vino = "test_pattern"
pal = false
camera_index = 0
daynaport = 5                 # SCSI 0:5; 21 would mean SCSI 1:5
dayna_mac = ""               # target-based default
dayna_subnet = "192.168.10.0/24"
ultra64 = false
```

### Experimental checkpoints

The **Checkpoints** page provides **Checkpoint name**, **Save**, **Restore** and
**List**. Boot the saved machine first, and save or discard option drafts before
an action. Names use 1–48 ASCII letters, digits, hyphens or underscores. Save
requires a new name, confirms before stopping the guest, and preserves its
running/paused state. Restore confirms replacement of RAM and disk overlays,
then leaves the machine **PAUSED**; press Ctrl+P to resume.

The supported boundary is **Indy/IP22, one Newport head, raw COW hard disks or
no disks**. Detach CDs and scratch, disable VINO/DaynaPort/Ultra64, GDB and tracing.
CHD overlays, second-head framebuffers, GR2/IMPACT, O2 and Octane do not yet have
complete checkpoint coverage through this integration. Existing CHDs remain
fully supported for ordinary emulation.

Each label keeps bundles in `checkpoints/<name>/` under its machine state
directory. A bundle contains native Iris state, private RAM/framebuffer chunks,
disk overlays and `prismatic.toml` provenance. Restore requires the **same
Prismatic executable, saved machine configuration and base disk contents**.
BLAKE3 hashes detect modified or missing state files and same-size base-disk
changes before native restore begins. Rebuilding Prismatic invalidates old
checkpoints. Full disk hashing can take time; the window remains responsive.

These are experimental local checkpoints, not portable VM images. Iris does not
restore host network connections, transient audio state or external NFS files;
applications may need to reconnect or restart audio. If native restore fails
after validation, Prismatic stops the machine rather than running partially
restored state. Keep normal guest backups and use checkpoints created locally
through this menu; arbitrary Iris monitor snapshots are not accepted here.

### CD changer and disk actions

On the Extensions **Hardware** page, cycle **Storage action target** to a configured disk or CD-ROM
on either SCSI bus. **Add CD to changer** appends an image to that drive's saved
list; **Clear queued CDs** removes the additional discs. The boot image from
Media remains first. These are draft settings: Save applies them with the usual
restart confirmation, and Discard leaves the current drive unchanged.

**Storage actions** opens a separate menu. Save or discard pending options
before using it:

- **Status** reports the attached drive, overlay or current CD tray and queue.
- CD drives offer **Insert CD**, **Next CD** and **Empty tray** while the guest
  is loaded. Unmount the CD inside the guest first. Each action asks for
  confirmation; a failed load preserves the current disc. Next requires at
  least two queued discs. These session changes do not rewrite saved boot media;
  reset or the next boot restores the saved list. Guest media detection/remounting
  is handled by the guest OS.
- Hard disks offer **Commit** (merge overlay writes into the base image) and
  **Reset overlay** (discard those writes). These are permanent, separately
  confirmed actions available only before boot or after guest power-off.
  Pausing or halting a loaded guest is insufficient. An exclusive disk lock
  prevents another Prismatic window from using the selected disk concurrently.
  Raw disks and hashed CHD disks use Iris's native COW operations. Hashless CHD
  overlays support Reset but reject Commit without altering the base.

Storage work runs in the background; its result screen supports Up/Down to
scroll and Enter/Escape to close. Once confirmed, a storage operation must finish
before another action or quit can proceed.

### Scratch transfer drive

**Scratch target** selects an unused target 1–7 on controller 0, or Off.
**Scratch size** selects 64, 256 or 1024 MB. Save these per-label settings, then
boot. Prismatic creates `scratch-SIZE.raw` in that machine's state directory
with an SGI volume header and a raw payload partition starting at byte 4096.
It is directly writable, without a COW overlay. Changing size selects a different
file and preserves the old one; disabling the drive preserves its file too.
Its green **XFRn** lamp shows reads, with amber writes.

**Import scratch payload** copies a chosen host file to the start of partition
0, retaining the volume header. It zero-pads the final partial 512-byte sector
and leaves later payload bytes unchanged. **Export scratch payload** copies the
entire payload to a new host file; an existing destination is never overwritten.
Both actions require confirmation and briefly stop the machine's devices for
the transfer, preserving the previous paused/running state.

This is a raw transfer volume, without filesystem or automatic guest commands.
Stop guest access first. For example, on IRIX with scratch at target 2, use
`/dev/rdsk/dks0d2s0` with `dd` to read an imported payload or write data for export.
Keep track of the meaningful byte count: exports include unused trailing bytes.
Other guest operating systems use their own raw-device names and partition tools.

The additional settings use `[media.cd_changers]` and `[media.scratch]`, or the
corresponding stanzas under `machines."LABEL"`:

```toml
[machines."Indy IRIX".media.cd_changers]
6 = ["/path/to/disc2.iso", "/path/to/disc3.efs"]

[machines."Indy IRIX".media.scratch]
slot = 2 # Omit to disable; cannot overlap a normal Media device.
size_mb = 64
```

## Media and activity LEDs

Open **F9 > Media**, select a SCSI target, and press Enter. Choose **Hard disk**, **CD-ROM**, **Disconnect** or **Cancel**. The
native file chooser accepts CHD and raw images, including `.img`, `.raw`,
`.dsk`, `.hdd`, `.iso` and `.efs`. Choose the device type explicitly: an EFS
CD image may have an `.img` extension. Selection edits the draft; **F9 Save**
validates the images and asks to restart before applying a media change.
Cancel retains the draft; Discard leaves the current devices unchanged.

The **SCSI controller** row at the bottom switches between buses where Iris
supports two controllers: IP22, O2 and Octane. Indy and IP28 currently expose
one. Each bus offers IDs 1–7, or 1–15 on Octane; ID 0 is reserved for the host
adapter. The same target ID may be used on different buses. Config keys encode
`controller * 16 + target`: `[media.devices.17]` is controller 1, target 1,
and `[machines."Octane".media.devices.31]` is controller 1, target 15 in that
label. Existing keys 1–7 retain their controller-0 meaning.

Changing models never silently disconnects media. Unsupported assignments stay
visible as **Unavailable**, including a bus selector when needed to reach them.
Save reports the incompatibility until you disconnect them or select a compatible
model. The same writable disk cannot occupy two targets, even across buses.

| Device | Supported storage |
| --- | --- |
| Hard disk | Raw images with 512-byte sectors; hard-disk CHDs with 512-byte sectors |
| CD-ROM | Cooked raw data images containing ISO9660 or EFS; single-track Mode 1 CD CHDs |

The Media menu labels primary-bus targets with these Indy-style suggested roles
and explains the selected role in the hint area. Additional buses and wider-bus
IDs use **General use**. These labels guide placement across machine profiles;
your guest OS may use different assignments.

| ID | Conventional assignment |
| --- | --- |
| 0 | Onboard controller; reserved, explained in the hints and not selectable |
| 1 | Primary system disk; the usual boot target is `dksc(0,1,0)` |
| 2 | Second hard disk |
| 3 | Optional floptical drive |
| 4 | Tape drive |
| 5 | Available for additional devices |
| 6 | CD-ROM; use this for installation and recovery media |
| 7 | Usually unused in the Indy setup; selectable as a target in Prismatic |

These are placement conventions, not enforced device types. Prismatic currently
supports hard disks and CD-ROMs; the tape and floptical entries describe the
original hardware. The Indy has one narrow SCSI bus shared by its internal bays
and external connector, so each attached device needs a unique ID. ID 0 is the
Indy's controller address; the common host-adapter-at-ID-7 convention on other
SCSI systems does not apply here.

CDs are read-only. Filesystems are interpreted by the guest; the frontend
passes their sectors through unchanged. Audio/multitrack CHDs, CUE sheets and
raw 2352-byte CD sectors need conversion to a supported data image first.

Hard disks always use **persistent copy-on-write**: raw images use adjacent
`.overlay` and `.overlay.dirty` files; CHDs use `.diff.chd`. Uncompressed CHDs
without a stored SHA-1 use `.diff.chd.overlay` and `.diff.chd.overlay.dirty`
instead. Both CHD paths honor Iris's `IRIS_CHD_DIFF_DIR` override.
These files hold guest changes across launches;
keep them with their base images. The image directory must allow creation of
the sidecars. Do not share writable sidecars between simultaneous instances.
Extensions exposes confirmed offline Commit/Reset actions (see above).

Older builds could report `InvalidFile` after creating an empty `.diff.chd`
beside a CHD without a hash. Prismatic checks that this old diff contains no
allocated hunks, leaves it untouched, and uses the sparse overlay. A damaged or
nonempty old diff is reported for recovery instead of being ignored. Sparse
CHD overlays cannot currently be committed through Iris or Prismatic.

**Shut down the guest OS before restarting or quitting.**
Saved Media changes replace the machine; Extensions' explicit CD actions swap
the live tray after you unmount it in the guest. After stopping
the CPU and devices, the worker flushes attached storage before replacement
or exit. With media attached it waits for that flush while SDL keeps pumping
events; flush failures are reported and prevent a replacement machine.

The bottom layout follows 1989: a centered machine description in red followed
by grey shortcut hints on a compact 16-pixel band, above a 22-pixel LED bar.
These are logical dimensions and scale with the window. The LED bar remains
visible when the optional status hints are hidden. F9 uses the siblings'
compact top-left panel, sized to the current section; confirmations are centered.

| LED | Function | Active color |
| --- | --- | --- |
| RUN | Machine CPU running | Blue |
| CPU | CPU activity | White |
| NET | Network activity | Yellow |
| HD1–HD7 | Hard-disk reads | Green |
| CD1–CD7 | CD-ROM reads | Cyan |
| XFR1–XFR7 | Scratch transfer disk | Green; amber on writes |
| HD write activity | Writes on the corresponding disk LED | Amber |

Idle lamps retain a dim version of their function's color. Disk/CD lamps appear
only for attached targets. Pulses remain visible briefly even when no new guest
frame is drawn. Opening an overlay dims both bottom bands consistently.

Each CPU LED follows its own cycle counter and goes idle independently when
that processor stops executing.

When many targets would overfill the LED strip, storage lamps are grouped by
controller and device type (for example `HD1:*`). They still show actual reads
and writes, with separate disk/CD colors and amber writes.

## Networking

Enable **General > Tinker**, then open the **Network** tab in F9. Every model
has Ethernet, using Iris's NAT gateway by default. Network settings belong to
the selected machine label and are saved in `prismatic.cfg`. Save validates the
draft. NAT subnet/port forwards/bind scope and the PCAP interface apply live
after a successful Save. Backend, MAC and service-folder/protocol changes ask
before restarting an already booted machine; it returns to PAUSED, ready for
Ctrl+P. Discard and failed saves leave the running network unchanged. Hiding
Tinker keeps saved network settings active.

| Control | Behavior |
| --- | --- |
| Network backend | NAT, optional PCAP bridge, or Off on O2/Octane. Indy/Indigo2 cannot disconnect Ethernet in the current core. |
| NAT subnet | Enter an IPv4 network/prefix, such as `192.168.5.0/24`. Gateway/DNS is network + 1 and the guest address is network + 2; the menu shows both and the netmask. |
| PCAP host interface | Choose an enumerated host interface, Automatic, or enter a name/index. Available in PCAP mode. |
| NFS shared folder | Choose or enter an existing host folder, or Disable. The guest has **read/write** access to this folder. |
| NFS protocol | Auto, V2 or V3. Auto accepts either; IRIX 5.3 uses v2. |
| NFS bridge IP | An unused LAN IPv4 address for the built-in NFS server when using PCAP. Required if NFS sharing is enabled in PCAP mode. |
| TFTP boot folder | Choose an existing folder to serve **read-only** at the NAT gateway, or Disable. |
| Port forwards | Enter comma-separated `tcp:HOST:GUEST` or `udp:HOST:GUEST` rules. For example, `tcp:2323:23, tcp:2222:22`. Empty clears the rules. |
| Forward bind scope | Localhost (default), or all host interfaces to allow LAN access to forwarded ports. Applies to all rules. |
| Guest Ethernet MAC | Optional unicast MAC, such as `02:00:00:00:00:22`; clear to use Iris's default. Requires restart. Existing Indy/Indigo2 NVRAM can retain its previous station address. |
| RSH file folder | Existing folder served read-only by Iris's NAT RSH service, or Disable. Supports the core's limited file commands, without executing a host shell. |

Text fields initially select the current value: typing replaces it, Ctrl+A
selects all, Backspace edits, Enter accepts into the draft and Escape cancels.
F9 saves the whole draft. Relative folder paths use the directory containing
`prismatic.cfg`; folder checks run in the background and never create folders.
Different windows need different host forwarding ports.

Changing the NAT subnet clears existing NAT connections and does not rewrite the guest's saved IP address, route or DNS
settings. Configure the guest to match the addresses shown in Network. With
the default subnet, mount an enabled NFS share inside IRIX with:

```sh
mkdir /shared
mount 192.168.0.1:/ /shared
```

Iris serves NFS over UDP. A PCAP guest mounts `<NFS bridge IP>:/` instead.
For TFTP booting, place the appropriate boot files in the selected folder and
use the guest PROM's `bootp()` command; for example, O2 accepts
`boot -f bootp():FILE`. The guest/PROM must have a suitable network address.

PCAP is an optional build feature. On Fedora, build it with:

```sh
sudo dnf install libpcap-devel
cargo run --release --locked --features pcap
```

PCAP requires host capture permission (on Linux, appropriate raw-packet
capabilities) and uses the LAN's addresses and DNS. Wired interfaces work best;
many Wi-Fi networks reject an additional guest MAC address. A missing feature
or failed capture is reported rather than silently switching to NAT. Prismatic
does not change host permissions. Live updates are requests to Iris; interface
failures are reported, and listener failures appear in Console debug output.
NAT port forwards, TFTP and RSH are inactive in
PCAP mode; their saved settings are retained for switching back. For further
guest setup, see [our Iris networking guide](https://github.com/salvogendut/iris/blob/xiri/NETWORKING.md).

For an unnamed machine, the configuration uses `[network]`; named machines
use `[machines."LABEL".network]`:

```toml
[machines."Indy IRIX".network]
mode = "nat"
nat_subnet = "192.168.5.0/24"
nfs_dir = "/path/to/shared"
nfs_version = "auto"
tftp_dir = "/path/to/bootfiles"
rsh_dir = "/path/to/read-only-files"
mac = "02:00:00:00:00:22"
forward_lan = false

[[machines."Indy IRIX".network.forwards]]
protocol = "tcp"
host_port = 2323
guest_port = 23
```

Omitting the folder keys disables those services. PCAP configurations also
accept `pcap_interface` and, for sharing, `nfs_pcap_ip`.

## Guest audio

Guest playback is enabled by default for Indy/Indigo2 (HAL2), O2 (MACE) and
Octane (RAD), using SDL3's default playback device. **F9 > General > Guest audio**
mutes/unmutes playback; **Guest volume** cycles down in 10% steps from 100% to
0%, then returns to 100%. Save applies the changes without restarting; Discard
keeps the current sound settings. These are shared desktop preferences across
machine labels. Muting keeps the guest's audio hardware enabled and does not
mute the screenshot shutter.

Pause, power-off, reset and label switching clear queued guest sound. Audio
buffers are bounded; when the UI falls behind, old samples are discarded to
avoid delayed playback. If opening the host device fails, emulation continues
and a notification explains how to retry: save Guest audio Off, then On.
The SDL default device is reopened after a new machine starts or audio is
retried. Host microphone recording is not connected.

## Persistent configuration

**`prismatic.cfg`** contains the current saved General, Media and display
configuration, including Advanced options. It is created on first launch in
SDL's per-user application directory, normally **`~/.local/share/prismatic/prismatic.cfg`** on Linux, and loaded
on subsequent launches. Every successful menu Save atomically updates this
file. Ctrl+Plus/Minus also save the window scale immediately. Menu Cancel/Discard
do not change the saved configuration, and restart changes are saved only
after confirmation. The path is printed at startup when Debug output is Console.

For a configuration in the current directory, run:

```sh
cargo run --release --locked -- --config ./prismatic.cfg
```

The file uses human-readable TOML with `[machine]`, `[advanced]`, `[network]`, `[audio]`, `[gif]` and one table per
attached device, for example:

```toml
scanlines = false
show_status = true
smoothing = false
tinker = false
debug_output = "off"
window_scale = 1.0 # 0.75 through 4.0, in steps of 0.25.

[audio]
muted = false
volume = 100 # 0 through 100 percent.

[machine]
profile = "indy_ip24"
cpu = "r4400"
ram_mb = 64
keyboard = "ansi"

[advanced]
resolution = "guest"
cache = "auto"
clock_offset_years = 0
scsi_deferred_interrupts = true
serial_logging = "off"

[advanced.expert]
# count_mhz = 33.0       # Omit for the model default.
idle_parking = true     # Effective only with the idle-pause build feature.
thread_affinity = false
# cpu_core = 2          # Omit a role to let the host scheduler choose.
jit_threads = 1         # Requires jitv2 to change.
jit_cache = false
monitor_port = 0        # Automatic loopback port.
serial_port_a = 0
serial_port_b = 0
serial_a = "tcp"
serial_b = "tcp"
# gdb_port = 1234       # Omit to disable; 0 chooses an automatic port.
# vnc_port = 5900
vnc_max_fps = 30
# boot_elf = "tests/loop.elf"
trace_on_boot = false   # Requires perfetto.
trace_categories = "all"
trace_guest_clock = false

[gif]
width = 0 # Native guest resolution; alternatively 320, 480 or 640.
fps = 25
encoder = "built_in"

[media.devices.1]
kind = "disk"
path = "/path/to/irix.chd"

[media.devices.4]
kind = "cdrom"
path = "/path/to/install.efs"
```

Relative image paths resolve beside the configuration file; menu selections
are saved as absolute paths. With the default location, an older
`prismatic.toml` is migrated if `prismatic.cfg` is absent; the old file is
retained. Older files without `[advanced]` receive the defaults above.
Invalid configurations report an error and are never replaced with
defaults. NVRAM/EEPROM remain separate machine-state files.

## Checks

For the managed Iris checkout selected in the Makefile:

```sh
make test-bootstrap
make check FEATURES=
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software make run ARGS=--smoke-test
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software make run ARGS='--boot-smoke-test --screenshot /tmp/prismatic-prom.bmp'
```

Repository, branch and feature overrides also apply to these targets. Use
`make check` to check the selected optional features as well. For direct Cargo
development against your maintained sibling `../iris` checkout:

```sh
cargo fmt -p prismatic --check
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo test --locked
# Omit perfetto on Iris revisions affected by the Odyssey issue above:
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

`--smoke-test` renders three UI frames with the options open and exits without
starting Iris. `--boot-smoke-test` uses fresh temporary NVRAM, waits for a
nonuniform guest framebuffer, renders for ten more seconds, optionally captures
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

## Acknowledgments

Thank you to [techomancer](https://github.com/techomancer) and the contributors
to [Iris](https://github.com/techomancer/iris) for developing the SGI emulation
core that powers Prismatic. Their emulator, hardware research and documentation
provide the foundation for [our fork](https://github.com/salvogendut/iris) and
its integration with the Happy Years Rust/SDL3 interface.

Thank you also to [Renato Tarabella](https://github.com/RenatoTarabella) and the
[SGIEmu project](https://github.com/RenatoTarabella/SGIEmu). SGIEmu's JIT register
caching and real-time clock caching techniques inspired the optional performance
optimizations adapted into our Iris fork, now available in Prismatic through
**Advanced > Performance > SGIEmu optimizations**.
