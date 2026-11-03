# Extensions: hardware, storage, devices and checkpoints

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Hardware](#hardware)
- [Storage](#storage)
- [Optional devices](#optional-devices)
- [Experimental checkpoints](#experimental-checkpoints)

Enable **General > Tinker** and open **Extensions** with Left/Right. Its topic
sub-tabs are **Hardware**, **Storage**, **Devices** and **Checkpoints**. Click a
sub-tab or use **Tab / Shift+Tab** to switch topics; Left/Right switches main
sections. Switching topics keeps pending edits and selects the first row.

## Hardware

On the **Hardware** sub-tab, Enter cycles **Graphics board**, **Display heads** and **CPU count** through the options
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

## Storage

### CD changer and disk actions

On **Extensions > Storage**, cycle **Storage action target** to a configured disk or CD-ROM
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

Scratch drive settings and import/export actions are in **Extensions > Storage**.

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


## Optional devices

The **Devices** sub-tab exposes Iris's VINO, DaynaPort and Ultra64 backends on the
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

## Experimental checkpoints

The **Checkpoints** sub-tab provides **Checkpoint name**, **Save**, **Restore** and
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

Related: [machine models](machines.md#supported-models), [media and disk overlays](media.md), [networking](networking.md).
