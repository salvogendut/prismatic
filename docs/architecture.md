# Frontend and Iris integration

## Design baseline

Prismatic uses Rust and SDL3. The guest display should occupy the window;
settings appear in an overlay. The reference is the native Happy Years
desktop UI, particularly 1989's draft settings and confirmation flow.
The first shell adopts the siblings' dark blue panels, amber selection,
compact bitmap text, function-key hints, F9 options, F11 fullscreen, and
F12 quit. Advanced controls are behind Tinker.

Rendering and host events belong to SDL on the main thread. UI state and
settings policy are independent of the renderer. The shell redraws on
events and notification expiry instead of continuously rendering an idle
screen. It presents a packed RGBA texture with the source aspect ratio
preserved; the current source is explicitly a preview.

| Module | Current responsibility |
| --- | --- |
| `src/main.rs` | CLI, SDL lifecycle, events, persistence handoff, smoke run |
| `src/overlay.rs` | Draft settings, navigation, modal choices and effects |
| `src/settings.rs` | Desktop preferences, validation and atomic file replacement |
| `src/display.rs` | Preview pixels and aspect-preserving placement |
| `src/view.rs` | SDL drawing, status strip, options, confirmations and notifications |

The options model emits a save request; the application closes the session
only after persistence succeeds. Discard requires no runtime undo operation.
There is no machine configuration model or emulation abstraction in this
shell yet. Those should be shaped around Iris's actual interfaces.

## Integration points inspected in the local fork

Iris already provides a Rust library. Use it directly, with a local path
dependency during development, rather than launching the existing GUI or
translating the core through C bindings. No Iris sources are copied into
this repository.

The following are findings from the current sibling checkout, not implemented
Prismatic features:

- `iris::workstation::build(MachineConfig)` constructs a boxed `Workstation`.
  Construction requires a worker thread with a large stack. The trait owns
  start/stop/reset, machine status, graphics heads and PS/2 access. Existing
  frontend sequencing is in `iris-gui/src/handle.rs`.
- `Workstation::get_display()` returns a `GfxDisplay`, whose `renderer_slot()`
  accepts `iris::dev::ng1::rex3::Renderer`. Install a capture renderer before
  starting the machine. Do not set `headless = true`: Iris needs its graphics
  devices alive even when its own window is not used.
- `iris::compositor::SwCompositor` and `Rex3Screen` provide software composition.
  `iris-gui/src/framebuffer.rs` shows how to pack the composited pixels into
  RGBA, account for the source stride, and omit the old Iris status bar.
  The renderer runs on the emulation refresh thread: publish the latest frame
  to a bounded shared slot; upload textures only on the SDL thread. Recreate
  textures for resolution changes and skip uploads for unchanged frames.
- `Workstation::get_ps2()` exposes keyboard and mouse injection. SDL physical
  scancodes need a dedicated translation to Iris's current key representation.
  Host shortcuts and modal keys must not leak into the guest. Release guest
  keys/buttons on focus loss, capture release and overlay entry. Choose the
  guest function-key escape convention before enabling host shortcuts that
  collide with SGI software.
- Iris's manifest currently includes winit/glutin and CPAL unconditionally.
  Calling its library without `Ui::run` avoids creating its winit window;
  it does not remove those dependencies or replace audio. A fully SDL3 host
  backend needs a feature boundary in the fork and a PCM sink for SDL audio.
  The fork currently uses nightly Rust; Prismatic's standalone shell does not.
- Iris has per-machine data and storage behavior, including persistent writes
  and overlays. Keep frontend preferences separate from machine TOML and
  preserve machine-relative paths. Never implicitly load the sibling's default
  config or start a guest just to preview the interface.

## Next milestones

1. Connect the local Iris dependency and implement the worker lifecycle plus
   frame capture. First validate an embedded PROM boot with no writable guest
   disks. Surface actual machine state and startup errors in the UI.
2. Add SDL keyboard/mouse capture, release handling and SDL audio. Feature-gate
   the existing host backends in Iris as needed, keeping device changes in
   the fork. Recheck build and input behavior on Linux, Windows and macOS.
3. Add General, Media, Extensions and Tinker-gated Advanced machine controls.
   Classify changes as live, next boot, or restart-required. Validate before
   applying; confirm disruptive changes. Use native media pickers, not path
   widgets that modify live disks while navigating.
4. Add the family's remaining actions: F4 screenshots, F5 confirmed reset,
   F6 recording, F7 pause, activity indicators and window persistence. Expose
   each only once its backend behavior exists.

A running workstation needs a guest-aware shutdown flow. CPU inactivity is
not proof that the guest has shut down. Preserve Iris's disk synchronization
and NVRAM behavior and test on disposable images before enabling writable
sessions. The current shell's quit dialog only governs frontend preferences.
