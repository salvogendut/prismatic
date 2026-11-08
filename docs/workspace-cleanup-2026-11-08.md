# Workspace cleanup — November 8, 2026

Recovered **80.5 GiB (86.4 GB) of actual filesystem space**. The
[machine-readable report](workspace-cleanup-2026-11-08.json) records exact
measurements, retained executable hashes, source revisions and audit-ledger
hashes.

| Measurement | Before | After |
| --- | ---: | ---: |
| Prismatic repository, including `target` (`du`) | 89.9 GiB | 8.6 GiB |
| Available space on `/var` | 263.6 GiB | 344.1 GiB |

The recovery figure is the observed increase in filesystem available space.
Directory and per-file totals can count shared Btrfs extents or hard-linked
files differently; they are not exclusive physical usage. Measurements are
host-wide snapshots taken during this cleanup.

Removed 126,304 generated files and aliases:

- Rust incremental, dependency, fingerprint and intermediate build caches.
- Go's compiler cache, retaining the pinned toolchain and downloaded modules.
- Flatpak builder caches and staging directories, including two clean,
  generated Iris source copies. Their revisions remain in the ledger.
- An old libpcap packaging stage and Python bytecode caches.
- Eleven superseded compatibility-test executables. Their test logs,
  screenshots and managed source checkouts remain.

The detailed local audit is under `target/maintenance/cleanup-20261108/`.
Its compressed `removals.jsonl.gz` records every removed path and original
file identity. `protected.json`, `verification.json`, `smoke-checks.json`
and `space-measurement.json` record preservation and validation.

## Preserved and checked

After cache removal, **1,499 protected file hashes** matched their values
before cleanup; documentation was updated afterward. These checks cover
tracked sources/assets, current Prismatic and Instigator executables,
licenses and packages, installation media, captures, build records, and user
configuration/NVRAM. The six retained source checkouts are unchanged and clean.

`make fetch` and the sibling Iris fetch found no new revisions:

- Iris `xiri`: `b23f674c235b37c3ac83808085d28ac128bb79c4`.
- Instigator `main`: `2db7231aa2572222c13b7db626b145ad42c66a45`.

This was storage maintenance: existing successful builds and their revision
records were preserved. No runtime source changed or rebuild was needed for
cleanup. The preserved release binary reports Prismatic 0.1.4 and passes
both `--smoke-test` and `--startup-smoke-test` with SDL's dummy video driver.
Instigator's help command also passes. The UI tests use temporary settings
and do not boot a guest. No Prismatic, Iris, Instigator or build processes
were left running.

Most remaining storage is intentional: about 6.6 GiB of installation-server
media and supporting files, 1.3 GiB of managed source/toolchain/module data,
and the current binaries, packages and compact evidence. The original
timestamped PPM/GIF captures and `~/.local/share/prismatic` settings remain
untouched. Installation paths and machine configurations still resolve to
the same files.

## Rebuilding caches

Normal `make` or direct Cargo builds regenerate their compiler caches;
`make flatpak` regenerates Flatpak build caches. First builds after cleanup
will take longer. Recreating superseded compatibility binaries requires
building the corresponding recorded source revisions.

Keep `target/release/prismatic`, `target/release/instigator`, their licenses
and build-info records when reclaiming caches. Managed sources, offline
toolchain inputs and the installation media currently under
`target/install-server` are retained inputs, not disposable compiler output.
See [workspace cleanup guidance](building.md#workspace-cleanup).
