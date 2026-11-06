# Linux Flatpak

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Install and run](#install-and-run)
- [Files and sandbox behavior](#files-and-sandbox-behavior)
- [Build locally](#build-locally)
- [Tagged releases and CI](#tagged-releases-and-ci)
- [Packaging maintenance](#packaging-maintenance)

## Install and run

The initial package targets **x86_64 Linux** and uses the Freedesktop **26.08**
runtime. Download `Prismatic-x86_64.flatpak` and `SHA256SUMS` from a GitHub release
or the **Prismatic-Linux-x86_64** artifact of the **Linux Flatpak** workflow.
The artifact is a ZIP; extract it first. Then run:

```sh
sha256sum -c SHA256SUMS
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user ./Prismatic-x86_64.flatpak
flatpak run io.github.salvogendut.Prismatic
```

Prismatic also appears in your desktop's application menu with the supplied
project icon. These are standalone bundles; Prismatic is not published on
Flathub. Install a newer bundle with the same command to update it.

## Files and sandbox behavior

The Flatpak keeps its configuration and state separate from a native build:

| Item | Host location |
| --- | --- |
| Configuration | `~/.var/app/io.github.salvogendut.Prismatic/data/prismatic/prismatic.cfg` |
| Machine state | Under the same `data/prismatic/` directory |
| F4 screenshots and F6 GIFs | `~/.var/app/io.github.salvogendut.Prismatic/data/prismatic/captures/` |
| Iris shader corpus | `~/.var/app/io.github.salvogendut.Prismatic/cache/prismatic/rex-jit-profile.bin` |

The launcher uses the captures directory as its working directory. Supply
**absolute paths** to command-line options such as `--config` or `--state-dir`.
Existing native configuration is not imported automatically; copied configurations
must refer to files the sandbox can access.

The package grants access to **Downloads** and **Documents/Prismatic**, so disk
images and the adjacent overlay/lock files can remain together. Put writable
disk images and NFS/TFTP share directories there. The SDL file chooser uses the
desktop portal for other locations, but permission to one selected disk does
not necessarily grant permission to create its adjacent overlay files. For a
disk library or share directory elsewhere, grant that directory explicitly:

```sh
flatpak override --user --filesystem=/absolute/path/to/disks io.github.salvogendut.Prismatic
```

Wayland, X11 fallback, accelerated rendering and audio are enabled. Network
access supports NAT, NFS/TFTP and [localhost port forwarding](networking.md#connect-from-the-host-to-the-guest-ssh).
The runtime includes FFmpeg for the optional GIF optimizer. The sandbox does
not grant raw-packet privileges for PCAP bridging. Use NAT in this package;
use the native build for host bridge configurations that need those privileges.

Instigator is bundled at `/app/libexec/instigator`. In Network > Installation,
choose **Native Instigator** and leave the executable at its automatic default.
The helper runs inside the sandbox, with access to the same allowed media
folders as Prismatic; host Podman is unnecessary. See [the installation guide](installation.md).
To check the included executable inside the sandbox:

```sh
flatpak run --command=/app/libexec/instigator io.github.salvogendut.Prismatic --version
```

Use **TCP serial routing** for terminals running on the host. A PTY created
inside the sandbox belongs to its device namespace, so its displayed `/dev/pts`
path is not a host terminal path. Host camera devices are also not granted by
default; VINO's test-pattern/black sources remain available. See Flatpak's
[sandbox permission reference](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
for optional device and directory overrides.

## Build locally

Install Git, Make, Python **3.11+**, Flatpak, and either your distribution's
`flatpak-builder` or the `org.flatpak.Builder` Flatpak. Host Rust, SDL3 and
libclang development packages are not required for this build.

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.flatpak.Builder
flatpak install --user flathub org.freedesktop.Platform//26.08 org.freedesktop.Sdk//26.08 org.freedesktop.Sdk.Extension.llvm22//26.08
make flatpak
```

The result is `target/flatpak/Prismatic-x86_64.flatpak`, with `SHA256SUMS` and
`build-info.json` beside it. Build data and downloaded sources stay under
`target/flatpak/` and `target/bootstrap/`; rebuilding reuses Flatpak Builder's cache there.

The helper reads the same editable `IRIS_REPO` and `IRIS_BRANCH` defaults as
the [normal Makefile build](building.md#choosing-an-iris-repository-and-branch):
**`https://github.com/salvogendut/iris.git`, branch `xiri`**. Each invocation
resolves that branch to a commit, then Flatpak Builder fetches that exact Iris
source into its own build directory. It never changes the developer's `../iris`
checkout. The generated manifest pins the commit even if the remote branch
advances while compilation is running. `INSTIGATOR_REPO` and `INSTIGATOR_BRANCH`
select the helper (default: `https://github.com/jamesbraid/instigator.git`, `main`).
Preparation fetches that branch, snapshots its tracked files, verifies and vendors
its Go modules, and records the commit in `build-info.json`. The builder compiles
this snapshot offline using a checksum-pinned Go archive. The Go compiler and
source caches are build tools; the installed app contains only the helper binary
and its license notices.

The package enables `pcap,idle-pause,jitv2`, with SGIEmu optimizations still
off by default and selectable per machine. Its Rust compiler is checksum-pinned
to the project's `rust-toolchain.toml`. Cargo crates are downloaded using
`Cargo.lock` checksums, and libchdman is compiled from source using its portable
CPU baseline. Compilation uses `cargo build --frozen` without network access.
An incompatible upstream dependency change fails the locked build instead of
silently rewriting `Cargo.lock`.

```sh
make flatpak FLATPAK_JOBS=4                    # Default: two compilation jobs
make flatpak FLATPAK_VERSION=0.2.0
make flatpak IRIS_REPO=https://github.com/salvogendut/iris.git IRIS_BRANCH=xiri
make flatpak-prepare                          # Generate pinned inputs only
```

`FLATPAK_BUILDER` can override the builder command. The helper automatically
chooses a native `flatpak-builder`, then falls back to `flatpak run org.flatpak.Builder`.
Preparing sources snapshots only the frontend's build inputs, not local disk
images, captures or configuration files. That snapshot is generated build data
and is replaced on the next invocation.

## Tagged releases and CI

[`.github/workflows/flatpak.yml`](../.github/workflows/flatpak.yml) runs on:

- **Any pushed tag:** builds and uploads the installable bundle as an Actions artifact.
- **A published GitHub release:** builds its tagged source and attaches the bundle,
  checksums and `build-info.json` to that release. This also covers prereleases.
- **Manual workflow dispatch:** builds the selected ref as an Actions artifact.

For example, after committing the release changes:

```sh
git tag v0.2.0
git push origin v0.2.0
```

The workflow fetches Iris `xiri`, installs the SDK, runs packaging helper tests,
builds the bundle, validates its metadata, and installs it for dummy-video and
diskless PROM smoke tests. Upload happens only after those checks pass. Creating
a tag does not automatically publish a GitHub release. Publishing one later
triggers the release build and attachment step.

Build provenance records the Prismatic and Iris commits, whether the local
frontend was modified, Rust toolchain, enabled features and lockfile checksum.
A release publication can use a newer Iris `xiri` commit than an earlier
tag-triggered build; consult the attached `build-info.json` for the exact core.
The build job has read-only repository permissions; only the release attachment
job has write access. No personal access token is required.

## Packaging maintenance

The editable manifest template and desktop assets live in
[`packaging/flatpak/`](../packaging/flatpak). Generate the final manifest with
`make flatpak-prepare`; the template refers to those generated inputs and is not
intended to be passed directly to Flatpak Builder.

The original icon is [retained here](../images/prismatic-icon-source.png); the
package contains a 512×512 derivative. Prismatic uses the
[BSD 3-Clause license](../LICENSE); the package includes it alongside the
separate Iris, CHD library and asset notices.

When changing `rust-toolchain.toml`, update the three Rust archive URLs and
SHA-256 hashes in the manifest. The helper rejects mismatched toolchain dates.
The Cargo source generator currently accepts crates.io and local path
dependencies; Git or alternate-registry crates require extending the helper.
Runtime/LLVM changes must also be reflected in the workflow installation step.
Automatic ELF debug extraction is disabled because it can modify the temporary
Rust compiler libraries between modules. The toolchain is removed by cleanup;
Cargo still builds the app with its normal optimized release profile.
The libpcap module explicitly installs to `/app/lib`; older Flatpak Builder
versions otherwise let CMake choose `/app/lib64`, which is outside the runtime
loader's search path. The app build checks the shared library before compiling.

```sh
python3 -m unittest discover -s tools -p 'test_*.py' -v
# After building and installing the bundle:
flatpak run --env=SDL_VIDEODRIVER=dummy --env=SDL_RENDER_DRIVER=software --env=SDL_AUDIODRIVER=dummy io.github.salvogendut.Prismatic --smoke-test
flatpak run --env=SDL_VIDEODRIVER=dummy --env=SDL_RENDER_DRIVER=software --env=SDL_AUDIODRIVER=dummy --env=IRIS_REX_JIT_PROFILE=/tmp/prismatic-flatpak-rex.bin io.github.salvogendut.Prismatic --boot-smoke-test
```

Related: [build options](building.md), [configuration and state](configuration.md),
[testing](testing.md), [Flatpak's build guide](https://docs.flatpak.org/en/latest/first-build.html).
