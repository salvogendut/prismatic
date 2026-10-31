# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for our [Iris fork](https://github.com/salvogendut/iris),
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
network and disk activity. **Guest audio is not connected yet**; screenshots
play the shared Happy Years shutter sound through SDL3. With fresh NVRAM and
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

Keep the checkouts alongside one another:

```text
Dev/
  iris/        # our Iris fork
  prismatic/
```

Prismatic uses the local Iris library directly, including the fork's power-off
and diagnostic-routing embedding APIs. Use Iris branch `xiri/netbsd-newport`
at commit `0382f6b` or newer and update both checkouts. Its root Cargo manifest
also preserves the fork's patched winit and block dependencies. Install Rust through
rustup; `rust-toolchain.toml` selects nightly, matching Iris's requirement.
On a Fedora development system, install the native build dependencies:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config sdl3-devel alsa-lib-devel clang-devel
cargo run --release --locked
```

The current Iris library still compiles its winit/glutin, CPAL and camera
modules. Prismatic does not open Iris's winit window, and guest audio is disabled
for this milestone; libclang is needed to build the Linux camera bindings.
SDL is linked dynamically through the [`sdl3` Rust bindings](https://github.com/vhspace/sdl3-rs).
SDL_ttf and a separate widget toolkit are not required. Linux is the first
validated platform; Windows and macOS integration builds remain to be checked.

The default Indy needs no guest disk or external PROM. O2 and Octane need an
external PROM image selected in General (see below). The default is diskless; only images
selected in Prismatic are attached. It does not read `iris.toml`, `prom.bin`,
or default SCSI image paths from either checkout. Normal machine state is kept under
SDL's per-user application directory, normally
`~/.local/share/prismatic/` on Linux. Each model has its own directory: `indy/`,
`indigo2/`, `indigo2-ip28/`, `o2/` or `octane/`, containing its NVRAM/EEPROM or
persistent flash. `--state-dir PATH` keeps the existing Indy layout at `PATH`;
other models use their named subdirectory under `PATH`. Firmware source files
are read-only; O2 saves flash in `nvram.bin`, Octane in `flash.bin` alongside its
`nvram.bin`. Existing flash preserves the guest's firmware and environment.
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

Enable **General > Tinker** to expose Advanced, including Iris core options,
bilinear filtering, debug output and GIF capture settings. Hiding Advanced retains its saved settings. Options
are edited in a private draft and applied only after a successful save. Cancelling or discarding leaves the live display
unchanged. Confirmation dialogs initially select Cancel.

## Screenshots and GIF capture

**F4** saves the guest framebuffer at its native resolution as a PPM image,
matching 1989. **F6** starts recording; press it again to finish the GIF. These
shortcuts work while input is captured and while menus are open. Captures contain
the guest display and guest cursor, without the frontend menus, debug panel,
scanlines, notifications or bottom bars. A guest frame must have arrived first.

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

| Machine | CPUs | RAM presets (MB) | Graphics | PROM |
| --- | --- | --- | --- | --- |
| Indy IP24 | R4400, R5000 | 32, 64, 128, 256 | Newport | Bundled |
| Indigo2 IP22 | R4400 | 32, 64, 128, 256, 384, 512 | Newport | Bundled |
| Indigo2 IMPACT IP28 | R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | Solid IMPACT | Bundled |
| O2 IP32 | R5000, R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | CRIME/GBE | External IP32 PROM |
| Octane IP30 | R10000 (one CPU) | 64, 128, 256, 384, 512, 768, 1024, 1536, 2048 | Solid IMPACT | External IP30 PROM |

All models offer ANSI, ISO and JIS keyboard layouts. Octane's second CPU,
additional graphics boards and second SCSI buses are not exposed yet.

Changing models keeps compatible CPU/RAM choices, selects R4400 when changing
from an R5000 Indy to Indigo2, and caps RAM at 256 MB when returning to Indy.
These hardware settings take effect by stopping the old machine and preparing a
fresh, paused configuration, so the current guest session is lost. Ctrl+P boots it.
Before the first boot, options can change without restarting a machine. The
window stays responsive
during restart. Display options apply on Save without restarting. Startup or
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

Each label remembers its machine model, CPU, RAM, keyboard, firmware, media and
Advanced machine options. Desktop preferences, capture options and Tinker remain
shared. F2 cycles labels alphabetically, and the status strip shows the active
label and its state. Close Options before using Ctrl+P. F5 asks before cold-resetting
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

### O2 and Octane firmware

Select the machine, then **General > PROM firmware > Choose file**. Choose the
extracted `.bin` image, press F9, and confirm restart. Paths are remembered
separately as `o2_prom` and `octane_prom` under `[machine]` in `prismatic.cfg`.
Relative paths in a hand-written config resolve beside that config. File selection
and Save check readability and size on a worker without creating machine state;
Save rejects a missing or invalid-sized PROM and keeps the draft open. Cancelling
the picker or discarding the draft leaves the running machine untouched. These
checks do not authenticate firmware contents or guarantee a successful guest boot.

Iris recommends the 512 KiB O2 4.18 PROM. Octane accepts an IP30 flash image up to
1 MiB, including the 879,312-byte IRIX `IP30prom.bin`; Prismatic accepts images
from 512 KiB to 1 MiB. MAME documents these full flash dumps:

| Model | File | SHA-1 |
| --- | --- | --- |
| O2 | `ip32prom.rev4.18.bin` | `f2cfa7246d67f88fe5490e40dac6c04b1deb4d28` |
| Octane | `ip30prom.rev4.9.bin` | `de250875c608add63749d3f9fb81a82cb58c3586` |

References: [MAME O2 firmware definitions](https://github.com/mamedev/mame/blob/master/src/mame/sgi/o2.cpp),
[MAME Octane firmware definition](https://github.com/mamedev/mame/blob/master/src/mame/sgi/octane.cpp).
Neither external PROM is bundled in Iris or Prismatic.

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
| Debug output | Off, Screen, Console; applies without restarting | Off |
| GIF resolution | Native guest resolution, or 320/480/640 pixels wide | Native |
| GIF frame rate | 10, 20, 25 fps | 25 |
| GIF encoder | Built-in, FFmpeg optimize (if available) | Built-in |

Debug output routes Iris's routine startup, bus-probe, Newport register/display
and draw-cache diagnostics. **Screen** shows the latest 12 wrapped lines above
the bottom strips; **Console** writes to the launching terminal. Changes apply
on Save, without restarting; Discard preserves the active destination. Screen
starts with new messages when enabled, and Off clears the screen log. To capture
startup diagnostics, save Screen or Console before the next launch.
Startup/storage failures still reach the terminal with debug output Off.
This setting is separate from guest serial logging and Iris monitor tracing.

On IP28, O2 and Octane, video mode is guest-controlled and the cache is fixed to
Shadow. O2 and Octane also use their controllers' SCSI timing and host RTC time;
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

Serial logging appends channel-B console output to **`serial.log`** beside the
selected machine's NVRAM: normally `~/.local/share/prismatic/indy/serial.log` or
`~/.local/share/prismatic/indigo2/serial.log`. `--state-dir` moves the log with the
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

Use the usual Indy SCSI assignments when setting up media:

| ID | Conventional assignment |
| --- | --- |
| 0 | Onboard controller; reserved and absent from the Media menu |
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
Prismatic does not expose merge/reset controls yet.

Older builds could report `InvalidFile` after creating an empty `.diff.chd`
beside a CHD without a hash. Prismatic checks that this old diff contains no
allocated hunks, leaves it untouched, and uses the sparse overlay. A damaged or
nonempty old diff is reported for recovery instead of being ignored. Sparse
CHD overlays cannot currently be committed through the Iris monitor.

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
SDL's per-user application directory, normally **`~/.local/share/prismatic/prismatic.cfg`** on Linux, and loaded
on subsequent launches. Every successful menu Save atomically updates this
file. Ctrl+Plus/Minus also save the window scale immediately. Menu Cancel/Discard
do not change the saved configuration, and restart changes are saved only
after confirmation. The path is printed at startup when Debug output is Console.

For a configuration in the current directory, run:

```sh
cargo run --release --locked -- --config ./prismatic.cfg
```

The file uses human-readable TOML with `[machine]`, `[advanced]`, `[gif]` and one table per
attached device, for example:

```toml
scanlines = false
show_status = true
smoothing = false
tinker = false
debug_output = "off"
window_scale = 1.0 # 0.75 through 4.0, in steps of 0.25.

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

`cargo test --locked` also checks named-label persistence, concurrent catalog
saves, instance locks, draft isolation and pause/reset/resume. The IP22 power-off
regression writes its panel register separately from a monitor halt, using
temporary state and no guest disks.

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
mouse and hardware inventory checked at 128 MB. O2/Octane have synthetic-ROM
startup, media attachment and shutdown coverage; real PROM/OS boots in Prismatic
still need verification with external firmware. These checks do not establish
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
detection and retained settings when Tinker is hidden. Linux is validated; other platforms still need integration checks.
