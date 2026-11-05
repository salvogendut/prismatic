# Controls, captures and audio

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [First launch](#first-launch)
- [Controls](#controls)
- [Screenshots and GIF capture](#screenshots-and-gif-capture)
- [Guest audio](#guest-audio)

## First launch

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
| Tab / Shift+Tab (in Advanced/Network/Extensions) | Next / previous topic sub-tab; clicking a sub-tab also selects it |
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
Advanced groups its controls into **Core**, **Performance**, **Connections**,
**Capture** and **Developer** sub-tabs. Extensions groups them into **Hardware**,
**Storage**, **Devices** and **Checkpoints**. Network has **Connection** and
**Installation** topics for network services and the optional
[installation helper](installation.md). Switching topics retains pending
edits; returning to a main section remembers its topic until the overlay closes.
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

Enable **General > Tinker**, then open **Advanced > Capture** for GIF resolution,
frame rate and encoder.
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

Related: [machine settings](machines.md), [Advanced settings](advanced.md), [activity LEDs](media.md#activity-leds).
