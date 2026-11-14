# Building and Iris compatibility

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Build and run](#build-and-run)
- [Choosing an Iris repository and branch](#choosing-an-iris-repository-and-branch)
- [Managed source checkouts](#managed-source-checkouts)
- [Build commands](#build-commands)
- [Manual Cargo builds](#manual-cargo-builds)
- [Iris compatibility](#iris-compatibility)
- [Dependencies and features](#dependencies-and-features)
- [Workspace cleanup](#workspace-cleanup)

## Build and run

Run shell commands in these guides from the Prismatic repository root.
Direct Cargo commands require the [sibling checkout](#manual-cargo-builds);
`make` manages its own Iris and Instigator sources.

Run `make` in a Rust/SDL3 build environment:

```sh
make                          # Fetch/update Iris and Instigator, then build a release
./target/release/prismatic
# Or build and launch in one step:
make run
```

## Choosing an Iris repository and branch

The first settings in [Makefile](../Makefile) choose the source:

```make
IRIS_REPO ?= https://github.com/salvogendut/iris.git
IRIS_BRANCH ?= xiri
INSTIGATOR_REPO ?= https://github.com/jamesbraid/instigator.git
INSTIGATOR_BRANCH ?= main
```

Edit these lines to change the defaults, or override them for one invocation:

```sh
make IRIS_BRANCH=main
make IRIS_REPO=https://github.com/YOUR-NAME/iris.git IRIS_BRANCH=your-branch
make INSTIGATOR_REPO=https://github.com/YOUR-NAME/instigator.git INSTIGATOR_BRANCH=main
make FEATURES=pcap,idle-pause,perfetto,jitv2
```

## Managed source checkouts

The helper clones each repository/branch combination into its own ignored
`target/bootstrap/<source-key>/iris` directory. Later invocations fetch and
fast-forward that managed checkout before compiling. It refuses local changes,
changed remotes/branches, and local or divergent commits instead of resetting
them. An existing sibling `../iris` checkout is not modified.

A generated Cargo workspace beside the managed Iris checkout links to this
repository's current `src`, assets and build script. This builds your latest
Prismatic edits together with the fetched core without rewriting the root Cargo
manifest or lockfile. Cargo builds use `--locked`; if a newer Iris dependency
requires a lockfile update, the build fails rather than silently resolving a new
dependency set. The successful build's Iris and Instigator revisions and settings are recorded
in `target/bootstrap/<source-key>/build-info.json`.
Instigator checkouts live under `target/bootstrap/instigator/<source-key>/instigator`
with the same clean-worktree and fast-forward rules. Every `make` fetches both
branches. The build downloads a checksum-pinned Go 1.27.1 toolchain into this
private cache, verifies modules against `go.sum`, and compiles Instigator beside
Prismatic in `target/release` (or `target/debug`). No system Go installation or
Podman is needed. A failed helper build preserves the previous helper and fails
the overall build. Upstream dependency/toolchain changes may require updating
the checked-in build inputs before a build succeeds.

Check results use a separate `check-info.json`, preserving the build record.

## Build commands

| Command / setting | Purpose |
| --- | --- |
| `make fetch` | Fetch both repositories without requiring Rust or compiling. |
| `make run ARGS='--config "my machine.cfg"'` | Build and run from this repository, so relative config/capture paths behave normally. |
| `make PROFILE=dev` | Build into `target/debug` instead of `target/release`. |
| `make FEATURES=` | Use the interpreter with optional Cargo features disabled. |
| `make check FEATURES=` | Fetch, then run frontend formatting, Clippy and tests with default Cargo features. |
| `make test-bootstrap` | Test the build helper using disposable local Git repositories; no network or Rust required. |
| `CARGO=...`, `PYTHON=...` | Select the Cargo command or Python interpreter. |
| `CARGO_TARGET_DIR=...` | Choose the Cargo artifact directory; default is this repository's `target`. |
| `make flatpak` | Build an installable x86_64 Linux Flatpak with isolated Iris sources; see the [Flatpak guide](flatpak.md). |
| `make flatpak-prepare` | Pin both repositories and prepare offline Rust/Go sources. |
| `make install-server-image` | Build the optional Instigator Podman image; see [network installation](installation.md). |
| `make install-server` | Build only the native helper into `target/install-server/instigator`. |

`make help` lists the helper options. Without Make, run `python3 tools/build.py`;
its `--repo` and `--branch` options select other sources. The helper requires
Git and Python 3.11+ on Unix, plus the Rust/native build dependencies below for
compilation. Use your Distrobox or other development environment with Cargo on
`PATH` and the native libraries installed. It does not install system packages
or change your Rust environment.

## Manual Cargo builds

For direct Cargo builds that use a checkout you maintain yourself, keep the
checkouts alongside one another:

```text
Dev/
  iris/        # our Iris fork
  prismatic/
```

Direct `cargo build` still builds only the frontend using `../iris`. To supply
the helper, run `make install-server` and select that executable in Network >
Installation, or put Instigator on `PATH`. Ordinary `make` bundles it automatically.

## Iris compatibility

Prismatic development uses our Iris fork's **`xiri`** branch, which includes the
merged performance work. **`perf/sgiemu-port`** is retired. Automated builds
default to **`xiri`**, retaining NetBSD and other guest OS compatibility work
alongside IRIX. Compatibility with the fork's **`main`** remains available. Direct `cargo`
commands use `../iris`; `make` uses its managed source checkout. Each binary
links the Iris checkout present when built; rebuild after changing sources.

**F9 > About** shows the Iris branch linked into the executable at build time.
This works with both `make` and direct Cargo builds. Detached checkouts show
`detached HEAD` with the short commit ID; builds without Git metadata show
`unknown (Git metadata unavailable)`.

`build.rs` detects optional embedding APIs from the Iris sources, independently
of branch names. Missing features are greyed out in the overlays, with a reason
in the hint strip. Saved preferences remain in `prismatic.cfg` and machine
stanzas; rebuilding against a core that provides the APIs enables them again.
Rebuilding against the integrated `xiri` automatically enables its APIs,
including the optional SGIEmu performance control.

The current `main` core lacks several frontend hooks: SDL guest audio,
diagnostic routing, PROM boot/console presets, PTY/Off serial routing, owned
GDB/VNC services, per-machine affinity/cache controls, optional-device lifecycle
support and paused checkpoint saving. Saved PTY/Off routes fall back to TCP at their saved port (0 for automatic);
choose distinct ports if previously inactive ports conflict. TCP serial ports, normal graphics,
raw disks, CDs, network services and supported CPU/JIT options remain usable.
Protected CHD hard disks require the fork's CHD COW API; unavailable builds
reject them before startup and accept raw images instead. Current `main` has
packed O2/Octane PROMs; older cores may require external firmware selected in
General. Diagnostics stay on the console;
guest audio is disabled when no SDL PCM hook exists. Network changes require
restart on cores without persistent live-network overrides.

The integrated `xiri` includes PROM presets, PTY/Off routing and the newer
Octane CPU selections. General cycles only CPUs available in the linked core. A label saved
with an unavailable CPU still loads, but boot asks you to select a supported
CPU rather than silently replacing the hardware. **Connection status** lists
which optional capabilities are unavailable in the current build.

## Dependencies and features

The root Cargo manifest preserves the fork's patched winit
and block dependencies. Install Rust through rustup; `rust-toolchain.toml`
selects `nightly-2026-10-28`, the toolchain used for the Iris performance work.
On a Fedora development system, install the native build dependencies:

```sh
sudo dnf install git python3 gcc gcc-c++ make pkgconf-pkg-config sdl3-devel alsa-lib-devel clang-devel libpcap-devel
# Recommended fetch-and-build workflow (defaults to our fork's xiri):
make
# Alternatively, with a manually maintained sibling checkout:
# Iris xiri:
cargo run --release --locked --features pcap,idle-pause,perfetto,jitv2
# Older Iris main (or omit --features for the interpreter):
cargo run --release --locked --features pcap,idle-pause,jitv2
```

Older `main` Odyssey revisions fail to compile with Iris's `perfetto` feature
(missing `events::cfifo`/`events::swap` references); the integrated `xiri` carries
the correction. Omit `perfetto` on affected checkouts. Tracing controls are grey
when not built or when the owned tracing API is absent.

The default build enables Iris's experimental CPU JIT. The integrated SGIEmu
integer register and RTC calendar caches are **off by default**; enable them per
machine through [Advanced > Performance](advanced.md#performance). Keep
`developer` disabled to use the register cache: developer/lockstep instrumentation
disables it. PCAP, idle-pause and optional Perfetto controls remain available.
These optimizations are workload-dependent; the original bare-metal results do
not establish a Prismatic desktop or OS boot speedup.

The current Iris library still compiles its winit/glutin, iris-audio and camera
modules. Prismatic uses its own SDL window. Guest audio uses a per-machine PCM handoff
when that API is available; otherwise it is disabled without opening another host backend. libclang is needed to build the
Linux camera bindings.
SDL is linked dynamically through the [`sdl3` Rust bindings](https://github.com/vhspace/sdl3-rs).
SDL_ttf and a separate widget toolkit are not required. The
[native package workflow](native-packages.md) compiles and smoke-tests Fedora,
Debian, Windows x86_64 and macOS Intel/ARM packages on their respective runners.

## Workspace cleanup

The [November 8 cleanup report](workspace-cleanup-2026-11-08.md) records cache
removal, retained inputs, runtime checks and actual space recovered. Stop
builds and guests before retiring caches. Rust `target/debug` and
`target/release` subdirectories named `incremental`, `build`, `deps` and
`.fingerprint` are regenerable; the top-level executables and licenses remain
usable after removing those caches. `target/bootstrap/gocache` and
`target/flatpak/cache` are also generated caches. Check generated source
copies for local edits before removing them.

Preserve the managed source checkouts, pinned toolchain/module inputs,
`build-info.json`, current executables and helper licenses, finished packages,
configuration/NVRAM, manual disks and captures. The installation sets under
`target/install-server` are currently retained inputs. Keep their paths stable;
new permanent machine data should use the locations described in the
[installation guide](installing-irix.md). Keep compact test evidence when
retiring superseded test executables or staging trees. Normal build commands
recreate removed compiler/packaging caches as needed.

Related: [configuration and runtime files](configuration.md#firmware-and-runtime-files), [testing](testing.md), [integration architecture](architecture.md).

The November 2026 core API uses CPU part names, packed embedded PROMs and
owned network sessions. `build.rs` detects these interfaces in the source;
Prismatic retains adapters for older cores. The supported machine list stays
Indy, Indigo2 IP22/IP28, O2 and Octane. New core-only machine profiles are
rejected when loading a Prismatic configuration.

### Optional graphics builds

`wgpu`, `gpujit` and `hostgl` pass through to the linked Iris core. For example:

```sh
make FEATURES=pcap,idle-pause,jitv2,wgpu,gpujit
```

They are opt-in and can require additional core dependencies. WGPU enables
supported boards' compute raster controls in Extensions > Render; Newport's
current upstream gate still applies. `gpujit` builds Iris's graphics JITs;
`hostgl` builds its host GL support. These flags do not replace SDL3 or guarantee
that every core/host combination implements every rendering path.

The newer platform transport hooks are optional. A core without them uses
Iris-owned TCP for L1/C/D and those machines' A/B UARTs; PTY/Off controls are
unavailable there. New-platform SCSI boot media still attaches through Iris,
but live SCSI/scratch actions require the embedding bus hooks. Upstream cores
whose CPU counters depend on a display cannot report activity for serial-only
Origin machines. Current `xiri` integration supplies independent counters.
