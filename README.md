# Prismatic

![Prismatic logo](images/Prismatic%20Rainbow%20Emulator%20Logo.png)

Prismatic is the Rust/SDL3 frontend being developed for [our](https://github.com/salvogendut/iris) fork of [Iris](https://github.com/techomancer/iris),
following the desktop conventions of the Happy Years emulators
([1984](https://github.com/salvogendut/1984), [1985](https://github.com/salvogendut/1985),
[1986](https://github.com/salvogendut/1986), and [1989](https://github.com/salvogendut/1989)).

Prismatic supports SGI Indy, Indigo2, Indigo2 IMPACT, O2 and Octane models.
It opens **PAUSED** with a diskless Indy by default. Configure the machine in
**F9 > General**, attach disks or CDs in **Media**, then press **Ctrl+P** to boot.
Saved machine labels let you keep multiple configurations and switch with **F2**.

Both **O2 and Octane** have been verified installing **IRIX 6.5.30** from scratch
and running its graphical desktop under emulation. See the
[step-by-step IRIX installation guide](docs/installing-irix.md) for the tested
machine settings and procedure.

The SDL3 interface provides keyboard and mouse capture, guest audio, per-CPU
and storage activity LEDs, screenshots and GIF recording. Enable **General >
Tinker** for Advanced, Network and Extensions controls. Available features depend
on the linked Iris core and guest OS; unsupported controls are greyed out.

## Screenshots

IRIX 6.5 desktop screenshots, captured at the guest's native 1282×1024 resolution.

![IRIX 6.5 desktop with the console and Icon Catalog](images/irix65-desktop.png)

![Xmahjongg running on the IRIX 6.5 desktop](images/irix65-xmahjongg.png)

SGI O2 booting IRIX:

![SGI O2 booting IRIX in Prismatic](images/O2-booting.gif)

SGI Octane displaying its hardware inventory with `hinv`:

![SGI Octane hardware inventory in Prismatic](images/Octane-hinv.gif)

## Build and run

Release CI builds [native packages](docs/native-packages.md) for Fedora (`.rpm`),
Debian (`.deb`), Windows and macOS (Intel/ARM), plus a [Linux Flatpak](docs/flatpak.md).
Packaged builds include Iris and the Instigator installation helper. Builds fetch
our Iris `xiri` branch and Instigator `main`, recording both source revisions.
See the guides for installation and platform requirements.

Use a Rust/SDL3 development environment with Git, Python 3.11+ and the native build
libraries installed. Linux is the validated platform; see the
[build guide](docs/building.md#dependencies-and-features) for prerequisites.

```sh
make                          # Fetch/update Iris and Instigator; build a release
./target/release/prismatic
# Or build and launch together:
make run
```

The defaults are [our Iris fork](https://github.com/salvogendut/iris) and its
**`xiri`** branch. Edit `IRIS_REPO` and `IRIS_BRANCH` in [Makefile](Makefile), or
override them for one build:

```sh
make IRIS_REPO=https://github.com/YOUR-NAME/iris.git IRIS_BRANCH=your-branch
```

`INSTIGATOR_REPO` and `INSTIGATOR_BRANCH` are editable in the same Makefile.
Managed sources live under `target/bootstrap`; an existing `../iris` checkout
stays independent. **F9 > About** shows the Iris branch used to build Prismatic.
The bundled `target/release/instigator` helper is found automatically; Podman is
optional. See [building and Iris compatibility](docs/building.md) for manual
Cargo builds, optional features and `main` compatibility.

## Essential controls

| Key | Action |
| --- | --- |
| Click guest display / Ctrl+Enter | Capture / release keyboard and mouse |
| Ctrl+P | Boot, pause or resume the selected machine |
| F2 | Cycle saved machine labels |
| F4 / F6 | Take a screenshot / start or stop GIF recording |
| F5 | Confirm a cold reset |
| F9 | Open options; save an open options session |
| Arrows / Enter / Escape | Navigate options, change a value, or close with Save/Discard choices |
| Tab / Shift+Tab | Switch topic sub-tabs in Advanced, Network and Extensions; sub-tabs are also clickable |
| Ctrl+Plus / Ctrl+Minus | Change and save the window scale |
| F11 / F12 | Toggle fullscreen / confirm quit |

Menu settings apply after a successful Save; changes that restart a running
machine ask for confirmation. Settings persist in `prismatic.cfg`, normally
under `~/.local/share/prismatic/` on Linux. Shut down the guest OS before
restarting or quitting. See the [usage guide](docs/usage.md) for first-boot PROM
instructions, full shortcuts, input handling, capture options and audio.

## Documentation

| Guide | Contents |
| --- | --- |
| [Building and Iris compatibility](docs/building.md) | Dependencies, Makefile options, manual Cargo builds and core features |
| [Linux Flatpak](docs/flatpak.md) | Installation, sandbox files, source builds and tagged-release CI |
| [Native packages](docs/native-packages.md) | Fedora RPM, Debian DEB, Windows ZIP, macOS Intel/ARM apps and release CI |
| [Controls, captures and audio](docs/usage.md) | First launch, keyboard/mouse input, window scaling, screenshots, GIFs and sound |
| [Machine settings and saved labels](docs/machines.md) | Models, CPU/RAM, PROM boot/console presets, firmware and multiple instances |
| [Advanced settings](docs/advanced.md) | Performance, SGIEmu optimizations, serial TCP/PTY, GDB/VNC, tracing and diagnostics |
| [Extensions](docs/extensions.md) | Graphics, extra CPUs, optional devices, checkpoints, CD changer and scratch storage |
| [Media and activity LEDs](docs/media.md) | Create empty disks, disk/CD formats, SCSI IDs, copy-on-write overlays and status lights |
| [Networking and file sharing](docs/networking.md) | SSH through localhost port forwarding, NAT, PCAP, NFS, TFTP and RSH |
| [Install IRIX step by step](docs/installing-irix.md) | Media preparation, blank disks, PROM boot, partitioning, Inst and the first desktop login |
| [IRIX installation errors and workarounds](docs/irix-installation-troubleshooting.md) | Observed PROM failures, helper disconnects, package conflicts and recovery steps |
| [Network installation helper](docs/installation.md) | Private Ethernet, native/Podman Instigator setup, platform limits and validation |
| [Configuration and saved state](docs/configuration.md) | `prismatic.cfg`, TOML examples, paths and persistent firmware state |
| [Testing and smoke checks](docs/testing.md) | Formatting, Clippy, unit tests, diskless PROM checks and coverage limits |
| [Integration architecture](docs/architecture.md) | Frontend/core boundaries, worker lifecycle, display/audio handoff and design details |

## Acknowledgments

Thank you to [techomancer](https://github.com/techomancer) and the contributors
to [Iris](https://github.com/techomancer/iris) for developing the SGI emulation
core that powers Prismatic. Their emulator, hardware research and documentation
provide the foundation for [our fork](https://github.com/salvogendut/iris) and
its integration with the Happy Years Rust/SDL3 interface.

Thank you also to [Renato Tarabella](https://github.com/RenatoTarabella) and the
[SGIEmu project](https://github.com/RenatoTarabella/SGIEmu). SGIEmu's JIT register
caching and real-time clock caching techniques inspired the optional performance
optimizations adapted into our Iris fork, now available in Prismatic through
**Advanced > Performance > SGIEmu optimizations**.

The bundled installation helper is James Braid's
[Instigator](https://github.com/jamesbraid/instigator), which serves SGI
installation media and provides a private emulator network.

## License

Prismatic is licensed under the [BSD 3-Clause license](LICENSE).
Iris, Instigator, other dependencies and attributed assets retain their own licenses.
