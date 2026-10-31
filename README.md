# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for our [Iris fork](../iris),
following the desktop conventions of the Happy Years emulators
([1984](../1984), [1985](../1985), [1986](../1986), and [1989](../1989)).

This initial build is a runnable **frontend foundation**. It displays an explicitly
labeled preview, with an SDL3 window, aspect-preserving texture presentation,
fullscreen, scanlines, filtering, an F9 options overlay, and saved desktop
preferences. **It does not boot Iris yet.** Machine controls, guest input, audio,
and media management are the next integration work.

## Build and run

Install a current stable Rust toolchain and SDL3 development libraries. On a
Fedora development system, the native packages are `gcc` and `sdl3-devel`.
SDL is linked dynamically through the [`sdl3` Rust bindings](https://github.com/vhspace/sdl3-rs).
The current lockfile uses bindings for SDL 3.4; this shell has been exercised
with SDL 3.4.16. SDL_ttf and a separate widget toolkit are not required.

```sh
cargo run --locked
```

The shell currently builds independently of `../iris`. Direct core integration
will use Iris's nightly Rust requirements and keep the fork as a separate
dependency; see [the integration design](docs/architecture.md).

## Controls

| Key | Action |
| --- | --- |
| F9 | Open options; save and close an open options session |
| Left / Right | Change section or confirmation choice |
| Up / Down | Select a row |
| Enter | Toggle an option or accept a confirmation |
| Escape | Close options; offer Save / Discard / Cancel if changed |
| F11 | Toggle fullscreen |
| F12 / window close | Confirm quit |

Enable **General > Tinker** to expose Advanced, which currently contains
bilinear filtering. Options are edited in a private draft and applied only
after a successful save. Cancelling or discarding leaves the live display
unchanged. Confirmation dialogs initially select Cancel.

Preferences use SDL's per-user application directory, normally
`~/.config/prismatic/prismatic.toml` on Linux. Override the path with
`--config PATH`. These are frontend preferences, separate from Iris machine
configuration. Invalid files produce an error instead of being overwritten.
No disk images, firmware, or Iris configuration are opened by this build.

## Checks

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software cargo run --locked -- --smoke-test
```

The smoke test renders three frames with the options open and exits. It uses
temporary preferences unless `--config` is supplied, and never saves options
automatically. Add `--screenshot /tmp/prismatic.bmp` to capture the rendered
window. Unit tests cover settings persistence, draft isolation, confirmation
behavior, the Tinker gate, and display proportions.
