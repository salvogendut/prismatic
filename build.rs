use std::{env, fs, path::PathBuf};

// Detect embedding APIs, not branch names: a merged API becomes available on
// the next rebuild. Watch every source so switching ../iris also reruns this.
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../iris/src");
    for (name, file, marker) in [
        (
            "audio",
            "host_audio.rs",
            "pub use external::{OutputFactory, PcmStream}",
        ),
        ("diagnostics", "lib.rs", "pub mod diagnostics;"),
        ("prom", "prom_env.rs", "pub fn prepare_environment("),
        ("serial", "config.rs", "pub serial_backends:"),
        ("chd_cow", "block_node.rs", "pub use chd_cow::open_chd_cow;"),
        ("affinity", "thread_affinity.rs", "pub fn available_cores("),
        ("gdb", "gdb_stub.rs", "pub struct GdbServer"),
        ("vnc", "vnc/mod.rs", "sessions.lock()"),
        ("trace", "perfetto/mod.rs", "pub fn stop_if_running("),
        ("jit_cache", "cpu/jitv2/pcache.rs", "pub fn configure("),
        (
            "sgiemu_optimizations",
            "config.rs",
            "pub sgiemu_optimizations: bool",
        ),
        ("firmware", "lib.rs", "pub mod prombinip32;"),
        (
            "poweroff",
            "workstation.rs",
            "pub fn set_exit_on_guest_poweroff(",
        ),
        ("devices", "machine.rs", "pub fn has_ultra64("),
        ("checkpoints", "machine.rs", "pub fn save_snapshot_paused("),
        ("octane_cpus", "config.rs", "pub const OCTANE: [Self; 3]"),
        ("live_network", "net/mod.rs", "fn runtime_config("),
    ] {
        println!("cargo:rustc-check-cfg=cfg(iris_{name})");
        let path = root.join(file);
        println!(
            "cargo:rerun-if-changed={}",
            if path.exists() {
                path.as_path()
            } else {
                path.parent().unwrap()
            }
            .display()
        );
        if fs::read_to_string(path).is_ok_and(|text| text.contains(marker)) {
            println!("cargo:rustc-cfg=iris_{name}");
        }
    }
}
