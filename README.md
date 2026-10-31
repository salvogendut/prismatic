# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for our [Iris fork](../iris),
following the desktop conventions of the Happy Years emulators
([1984](../1984), [1985](../1985), [1986](../1986), and [1989](../1989)).

Prismatic embeds Iris and starts a **diskless SGI Indy** by default, with an
R4400, 64 MB RAM, Newport graphics and Iris's built-in PROM. **F9 > General**
now configures the machine model, RAM, CPU and keyboard, including Indigo2 IP22.
The guest framebuffer
appears in the SDL3 window, with the Happy Years F9 overlay, fullscreen,
scanlines, filtering and saved desktop preferences. SDL keyboard and relative
mouse input now reach the guest through Iris's PS/2 controller.

**F9 > Media** attaches hard disks and CD-ROMs; bottom LEDs show machine, CPU,
network and disk activity. **Audio is not connected yet.** With fresh NVRAM and
no disk, the PROM normally displays "Unable to boot; press any key to continue".
Click the display to capture input, then press Enter to reach the PROM menu.
Choose **Enter Command Monitor** (5) and try `hinv` to inspect the emulated
hardware. Startup errors appear in the window
and terminal; confirmed quit stops the worker and saves the machine's NVRAM before closing.

## Build and run

Keep the checkouts alongside one another:

```text
Dev/
  iris/        # our Iris fork
  prismatic/
```

Prismatic uses the local Iris library directly, including the fork's
`workstation::set_exit_on_guest_poweroff` embedding API. Use Iris branch
`xiri/netbsd-newport` at commit `53c9fb2` or newer and update both checkouts. Its root Cargo manifest also
preserves the fork's patched winit and block dependencies. Install Rust through
rustup; `rust-toolchain.toml` selects nightly, matching Iris's requirement.
On a Fedora development system, install the native build dependencies:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config sdl3-devel alsa-lib-devel clang-devel
cargo run --release --locked
```

The current Iris library still compiles its winit/glutin, CPAL and camera
modules. Prismatic does not open Iris's winit window, and audio is disabled
for this milestone; libclang is needed to build the Linux camera bindings.
SDL is linked dynamically through the [`sdl3` Rust bindings](https://github.com/vhspace/sdl3-rs).
SDL_ttf and a separate widget toolkit are not required. Linux is the first
validated platform; Windows and macOS integration builds remain to be checked.

No guest disk or external PROM is needed. The default is diskless; only images
selected in Prismatic are attached. It does not read `iris.toml`, `prom.bin`,
or default SCSI image paths from either checkout. Normal machine state is kept under
SDL's per-user application directory, normally
`~/.config/prismatic/indy/` on Linux, or `~/.config/prismatic/indigo2/` for Indigo2.
The models have separate NVRAM and EEPROM files. `--state-dir PATH` keeps the
existing Indy layout at `PATH`; Indigo2 uses `PATH/indigo2/` when overridden.
The Iris monitor binds an ephemeral loopback port,
printed in the terminal; serial ports also use ephemeral loopback ports.
Iris also maintains its shared draw-shape cache at
`~/.iris/rex-jit-profile.bin`; `IRIS_REX_JIT_PROFILE` overrides that cache path.

See [the integration design](docs/architecture.md) for module boundaries and
remaining work.

## Controls

| Key | Action |
| --- | --- |
| Click guest display | Capture keyboard and mouse; the capture click is consumed |
| Ctrl+Enter | Release input and return the pointer to the desktop |
| F9 | Open options; save and close an open options session |
| Left / Right (in options/dialogs) | Change section or confirmation choice |
| Up / Down (in options) | Select a row |
| Enter (in options/dialogs) | Cycle a value, toggle an option or accept a confirmation |
| Escape (in options/dialogs) | Close; offer Save / Discard / Cancel if changed |
| F11 | Toggle fullscreen |
| F12 / window close | Confirm quit |
| Shift+F9 / Shift+F11 / Shift+F12 (captured) | Send a bare guest function-key tap |

While captured, other keys, including Escape and the remaining function keys,
go to the guest. Opening options or a quit confirmation, losing focus, or
stopping the machine releases capture and any held guest keys/buttons. Click
the display again to resume input. The host shortcuts remain available when
input is released. Shift+reserved-function-key sends one unmodified tap, not
a held key or a Shift combination.

Keyboard input follows **physical key positions**, preserving left/right
modifiers and the numeric keypad. General > Keyboard selects Iris's physical
keyboard type (default: ANSI). `--keyboard ansi|iso|jis` overrides the saved
choice for the session; saving options also persists that choice. Use ISO for
a European keyboard. Character layout is determined by the guest. Text paste and IME
input are not implemented. Left, middle and right mouse buttons are forwarded;
wheel and extra buttons work when the guest enables the corresponding PS/2
mouse protocol.

Enable **General > Tinker** to expose Advanced, including Iris core options
and bilinear filtering. Hiding Advanced retains its saved settings. Options
are edited in a private draft and applied only after a successful save. Cancelling or discarding leaves the live display
unchanged. Confirmation dialogs initially select Cancel.

## Machine settings

In **F9 > General**, select a row with Up/Down and press Enter to cycle its
value. F9 saves. Hardware changes ask for **Restart / Cancel** before they are
saved and applied; choose Restart with Left/Right, then Enter. Cancel or Escape
returns to the draft. Escape from the editor offers Save / Discard / Cancel.

| Setting | Available values |
| --- | --- |
| Machine | SGI Indy IP24, SGI Indigo2 IP22 |
| RAM (Indy) | 32, 64, 128, 256 MB |
| RAM (Indigo2) | 32, 64, 128, 256, 384, 512 MB |
| CPU (Indy) | R4400, R5000 |
| CPU (Indigo2) | R4400 |
| Keyboard | ANSI, ISO, JIS |

Changing models keeps compatible CPU/RAM choices, selects R4400 when changing
from an R5000 Indy to Indigo2, and caps RAM at 256 MB when returning to Indy.
These hardware settings take effect by stopping the old machine and starting a
fresh one, so the current guest session is lost. The window stays responsive
during restart. Display options apply on Save without restarting. Startup or
restart failures remain visible in the window and terminal; F9 lets you revise
the saved settings. A new worker is never started after a stop timeout.

Both models use Newport graphics and bundled firmware. Prismatic explicitly
passes a temporary copy of Iris's bundled Indigo2 PROM to avoid Iris's alternate
ROM search in the working directory. IMPACT, O2 and Octane need other display
integrations and are not selectable yet.

## Advanced settings

Enable **F9 > General > Tinker**, then use Left/Right to open **Advanced**.
Up/Down selects a row and Enter cycles its value. F9 saves; core changes use
the same **Restart / Cancel** confirmation as General and Media. Cancel keeps
the draft, and Discard leaves the running machine and saved file unchanged.
Turning Tinker off hides the controls without resetting their values.

| Setting | Choices | Default |
| --- | --- | --- |
| Guest resolution | Guest-controlled, 1024×768, 1280×960, 1280×1024 | Guest-controlled |
| CPU cache model | Auto, Full, Shadow | Auto |
| Guest clock offset | Host time or −5/−10/−15/−20/−25/−30 years | Host time |
| SCSI interrupt timing | Deferred, Immediate | Deferred |
| Serial console log | Off, Raw, Timestamped (UTC) | Off |
| Bilinear filtering | Off, On; applies without restarting | Off |

Resolution selects the **initial Newport video mode**; the PROM or guest may
program another mode later. Cache Auto selects Iris's implementation for the
build (Full in the current interpreter build). This is a cache model control,
not a JIT switch. Deferred SCSI interrupts are Iris's BSD-compatible default.

The clock offset shifts the RTC's host-time seed at each boot; time then runs
normally and the guest can still set its clock. It is a relative offset, not a
fixed date. The menu uses five-year presets; `clock_offset_years` in the file
also accepts individual whole years from −30 through 0. Iris clamps RTC dates
to the emulated chip's supported range (1970–2039).

Serial logging appends channel-B console output to **`serial.log`** beside the
selected machine's NVRAM: normally `~/.config/prismatic/indy/serial.log` or
`~/.config/prismatic/indigo2/serial.log`. `--state-dir` moves the log with the
machine state. Raw preserves serial bytes; Timestamped prefixes lines with
host UTC timestamps. The graphical display is not transcribed. Existing logs
are preserved, and no log is created merely by editing or discarding a draft.
An unopenable log is reported as a startup error.

## Media and activity LEDs

Open **F9 > Media**, select a SCSI target (controller 0, IDs 1–7), and press
Enter. Choose **Hard disk**, **CD-ROM**, **Disconnect** or **Cancel**. The
native file chooser accepts CHD and raw images, including `.img`, `.raw`,
`.dsk`, `.hdd`, `.iso` and `.efs`. Choose the device type explicitly: an EFS
CD image may have an `.img` extension. Selection edits the draft; **F9 Save**
validates the images and asks to restart before applying a media change.
Cancel retains the draft; Discard leaves the current devices unchanged.

| Device | Supported storage |
| --- | --- |
| Hard disk | Raw images with 512-byte sectors; hard-disk CHDs with 512-byte sectors |
| CD-ROM | Cooked raw data images containing ISO9660 or EFS; single-track Mode 1 CD CHDs |

CDs are read-only. Filesystems are interpreted by the guest; the frontend
passes their sectors through unchanged. Audio/multitrack CHDs, CUE sheets and
raw 2352-byte CD sectors need conversion to a supported data image first.

Hard disks always use **persistent copy-on-write**: raw images use adjacent
`.overlay` and `.overlay.dirty` files; CHDs use `.diff.chd` (or Iris's
`IRIS_CHD_DIFF_DIR` override). These files hold guest changes across launches;
keep them with their base images. The image directory must allow creation of
the sidecars. Do not share writable sidecars between simultaneous instances.
Prismatic does not expose merge/reset controls yet.

**Shut down the guest OS before restarting, changing media or quitting.**
Restart is a fresh boot, and media changes are not hot swaps. After stopping
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
| HD write activity | Writes on the corresponding disk LED | Amber |

Idle lamps retain a dim version of their function's color. Disk/CD lamps appear
only for attached targets. Pulses remain visible briefly even when no new guest
frame is drawn. Opening an overlay dims both bottom bands consistently.

## Persistent configuration

**`prismatic.cfg`** contains the current saved General, Media and display
configuration, including Advanced options. It is created on first launch in
SDL's per-user application directory, normally **`~/.config/prismatic/prismatic.cfg`** on Linux, and loaded
on subsequent launches. Every successful menu Save atomically updates this
file. Cancel/Discard do not change it, and restart changes are saved only
after confirmation. The path is printed at startup.

For a configuration in the current directory, run:

```sh
cargo run --release --locked -- --config ./prismatic.cfg
```

The file uses human-readable TOML with `[machine]`, `[advanced]` and one table per
attached device, for example:

```toml
scanlines = false
show_status = true
smoothing = false
tinker = false

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

```sh
cargo fmt -p prismatic --check
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo test --locked
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --smoke-test
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --boot-smoke-test --screenshot /tmp/prismatic-prom.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --input-smoke-test --screenshot /tmp/prismatic-input.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --settings-smoke-test --screenshot /tmp/prismatic-settings.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --media-smoke-test --screenshot /tmp/prismatic-media.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --advanced-smoke-test --screenshot /tmp/prismatic-advanced.bmp
```

`--smoke-test` renders three UI frames with the options open and exits without
starting Iris. `--boot-smoke-test` uses fresh temporary NVRAM, waits for a
nonuniform guest framebuffer, renders for ten more seconds, optionally captures
the SDL output, and stops Iris. It fails if startup, display output (120-second
timeout), or shutdown fails. This checks the display path; it does not identify
PROM text or prove that a complete OS boot works.

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
detection and retained settings when Tinker is hidden. Linux is validated; other platforms still need integration checks.
