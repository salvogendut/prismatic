# Machine settings and saved labels

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [General and PROM settings](#general-and-prom-settings)
- [Supported models](#supported-models)
- [Boot, reset and power-off](#boot-reset-and-power-off)
- [Named machines](#named-machines)
- [O2 and Octane firmware](#o2-and-octane-firmware)

## General and PROM settings

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
The original five machine profiles have offline PROM-preset support; this does not imply
that either guest OS boots on every model. The seven newer platforms use their
firmware console for OS boot variables; the preset controls are greyed out. Unrecognized or damaged firmware
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
use minicom with the [PTY instructions](advanced.md#connections-and-serial-terminals). Use distinct fixed TCP ports for
simultaneously running labels. Serial output appears in that terminal; the
Prismatic guest display may remain blank. The overlay remains available to
switch back to Graphics.

## Supported models

| Machine | CPUs | RAM presets (MB) | Graphics | PROM |
| --- | --- | --- | --- | --- |
| Indy IP24 | R4400, R5000 | 32, 64, 128, 256 | Newport, XZ | Bundled |
| Indigo2 IP22 | R4400, R5000 | 32, 64, 128, 256, 384, 512 | Newport, XZ, Extreme, Solid/High/Maximum IMPACT | Bundled |
| Indigo2 IMPACT IP28 | R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | Solid/High/Maximum IMPACT | Bundled |
| O2 IP32 | R5000, R10000 | 32, 64, 128, 256, 384, 512, 768, 1024 | CRIME/GBE | Bundled 4.18; optional external IP32 PROM |
| Octane IP30 | R10000, R12000, R14000 (one or two CPUs) | 64, 128, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 6144, 8192 | Solid/High/Maximum IMPACT; VPro V6/V8/V10/V12; optional serial-only | Bundled 4.9; optional external IP30 PROM |

| Origin 200 IP29 | R10000, R12000; one node | 128–2048 per node | Serial only | Bundled IP27; optional override |
| Origin 2000 IP27 | R10000, R12000; one or two nodes | 128–2048 per node | Serial only | Bundled IP27; optional override |
| Onyx2 IP27 | R10000, R12000; one or two nodes | 128–2048 per node | IR2/IR2E, one or two RMs; optional serial-only | Bundled IP27; optional override |
| Fuel IP34 | R14000 500 MHz | 512 | V10/V12; optional serial-only | Bundled; optional override |
| Tezro IP53 | R16000; one, two or four CPUs, depending on part | 1024–8192 | V12; optional serial-only | Bundled; optional override |
| Origin 3200 IP35 | R12000; one or two C-bricks | 512–4096 per brick | Serial only | Bundled; optional override |
| Onyx 3000 IP35 | R12000; graphics selects one or two C-bricks | 512–4096 per brick | IR3 or V12 | Bundled; optional override |

**Extensions > Hardware** selects an exact processor part, CPU count and node
population. General retains the simpler CPU-family selector. The status strip
has a separate lamp for each processor. Each IP27 node or IP35 C-brick currently
contains one CPU in IRIS. RAM is **per node/brick** on those platforms; the status
summary shows the total. Core validation checks board, processor and memory
combinations before saving. The 180 MHz R10000 uses two 128 MB banks instead of
a 256 MB bank, which its PROM disables.

Onyx 3000 uses one C-brick with IR3 and two with V12. Fuel's current core requires
exactly two 256 MB banks. Tezro's 800/900 MHz parts require four CPUs; its 1 GHz
part permits two or four. The default 700 MHz part permits one, two or four.
The new models are integration targets: configuration and startup checks do not
establish complete IRIX, NetBSD or graphics-workload compatibility. The previously
verified O2/Octane IRIX installs remain documented in the installation guide.

Origin 3200 and Onyx 3000 use **Fibre Channel**, configured under **Extensions >
Disks**, rather than the SCSI Media slots. Serial-only machines keep the frontend
and overlays available without requiring a framebuffer. Open **Advanced >
Connections > Connection status** for the actual loopback ports or PTY paths.
Fuel, Tezro and IP35 bricks expose L1; Tezro also exposes channels C and D.
Choose the main logging console in **Extensions > Platform**. Persistent boot
flash, controller storage and graphics flash are isolated by machine label.

For Odyssey, choose **Octane IP30** in General, enable **Tinker**, then select
**Extensions > Hardware > Graphics board > VPro V6, V8, V10 or V12**, save, and boot. The selected card is saved per machine label and
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

## Boot, reset and power-off

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

## Named machines

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
make run ARGS='--machine-label "Indy IRIX"'
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

The settings in these guides apply when the linked Iris core exposes the
[required APIs](building.md#iris-compatibility). Greyed controls show their availability reason in the hint strip.

## O2 and Octane firmware

Current `xiri` and upstream cores embed O2 4.18 and Octane 4.9 as packed
release firmware. Leave **PROM firmware** at **Bundled with Iris**. On older
cores without embedded firmware, **Use bundled** is greyed out and an external
PROM is required before boot. Detection follows the core's available APIs.

A fresh Octane release image has no initialized persistent environment. Boot
once with **PROM boot > Keep existing** before applying an IRIX or NetBSD
preset. Existing saved flash remains usable and is never replaced by this
upgrade.

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

The embedded payloads are 431,616 bytes for O2 and 879,312 bytes for Octane;
the emulator pads them to their 512 KiB and 1 MiB flash capacities. The packed
images and version catalog live in `data/proms/` in
[our Iris fork](https://github.com/salvogendut/iris/tree/xiri).
Older cores embedded full flash dumps with initialized environment records.
The new release payloads therefore need not have the same hashes or initial
settings as those dumps.

Existing machine flash remains authoritative: switching firmware does not erase
O2's `nvram.bin` or Octane's `flash.bin`. Use a new machine label for a fresh
firmware boot while retaining the existing machine's state.

Related: [configuration and state files](configuration.md), [media](media.md), [graphics and optional devices](extensions.md).
