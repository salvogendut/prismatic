# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for our [Iris fork](../iris),
following the desktop conventions of the Happy Years emulators
([1984](../1984), [1985](../1985), [1986](../1986), and [1989](../1989)).

Prismatic now embeds Iris and starts a **diskless SGI Indy** with an R4400,
64 MB RAM, Newport graphics and Iris's built-in PROM. The guest framebuffer
appears in the SDL3 window, with the Happy Years F9 overlay, fullscreen,
scanlines, filtering and saved desktop preferences. SDL keyboard and relative
mouse input now reach the guest through Iris's PS/2 controller.

**Audio and media configuration are not connected yet.** With fresh NVRAM and
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

Prismatic uses the local Iris library directly. Its root Cargo manifest also
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

No guest disk or external PROM is needed. Prismatic constructs a diskless
configuration explicitly, without reading `iris.toml`, `prom.bin`, or default
SCSI image paths from either checkout. Normal machine state is kept under
SDL's per-user application directory, normally
`~/.config/prismatic/indy/` on Linux. Use `--state-dir PATH` for a separate
machine state directory. The Iris monitor binds an ephemeral loopback port,
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
| Enter (in options/dialogs) | Toggle an option or accept a confirmation |
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
modifiers and the numeric keypad. `--keyboard ansi|iso|jis` selects Iris's
physical keyboard type (default: ANSI); use `--keyboard iso` for a European
keyboard. Character layout is determined by the guest. Text paste and IME
input are not implemented. Left, middle and right mouse buttons are forwarded;
wheel and extra buttons work when the guest enables the corresponding PS/2
mouse protocol.

Enable **General > Tinker** to expose Advanced, which currently contains
bilinear filtering. Options are edited in a private draft and applied only
after a successful save. Cancelling or discarding leaves the live display
unchanged. Confirmation dialogs initially select Cancel.

Preferences use SDL's per-user application directory, normally
`~/.config/prismatic/prismatic.toml` on Linux. Override the path with
`--config PATH`. These are frontend preferences, separate from Iris machine
configuration. Invalid files produce an error instead of being overwritten.
The machine uses its embedded firmware and separate NVRAM files; no guest disk
images or Iris configuration files are opened.

## Checks

```sh
cargo fmt -p prismatic --check
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo test --locked
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --smoke-test
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --boot-smoke-test --screenshot /tmp/prismatic-prom.bmp
IRIS_REX_JIT_PROFILE=/tmp/prismatic-rex-profile.bin SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --input-smoke-test --screenshot /tmp/prismatic-input.bmp
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

The machine smoke commands redirect Iris's draw-shape cache to `/tmp` as well.
All tests use temporary preferences unless `--config` is supplied, and none
automatically saves frontend options. The boot and input tests always use
temporary machine state and reject `--state-dir`. A screenshot path is accepted
only with a smoke test. Unit tests cover draft settings, persistence,
confirmation behavior, framebuffer packing/stride, frame replacement, startup
failure, physical key mapping, host/modal routing, PS/2 releases, mouse motion
and scrolling, and pointer conversion for resized/high-DPI windows.
