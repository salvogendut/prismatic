#!/usr/bin/env python3
"""Prepare a pinned source manifest, then build an offline Flatpak bundle."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib

from build import BuildError, ROOT, DEFAULT_REPO, DEFAULT_BRANCH, build_lock, execute, git, write_generated

APP_ID = 'io.github.salvogendut.Prismatic'
FEATURES = 'pcap,idle-pause,jitv2'
PACKAGING = ROOT / 'packaging/flatpak'
OUTPUT = ROOT / 'target/flatpak'


def crate_sources(lock):
    """Same archive/checksum layout as flatpak-cargo-generator, for crates.io.

    Path dependencies (Iris and its patches) are supplied by the source checkout.
    Fail on other registries/Git dependencies instead of silently omitting them.
    """
    sources = []
    for package in lock['package']:
        origin = package.get('source')
        if origin is None:
            continue
        if origin != 'registry+https://github.com/rust-lang/crates.io-index':
            raise BuildError(f'Unsupported Cargo source: {origin}; extend the Flatpak vendoring helper')
        name, version, checksum = (package.get(key, '') for key in ('name', 'version', 'checksum'))
        if (not re.fullmatch(r'[A-Za-z0-9_-]+', name)
                or not re.fullmatch(r'[A-Za-z0-9.+_-]+', version)
                or not re.fullmatch(r'[0-9a-f]{64}', checksum)):
            raise BuildError(f'Invalid locked crate metadata: {name} {version}')
        destination = f'cargo/vendor/{name}-{version}'
        sources.extend([
            {'type': 'archive', 'archive-type': 'tar-gzip',
             'url': f'https://static.crates.io/crates/{name}/{name}-{version}.crate',
             'sha256': checksum, 'dest': destination},
            {'type': 'inline', 'contents': json.dumps({'package': checksum, 'files': {}}),
             'dest': destination, 'dest-filename': '.cargo-checksum.json'},
        ])
    sources.append({
        'type': 'inline', 'dest': 'cargo', 'dest-filename': 'config.toml',
        'contents': '[source.crates-io]\nreplace-with = "vendored-sources"\n'
                    '[source.vendored-sources]\ndirectory = "/run/build/prismatic/cargo/vendor"\n'
                    '[net]\noffline = true\n',
    })
    return sources


def resolve_iris(repository, branch):
    git(ROOT, 'check-ref-format', '--branch', branch)
    output = git(ROOT, 'ls-remote', '--exit-code', '--', repository, f'refs/heads/{branch}')
    lines = [line.split() for line in output.splitlines()]
    if len(lines) != 1 or len(lines[0]) != 2 or not re.fullmatch(r'[0-9a-f]{40}', lines[0][0]):
        raise BuildError(f'Cannot resolve Iris branch {branch}')
    return lines[0][0]


def prepare(repository, branch, version, jobs):
    manifest = json.loads((PACKAGING / f'{APP_ID}.json').read_text())
    toolchain = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    for source in manifest['modules'][0]['sources']:
        if not toolchain.startswith('nightly-') or f'/dist/{toolchain[8:]}/' not in source['url']:
            raise BuildError('Update the checksum-pinned Flatpak Rust archives to match rust-toolchain.toml')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    sources = crate_sources(lock)
    revision = resolve_iris(repository, branch)
    frontend = tomllib.loads((ROOT / 'Cargo.toml').read_text())
    version = version or frontend['package']['version']
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+_-]*', version):
        raise BuildError('Version must contain only letters, digits, dots, plus, underscore or hyphen')
    if version.startswith('v') and len(version) > 1 and version[1].isdigit():
        version = version[1:]
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH') or git(ROOT, 'log', '-1', '--format=%ct'))
    info = {
        'app_id': APP_ID, 'version': version,
        'release_date': datetime.fromtimestamp(epoch, timezone.utc).date().isoformat(),
        'prismatic_revision': git(ROOT, 'rev-parse', 'HEAD'),
        'prismatic_dirty': bool(git(ROOT, 'status', '--porcelain')),
        'iris_repository': repository, 'iris_branch': branch, 'iris_revision': revision,
        'rust_toolchain': toolchain, 'features': FEATURES,
        'runtime': manifest['runtime'], 'runtime_version': manifest['runtime-version'],
        'architecture': 'x86_64',
        'cargo_lock_sha256': hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest(),
    }
    # Snapshot only project build inputs. Never copy local disks, captures,
    # preferences, target/, or the developer's sibling Iris checkout.
    destination = OUTPUT / 'prismatic'
    if destination.is_symlink():
        raise BuildError(f'Refusing to replace a symlink: {destination}')
    with tempfile.TemporaryDirectory(prefix='.source-', dir=OUTPUT) as temporary:
        staged = Path(temporary) / 'prismatic'
        staged.mkdir()
        for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'build.rs', 'README.md', 'LICENSE'):
            shutil.copyfile(ROOT / name, staged / name)
        for name in ('src', 'assets', 'packaging/flatpak'):
            shutil.copytree(ROOT / name, staged / name, ignore=shutil.ignore_patterns('__pycache__'))
        if destination.exists():
            shutil.rmtree(destination)
        staged.rename(destination)
    manifest['build-options']['env']['CARGO_BUILD_JOBS'] = str(jobs)
    iris = manifest['modules'][-1]['sources'][1]
    iris.update(url=repository, branch=branch, commit=revision)
    for path, value in [
        ('build-info.json', info), ('cargo-sources.json', sources),
        (f'{APP_ID}.json', manifest),
    ]:
        write_generated(OUTPUT / path, (json.dumps(value, indent=2) + '\n').encode())
    print(f'Flatpak sources: Iris {branch} at {revision}; Rust {toolchain}', flush=True)
    return OUTPUT / f'{APP_ID}.json'


def builder_command():
    if os.environ.get('FLATPAK_BUILDER'):
        return shlex.split(os.environ['FLATPAK_BUILDER'])
    if shutil.which('flatpak-builder'):
        return ['flatpak-builder']
    if subprocess.run(['flatpak', 'info', 'org.flatpak.Builder'],
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        return ['flatpak', 'run', 'org.flatpak.Builder']
    raise BuildError('Install flatpak-builder or the org.flatpak.Builder Flatpak first')


def bundle(output):
    destination = output / 'Prismatic-x86_64.flatpak'
    # Preserve the previous usable artifact if exporting a new bundle fails.
    with tempfile.TemporaryDirectory(prefix='.bundle-', dir=output) as temporary:
        staged = Path(temporary) / destination.name
        execute(['flatpak', 'build-bundle', '--arch=x86_64',
                 '--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo',
                 output / 'repo', staged, APP_ID], cwd=ROOT)
        with staged.open('rb') as stream:
            checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
        staged.replace(destination)
        write_generated(output / 'SHA256SUMS', f'{checksum}  {destination.name}\n'.encode())
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('prepare', 'build'), nargs='?', default='build')
    parser.add_argument('--repo', default=os.environ.get('IRIS_REPO', DEFAULT_REPO))
    parser.add_argument('--branch', default=os.environ.get('IRIS_BRANCH', DEFAULT_BRANCH))
    parser.add_argument('--version', default=os.environ.get('FLATPAK_VERSION'))
    parser.add_argument('--jobs', type=int, default=int(os.environ.get('FLATPAK_JOBS', '2')))
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    if execute(['flatpak', '--default-arch'], cwd=ROOT, capture=True) != 'x86_64':
        raise BuildError('The initial Flatpak package supports x86_64 Linux only')
    if OUTPUT.is_symlink():
        raise BuildError(f'Refusing a symlink build directory: {OUTPUT}')
    with build_lock(OUTPUT):
        manifest = prepare(args.repo, args.branch, args.version, args.jobs)
        if args.action == 'prepare':
            print(manifest)
            return
        execute([*builder_command(), '--user', '--force-clean', f'--jobs={args.jobs}',
                 f'--state-dir={OUTPUT / "cache"}', f'--repo={OUTPUT / "repo"}',
                 OUTPUT / 'build', manifest], cwd=ROOT)
        print(f'Bundle ready: {bundle(OUTPUT)}', flush=True)


if __name__ == '__main__':
    try:
        main()
    except (BuildError, OSError, ValueError) as error:
        print(f'Prismatic Flatpak: {error}', file=sys.stderr)
        sys.exit(1)
