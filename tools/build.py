#!/usr/bin/env python3
"""Fetch a managed Iris checkout and build the current Prismatic sources."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REPO = "https://github.com/salvogendut/iris.git"
DEFAULT_BRANCH = "xiri"


class BuildError(Exception):
    pass


def execute(args, *, cwd, capture=False, env=None):
    result = subprocess.run(
        [str(arg) for arg in args], cwd=cwd, env=env, text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if result.returncode:
        detail = (result.stderr or "").strip()
        raise BuildError(f"{args[0]} exited with status {result.returncode}" +
                         (f": {detail}" if detail else ""))
    return result.stdout.strip() if capture else None


def git(directory, *args):
    return execute(["git", *args], cwd=directory, capture=True)


@contextmanager
def build_lock(directory):
    # Keep fetches, generated inputs and Cargo from racing another make process.
    # flock releases automatically even after a crash; the empty file may stay.
    import fcntl
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / "build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def source_directory(cache, repository, branch):
    identity = json.dumps([repository, branch]).encode()
    return cache / hashlib.sha256(identity).hexdigest()[:20]


def fetch_iris(directory, repository, branch):
    """Update only our cache, never a developer's sibling checkout."""
    directory.mkdir(parents=True, exist_ok=True)
    git(directory, "check-ref-format", "--branch", branch)
    source = directory / "iris"
    if source.is_symlink():
        raise BuildError(f"Managed Iris checkout must not be a symlink: {source}")
    if not source.exists():
        print(f"Cloning Iris branch {branch} into {source}", flush=True)
        with tempfile.TemporaryDirectory(prefix=".clone-", dir=directory) as temporary:
            staged = Path(temporary) / "iris"
            execute(["git", "clone", "--depth", "1", "--single-branch",
                     "--branch", branch, "--", repository, staged], cwd=directory)
            staged.rename(source)
    else:
        if source.is_symlink() or git(source, "rev-parse", "--show-toplevel") != str(source.resolve()):
            raise BuildError(f"Not a managed Git checkout: {source}")
        if git(source, "remote", "get-url", "origin") != repository:
            raise BuildError(f"The cached Iris origin has changed: {source}")
        if git(source, "symbolic-ref", "--quiet", "--short", "HEAD") != branch:
            raise BuildError(f"The cached Iris branch has changed: {source}")
        if git(source, "status", "--porcelain", "--untracked-files=all"):
            raise BuildError(f"Cached Iris has local changes; preserve or resolve them before retrying: {source}")
        print(f"Fetching Iris branch {branch}", flush=True)
        execute(["git", "fetch", "--no-tags", "origin", branch], cwd=source)
        upstream = git(source, "rev-parse", "FETCH_HEAD")
        current = git(source, "rev-parse", "HEAD")
        if current != upstream:
            ancestor = subprocess.run(
                ["git", "merge-base", "--is-ancestor", current, upstream], cwd=source,
            )
            if ancestor.returncode != 0:
                raise BuildError(f"Cached Iris has local or divergent commits; refusing to reset it: {source}")
            execute(["git", "merge", "--ff-only", upstream], cwd=source)
    revision = git(source, "rev-parse", "HEAD")
    print(f"Iris {branch} at {revision[:12]}", flush=True)
    return source, revision


def write_generated(path, data):
    if path.is_file() and not path.is_symlink() and path.read_bytes() == data:
        return
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
        name = Path(temporary.name)
        temporary.write(data)
    try:
        name.replace(path)
    finally:
        name.unlink(missing_ok=True)


def prepare_frontend(root, directory):
    """Keep Cargo's ../iris layout while compiling live frontend sources."""
    frontend = directory / "prismatic"
    if frontend.is_symlink():
        raise BuildError(f"Generated frontend directory must not be a symlink: {frontend}")
    frontend.mkdir(exist_ok=True)
    for name in ("src", "assets", "build.rs", "rust-toolchain.toml", "README.md"):
        target = root / name
        link = frontend / name
        if link.is_symlink() and link.resolve() == target.resolve():
            continue
        if link.exists() or link.is_symlink():
            raise BuildError(f"Unexpected file in generated build workspace: {link}")
        link.symlink_to(target, target_is_directory=target.is_dir())
    # An explicit workspace prevents discovery of any surrounding workspace.
    manifest = (root / "Cargo.toml").read_text()
    manifest += "\n# Generated by tools/build.py. Edit the root manifest instead.\n[workspace]\n"
    write_generated(frontend / "Cargo.toml", manifest.encode())
    write_generated(frontend / "Cargo.lock", (root / "Cargo.lock").read_bytes())
    return frontend / "Cargo.toml"


def cargo_steps(action, manifest, features, profile, run_args):
    common = ["--manifest-path", str(manifest), "--locked"]
    if features.strip():
        common += ["--features", features]
    if action == "check":
        return [
            ["fmt", "--manifest-path", str(manifest), "-p", "prismatic", "--check"],
            ["clippy", *common, "--all-targets", "--no-deps", "--", "-D", "warnings"],
            ["test", *common],
        ]
    command = [action, *common, "--profile", profile]
    if action == "run":
        command += ["--", *run_args]
    return [command]


def arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        "Makefile settings: IRIS_REPO, IRIS_BRANCH, FEATURES, PROFILE, CARGO, ARGS. "
        "Source caches live under target/bootstrap; ../iris is never modified. "
        "Requires Git, Python 3, Rust and the SDL3/native build dependencies (Unix)."
    ))
    parser.add_argument("action", nargs="?", default="build", choices=("fetch", "build", "run", "check"))
    parser.add_argument("--repo", default=os.environ.get("IRIS_REPO", DEFAULT_REPO))
    parser.add_argument("--branch", default=os.environ.get("IRIS_BRANCH", DEFAULT_BRANCH))
    parser.add_argument("--features", default=os.environ.get("FEATURES", "pcap,idle-pause,jitv2"))
    parser.add_argument("--profile", choices=("release", "dev"), default=os.environ.get("PROFILE", "release"))
    return parser.parse_args(argv)


def main(argv=None):
    options = arguments(argv)
    if not shutil.which("git"):
        raise BuildError("Git is required to fetch Iris.")
    cargo = shlex.split(os.environ.get("CARGO", "cargo"))
    if options.action != "fetch" and (not cargo or not shutil.which(cargo[0])):
        raise BuildError("Cargo is not on PATH. Install Rust with rustup or run make in your Rust/SDL3 build environment (for example, your distrobox).")
    run_args = shlex.split(os.environ.get("ARGS", "")) if options.action == "run" else []
    cache = ROOT / "target" / "bootstrap"
    with build_lock(cache):
        directory = source_directory(cache, options.repo, options.branch)
        source, revision = fetch_iris(directory, options.repo, options.branch)
        if options.action == "fetch":
            return
        manifest = prepare_frontend(ROOT, directory)
        env = dict(os.environ)
        target = Path(env.get("CARGO_TARGET_DIR") or ROOT / "target")
        if not target.is_absolute():
            target = ROOT / target
        env["CARGO_TARGET_DIR"] = str(target)
        # Checks must not write the user's Iris shader corpus or exit on poweroff.
        if options.action == "check":
            env.setdefault("IRIS_REX_JIT_PROFILE", str(directory / "test-rex-profile.bin"))
            env["IRIS_NO_EXIT_ON_POWEROFF"] = "1"
        for step in cargo_steps(options.action, manifest, options.features, options.profile, run_args):
            profile = "" if options.action == "check" else f"{options.profile}, "
            print(f"Cargo {step[0]} ({profile}features: {options.features or 'default'})", flush=True)
            execute([*cargo, *step], cwd=ROOT, env=env)
        info = {"iris_repository": options.repo, "iris_branch": options.branch,
                "iris_revision": revision, "iris_source": str(source),
                "manifest": str(manifest), "features": options.features,
                "profile": None if options.action == "check" else options.profile,
                "action": options.action,
                "cargo_target_dir": str(target)}
        info_path = directory / ("check-info.json" if options.action == "check" else "build-info.json")
        write_generated(info_path, (json.dumps(info, indent=2) + "\n").encode())
        print(f"Build information: {info_path}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (BuildError, OSError, ValueError) as error:
        print(f"Prismatic build: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)
