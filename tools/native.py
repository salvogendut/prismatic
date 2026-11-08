#!/usr/bin/env python3
"""Build native release packages from an isolated, revision-pinned Iris checkout."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

from build import BuildError, ROOT, execute, git
import instigator

APP_ID = 'io.github.salvogendut.Prismatic'
ART = ROOT / 'packaging/flatpak'
SDL_URL = 'https://github.com/libsdl-org/SDL/releases/download/release-3.4.18/SDL3-devel-3.4.18-VC.zip'
SDL_SHA256 = '78d84602ae616cfe26b33a73b7d3b9b6b56e8a6650e53d92c9c6ca938f747acd'
PLATFORMS = ('fedora', 'debian', 'windows', 'macos')


def run(args, *, cwd=ROOT, env=None, capture=False):
    return execute(args, cwd=cwd, env=env, capture=capture)


def make_default(name):
    value = os.environ.get(name)
    if value:
        return value
    match = re.search(rf'^{name}\s*\?=\s*(\S+)\s*$', (ROOT / 'Makefile').read_text(), re.M)
    if not match:
        raise BuildError(f'Cannot read {name} from Makefile; set it in the environment')
    return match[1]


def plan(tag=''):
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise BuildError('Native package versions must have the form MAJOR.MINOR.PATCH')
    if tag and tag.removeprefix('v') != version:
        raise BuildError(f'Release tag {tag!r} does not match Cargo version {version}')
    repository, branch = make_default('IRIS_REPO'), make_default('IRIS_BRANCH')
    git(ROOT, 'check-ref-format', '--branch', branch)
    remote = git(ROOT, 'ls-remote', '--exit-code', '--', repository, f'refs/heads/{branch}').split()
    if len(remote) != 2 or not re.fullmatch('[0-9a-f]{40}', remote[0]):
        raise BuildError('Cannot resolve the requested Iris branch')
    return {'version': version, 'prismatic_revision': git(ROOT, 'rev-parse', 'HEAD'),
            'iris_repository': repository, 'iris_branch': branch, 'iris_revision': remote[0],
            **instigator.resolve(make_default('INSTIGATOR_REPO'), make_default('INSTIGATOR_BRANCH'))}


def checkout_iris(directory, info):
    if not re.fullmatch('[0-9a-f]{40}', info['iris_revision']):
        raise BuildError('Expected a pinned Iris commit')
    directory.mkdir()
    run(['git', 'init', '--initial-branch', info['iris_branch']], cwd=directory)
    run(['git', 'remote', 'add', 'origin', info['iris_repository']], cwd=directory)
    run(['git', 'fetch', '--depth', '1', 'origin', info['iris_revision']], cwd=directory)
    run(['git', 'checkout', '-B', info['iris_branch'], 'FETCH_HEAD'], cwd=directory)
    if git(directory, 'rev-parse', 'HEAD') != info['iris_revision']:
        raise BuildError('Iris checkout differs from the release plan')


def snapshot(directory):
    directory.mkdir()
    for name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain.toml'):
        shutil.copyfile(ROOT / name, directory / name)
    for name in ('src', 'assets'):
        shutil.copytree(ROOT / name, directory / name)
    with (directory / 'Cargo.toml').open('a') as stream:
        stream.write('\n[workspace]\n')


def windows_sdl(directory):
    archive = directory / 'sdl.zip'
    attempts = 3
    for attempt in range(1, attempts + 1):
        print(f'Downloading SDL3 SDK (attempt {attempt}/{attempts}): {SDL_URL}', flush=True)
        try:
            # Reopen with wb on every attempt: never append to a partial ZIP.
            with urllib.request.urlopen(SDL_URL, timeout=120) as source, archive.open('wb') as output:
                shutil.copyfileobj(source, output)
            break
        except (urllib.error.URLError, TimeoutError, ConnectionError, http.client.IncompleteRead) as error:
            transient = not isinstance(error, urllib.error.HTTPError) or error.code in (429, 500, 502, 503, 504)
            if not transient or attempt == attempts:
                raise BuildError(f'SDL3 SDK download failed after {attempt} attempt(s): {SDL_URL}: {error}') from error
            delay = 2 ** attempt
            print(f'SDL3 download interrupted: {error}; retrying in {delay}s', flush=True)
            time.sleep(delay)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SDL_SHA256:
        raise BuildError('SDL3 archive checksum mismatch')
    with zipfile.ZipFile(archive) as source:
        for entry in source.infolist():
            if Path(entry.filename).is_absolute() or '..' in Path(entry.filename).parts:
                raise BuildError('Unsafe path in SDL archive')
        source.extractall(directory / 'sdl')
    return directory / 'sdl/SDL3-3.4.18'


def copy_file(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def notices(destination, iris, metadata):
    destination.mkdir(parents=True)
    copy_file(ROOT / 'LICENSE', destination / 'Prismatic-LICENSE')
    copy_file(ROOT / 'assets/README.md', destination / 'assets-credits.md')
    for source in iris.glob('LICENSE*'):
        if source.is_file():
            copy_file(source, destination / f'Iris-{source.name}')
    for package in metadata['packages']:
        if not package.get('source'):
            continue
        source = Path(package['manifest_path']).parent
        for entry in source.rglob('*'):
            if entry.is_file() and entry.name.upper().startswith(
                    ('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE', 'UNLICENSE')):
                copy_file(entry, destination / 'cargo' / f"{package['name']}-{package['version']}" / entry.relative_to(source))
    for name in ('winit-0.30.13', 'block-0.1.6'):
        for entry in (iris / 'third_party' / name).glob('LICENSE*'):
            copy_file(entry, destination / name / entry.name)


def smoke(binary, *, env=None):
    helper = binary.parent / ('instigator.exe' if os.name == 'nt' else 'instigator')
    if not helper.is_file():
        helper = binary.parent.parent / 'libexec/prismatic/instigator'
    subprocess.run([str(helper), '--version'], check=True, timeout=30)
    environment = dict(env or os.environ, SDL_VIDEODRIVER='dummy', SDL_RENDER_DRIVER='software',
                       SDL_AUDIODRIVER='dummy', IRIS_NO_EXIT_ON_POWEROFF='1')
    with tempfile.TemporaryDirectory(prefix='prismatic-native-smoke-') as temporary:
        directory = Path(temporary)
        environment['IRIS_REX_JIT_PROFILE'] = str(directory / 'rex.bin')
        for flag, timeout in (('--smoke-test', 45), ('--boot-smoke-test', 180)):
            subprocess.run([str(binary), '--config', str(directory / 'prismatic.cfg'), flag],
                           cwd=directory, env=environment, check=True, timeout=timeout)


def linux_payload(stage, binary, licenses, info):
    copy_file(binary.parent / 'instigator', stage / 'usr/libexec/prismatic/instigator', True)
    copy_file(binary, stage / 'usr/bin/prismatic', True)
    copy_file(ART / f'{APP_ID}.desktop', stage / f'usr/share/applications/{APP_ID}.desktop')
    copy_file(ART / f'{APP_ID}.png', stage / f'usr/share/icons/hicolor/512x512/apps/{APP_ID}.png')
    xml = ET.parse(ART / f'{APP_ID}.metainfo.xml')
    destination = stage / f'usr/share/metainfo/{APP_ID}.metainfo.xml'
    destination.parent.mkdir(parents=True)
    xml.write(destination, encoding='utf-8', xml_declaration=True)
    shutil.copytree(licenses, stage / 'usr/share/licenses/prismatic')
    shutil.copytree(ROOT / 'docs', stage / 'usr/share/doc/prismatic/docs')
    copy_file(ROOT / 'README.md', stage / 'usr/share/doc/prismatic/README.md')
    (stage / 'usr/share/doc/prismatic/build-info.json').write_text(json.dumps(info, indent=2) + '\n')


def debian_package(stage, output, info):
    control = ('Source: prismatic\nSection: otherosfs\nPriority: optional\n'
               'Maintainer: Prismatic contributors <salvogendut@gmail.com>\n'
               'Standards-Version: 4.7.0\n\nPackage: prismatic\nArchitecture: any\n'
               'Description: Happy Years SGI workstation emulator\n')
    (stage / 'debian').mkdir()
    (stage / 'debian/control').write_text(control)
    depends = run(['dpkg-shlibdeps', '-O', '-eusr/bin/prismatic'], cwd=stage, capture=True)
    depends = next((s.removeprefix('shlibs:Depends=') for s in depends.splitlines() if s.startswith('shlibs:Depends=')), '')
    if not depends:
        raise BuildError('dpkg-shlibdeps produced no runtime dependencies')
    shutil.rmtree(stage / 'debian')
    (stage / 'DEBIAN').mkdir()
    (stage / 'DEBIAN/control').write_text(
        f"Package: prismatic\nVersion: {info['version']}\nArchitecture: amd64\n"
        'Maintainer: Prismatic contributors <salvogendut@gmail.com>\nSection: otherosfs\nPriority: optional\n'
        f'Depends: {depends}, hicolor-icon-theme\nRecommends: ffmpeg\n'
        'Homepage: https://github.com/salvogendut/prismatic\n'
        'Description: Happy Years SGI workstation emulator\n Rust and SDL3 frontend for the Iris emulator.\n')
    package = output / f"prismatic_{info['version']}_amd64.deb"
    run(['dpkg-deb', '--root-owner-group', '--build', stage, package])
    return package


def fedora_package(stage, output, info, work):
    top = work / 'rpmbuild'
    for name in ('SOURCES', 'SPECS', 'BUILD', 'BUILDROOT', 'RPMS', 'SRPMS'):
        (top / name).mkdir(parents=True)
    with tarfile.open(top / 'SOURCES/payload.tar', 'w') as archive:
        archive.add(stage / 'usr', arcname='usr')
    spec = (ROOT / 'packaging/native/prismatic.spec').read_text().replace('@VERSION@', info['version'])
    (top / 'SPECS/prismatic.spec').write_text(spec)
    run(['rpmbuild', '--define', f'_topdir {top}', '-bb', top / 'SPECS/prismatic.spec'])
    packages = list((top / 'RPMS').rglob('prismatic-*.rpm'))
    if len(packages) != 1:
        raise BuildError(f'Expected one RPM, found {packages}')
    package = output / packages[0].name
    shutil.copyfile(packages[0], package)
    return package


def windows_package(stage, binary, licenses, info, sdl, output):
    copy_file(binary, stage / 'prismatic.exe', True)
    copy_file(binary.parent / 'instigator.exe', stage / 'instigator.exe', True)
    copy_file(sdl / 'lib/x64/SDL3.dll', stage / 'SDL3.dll', True)
    copy_file(sdl / 'LICENSE.txt', stage / 'SDL3-LICENSE.txt')
    redist = Path(os.environ['VCToolsRedistDir']) / 'x64'
    crt = list(redist.glob('Microsoft.VC*.CRT'))
    if len(crt) != 1:
        raise BuildError('Cannot locate the MSVC redistributable runtime')
    for library in crt[0].glob('*.dll'):
        copy_file(library, stage / library.name, True)
    shutil.copytree(licenses, stage / 'licenses')
    shutil.copytree(ROOT / 'docs', stage / 'docs')
    copy_file(ROOT / 'README.md', stage / 'README.md')
    (stage / 'build-info.json').write_text(json.dumps(info, indent=2) + '\n')
    smoke(stage / 'prismatic.exe')
    destination = output / f"Prismatic-{info['version']}-windows-x86_64.zip"
    shutil.make_archive(str(destination.with_suffix('')), 'zip', stage)
    return destination


def macos_package(stage, binary, licenses, info, output):
    app = stage / 'Prismatic.app'
    contents = app / 'Contents'
    executable = contents / 'MacOS/prismatic'
    resources = contents / 'Resources'
    frameworks = contents / 'Frameworks'
    resources.mkdir(parents=True)
    frameworks.mkdir()
    copy_file(binary, executable, True)
    helper = contents / 'MacOS/instigator'
    copy_file(binary.parent / 'instigator', helper, True)
    # macOS normally uses a case-insensitive filesystem: the launcher name
    # must differ from the binary by more than capitalization.
    launcher = contents / 'MacOS/prismatic-launcher'
    copy_file(ROOT / 'packaging/native/prismatic-macos', launcher, True)
    shutil.copytree(licenses, resources / 'licenses')
    shutil.copytree(ROOT / 'docs', resources / 'docs')
    copy_file(ROOT / 'README.md', resources / 'README.md')
    for name in ('SDL3-LICENSE.txt', 'libpcap-LICENSE'):
        copy_file(ROOT / 'packaging/native' / name, resources / 'licenses' / name)
    (resources / 'build-info.json').write_text(json.dumps(info, indent=2) + '\n')
    with (contents / 'Info.plist').open('wb') as stream:
        plistlib.dump({'CFBundleName': 'Prismatic', 'CFBundleDisplayName': 'Prismatic',
                      'CFBundleIdentifier': APP_ID, 'CFBundleExecutable': launcher.name,
                      'CFBundlePackageType': 'APPL', 'CFBundleInfoDictionaryVersion': '6.0',
                      'CFBundleVersion': info['version'], 'CFBundleShortVersionString': info['version'],
                      'CFBundleIconFile': 'Prismatic.icns', 'LSMinimumSystemVersion': '15.0',
                      'LSApplicationCategoryType': 'public.app-category.entertainment',
                      'NSHighResolutionCapable': True,
                      'NSCameraUsageDescription': 'Provide camera input to the emulated SGI workstation when enabled.',
                      'NSMicrophoneUsageDescription': 'Provide audio input to the emulated SGI workstation when enabled.'}, stream)
    iconset = stage / 'Prismatic.iconset'
    iconset.mkdir()
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            name = f'icon_{size}x{size}' + ('@2x' if scale == 2 else '') + '.png'
            run(['sips', '-z', str(size * scale), str(size * scale), ROOT / 'images/prismatic-icon-source.png', '--out', iconset / name])
    run(['iconutil', '-c', 'icns', iconset, '-o', resources / 'Prismatic.icns'])
    prefix = run(['brew', '--prefix'], capture=True)
    run(['dylibbundler', '-od', '-b', '-x', executable, '-d', frameworks,
         '-p', '@executable_path/../Frameworks/', '-s', f'{prefix}/lib',
         '-s', run(['brew', '--prefix', 'libpcap'], capture=True) + '/lib'])
    for library in [helper, executable, *frameworks.glob('*')]:
        if not library.is_file():
            continue
        dependencies = run(['otool', '-L', library], capture=True).splitlines()[1:]
        if any(prefix + '/' in line for line in dependencies):
            raise BuildError(f'Unbundled Homebrew dependency in {library}')
        signing = ['codesign', '--force', '--sign', '-']
        if library == executable:
            signing += ['--entitlements', ROOT / 'packaging/native/macos.entitlements']
        run([*signing, library])
    run(['codesign', '--force', '--sign', '-', '--entitlements', ROOT / 'packaging/native/macos.entitlements', app])
    run(['codesign', '--verify', '--deep', '--strict', app])
    run(['lipo', executable, '-verify_arch', info['architecture']])
    smoke(launcher)
    destination = output / f"Prismatic-{info['version']}-macos-{info['architecture']}.zip"
    run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', app, destination])
    return destination


def build(info, target_platform, output, install_test=False):
    if git(ROOT, 'rev-parse', 'HEAD') != info['prismatic_revision']:
        raise BuildError('Frontend checkout differs from the release plan')
    output.mkdir(parents=True, exist_ok=True)
    cache = ROOT / 'target/native'
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='work-', dir=cache) as temporary:
        work = Path(temporary)
        iris, frontend = work / 'iris', work / 'prismatic'
        checkout_iris(iris, info)
        helper_source = work / 'instigator'
        instigator.checkout(helper_source, info)
        snapshot(frontend)
        features = 'idle-pause,jitv2' if target_platform == 'windows' else 'pcap,idle-pause,jitv2'
        environment = dict(os.environ, CARGO_TARGET_DIR=str(cache / 'cargo'),
                           LIBCHDMAN_FORCE_SOURCE='1', LIBCHDMAN_CPU='portable')
        environment.setdefault('CARGO_BUILD_JOBS', '2')
        sdl = None
        flags = environment.get('CARGO_ENCODED_RUSTFLAGS', '').split('\x1f') if environment.get('CARGO_ENCODED_RUSTFLAGS') else []
        if target_platform == 'windows':
            sdl = windows_sdl(work)
            flags += ['-L', 'native=' + str(sdl / 'lib/x64')]
            environment['PATH'] = str(sdl / 'lib/x64') + os.pathsep + environment['PATH']
        elif target_platform == 'macos':
            for formula in ('sdl3', 'libpcap'):
                prefix = run(['brew', '--prefix', formula], capture=True)
                flags += ['-L', 'native=' + prefix + '/lib']
            environment['MACOSX_DEPLOYMENT_TARGET'] = '15.0'
        if flags:
            environment['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
        run(['cargo', 'test', '--release', '--locked', '--features', features, 'disk_image::tests'], cwd=frontend, env=environment)
        run(['cargo', 'build', '--release', '--locked', '--features', features], cwd=frontend, env=environment)
        metadata = json.loads(run(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=frontend, env=environment, capture=True))
        licenses = work / 'licenses'
        notices(licenses, iris, metadata)
        binary = cache / 'cargo/release' / ('prismatic.exe' if target_platform == 'windows' else 'prismatic')
        instigator.build(helper_source, binary.parent / ('instigator.exe' if target_platform == 'windows' else 'instigator'),
                         cache, licenses / 'instigator')
        environment['PRISMATIC_INSTALL_HELPER'] = str(binary.parent / ('instigator.exe' if target_platform == 'windows' else 'instigator'))
        run(['cargo', 'test', '--release', '--locked', '--features', features,
             'native_instigator_answers_bootp_and_tftp', '--', '--ignored'], cwd=frontend, env=environment)
        arch = 'arm64' if platform.machine().lower() in ('arm64', 'aarch64') else 'x86_64'
        info = dict(info, platform=target_platform, architecture=arch, features=features,
                    rust_toolchain=tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel'])
        stage = work / 'stage'
        stage.mkdir()
        if target_platform in ('fedora', 'debian'):
            linux_payload(stage, binary, licenses, info)
            smoke(stage / 'usr/bin/prismatic', env=environment)
            if target_platform == 'debian':
                package = debian_package(stage, output, info)
            else:
                package = fedora_package(stage, output, info, work)
            if install_test:
                command = ['dpkg', '-i'] if target_platform == 'debian' else ['rpm', '-Uvh', '--replacepkgs']
                run([*command, package])
                smoke(Path('/usr/bin/prismatic'), env=environment)
        elif target_platform == 'windows':
            package = windows_package(stage, binary, licenses, info, sdl, output)
        else:
            package = macos_package(stage, binary, licenses, info, output)
        prefix = f"Prismatic-{info['version']}-{target_platform}-{arch}"
        (output / f'{prefix}-build-info.json').write_text(json.dumps(info, indent=2) + '\n')
        checksum = hashlib.sha256(package.read_bytes()).hexdigest()
        (output / f'{prefix}-SHA256SUMS').write_text(f'{checksum}  {package.name}\n')
        print(f'Package ready: {package}', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('plan', 'build'))
    parser.add_argument('--tag', default=os.environ.get('RELEASE_TAG', ''))
    parser.add_argument('--plan', type=Path, default=ROOT / 'target/native-plan.json')
    parser.add_argument('--platform', choices=PLATFORMS)
    parser.add_argument('--install-test', action='store_true', help='Install Linux package in a disposable CI container and smoke-test it')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/native/dist')
    args = parser.parse_args()
    if args.action == 'plan':
        args.plan.parent.mkdir(parents=True, exist_ok=True)
        args.plan.write_text(json.dumps(plan(args.tag), indent=2) + '\n')
        print(args.plan)
    else:
        if not args.platform:
            parser.error('--platform is required for build')
        build(json.loads(args.plan.read_text()), args.platform, args.output.resolve(), args.install_test)


if __name__ == '__main__':
    try:
        main()
    except (BuildError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'Prismatic native packaging: {error}', file=sys.stderr)
        sys.exit(1)
