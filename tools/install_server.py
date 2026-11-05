#!/usr/bin/env python3
"""Build the optional, pinned Instigator helper without changing any Iris checkout."""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REPO = "https://github.com/jamesbraid/instigator.git"
REVISION = "2db7231aa2572222c13b7db626b145ad42c66a45"
IMAGE = f"localhost/prismatic-instigator:{REVISION[:12]}"


def run(*args, cwd=ROOT, capture=False):
    return subprocess.run(
        args, cwd=cwd, check=True, text=True, encoding="utf-8",
        stdout=subprocess.PIPE if capture else None,
    ).stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["image", "native"])
    args = parser.parse_args()
    destination = ROOT / "target/install-server"
    source = destination / "source"
    source.mkdir(parents=True, exist_ok=True)
    if not (source / ".git").exists():
        if any(source.iterdir()):
            raise SystemExit(f"Refusing to replace a nonempty source directory: {source}")
        run("git", "init", str(source))
        run("git", "remote", "add", "origin", REPO, cwd=source)
    if run("git", "remote", "get-url", "origin", cwd=source, capture=True).strip() != REPO:
        raise SystemExit("The helper source checkout has an unexpected origin")
    if run("git", "status", "--porcelain", cwd=source, capture=True).strip():
        raise SystemExit("The helper source checkout has local changes; leaving it untouched")
    run("git", "fetch", "--depth=1", "origin", REVISION, cwd=source)
    run("git", "checkout", "--detach", REVISION, cwd=source)
    run("podman", "build", "--file", str(ROOT / "packaging/install-server/Containerfile"),
        "--tag", IMAGE, str(source))
    if args.action == "native":
        # Extract the static host-architecture Linux binary without starting it.
        container = run("podman", "create", IMAGE, capture=True).strip()
        try:
            with tempfile.TemporaryDirectory(dir=destination) as tmp:
                output = Path(tmp) / "instigator"
                run("podman", "cp", f"{container}:/instigator", str(output))
                output.chmod(0o755)
                output.replace(destination / "instigator")
        finally:
            run("podman", "rm", container)
        print(f"Native Linux helper: {destination / 'instigator'}")
    print(f"Install image: {IMAGE}\nSource: {REPO} @ {REVISION}")


if __name__ == "__main__":
    main()
