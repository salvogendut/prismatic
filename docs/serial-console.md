# Headless guests and serial consoles

Prismatic keeps its SDL control window when a machine is headless. The guest
uses a serial console, with no attached physical keyboard or mouse and no
frontend graphics display. Optional graphics boards are omitted; integrated
chips such as O2 GBE remain part of the emulated motherboard. CPU and device
emulation still run in IRIS.

This feature requires a Unix host and an IRIS build with the headless-device
and serial-backend APIs. General greys out the controls when those APIs are
missing and preserves saved preferences. Windows builds retain their existing
TCP serial routing. The console UI and terminal behavior are adapted from
[Heliacal](https://github.com/salvogendut/heliacal).

## Start a headless machine

1. Open **F9 > General** and select or save a machine label.
2. Turn **Headless guest** on. Keyboard and PROM console become fixed to
   disconnected and serial while this mode is selected. Saved graphical
   preferences remain available when headless mode is turned off.
3. Optionally edit **Serial PTY alias**, for example `/tmp/prismatic-indy`.
   The parent directory must already exist. Relative paths in configuration
   files are resolved relative to that file.
4. Save the draft. The machine remains at the normal pre-boot PAUSED screen
   until **Ctrl+P**. For a running machine, save and confirm its restart only
   after shutting down the guest safely.
5. Boot with **Ctrl+P**. Prismatic displays the serial terminal, the allocated
   `/dev/pts/N`, and the configured alias if present.

The kernel assigns the `/dev/pts/N` number; it cannot be selected in advance.
The alias supplies a stable, customizable name. Aliases never overwrite a
regular file, a foreign symlink, or another running instance's endpoint. On
shutdown Prismatic removes only the symlink it owns. The small persistent
`<alias>.prismatic-lock` coordination file prevents races between instances.
Cold reset creates a new PTY; reconnect the external terminal after reset.

The headless console uses serial B on Indy/Indigo2, serial A on O2/Octane and
Origin 200/2000/Onyx2, and the selected platform console on Fuel, Tezro and
Origin 3200/Onyx 3000. Headless mode overrides the route for that one line;
other serial routes retain their Advanced settings. Legacy PROMs are set to
`console=d` using IRIS's existing firmware-state API. Boot-OS presets and
other firmware variables are preserved. Turning headless off selects graphics
when the saved PROM policy was "Keep existing", so the earlier `console=d`
does not leave a blank window. An explicit Serial policy remains serial.

CLI overrides are also available:

```sh
prismatic --machine-label 'Serial Indy' --headless --serial-pty-link /tmp/prismatic-indy
```

`--graphical` selects graphical mode for this launch. CLI alias paths are
relative to the launching directory; menu Save makes overrides persistent.

## Terminal input, history and clipboard

Click the terminal or press **Ctrl+Enter** to focus keyboard input. The mouse
stays free. Printable text uses SDL text input and the host keyboard layout;
Enter, Backspace, arrows, editing keys and control characters are sent as
serial bytes. **Ctrl+C** remains a guest interrupt. Use `TERM=vt100` in the
guest for terminal-aware applications.

- Drag over text and press **Ctrl+Shift+C** to copy it. With no selection,
  that shortcut copies the minicom command.
- **Ctrl+Shift+V** pastes into the focused console. Newlines become Enter,
  tabs and UTF-8 are preserved, other control characters are filtered, and
  guest-selected bracketed paste is honored. Pastes over 4 KiB are rejected
  whole, including any bracketed-paste delimiters.
- Use the mouse wheel, scrollbar, or **Shift+PageUp/PageDown** to browse up
  to 10,000 lines. **Shift+Home** goes to the oldest retained line;
  **Shift+End** returns to live output. Typing and pasting also go live.
- A selection freezes its displayed page while new output arrives. History
  remains readable while paused and after guest power-off. A new machine
  starts a new history.
- **Ctrl+Enter**, opening a menu, pausing, or losing window focus releases
  keyboard focus and discards queued embedded input. F2/F4/F5/F6/F9/F11/F12
  remain frontend shortcuts; Shift allows guest function keys.

The terminal supports an ANSI/VT subset with colors, cursor movement, erase
commands, wrapping and alternate screens. The SDL debug font displays ASCII;
unsupported glyphs appear as `?`, but copied text retains its Unicode content.
Guest terminal escape sequences cannot read or write the host clipboard.

## External terminal

Click **Copy command**, then paste the command into a host terminal:

```sh
minicom -o -8 -b 9600 -D '/tmp/prismatic-indy'
```

Use 9600 baud, 8 data bits, no parity, one stop bit, and disable hardware and
software flow control in the terminal. The embedded terminal and external
client receive the same output independently. Both can send input; use one
input source at a time to avoid interleaved commands. Quitting minicom leaves
the guest running and the endpoint supports reconnecting.

Queues and scrollback are bounded. Slow/disconnected external readers cannot
block emulation; the UI reports discarded output bytes. Serial devices stay
inside IRIS, and the frontend never substitutes physical keyboard events for
serial input. A PTY is visible only in its host namespace: run the external
terminal in the same distrobox/Flatpak namespace when necessary.
