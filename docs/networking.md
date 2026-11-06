# Networking and file sharing

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Network settings](#network-settings)
- [Connect from the host to the guest (SSH)](#connect-from-the-host-to-the-guest-ssh)
- [Guest setup, NFS and TFTP](#guest-setup-nfs-and-tftp)
- [Network installation helper](installation.md)
- [Installing IRIX](#installing-irix)
- [PCAP bridging](#pcap-bridging)
- [Configuration example](#configuration-example)

## Network settings

Enable **General > Tinker**, then open the **Network** tab in F9. Every model
has Ethernet, using Iris's NAT gateway by default. Network settings belong to
the selected machine label and are saved in `prismatic.cfg`. Save validates the
draft. NAT subnet/port forwards/bind scope and the PCAP interface apply live
after a successful Save. Backend, MAC and service-folder/protocol changes ask
before restarting an already booted machine; it returns to PAUSED, ready for
Ctrl+P. Discard and failed saves leave the running network unchanged. Hiding
Tinker keeps saved network settings active.

| Control | Behavior |
| --- | --- |
| Network backend | NAT, optional PCAP bridge, private Stream where supported, or Off on O2/Octane. Indy/Indigo2 cannot disconnect Ethernet in the current core. |
| NAT subnet | Enter an IPv4 network/prefix, such as `192.168.5.0/24`. Gateway/DNS is network + 1 and the guest address is network + 2; the menu shows both and the netmask. |
| PCAP host interface | Choose an enumerated host interface, Automatic, or enter a name/index. Available in PCAP mode. |
| NFS shared folder | Choose or enter an existing host folder, or Disable. The guest has **read/write** access to this folder. |
| NFS protocol | Auto, V2 or V3. Auto accepts either; IRIX 5.3 uses v2. |
| NFS bridge IP | An unused LAN IPv4 address for the built-in NFS server when using PCAP. Required if NFS sharing is enabled in PCAP mode. |
| TFTP boot folder | Choose an existing folder to serve **read-only** at the NAT gateway, or Disable. |
| Port forwards | Enter comma-separated `tcp:HOST:GUEST` or `udp:HOST:GUEST` rules. For example, `tcp:2323:23, tcp:2222:22`. Empty clears the rules. |
| Forward bind scope | Localhost (default), or all host interfaces to allow LAN access to forwarded ports. Applies to all rules. |
| Guest Ethernet MAC | Optional unicast MAC, such as `02:00:00:00:00:22`; clear to use Iris's default. Requires restart. Existing Indy/Indigo2 NVRAM can retain its previous station address. |
| RSH file folder | Existing folder served read-only by Iris's NAT RSH service, or Disable. Supports the core's limited file commands, without executing a host shell. |
| Stream endpoint | `tcp:IP:PORT` or `unix:/absolute/socket/path`. The external peer supplies network services. A managed installation helper supplies its own temporary socket. |

These controls are on **Network > Connection**. **Network > Installation**
configures an optional native or Podman Instigator helper; see the
[installation guide](installation.md). Stream mode retains NAT/PCAP preferences
but does not run Iris's NAT services. It cannot be combined with DaynaPort.

Text fields initially select the current value: typing replaces it, Ctrl+A
selects all, Backspace edits, Enter accepts into the draft and Escape cancels.
F9 saves the whole draft. Relative folder paths use the directory containing
`prismatic.cfg`; folder checks run in the background and never create folders.
Different windows need different host forwarding ports.

## Connect from the host to the guest (SSH)

Prismatic already exposes Iris's **NAT port forwarding**. The guest's private
address does not need to be directly reachable from the host: a listener on a
host port forwards connections to a service inside the guest.

To reach the guest's SSH server through `127.0.0.1:2222`:

1. Open **F9 > General** and enable **Tinker**.
2. Open **Network** and select **Network backend > NAT**.
3. Open **Port forwards**, enter `tcp:2222:22`, then press Enter to accept the draft.
4. Keep **Forward bind scope > Localhost only**, then press **F9** to save.
   Confirm a restart if you changed the backend.
5. Boot or resume the guest with **Ctrl+P** if needed, and connect from the host:

   ```sh
   ssh -p 2222 username@127.0.0.1
   ```

Replace `username` with a guest account. The guest needs a running SSH server on
port 22 and working Ethernet configuration. Use the guest address, gateway and
mask shown in Network. With the default NAT subnet these are `192.168.0.2`,
`192.168.0.1` and `255.255.255.0`. Port forwarding does not install or start a
guest service. If the guest has not sent any network traffic yet, ping the
gateway from the guest before retrying so Iris can learn its network address.

The rule format is `PROTOCOL:HOST_PORT:GUEST_PORT`. Choose a free local port;
`2222` is an example, not an automatically assigned port. Multiple rules are
comma-separated, for example `tcp:2222:22, tcp:8080:80, udp:11177:177`. The second
rule makes a guest web server available at `http://127.0.0.1:8080/` on the host.
Editing the field replaces the whole rule list, so retain existing entries when
adding another mapping. Clear the field and Save to remove all rules.

Rules persist per machine label. On the integrated `xiri` core, forwarding-only
changes apply live after Save, unless an enabled [DaynaPort](extensions.md#optional-devices)
requires a restart. Cores without live network updates also ask for a restart.
Give simultaneously running labels different host ports, such as
`2222` and `2223`. These forwards are active only with the NAT backend.

If the connection fails, check that the guest is running and its service is
listening. If another host application already uses the chosen port, select a
different one. **Advanced > Core > Debug output > Console** reports Iris listener
bind failures; enable it before saving the forwarding change or booting.

## Guest setup, NFS and TFTP

Changing the NAT subnet clears existing NAT connections and does not rewrite the guest's saved IP address, route or DNS
settings. Configure the guest to match the addresses shown in Network. With
the default subnet, mount an enabled NFS share inside IRIX with:

```sh
mkdir /shared
mount 192.168.0.1:/ /shared
```

Iris serves NFS over UDP. A PCAP guest mounts `<NFS bridge IP>:/` instead.
For TFTP booting, place the appropriate boot files in the selected folder and
use the guest PROM's `bootp()` command; for example, O2 accepts
`boot -f bootp():FILE`. The guest/PROM must have a suitable network address.

## Installing IRIX

Keep [Tech-Pubs: installing IRIX 6.5 over the network from Linux](https://tech-pubs.net/tutorials/remote-irix-6.5-installation-from-linux.html)
as a reference for a future installation from scratch. It covers preparing
installation media, configuring BOOTP/TFTP and RSH on a Linux server, booting
the SGI installer, partitioning a disk and installing distributions with `inst`.
A complete installation needs the Foundation media as well as the installation
tools and overlays appropriate for the machine.

The tutorial targets physical SGI systems and a separate Linux server. When
adapting it to Prismatic, evaluate Iris's built-in NAT TFTP and RSH services
through the Network overlay and use the guest/gateway addresses shown there.
The [step-by-step installation guide](installing-irix.md)
adapts this procedure to the managed helper and records the tested PROM
commands, media layout and installation caveats.

The optional [installation helper](installation.md) uses
[Instigator](https://github.com/jamesbraid/instigator), which serves SGI CD
images directly and supplies BOOTP, TFTP and RSH installation services. Its
[private emulator connection](https://github.com/jamesbraid/instigator/blob/main/docs/operations.md#emulator-connections)
uses QEMU stream framing over a Unix socket or loopback TCP, matching the
stream backend in the current Iris `xiri` sources. Prismatic can manage a
temporary install-server container with read-only media and a private socket,
or a native helper process. Fresh Indy R4400 / IRIX 6.5.22f and O2 R5000 /
IRIX 6.5.30f installations have booted to the graphical desktop with the
helper disabled.
Flatpak host-container management remains future work.

## PCAP bridging

PCAP is an optional build feature. On Fedora, build it with:

```sh
sudo dnf install libpcap-devel
make run FEATURES=pcap,idle-pause,jitv2
```

PCAP requires host capture permission (on Linux, appropriate raw-packet
capabilities) and uses the LAN's addresses and DNS. Wired interfaces work best;
many Wi-Fi networks reject an additional guest MAC address. A missing feature
or failed capture is reported rather than silently switching to NAT. Prismatic
does not change host permissions. Live updates are requests to Iris; interface
failures are reported, and listener failures appear in Console debug output.
NAT port forwards, TFTP and RSH are inactive in
PCAP mode; their saved settings are retained for switching back. For further
guest setup, see [our Iris networking guide](https://github.com/salvogendut/iris/blob/xiri/NETWORKING.md).

## Configuration example

For an unnamed machine, the configuration uses `[network]`; named machines
use `[machines."LABEL".network]`:

```toml
[machines."Indy IRIX".network]
mode = "nat"
nat_subnet = "192.168.5.0/24"
nfs_dir = "/path/to/shared"
nfs_version = "auto"
tftp_dir = "/path/to/bootfiles"
rsh_dir = "/path/to/read-only-files"
mac = "02:00:00:00:00:22"
forward_lan = false

[[machines."Indy IRIX".network.forwards]]
protocol = "tcp"
host_port = 2323
guest_port = 23
```

Omitting the folder keys disables those services. PCAP configurations also
accept `pcap_interface` and, for sharing, `nfs_pcap_ip`.

Related: [configuration](configuration.md), [DaynaPort](extensions.md#optional-devices), [serial and remote connections](advanced.md#connections-and-serial-terminals).
