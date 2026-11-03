# Media and activity LEDs

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Attaching disks and CDs](#attaching-disks-and-cds)
- [SCSI controllers](#scsi-controllers)
- [Supported images](#supported-images)
- [SCSI ID conventions](#scsi-id-conventions)
- [Disk overlays and safe changes](#disk-overlays-and-safe-changes)
- [Activity LEDs](#activity-leds)

## Attaching disks and CDs

Open **F9 > Media**, select a SCSI target, and press Enter. Choose **Hard disk**, **CD-ROM**, **Disconnect** or **Cancel**. The
native file chooser accepts CHD and raw images, including `.img`, `.raw`,
`.dsk`, `.hdd`, `.iso` and `.efs`. Choose the device type explicitly: an EFS
CD image may have an `.img` extension. Selection edits the draft; **F9 Save**
validates the images and asks to restart before applying a media change.
Cancel retains the draft; Discard leaves the current devices unchanged.

## SCSI controllers

The **SCSI controller** row at the bottom switches between buses where Iris
supports two controllers: IP22, O2 and Octane. Indy and IP28 currently expose
one. Each bus offers IDs 1–7, or 1–15 on Octane; ID 0 is reserved for the host
adapter. The same target ID may be used on different buses. Config keys encode
`controller * 16 + target`: `[media.devices.17]` is controller 1, target 1,
and `[machines."Octane".media.devices.31]` is controller 1, target 15 in that
label. Existing keys 1–7 retain their controller-0 meaning.

Changing models never silently disconnects media. Unsupported assignments stay
visible as **Unavailable**, including a bus selector when needed to reach them.
Save reports the incompatibility until you disconnect them or select a compatible
model. The same writable disk cannot occupy two targets, even across buses.

## Supported images

| Device | Supported storage |
| --- | --- |
| Hard disk | Raw images with 512-byte sectors; hard-disk CHDs with 512-byte sectors |
| CD-ROM | Cooked raw data images containing ISO9660 or EFS; single-track Mode 1 CD CHDs |

CDs are read-only. Filesystems are interpreted by the guest; the frontend
passes their sectors through unchanged. Audio/multitrack CHDs, CUE sheets and
raw 2352-byte CD sectors need conversion to a supported data image first.

## SCSI ID conventions

The Media menu labels primary-bus targets with these Indy-style suggested roles
and explains the selected role in the hint area. Additional buses and wider-bus
IDs use **General use**. These labels guide placement across machine profiles;
your guest OS may use different assignments.

| ID | Conventional assignment |
| --- | --- |
| 0 | Onboard controller; reserved, explained in the hints and not selectable |
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

## Disk overlays and safe changes

Hard disks always use **persistent copy-on-write**: raw images use adjacent
`.overlay` and `.overlay.dirty` files; CHDs use `.diff.chd`. Uncompressed CHDs
without a stored SHA-1 use `.diff.chd.overlay` and `.diff.chd.overlay.dirty`
instead. Both CHD paths honor Iris's `IRIS_CHD_DIFF_DIR` override.
These files hold guest changes across launches;
keep them with their base images. The image directory must allow creation of
the sidecars. Do not share writable sidecars between simultaneous instances.
Extensions exposes confirmed offline [Commit/Reset actions](extensions.md#cd-changer-and-disk-actions).

Older builds could report `InvalidFile` after creating an empty `.diff.chd`
beside a CHD without a hash. Prismatic checks that this old diff contains no
allocated hunks, leaves it untouched, and uses the sparse overlay. A damaged or
nonempty old diff is reported for recovery instead of being ignored. Sparse
CHD overlays cannot currently be committed through Iris or Prismatic.

**Shut down the guest OS before restarting or quitting.**
Saved Media changes replace the machine; Extensions' explicit CD actions swap
the live tray after you unmount it in the guest. After stopping
the CPU and devices, the worker flushes attached storage before replacement
or exit. With media attached it waits for that flush while SDL keeps pumping
events; flush failures are reported and prevent a replacement machine.

## Activity LEDs

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
| XFR1–XFR7 | Scratch transfer disk | Green; amber on writes |
| HD write activity | Writes on the corresponding disk LED | Amber |

Idle lamps retain a dim version of their function's color. Disk/CD lamps appear
only for attached targets. Pulses remain visible briefly even when no new guest
frame is drawn. Opening an overlay dims both bottom bands consistently.

Each CPU LED follows its own cycle counter and goes idle independently when
that processor stops executing.

When many targets would overfill the LED strip, storage lamps are grouped by
controller and device type (for example `HD1:*`). They still show actual reads
and writes, with separate disk/CD colors and amber writes.

Related: [CD changer and disk actions](extensions.md#cd-changer-and-disk-actions), [scratch transfer drive](extensions.md#scratch-transfer-drive), [machine settings](machines.md).
