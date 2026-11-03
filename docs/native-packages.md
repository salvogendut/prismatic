# Native packages and release builds

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

The **Native packages** workflow builds these formats, following the packaging
layout used by the other Happy Years emulators:

| Platform | Build environment | Download |
| --- | --- | --- |
| Fedora x86_64 | Fedora 44 container | `.rpm` |
| Debian amd64 | Debian 13 (trixie) container | `.deb` |
| Windows x86_64 | Windows 2025 runner, MSVC | Portable `.zip` |
| macOS Apple Silicon | macOS 15 ARM runner | `.app` in a `.zip` |
| macOS Intel | macOS 15 Intel runner | `.app` in a `.zip` |

The existing [Linux Flatpak](flatpak.md) workflow remains available separately.
Native packages use the operating system's normal Prismatic configuration;
Flatpak keeps its own configuration and state inside its sandbox.

## Install and run

Download the package for your platform from a published GitHub release. CI runs
on `main`, pull requests, tags and manual dispatch also provide Actions artifacts;
extract the artifact ZIP to find the package, checksum and build information.

On Fedora or Debian, let the package manager install the required shared libraries:

```sh
sudo dnf install ./prismatic-*.x86_64.rpm
sudo apt install ./prismatic_*_amd64.deb
prismatic
```

The Linux packages include the project icon, desktop entry, documentation and
license notices. They are built for the listed distributions; the Flatpak is
the alternative for Linux distributions with older or different libraries.

On Windows, extract the entire portable ZIP into a writable folder and launch
`prismatic.exe`. Keep `SDL3.dll` and the included Visual C++ runtime DLLs beside
the executable. It targets x86_64 Windows 10/11. Screenshots and GIFs are saved
in the working directory, normally the extracted folder.

On macOS, extract the ZIP and drag `Prismatic.app` into Applications. Choose
the Intel or ARM package for your Mac; these are separate bundles, requiring
macOS 15 or newer. SDL3, libpcap and other non-system dynamic libraries are
included in the app. The launcher puts screenshots and GIFs in
`~/Library/Application Support/prismatic/captures`. Use absolute paths when
passing command-line options through this launcher.

macOS apps are signed ad hoc, with the entitlements Iris needs for its JIT.
They are not Developer ID signed or notarized; macOS can require approval in
Privacy & Security on first launch. Windows packages are not Authenticode signed.

## Features and dependencies

All native builds use the pinned Rust toolchain in `rust-toolchain.toml` and
compile libchdman from source with a portable CPU baseline. Linux and macOS
enable `pcap,idle-pause,jitv2`. Windows enables `idle-pause,jitv2`: PCAP bridging
is disabled so launching Prismatic does not require a separately installed
WinPcap/Npcap driver. NAT, forwarding, NFS and TFTP remain available, and the
unavailable PCAP controls are greyed out. Windows also uses TCP serial rather
than Unix PTYs.

FFmpeg is optional and is not bundled in the Windows/macOS archives. The Rust
GIF recorder works without it; install FFmpeg on `PATH` to use its optimization
option. Linux package metadata recommends FFmpeg.

The packages include the frontend's BSD-3-Clause license, Iris's notices,
patched dependency licenses and available Cargo dependency notices. Bundled
SDL/libpcap runtime libraries retain their own licenses.

## CI and source pinning

[`.github/workflows/native.yml`](../.github/workflows/native.yml) first resolves
the editable `IRIS_REPO` and `IRIS_BRANCH` defaults in [Makefile](../Makefile).
These default to `https://github.com/salvogendut/iris.git`, branch **`xiri`**.
All five jobs consume the same resolved Iris commit, even if the branch advances
while the builds run. Each job creates its own temporary sibling layout under
`target/native`; a developer's `../iris` checkout is never changed.

Cargo builds use `--locked`. Windows downloads a checksum-pinned official SDL3
SDK. Debian computes runtime dependencies with `dpkg-shlibdeps`; RPM computes
its ELF dependencies through `rpmbuild`. macOS rewrites library paths into the
app bundle and checks that no Homebrew library paths remain. Each platform
runs raw/CHD disk-creation tests, plus dummy-video UI and diskless PROM checks
against the packaged application.
The Linux jobs also install the resulting RPM/DEB in their disposable containers
and repeat the smoke checks against the installed executable. Those containers
provide 1 GiB of shared memory for Iris's guest RAM mappings.

Published-release builds attach packages only after all platforms succeed.
Each package has a uniquely named checksum file and `build-info.json`, avoiding
collisions with other native builds or the Flatpak assets. Build information
records the frontend and Iris revisions, platform, architecture, toolchain and
enabled features. A release tag must match Cargo's numeric `MAJOR.MINOR.PATCH`
version. Pushing a tag provides Actions artifacts; publishing the GitHub release
triggers the attachment job, as with the Flatpak workflow.

## Running the helpers

The workflow lists the native build dependencies for each host. With those
dependencies, Git, Python 3.11+ and rustup available:

```sh
python3 tools/native.py plan
python3 tools/native.py build --platform debian
# Other platform values: fedora, windows, macos
```

Environment overrides for `IRIS_REPO` and `IRIS_BRANCH` take precedence over
the Makefile defaults when creating a plan. Outputs are placed in
`target/native/dist`. Building packages does not install them; the Linux-only
`--install-test` option is intended for disposable CI containers. Windows also
needs the MSVC environment initialized, including `VCToolsRedistDir`; the
workflow uses [the setup script](../packaging/native/msvc.ps1).

Run helper regression tests and workflow validation after packaging changes:

```sh
python3 -m unittest discover -s tools -p 'test_*.py' -v
actionlint
```

These checks cover pinned source selection, advancing branches, isolated build
inputs, SDL archive integrity and dependency-scan failures. Native runner jobs
provide the platform-specific compilation and launch checks.
