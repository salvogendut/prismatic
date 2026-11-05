# Network installation helper

[Back to Prismatic](../README.md) · [Network settings](networking.md)

Prismatic can start [Instigator](https://github.com/jamesbraid/instigator) for
one machine label and connect Iris to its private Ethernet segment. Instigator
serves your SGI installation images through BOOTP, TFTP and RSH. The guest
installer still runs inside the emulated SGI. This first version accepts an
Instigator setup folder; it does not yet assemble installation profiles or
drive partitioning and installer prompts for you.

## Build the helper on Linux

With rootless Podman installed, run either:

```sh
make install-server-image   # Build the image for managed containers
make install-server         # Also extract a native Linux helper executable
```

Both commands fetch the source revision pinned in `tools/install_server.py`
into `target/install-server/source`. They use the pinned Go version in
`packaging/install-server/Containerfile` and leave your Iris checkout alone.
The native binary is `target/install-server/instigator`. The image name is
printed when the build finishes and matches the default Installation setting.
Downloads happen during this explicit build; boot uses `--pull=never`.

The helper is optional. Normal builds and NAT networking do not require Podman
or Go. A native Instigator build can also be selected on macOS. Managed servers
currently use Unix sockets, so Windows users must run an external server with
a loopback TCP endpoint. Native macOS and Windows attachment still need host
integration testing.

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
   so the installer sees the emulated graphics hardware.
2. Enable **General > Tinker**, then open **Network > Installation**. Tab and
   Shift+Tab switch between Connection and Installation.
3. Choose **Native Instigator** or **Temporary Podman container**. Select the
   setup folder. For Native, enter the executable's absolute path or a command
   name on PATH. For Podman, retain the locally built image name.
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
backend = "podman"
folder = "/home/user/IRIX setup"
program = "instigator"
image = "localhost/prismatic-instigator:2db7231aa257"
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

Flatpak does not currently launch host Podman. Use a native helper executable
accessible inside the sandbox, or an external TCP stream server. Building and
bundling the helper into release packages remains separate work.

## Validation status

Tests cover saved-label isolation, Save/Discard, endpoint validation, bounded
logs, failed checks, cancellation and stream packet transport. Optional tests
exercise the real helper with synthetic media over Iris's BOOTP/TFTP path;
see [testing](testing.md#installation-helper-checks).

Local 6.5.22 and 6.5.30 installation sets have passed Instigator's full media
check. With Iris `xiri` at `fb24628`, a diskless Indy R4400 with 128 MB reached
the serial PROM monitor, fetched `6.5.22/stand/fx.ARCS` through BOOTP/TFTP and
ran `fx` to its safe read-only prompt. For the example addresses, the commands
were:

```text
setenv netaddr 192.168.0.2
boot -f bootp():/6.5.22/stand/fx.ARCS
```

No hard disk was attached to this test. The O2 serial test connected to the
helper but did not reach a console prompt; 6.5.30 guest boot is therefore still
unverified. Miniroot boot and a completed installation remain to be validated
before adding automatic installer input or a media-profile wizard.
