# IRIX installation errors and workarounds

[Back to Prismatic](../README.md) · [Step-by-step installation](installing-irix.md)

These are observations from installations using the original IRIX media and
Prismatic's private Instigator network. They describe recovery steps and their
limits, rather than assuming every warning is an installation failure. Keep
each machine's disk, overlay sidecars and firmware state separate.

## Octane: PROM exception after copying the miniroot

Observed with Iris `xiri` at `b23f674`, a single R10000, 512 MB RAM, Solid
IMPACT, bundled PROM 4.9 and IRIX 6.5.30. `fx.64` successfully partitioned the
new disk. This command copied the installation miniroot to swap:

```text
setenv tapedevice bootp():/6.5.30/dist/sa
boot -f $tapedevice(sash64) --m
```

After **Copy complete**, loading `dist/miniroot/unix.IP30` failed with:

```text
Exception: <vector=XUT>
Exception PC: 0xffffffff9fc62b98, Exception RA: 0xffffffff9fc62ec8
Read TLB miss exception, bad address: 0xffffffffffff8080
PANIC: Unexpected exception
[Press reset or ENTER to restart.]
```

Enter produced another exception at `0xffffffff9fc62c34`, with bad address
`0xffffffffffff8081`. The underlying cause has not been established.

The tested recovery was:

1. Close Prismatic while it displays the PROM exception, before IRIX has booted.
2. Relaunch the same configuration with the **same disk and overlay files**.
   Keep the serial console and installation network selected.
3. Press Ctrl+P, enter maintenance, then choose **5, Enter Command Monitor**.
4. Check that the installation helper is still serving. The firmware can
   reset internally during startup; if that disconnects the helper, follow
   the next section before continuing.
5. Load the kernel directly from the PROM:

   ```text
   setenv netaddr 192.168.0.2
   bootp():/6.5.30/dist/miniroot/unix.IP30
   ```

This reached **IRIX Release 6.5 IP30 ... System V - 64 Bit** and the
filesystem-creation prompt using the already-copied miniroot. Continue with
the XFS formatting and Inst steps in the installation guide. Do not use this
shortcut until the miniroot copy has completed; an empty swap partition does
not contain an installer.

The recovered installer completed the package installation and kernel build.
The installed system reached a serial root login and, on a later cold boot
with NAT and the helper disabled, the 1280×1024 IMPACT desktop. The failure
above did not prevent those disk boots.

PROM 4.17 was also located in the supplied 6.5.30 media, but this recovery
used the bundled 4.9 image. Finding a newer image does not establish that it
fixes the exception.

## Octane: installation server disappears after an internal reset

In the same test, the frontend log showed:

```text
Machine: SIN hard reset
iris: network stream closed by peer
iris: network stream unix:/tmp/prismatic-install-.../net.sock: No such file or directory (os error 2); retrying
```

Instigator recorded `server_stop` with result `disconnected`. It exits when
the emulator's stream connection closes. A guest-initiated hardware reset
can close that connection without starting a new Prismatic machine worker,
so the managed helper is not automatically relaunched. The subsequent PROM
message was:

```text
No server for :/6.5.30/dist/miniroot/unix.IP30.
Your netaddr environment variable may be set incorrectly, or
the net may be too busy for a connection to be made.
```

The address was still correct. Restarting the helper restored network boot.
For the native helper, while Prismatic remains at the PROM:

1. Run Prismatic from a terminal, with **Advanced > Core > Debug output >
   Console** enabled before boot. Find the current machine's `Octane Ethernet
   on stream segment unix:...` line, or its stream-retry message. Use that
   run's full socket path; the generated directory changes on the next
   Prismatic launch.
2. In another host terminal, change to this machine's installation setup
   folder and restart the helper on that socket:

   ```sh
   cd /absolute/path/to/installation-setup
   /absolute/path/to/instigator serve \
     --network-socket /tmp/prismatic-install-REPLACE/net.sock instigator.yaml
   ```

3. Leave that terminal running. Iris retries the connection. Cancel a pending
   PROM network request with Ctrl+C if necessary, then retry the boot command.
4. After the guest has shut down and Prismatic has closed, stop any manually
   started helper that remains running.

Only restart the helper after confirming it exited. Do not start a second
server on a socket that is still in use. This manual recovery was tested
with native Instigator; it is not a tested Podman recovery procedure.

For an installation managed entirely from a terminal, the helper also
supports a fixed loopback TCP endpoint; see
[external servers](installation.md#external-servers-and-packaging). It still
needs restarting if a reset disconnects the client.

## Octane: autoboot still stops at maintenance

During this test, `nvram AutoLoad Yes` inside IRIX reported `Y`, but the next
cold boot stopped at maintenance and IRIX subsequently reported `N`. The
cause was not isolated. Setting it through the PROM worked:

1. Shut down IRIX cleanly and cold-start the same machine.
2. At maintenance choose **5, Enter Command Monitor**.
3. Enter `setenv AutoLoad Yes`, then `printenv AutoLoad` to confirm.
4. Exit and choose Start System, or close Prismatic while still at the PROM
   and relaunch it to check persistence.

The latter check automatically booted the installed disk after Ctrl+P,
reached X again, and reported `Y` from IRIX. The General IRIX preset configures
the loader and kernel; the PROM's `AutoLoad` setting remains separate.

## Indy: fx stops accepting serial input

The Indy R4400 / 6.5.22 test stalled in `fx` with idle parking enabled.
Set **Advanced > Performance > Idle parking > Off**, Save, and restart the
installation machine. The successful Indy and O2 installations used it off,
with JIT enabled. The Octane test also uses this setting.

## Wrong standalone program for the machine

| Machine | Partitioner | Standalone shell |
| --- | --- | --- |
| Indy R4400 | `fx.ARCS` | `sashARCS` |
| O2 R5000 | `fx.ARCS` | `sashARCS` |
| Octane R10000 | `fx.64` | `sash64` |

The O2 rejected `fx.64` with a load/relocation error. Selecting `fx.ARCS`
worked. The processor being 64-bit does not by itself determine which
standalone program its PROM requires.

## Indy: PROM remote-install menu fails

An Indy menu-based attempt failed with a UTLB exception. The explicit
`boot -f $tapedevice(sashARCS) --m` sequence in the guide subsequently
reached the miniroot with no CD attached. The first complete Indy install
used a CD miniroot copy followed by network packages; that fallback is
documented in the [installation guide](installing-irix.md#troubleshooting).

## Inst: optional packages have missing prerequisites

The tested standard selections requested optional packages whose prerequisites
were absent:

- `java2_plugin.sw32.mozilla_freeware`
- `inventor_dev.sw.base`
- `inventor_dev.sw.lib` after deselecting `inventor_dev.sw.base`

Use `conflicts` and select the displayed **Do not install** choices for
these packages. The choice numbers change as conflicts are resolved. Rerun
`conflicts` until it reports **No conflicts**, then use `go`. Do not apply
`rulesoverride`. Missing prerequisites for core OS packages instead call for
checking that all matching overlay and Foundation distributions are open.

## O2: corrupted display after graphical halt

On the installed O2 / 6.5.30 desktop, `halt -y` returned to firmware with a
corrupted display. Its cause remains unconfirmed. A cold boot reached the
normal graphical login again. For normal shutdown use IRIX's **Power Off
the System** option, or run as root:

```text
shutdown -p -g0 -y
```

That command was verified to reach Prismatic's **Machine powered off** state.
Wait for that state before closing the frontend. The serial `halt -y` used
during the earlier installation completed normally.

## Boot messages that did not prevent installation

- `Cannot load runtime symbol table from bootp'ed kernel`: observed while
  starting the network installation kernels; they still reached Inst.
- `RAD UST not incrementing`: observed on the Octane miniroot before Inst
  started. This test does not establish that Octane audio works.
- Requickstarting at 99% for several minutes: observed during successful
  installations. Allow it to finish and wait for automatic kernel
  configuration; do not reset simply because the percentage stops changing.

The Octane also completed `shutdown -p -g0 -y` from both serial and graphical
sessions, with Prismatic showing **Machine powered off**.

See the [validation record](installation.md#validation-status) for what has
been completed on each machine.
