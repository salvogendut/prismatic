# Configuration and saved state

[Back to Prismatic](../README.md) · [All guides](../README.md#documentation)

- [Persistent configuration](#persistent-configuration)
- [Firmware and runtime files](#firmware-and-runtime-files)

## Persistent configuration

**`prismatic.cfg`** contains the current saved General, Media and display
configuration, including Advanced options. It is created on first launch in
SDL's per-user application directory, normally **`~/.local/share/prismatic/prismatic.cfg`** on Linux, and loaded
on subsequent launches. Every successful menu Save atomically updates this
file. Ctrl+Plus/Minus also save the window scale immediately. Menu Cancel/Discard
do not change the saved configuration, and restart changes are saved only
after confirmation. The path is printed at startup when Debug output is Console.

For a configuration in the current directory, run:

```sh
make run ARGS="--config ./prismatic.cfg"
```

The file uses human-readable TOML with `[machine]`, `[advanced]`, `[network]`, `[audio]`, `[gif]` and one table per
attached device, for example:

```toml
scanlines = false
show_status = true
smoothing = false
tinker = false
debug_output = "off"
window_scale = 1.0 # 0.75 through 4.0, in steps of 0.25.

[audio]
muted = false
volume = 100 # 0 through 100 percent.

[machine]
profile = "indy_ip24"
cpu = "r4400"
ram_mb = 64
keyboard = "ansi"

[advanced]
resolution = "guest"
cache = "auto"
clock_offset_years = 0
scsi_deferred_interrupts = true
serial_logging = "off"

[advanced.expert]
# count_mhz = 33.0       # Omit for the model default.
idle_parking = true     # Effective only with the idle-pause build feature.
thread_affinity = false
# cpu_core = 2          # Omit a role to let the host scheduler choose.
jit_threads = 1         # Requires jitv2 to change.
jit_cache = false
monitor_port = 0        # Automatic loopback port.
serial_port_a = 0
serial_port_b = 0
serial_a = "tcp"
serial_b = "tcp"
# gdb_port = 1234       # Omit to disable; 0 chooses an automatic port.
# vnc_port = 5900
vnc_max_fps = 30
# boot_elf = "tests/loop.elf"
trace_on_boot = false   # Requires perfetto.
trace_categories = "all"
trace_guest_clock = false

[gif]
width = 0 # Native guest resolution; alternatively 320, 480 or 640.
fps = 25
encoder = "built_in"

[media.devices.1]
kind = "disk"
path = "/path/to/irix.chd"

[media.devices.4]
kind = "cdrom"
path = "/path/to/install.efs"
```

Relative image paths resolve beside the configuration file; menu selections
are saved as absolute paths. With the default location, an older
`prismatic.toml` is migrated if `prismatic.cfg` is absent; the old file is
retained. Older files without `[advanced]` receive the defaults above.
Invalid configurations report an error and are never replaced with
defaults. NVRAM/EEPROM remain separate machine-state files.

## Firmware and runtime files

With Iris bundled-firmware support, all five models include PROMs; otherwise
O2 and Octane require external PROM files. General accepts these files or
overrides (see [O2 and Octane firmware](machines.md#o2-and-octane-firmware)). The default is diskless; only images
selected in Prismatic are attached. It does not read `iris.toml`, `prom.bin`,
or default SCSI image paths from either checkout. Normal machine state is kept under
SDL's per-user application directory, normally
`~/.local/share/prismatic/` on Linux. Each model has its own directory: `indy/`,
`indigo2/`, `indigo2-ip28/`, `o2/` or `octane/`, containing its NVRAM/EEPROM or
persistent flash. `--state-dir PATH` keeps the existing Indy layout at `PATH`;
other models use their named subdirectory under `PATH`. Firmware source files
are read-only; O2 saves flash in `nvram.bin`, Octane in `flash.bin` alongside its
`nvram.bin`. Existing flash preserves the guest's firmware and environment.
The Iris monitor and serial ports default to ephemeral loopback ports.
Advanced > Connections selects serial TCP ports or pseudoterminals, plus optional
GDB/VNC servers. Connection status shows all active addresses and PTY paths.
Iris also maintains its shared draw-shape cache at
`~/.iris/rex-jit-profile.bin`; `IRIS_REX_JIT_PROFILE` overrides that cache path.

Related: [named machines](machines.md#named-machines), [network settings](networking.md#configuration-example), [disk overlays](media.md#disk-overlays-and-safe-changes), [checkpoints](extensions.md#experimental-checkpoints).
