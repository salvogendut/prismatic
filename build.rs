use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git_output(source: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        // A build launched from a Git hook must still inspect the Iris repo.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn iris_branch(source: &Path) -> String {
    let metadata = source.join(".git");
    if metadata.is_file() || !metadata.exists() {
        // Linked worktrees store the Git directory in this file.
        println!("cargo:rerun-if-changed={}", metadata.display());
    }
    if !metadata.exists() {
        // Do not accidentally report a source archive's enclosing repository.
        return "unknown (Git metadata unavailable)".into();
    }
    if let Some(head) = git_output(source, &["rev-parse", "--git-path", "HEAD"]) {
        // HEAD changes even when switching branches at the same commit.
        println!("cargo:rerun-if-changed={}", source.join(head).display());
    }
    git_output(source, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .or_else(|| {
            git_output(source, &["rev-parse", "--short", "HEAD"])
                .map(|revision| format!("detached HEAD ({revision})"))
        })
        .unwrap_or_else(|| "unknown (Git metadata unavailable)".into())
}

// Detect embedding APIs, not branch names: a merged API becomes available on
// the next rebuild. Watch every source so switching ../iris also reruns this.
fn main() {
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../iris");
    println!(
        "cargo:rustc-env=PRISMATIC_IRIS_BRANCH={}",
        iris_branch(&source)
    );
    let root = source.join("src");
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
        ("stream_network", "config.rs", "pub stream: Option<String>"),
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
