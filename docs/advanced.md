# Advanced settings

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Core settings](#core-settings)
- [Performance](#performance)
- [Connections and serial terminals](#connections-and-serial-terminals)
- [Capture](#capture)
- [Developer tools and tracing](#developer-tools-and-tracing)
- [Diagnostic output](#diagnostic-output)
- [Core behavior and serial logging](#core-behavior-and-serial-logging)

Enable **F9 > General > Tinker**, then use Left/Right to open **Advanced**.
Up/Down selects a row and Enter cycles its value. F9 saves; core changes use
the same **Restart / Cancel** confirmation as General and Media. Cancel keeps
the draft, and Discard leaves the running machine and saved file unchanged.
Turning Tinker off hides the controls without resetting their values.

The topic sub-tabs are **Core**, **Performance**, **Connections**, **Capture** and
**Developer**. Click a sub-tab or use **Tab / Shift+Tab** to switch topics;
Left/Right still switches main sections. Switching topics keeps pending edits
and selects the first row. Enter opens a text editor for numeric values and paths;
Ctrl+A replaces the value, Enter accepts the draft, and Escape cancels.

## Core settings

| Setting | Choices | Default |
| --- | --- | --- |
| Guest resolution | Guest-controlled, 1024×768, 1280×960, 1280×1024 | Guest-controlled |
| CPU cache model | Auto, Full, Shadow | Auto |
| Guest clock offset | Host time or −5/−10/−15/−20/−25/−30 years | Host time |
| SCSI interrupt timing | Deferred, Immediate | Deferred |
| Serial console log | Off, Raw, Timestamped (UTC) | Off |
| Bilinear filtering | Off, On; applies without restarting | Off |
| Debug output | Off, Screen, Console; applies without restarting | Off |

## Performance

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

## Connections and serial terminals

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

The integrated `xiri` core fixes the older performance branch's in-process JIT
restart panic (see [the integration notes](architecture.md#performance-configuration)).
If using a core without that fix, configure the channels before the first
Ctrl+P boot, or quit and relaunch after saving routing changes to a running
machine. The saved routes work on a fresh launch.

## Capture

The **Capture** sub-tab contains the shared GIF preferences. Changes apply
without restarting a machine and take effect on the next recording.

| Setting | Choices | Default |
| --- | --- | --- |
| GIF resolution | Native guest resolution, or 320/480/640 pixels wide | Native |
| GIF frame rate | 10, 20, 25 fps | 25 |
| GIF encoder | Built-in, FFmpeg optimize (if available) | Built-in |

Use F4 for a screenshot and F6 to start or stop recording. See
[Screenshots and GIF capture](usage.md#screenshots-and-gif-capture) for output
locations and recording behavior.

## Developer tools and tracing

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
make run FEATURES=pcap,idle-pause,perfetto
# Enable the CPU JIT; opt in to SGIEmu caches through Advanced > Performance:
make run FEATURES=pcap,idle-pause,perfetto,jitv2
# Also enable the experimental CPU JIT and developer monitor instrumentation:
# This diagnostic build disables the GPR cache.
make run FEATURES=pcap,idle-pause,perfetto,jitv2,developer
```

Performance, connection and developer setting changes use the normal Save and
restart confirmation. Draft validation checks ELF format, host core IDs and
build support without starting servers or creating cache/trace files. Discard
has no runtime effects. Settings persist in `[advanced.expert]` and each named
machine's `[machines."Label".advanced.expert]` stanza. Existing configs keep their
previous behavior; optional remote services, boot ELF and tracing default off.

## Diagnostic output

Debug output routes Iris's routine startup, bus-probe, Newport register/display
and draw-cache diagnostics. **Screen** shows the latest 12 wrapped lines above
the bottom strips; **Console** writes to the launching terminal. Changes apply
on Save, without restarting; Discard preserves the active destination. Screen
starts with new messages when enabled, and Off clears the screen log. To capture
startup diagnostics, save Screen or Console before the next launch.
Startup/storage failures still reach the terminal with debug output Off.
This setting is separate from guest serial logging and Iris monitor tracing.

## Core behavior and serial logging

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

Related: [building and features](building.md), [PROM console settings](machines.md#general-and-prom-settings), [GIF capture](usage.md#screenshots-and-gif-capture), [network services](networking.md).
