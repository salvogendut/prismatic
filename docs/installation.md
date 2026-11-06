# Network installation helper

[Back to Prismatic](../README.md) · [Network settings](networking.md)

Prismatic can start [Instigator](https://github.com/jamesbraid/instigator) for
one machine label and connect Iris to its private Ethernet segment. Instigator
serves your SGI installation images through BOOTP, TFTP and RSH. The guest
installer still runs inside the emulated SGI. This first version accepts an
Instigator setup folder; it does not yet assemble installation profiles or
drive partitioning and installer prompts for you.

## Bundled helper and optional builds

Normal `make` builds, Linux RPM/DEB packages, Windows ZIPs, macOS apps and the
Flatpak include Instigator. Select **Native Instigator** and leave **Instigator
executable** at **Automatic (bundled / PATH)**. An explicit absolute path or
another executable name still overrides this default. Existing saved paths
remain unchanged; enter `instigator` to return to automatic discovery.

Source builds fetch `INSTIGATOR_REPO` / `INSTIGATOR_BRANCH` from the Makefile
(default: James Braid's repository, `main`) and record the exact commit alongside
Iris. A checksum-pinned Go toolchain is downloaded into the build cache; no
system Go or Podman installation is needed. The source bundle places the helper
at `target/release/instigator`. Updates occur during builds, never during boot.

Optional standalone targets remain available:

```sh
make install-server         # Native helper at target/install-server/instigator
make install-server-image   # Alternate Linux container backend; requires Podman
```

The image command prints its revision-specific image name; copy that into the
Installation setting if using Podman. Boot uses `--pull=never`. Managed native
servers use Unix sockets on Linux/macOS and a temporary loopback TCP port on
Windows. Neither mode needs privileged host BOOTP/TFTP/RSH ports.

## Prepare a setup folder

Create a folder containing `instigator.yaml` and your media. Start from
[the example](../packaging/install-server/instigator.example.yaml), then edit
its file paths, client MAC/IP and installation sets. Media paths should be
relative to the setup folder, such as `media/overlays1.iso`; filenames may
contain spaces. The Podman backend sees this folder read-only at `/install`.
Files and symlink targets outside it are not mounted. Use local media for this
backend: its host network is disabled. Archive caches go in the temporary
runtime directory; do not set `cache_dir` to the read-only setup folder.

The example illustrates IRIX 6.5.30 media for an O2 or Octane. Select media
appropriate for the emulated hardware: Indy and Indigo2 need an earlier
supported release, such as 6.5.22. Verify the actual PROM Ethernet MAC with
`printenv eaddr`; an existing NVRAM image may retain an address different from
the Network menu's default. Match it in `clients` in the YAML configuration.

For Indy/IP22, use [the 6.5.22 example](../packaging/install-server/instigator-6.5.22.example.yaml).
It keeps the overlay discs in separate sets because the tested images contain
conflicting `.prereq_hints` files. This avoids choosing an arbitrary winner
when merging them. The primary set still supplies the boot programs.

For native checks outside Prismatic, run the helper **from the setup folder**:

```sh
/absolute/path/to/instigator check --full instigator.yaml
```

See [Instigator's configuration guide](https://github.com/jamesbraid/instigator/blob/main/docs/configuration.md)
for distribution merging, conflicts, additional CDs and installer selections.
Its example profile must be adapted to the images you actually have.

## Configure Prismatic

1. Save a separate machine label in General. Choose its hardware and attach a
   suitable empty disk through Media. Keep graphics enabled during installation
   so the installer sees the emulated graphics hardware. Choose **PROM boot >
   Keep existing** while installing; presets for an installed OS can interfere
   with the installer. This choice does not clear previously saved PROM variables.
2. Enable **General > Tinker**, then open **Network > Installation**. Tab and
   Shift+Tab switch between Connection and Installation.
3. Choose **Native Instigator** or **Temporary Podman container**. Select the
   setup folder. For Native, keep **Automatic (bundled / PATH)**, or enter an
   explicit executable path. For Podman, enter the locally built image name.
4. Select **Use installation network**, then **F9** to save. This selects Stream
   on Connection. A running machine requires restart confirmation and returns
   to the normal pre-boot PAUSED state.
5. Press **Ctrl+P**. Prismatic runs `instigator check --full`, starts the private
   server, then boots Iris. The check has a 120-second timeout; server startup
   has a 30-second timeout. Closing Prismatic cancels startup.
6. Open **Installation status** to see the helper state and recent messages.
   Escape during the PROM startup enters its maintenance menu. Use its command
   monitor and the server/client addresses and installation-set paths from
   `instigator.yaml` to follow the
   [Instigator installation guide](https://github.com/jamesbraid/instigator/blob/main/docs/install.md).
7. After installation, shut down the guest, select NAT on Connection and Save.
   The helper stops with the old machine; the installed disk remains attached.

Save checks paths and persists settings before closing; it never starts a
helper. Full media assembly is checked at boot. Discard has no process or
network side effects. Settings are saved per label, for example:

```toml
[machines."O2 install".network]
mode = "stream"

[machines."O2 install".network.installation]
backend = "native"
folder = "/home/user/IRIX setup"
program = "instigator"
```

Each boot gets a separate runtime directory, socket and container name. The
server lives until that machine stops, including across pauses. Reset starts a
fresh helper. A failed server is reported in Installation status; it does not
prove installation completion or automatically restart the guest. Upstream
Instigator exits when its emulator connection closes. Native checkpoints are
disabled with a stream peer because the external server cannot be restored.

## External servers and packaging

To manage Instigator yourself, leave **Install server > Off / external server**,
choose **Network backend > Stream**, and enter a **Stream endpoint**, such as
`unix:/tmp/irix-install.sock` or `tcp:127.0.0.1:7000`. Start the server first:

```sh
instigator serve --network-tcp 127.0.0.1:7000 instigator.yaml
```

The peer supplies all network services. Iris NAT, NFS, TFTP, RSH and port
forwarding are inactive on this segment, but their saved settings are retained.
DaynaPort cannot join it. Builds whose Iris sources lack stream networking
retain these preferences and grey out the unavailable controls.

The managed Podman backend runs on native Linux. It uses no host network,
no added capabilities, a read-only root filesystem and read-only media. The
temporary runtime directory is writable. SELinux labelling is disabled for
this container to avoid relabelling your source media; user and network
namespaces still apply. The image includes Instigator's licence and notices.

The Flatpak uses its bundled native helper inside the sandbox and does not
launch host Podman. Keep the setup folder and media in an allowed location
(such as Downloads), or grant access as described in the [Flatpak guide](flatpak.md).
An external TCP stream server remains an option.

## Install IRIX

Follow [Installing IRIX step by step](installing-irix.md) for media preparation,
saved machine settings, blank-disk creation, PROM network boot, `fx`, Inst
package selection and the first desktop boot. It covers Indy / 6.5.22 and
O2 / 6.5.30 and Octane / 6.5.30, including the tested workarounds and
disk-overlay backup rules. See [installation errors and workarounds](irix-installation-troubleshooting.md)
for exact failure messages and recovery steps.

## Validation status

Tests cover saved-label isolation, Save/Discard, endpoint validation, bounded
logs, failed checks, cancellation and stream packet transport. Optional tests
exercise the real helper with synthetic media over Iris's BOOTP/TFTP path,
using both Unix sockets and loopback TCP;
see [testing](testing.md#installation-helper-checks). Linux bundling checks also
cover RPM contents, Flatpak export, and a diskless PROM boot in the Platform
runtime using automatic helper discovery and synthetic media. Windows x86_64
and macOS Intel/ARM helpers cross-compile successfully; native execution checks
run in release CI. Full IRIX installation on those hosts remains unverified.

Local 6.5.22 and 6.5.30 installation sets have passed Instigator's full media
check. With Iris `xiri` at `fb24628`, a diskless Indy R4400 with 128 MB reached
the serial PROM monitor, fetched `6.5.22/stand/fx.ARCS` through BOOTP/TFTP and
ran `fx` to its safe read-only prompt. For the example addresses, the commands
were:

```text
setenv netaddr 192.168.0.2
boot -f bootp():/6.5.22/stand/fx.ARCS
```

No hard disk was attached to that initial test. Subsequent tests with the same
core completed the following checks:

- A fresh 4 GiB disk was partitioned and formatted as XFS. Inst installed the
  standard system and applications from the five network distribution sets,
  reported successful installation, and completed requickstarting and kernel
  configuration. This run initially copied the miniroot from Overlay 1 CD.
- The installed system reached a root login and reported `6.5 6.5.22f` from
  `uname -R`. Hardware inventory showed R4400, 128 MB and Indy 24-bit graphics.
- A subsequent cold boot with NAT, no CD and the helper disabled reached the
  graphical login manager and the 4Dwm desktop. NAT assigned `192.168.0.2`.
- A separate fresh disk verified the corrected network-only `sashARCS`
  sequence: miniroot copy and kernel boot succeeded with no CD attached,
  reaching the filesystem-creation prompt. A second full package installation
  was not repeated on that disk.

The O2 R5000 test used 256 MB RAM and a fresh 8 GiB raw disk. With the same
core and PROM 4.18, the network-only bootstrap loaded `sashARCS`, copied the
miniroot, and booted `unix.IP32`. Inst completed installation, requickstarting
and kernel configuration without dependency overrides. `uname -R` reported
`6.5 6.5.30f`; hardware inventory showed R5000, 256 MB and CRM graphics. A
subsequent cold boot with NAT and the helper disabled reached the graphical
login manager and 4Dwm desktop, with guest address `192.168.0.2`. No CD was
attached at any stage. The final XFS root used about 1.23 GiB of its 7.87 GiB.
Another cold boot confirmed PROM autoboot. A graphical `halt -y` returned to
firmware with a corrupted display; its cause remains unconfirmed. Explicit
`shutdown -p -g0 -y` successfully powered off the O2 and Prismatic displayed
**Machine powered off**. Use that command for normal graphical shutdown.

An Octane test with Iris `xiri` at `b23f674` used one R10000, 512 MB RAM,
Solid IMPACT and a fresh 8 GiB raw disk at controller 0, ID 1. The bundled
4.9 PROM ran `fx.64` and copied the miniroot through `sash64`, but raised an
XUT exception when loading `unix.IP30`. A cold restart, helper restart after
an internal hardware reset, and direct network loading of `unix.IP30`
recovered the copied miniroot. No CD or replacement PROM was used.

Inst reported successful installation, completed requickstarting and built
the kernel without dependency overrides. The installed system reported
`IRIX64 ... 6.5 ... IP30` and `6.5 6.5.30f`; `hinv` confirmed one 195 MHz
R10000, 512 MB and SI graphics. XFS root used about 1.44 GiB of its 7.87 GiB.
A cold boot with NAT and the helper disabled reached graphical login and
the 4Dwm desktop. `gfxinfo` reported IMPACTSR, one GE, one RE, no TRAMs and
1280×1024 at 60 Hz. The guest used `192.168.0.2` and received all three
ping replies from the NAT gateway. The RAD timer warning remained; audio
was not validated. The [troubleshooting document](irix-installation-troubleshooting.md)
records the boot failures and the native-helper recovery in detail.
Setting `AutoLoad` through the PROM then passed another cold autoboot to X.
Explicit `shutdown -p -g0 -y` reached **Machine powered off** from both
serial and graphical sessions.

These real-media installations used the native helper; managed Podman
transport was checked separately with synthetic media. See the
[step-by-step guide](installing-irix.md) for the complete procedure.
