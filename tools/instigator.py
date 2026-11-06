"""Managed Instigator sources and a checksum-pinned, private Go toolchain."""
import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile

from build import BuildError, ROOT, execute, fetch_source, git, source_directory

REPO = 'https://github.com/jamesbraid/instigator.git'
BRANCH = 'main'
GO_VERSION = '1.27.1'
# Official archive checksums: https://go.dev/dl/?mode=json&include=all
GO_SHA256 = {
    'linux-amd64': '63d339f0da5ab53635a56f2490a7984dfe12dfcff22ad749f63edaf590168445',
    'darwin-amd64': '8f8f52c6649542cf027bbc9b9c68d1ec042f9f34808a40413f0b8b3f66f3caa4',
    'darwin-arm64': 'ee215d57e0ec269c60cc9ceca68e6bda321ba9ee5afe24f4b0988703c2d87d12',
    'windows-amd64': 'a3911b5e0e1b1053f25ed0675f4c1c6aad1e2bfcf253df2b9be4caabd2edd95d',
}


def defaults():
    return os.environ.get('INSTIGATOR_REPO', REPO), os.environ.get('INSTIGATOR_BRANCH', BRANCH)


def metadata(repository, branch, revision):
    return dict(instigator_repository=repository, instigator_branch=branch,
                instigator_revision=revision, instigator_go_version=GO_VERSION)


def fetch(cache):
    repository, branch = defaults()
    directory = source_directory(cache / 'instigator', repository, branch)
    source, revision = fetch_source(directory, repository, branch, 'instigator')
    return source, metadata(repository, branch, revision)


def resolve(repository, branch):
    git(ROOT, 'check-ref-format', '--branch', branch)
    result = git(ROOT, 'ls-remote', '--exit-code', '--', repository, f'refs/heads/{branch}').split()
    if len(result) != 2 or not re.fullmatch('[0-9a-f]{40}', result[0]):
        raise BuildError('Cannot resolve the requested Instigator branch')
    return metadata(repository, branch, result[0])


def checkout(directory, info):
    revision = info['instigator_revision']
    if not re.fullmatch('[0-9a-f]{40}', revision):
        raise BuildError('Expected a pinned Instigator commit')
    directory.mkdir()
    execute(['git', 'init', '--initial-branch', info['instigator_branch']], cwd=directory)
    execute(['git', 'remote', 'add', 'origin', info['instigator_repository']], cwd=directory)
    execute(['git', 'fetch', '--depth=1', 'origin', revision], cwd=directory)
    execute(['git', 'checkout', '--detach', 'FETCH_HEAD'], cwd=directory)
    if git(directory, 'rev-parse', 'HEAD') != revision:
        raise BuildError('Instigator checkout differs from the release plan')


def go_archive(target):
    if target not in GO_SHA256:
        raise BuildError(f'No pinned Go toolchain for {target}')
    extension = 'zip' if target.startswith('windows-') else 'tar.gz'
    filename = f'go{GO_VERSION}.{target}.{extension}'
    return {'url': f'https://go.dev/dl/{filename}', 'sha256': GO_SHA256[target]}


def extract_tar(archive, destination):
    # Source/toolchain archives need only files and directories. Reject links,
    # device nodes and paths that could escape the private staging directory.
    with tarfile.open(archive) as stream:
        for entry in stream.getmembers():
            path = Path(entry.name)
            if path.is_absolute() or '..' in path.parts or not (entry.isfile() or entry.isdir()):
                raise BuildError(f'Unsafe archive member: {entry.name}')
        stream.extractall(destination, filter='data')


def toolchain(cache):
    arch = {'x86_64': 'amd64', 'amd64': 'amd64', 'aarch64': 'arm64', 'arm64': 'arm64'}.get(platform.machine().lower())
    target = f'{platform.system().lower()}-{arch}'
    spec = go_archive(target)
    destination = cache / f'go-{GO_VERSION}-{target}'
    binary = destination / 'go/bin' / ('go.exe' if os.name == 'nt' else 'go')
    if binary.is_file():
        return binary
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.go-', dir=cache) as temporary:
        work = Path(temporary)
        archive = work / spec['url'].rsplit('/', 1)[1]
        print(f'Downloading Go {GO_VERSION} for {target}', flush=True)
        with urllib.request.urlopen(spec['url'], timeout=120) as source, archive.open('wb') as output:
            shutil.copyfileobj(source, output)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != spec['sha256']:
            raise BuildError('Go toolchain checksum mismatch')
        staged = work / 'toolchain'
        staged.mkdir()
        if archive.suffix == '.zip':
            with zipfile.ZipFile(archive) as stream:
                for entry in stream.infolist():
                    path = Path(entry.filename.replace('\\', '/'))
                    if path.is_absolute() or '..' in path.parts or ':' in entry.filename:
                        raise BuildError(f'Unsafe Go archive member: {entry.filename}')
                stream.extractall(staged)
        else:
            extract_tar(archive, staged)
        staged.rename(destination)
    return binary


def go_environment(cache):
    return dict(os.environ, GOTOOLCHAIN='local', CGO_ENABLED='0',
                GOPATH=str(cache / 'gopath'), GOCACHE=str(cache / 'gocache'),
                GOFLAGS='', GOWORK='off', GOROOT=str(toolchain(cache).parent.parent))


def vendor(source, destination, cache):
    """Snapshot only tracked source, then vendor go.sum-verified modules."""
    if git(source, 'status', '--porcelain', '--untracked-files=all'):
        raise BuildError('Instigator source has local changes; refusing an unrecorded build')
    destination.mkdir()
    with tempfile.TemporaryDirectory(dir=cache) as temporary:
        archive = Path(temporary) / 'source.tar'
        execute(['git', 'archive', '--format=tar', '-o', archive, 'HEAD'], cwd=source)
        extract_tar(archive, destination)
    locked = {name: (destination / name).read_bytes() for name in ('go.mod', 'go.sum')}
    env = go_environment(cache)
    execute([toolchain(cache), 'mod', 'vendor'], cwd=destination, env=env)
    execute([toolchain(cache), 'mod', 'verify'], cwd=destination, env=env)
    if any((destination / name).read_bytes() != content for name, content in locked.items()):
        raise BuildError('Instigator vendoring changed go.mod/go.sum; update the upstream lock files')


def notices(source, destination, cache):
    destination.mkdir(parents=True, exist_ok=True)
    for name in ('LICENSE', 'THIRD_PARTY_NOTICES'):
        shutil.copyfile(source / name, destination / name)
    shutil.copyfile(toolchain(cache).parent.parent / 'LICENSE', destination / 'Go-LICENSE')
    for entry in (source / 'vendor').rglob('*'):
        if entry.is_file() and entry.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE', 'UNLICENSE')):
            target = destination / 'modules' / entry.relative_to(source / 'vendor')
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(entry, target)


def build(source, output, cache, licenses=None):
    cache.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.instigator-build-', dir=cache) as temporary:
        work = Path(temporary)
        staged = work / output.name
        snapshot = work / 'source'
        vendor(source, snapshot, cache)
        execute([toolchain(cache), 'build', '-mod=vendor', '-trimpath', '-buildvcs=false',
                 '-ldflags=-s -w', '-o', staged, './cmd/instigator'],
                cwd=snapshot, env=go_environment(cache))
        execute([staged, '--version'], cwd=work)
        if licenses:
            notices(snapshot, licenses, cache)
        # Do not overwrite the last usable helper after a failed build.
        with tempfile.NamedTemporaryFile(dir=output.parent, delete=False) as stream:
            pending = Path(stream.name)
        try:
            shutil.copyfile(staged, pending)
            pending.chmod(0o755)
            pending.replace(output)
        finally:
            pending.unlink(missing_ok=True)
    print(f'Bundled Instigator: {output}', flush=True)
