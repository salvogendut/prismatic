# Installing IRIX step by step

[Back to Prismatic](../README.md) · [Installation helper reference](installation.md)

This guide installs IRIX from original CD images onto a **new disk** in
Prismatic. It uses a private network served by Instigator: no physical SGI,
host BOOTP service or host firewall changes are needed. Prismatic starts the
helper; you answer the SGI partitioning and installation prompts yourself.

The procedure adapts the [Tech-Pubs remote installation guide](https://tech-pubs.net/tutorials/remote-irix-6.5-installation-from-linux.html)
to Prismatic. The real-media tests use native Linux and the native Instigator
helper: Iris `xiri` at `fb24628` for Indy/O2 and `b23f674` for Octane.
Other host platforms need separate testing.

| Guest | Machine settings | New disk | Installation media |
| --- | --- | --- | --- |
| IRIX 6.5.22f | Indy IP24, R4400, 128 MB, Newport | 4096 MiB | 6.5.22 overlays and matching Applications |
| IRIX 6.5.30f | O2 IP32, R5000, 256 MB, integrated graphics | 8192 MiB | 6.5.30 overlays and matching Applications |
| IRIX 6.5.30f | Octane IP30, one R10000, 512 MB, Solid IMPACT (SI) | 8192 MiB | 6.5.30 overlays and matching Applications |

All also need IRIX 6.5 Foundation 1 and Foundation 2. Use **6.5.22 for the
Indy**; 6.5.30 does not support it. Indy and O2 use **`fx.ARCS` and
`sashARCS`**. Octane uses **`fx.64` and `sash64`**. The Octane test needed
additional PROM and helper recovery steps, described below.

## 1. Build Prismatic and the helper

From the Prismatic repository, in your Rust/SDL3 development environment:

```sh
make
```

`make` fetches our Iris fork's `xiri` branch and Instigator's `main` branch,
then builds `target/release/prismatic` and `target/release/instigator`. The
required Go toolchain is downloaded into the private build cache; Podman is
not required. Release packages also include the helper. See [building](building.md)
for dependencies and manual Cargo builds.

The native helper is sufficient for this procedure. Select **Native Instigator**
and keep the executable at **Automatic (bundled / PATH)**. The optional Podman
backend is described in the [helper reference](installation.md). Real-media
validation here used native Linux; other hosts need separate installation tests.

## 2. Prepare the installation media

Create a separate setup folder for each release. Keep it outside `target/`
so cleaning build artifacts does not remove your media. For example:

```text
IRIX-setup/6.5.22/             # Or IRIX-setup/6.5.30/
  instigator.yaml
  media/
    overlays1.iso
    overlays2.iso
    overlays3.iso
    foundation1.iso
    foundation2.iso
    applications.iso
```

Copy the appropriate template to that folder as `instigator.yaml`:

| Release | Template | Applications disc used in testing |
| --- | --- | --- |
| 6.5.22 | [instigator-6.5.22.example.yaml](../packaging/install-server/instigator-6.5.22.example.yaml) | November 2003 |
| 6.5.30 | [instigator.example.yaml](../packaging/install-server/instigator.example.yaml) | August 2006 |

Place your original disc images under `media/`, or edit the template's
`source` paths to match their names. Names containing spaces are supported;
you do not need to rename the originals. Paths are relative to the setup
folder. For Podman, the images and any symlink targets must be inside that
folder because only that folder is mounted into the container.

Keep each release's overlays and Applications disc together. The 6.5.22
template exposes the overlay discs separately to avoid conflicting metadata;
the 6.5.30 template merges its three overlays into one distribution.
Foundation 1 and 2 form the `foundations` distribution in both templates.

Retain the example addresses initially:

- Server: `192.168.0.1`.
- Guest: `192.168.0.2`, netmask `255.255.255.0` (`/24`).
- Client MAC: `08:00:69:12:34:56`; verify it in step 6.

Check the setup **from inside its folder**, using the absolute helper path:

```sh
cd /absolute/path/to/IRIX-setup/6.5.22
/absolute/path/to/prismatic/target/release/instigator check --full instigator.yaml
```

Use the `6.5.30` folder for O2 or Octane. Correct missing paths or merge conflicts
before proceeding. The helper reads the images directly; do not extract or
modify their contents to install.

## 3. Create a separate machine configuration

Launch Prismatic. It starts **PAUSED**, so configure it before pressing
Ctrl+P. In **F9 > General**:

1. Select the model, CPU and RAM from the table above.
2. Select **PROM boot > Keep existing** during installation. The normal IRIX
   preset is for an already installed system and can interfere with miniroot boot.
3. Select **PROM console > Serial** for a terminal transcript and easy command
   entry. Graphics remains present in the emulated machine.
4. Enable **Tinker**.
5. Choose **Save machine as...**, enter a new label such as `O2 IRIX 6.5.30`,
   and save with **F9**. Use a new label to isolate its firmware and settings.

For Octane, retain one CPU and choose **Solid IMPACT** in **Extensions**.
The test used the bundled 4.9 PROM. Keep the graphics board present while
installing through the serial console so Inst selects its graphics software.

In **Advanced > Performance**, set **Idle parking > Off**. In the tested core,
idle parking could stall the Indy's serial `fx` polling loop. These installation
tests used it disabled, with JIT enabled. Leave the other performance settings
at their defaults. Save with F9.

In **Advanced > Connections**, choose TCP routing on the console channel:

| Machine | Console channel | Example fixed TCP port |
| --- | --- | --- |
| Indy | Serial B | 2323 |
| O2 | Serial A | 2324 |
| Octane | Serial A | 2325 |

Choose an unused port, or leave `auto` and read the assigned port from
**Connection status** after boot. Fixed ports must differ between simultaneous
machines. Optionally enable **Advanced > Core > Serial console log > Raw**.
Save before booting. A [pseudoterminal and minicom](advanced.md#connections-and-serial-terminals)
can be used instead of TCP.

## 4. Create and attach the blank disk

Open **F9 > Media**. On O2 or Octane, select **SCSI controller 0**. Choose **SCSI ID 1 >
New disk…**, then:

1. Select **Raw** for the tested path.
2. Enter **4096 MiB** for Indy or **8192 MiB** for O2/Octane.
3. Choose a new filename in a permanent machine folder outside `target/`.
4. Select **Create disk and attach to draft**, then **F9 Save**.

This disk is empty: it has no partitions or filesystem yet. The commands in
step 6 will initialize it and destroy anything already on the selected disk.
Confirm that controller 0, ID 1 refers to this **new** image. Disconnect any
unrelated disks from this installation label. No CD attachment is required
for the network boot procedure below.

See [disk creation](media.md#creating-an-empty-hard-disk) for CHD alternatives.
The completed installation must retain the raw image's `.overlay` and
`.overlay.dirty` sidecars as well as the base image.

## 5. Connect the private installation network

Open **F9 > Network > Installation**; Tab switches Network sub-tabs.

1. Set **Install server > Native Instigator**.
2. Set **Setup folder** to the folder containing your `instigator.yaml`.
3. Leave **Instigator executable** at **Automatic (bundled / PATH)**, or select
   an explicit helper path if using a manual Cargo build.
4. Select **Use installation network**. This chooses Stream on Connection.
5. Save with **F9**, then press **Ctrl+P** to start the machine and helper.

The helper performs a full media check before boot. **Installation status**
shows startup errors and recent messages. Iris's normal NAT services are
inactive on this private network; Instigator supplies BOOTP, TFTP and RSH.

Connect a host terminal to the configured serial port, for example:

```sh
telnet 127.0.0.1 2323          # Indy example
# telnet 127.0.0.1 2324        # O2 example
# telnet 127.0.0.1 2325        # Octane example
```

Use the actual port from Connection status if it was allocated automatically.
The serial terminal carries the guest's prompts; a blank Prismatic guest
display is normal with the serial console selected.

## 6. Partition the disk with fx

The following commands run **in the guest PROM**, not in a Linux shell.
Press Escape during startup to enter maintenance. If the blank disk produces
“Unable to boot; press any key to continue”, press Enter. Choose **5, Enter
Command Monitor**.

Check the guest MAC:

```text
printenv eaddr
```

It must match the client MAC in `instigator.yaml`. If it differs, correct
the YAML and restart from the PROM so the helper reloads it. Then enter:

```text
setenv netaddr 192.168.0.2
setenv AutoLoad No
unsetenv OSLoader
unsetenv OSLoadFilename
unsetenv OSLoadOptions
```

For Indy, load:

```text
bootp():/6.5.22/stand/fx.ARCS -x
```

For O2, load:

```text
bootp():/6.5.30/stand/fx.ARCS -x
```

For Octane, load:

```text
bootp():/6.5.30/stand/fx.64 -x
```

At `fx`'s prompts, accept **dksc**, controller **0**, drive **1** by pressing
Enter at each prompt. A warning that the new disk has no valid label is expected.

At `fx>`, follow this sequence, waiting for each prompt:

| Enter | Action |
| --- | --- |
| `r` | Open repartition menu |
| `ro` | Select root drive layout |
| Enter | Accept `xfs` |
| `yes` | Confirm repartitioning the new disk |
| `..` | Return to the main fx menu |
| `l` | Open label menu |
| `sy` | Write the disk label |
| `/exi` | Exit fx |

The tested default layout gives swap 128 MiB, reserves 2 MiB for the volume
header, and gives the remaining space to XFS root. Return to the PROM
maintenance menu and choose **5** again.

## 7. Boot the installation miniroot

Set the boot disk partitions:

```text
setenv SystemPartition scsi(0)disk(1)rdisk(0)partition(8)
setenv OSLoadPartition scsi(0)disk(1)rdisk(0)partition(0)
```

For Indy:

```text
setenv tapedevice bootp():/6.5.22/dist/sa
boot -f $tapedevice(sashARCS) --m
```

For O2:

```text
setenv tapedevice bootp():/6.5.30/dist/sa
boot -f $tapedevice(sashARCS) --m
```

Wait for **Copy complete**, the IRIX banner and the filesystem prompt.
The miniroot is copied to swap; the installation kernel loads over the network.
A warning about runtime symbols from a BOOTP kernel did not prevent installation.

For Octane, use this sequence instead:

```text
setenv tapedevice bootp():/6.5.30/dist/sa
boot -f $tapedevice(sash64) --m
```

In the Octane test, the copy completed but the PROM raised an XUT exception
when loading the kernel. After a cold restart and restarting the disconnected
helper, the following command booted the already-copied miniroot:

```text
bootp():/6.5.30/dist/miniroot/unix.IP30
```

Follow the [exact recovery steps](irix-installation-troubleshooting.md#octane-prom-exception-after-copying-the-miniroot)
if this occurs. Do not skip the miniroot copy.

For the newly partitioned disk, answer:

| Prompt | Answer |
| --- | --- |
| Make new file system on `/dev/dsk/realroot`? | `yes` |
| Are you sure? | `y` |
| Block size, 512 or 4096? | `4096` |

When prompted for networking, use a hostname such as `prismatic-indy` or
`prismatic-o2` or `prismatic-octane`, address `192.168.0.2`, and accept
`0xffffff00` for the netmask.
These prompts can appear before Inst or when opening the first distribution.

## 8. Load the software catalogs

At `Inst>`:

```text
set page_output off
```

For Indy:

```text
from 192.168.0.1:/6.5.22/dist
```

For O2 or Octane:

```text
from 192.168.0.1:/6.5.30/dist
```

Select **2, feature stream**, when asked. After reading the first catalog,
Inst repeatedly prompts **Install software from**. Enter the additional
paths below one at a time, waiting for each catalog to finish:

| Indy / 6.5.22 | O2 or Octane / 6.5.30 |
| --- | --- |
| `192.168.0.1:/overlays2/dist` | Already merged into `6.5.30/dist` |
| `192.168.0.1:/overlays3/dist` | Already merged into `6.5.30/dist` |
| `192.168.0.1:/foundations/dist` | `192.168.0.1:/foundations/dist` |
| `192.168.0.1:/applications/dist` | `192.168.0.1:/applications/dist` |
| `done` | `done` |

README and licence text scrolls automatically with `page_output off`. Do
not send `q` to dismiss it: at a subsequent prompt that could mean quit.
The distribution names above come from the supplied YAML templates.

Optional media can be added later. The O2 test also merged ONC3 NFS into the
foundations set using its `dist6.5` directory; this is optional and requires
adding a layer to your YAML. Development and Complementary Applications CDs
are not required for the standard desktop selection used here.

## 9. Select packages and install

Back at `Inst>`:

```text
keep *
install standard
conflicts
```

Resolve the conflicts shown for your media. In both tests, the optional
`java2_plugin.sw32.mozilla_freeware` and `inventor_dev.sw.base` selections
lacked prerequisites. Select their **Do not install** choices using the
numbers displayed by `conflicts`. Rerun `conflicts`; deselect the dependent
`inventor_dev.sw.lib` if it then appears.

For example, if the current screen labels those choices `1a` and `2a`, enter
`conflicts 1a 2a`. Numbers change as conflicts are resolved: read the current
list before entering them. If a core OS package is missing prerequisites,
check that you opened every required distribution instead of deselecting it.
Do not use `rulesoverride` to force an inconsistent install.

Once Inst reports **No conflicts**:

```text
go
```

Let copying, exit commands and dependency checks complete. Success is the
explicit message **Installations and removals were successful.** Then enter:

```text
quit
```

Wait for **Requickstarting ELF files** and **Automatically reconfiguring the
operating system** to finish. Requickstarting can remain at 99% for several
minutes. Do not reset or close Prismatic while it is working.

## 10. Boot and verify the installed system

At **Ready to restart the system. Restart?**, answer `y`. If the PROM stops
at maintenance, choose **1, Start System**. The installed system should reach
a serial login prompt. Log in as `root`; a fresh installation has no password.
Accept the default terminal type if asked, then check:

```text
uname -R
hinv
df -k /
ls -l /unix /usr/bin/X11/Xsgi /usr/gfx/gfxinit
```

Confirm the release, CPU, RAM, graphics inventory and XFS root filesystem.
The base system initially uses the `IRIS` hostname; the miniroot's hostname
does not necessarily become the installed system's hostname.

Enable the graphical window system and shut down cleanly:

```text
chkconfig windowsystem on
sync
halt -y
```

Wait for **Okay to power off the system now** or Prismatic's powered-off
state. Only then change these settings and Save:

1. **General > PROM boot > IRIX**.
2. **General > PROM console > Graphics**.
3. **Network > Connection > Network backend > NAT**.
4. **Network > Installation > Install server > Off / external server**.
5. Disconnect any installation CD attached for a fallback attempt.

Restart the machine when prompted, or quit and relaunch the same label.
Press **Ctrl+P**. If the PROM displays its menu, click the guest display to
capture input and press **1** to start the system. To enable automatic PROM
boot, enter its command monitor and use `setenv AutoLoad Yes`; the General
IRIX preset intentionally leaves this setting unchanged.

Wait for the graphical login manager, log in as `root`, and confirm the
4Dwm desktop appears. **Ctrl+Enter** releases captured input. NAT normally
assigns `192.168.0.2`; [port forwarding](networking.md#connect-from-the-host-to-the-guest-ssh)
can expose guest services on localhost after installation.

For normal shutdown from the graphical desktop, select **Power Off the
System** in IRIX's shutdown dialog, or run this as root:

```text
shutdown -p -g0 -y
```

Wait for Prismatic to show **Machine powered off** before closing it. This
command was verified on the O2 after a graphical login. A graphical `halt -y`
returned to firmware with a corrupted display in the O2 test; explicit
power-off avoids that path. The serial halt used earlier in the installation
completed normally, and a subsequent cold graphics boot worked.

Keep the base disk, **all** its
overlay sidecars, the saved configuration and its firmware state together
when backing up or moving the machine. See [configuration and state paths](configuration.md).

## Troubleshooting

See [IRIX installation errors and workarounds](irix-installation-troubleshooting.md)
for exact error messages, recovery steps and the Octane investigation.

| Symptom | What to check |
| --- | --- |
| Serial fx stops responding | Disable Advanced > Performance > Idle parking; the Indy test stalled with it enabled. |
| O2 rejects `fx.64` | Use `fx.ARCS` and `sashARCS` as shown above. |
| Network boot gets no response | Check Installation status, the PROM MAC against YAML, `netaddr`, and the selected Stream backend. |
| sash tries `/unix` on the empty disk | Keep existing does not clear old PROM settings. Clear `OSLoader`, `OSLoadFilename` and `OSLoadOptions` in step 6. |
| PROM Remote Directory menu fails | Use the explicit `sa(sashARCS)` command in step 7. An Indy menu-path attempt failed with a UTLB exception; the explicit command worked. |
| Many missing package prerequisites | Open all matching overlay, foundation and application catalogs before `install standard`. |
| Helper or Stream controls are grey | Rebuild against our current `xiri` sources; availability is detected from the core APIs. |
| O2 screen is corrupted after graphical halt | Use `shutdown -p -g0 -y` or IRIX's Power Off option for future shutdowns. The observed glitch was in the return to firmware; cold graphics boots and explicit power-off passed. |

For an **Indy-only fallback tested during the first run**, attach 6.5.22
Overlay 1 as a CD-ROM at controller 0, ID 6. After partitioning and clearing
the boot overrides, `boot -f dksc(0,6,8)sashARCS --m` copies its miniroot.
If that copy finishes but the kernel does not load, use
`bootp():/6.5.22/dist/miniroot/unix.IP22` to boot the copied miniroot. Continue
with the same network catalogs. Never skip the miniroot copy and try to boot
the kernel against an empty swap partition.

## Validation limits

The Indy 6.5.22f test completed installation and a cold graphical boot with
the helper and CD disconnected. That run used the CD miniroot fallback;
the corrected network-only miniroot sequence was separately verified on
another fresh disk. The O2 6.5.30f run used the network-only bootstrap,
completed package installation, and reached both a serial root login and the
4Dwm desktop after a cold boot with NAT and the helper disabled. No CD was
attached during that run.

The Octane 6.5.30f run used network media throughout, with a cold-PROM
kernel-load recovery and manual helper restart as described above. It
completed installation and reached a serial root login, then a cold graphical
boot and 4Dwm desktop with NAT and the helper disabled. All three tests
retained graphics hardware while using the serial console for installation.
See the [validation record](installation.md#validation-status) for details.
