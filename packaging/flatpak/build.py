"""Runs inside flatpak-builder, with all sources already downloaded."""
import json
from pathlib import Path
import shutil
import subprocess
import xml.etree.ElementTree as ET

root = Path.cwd()
info = json.loads((root / 'build-info.json').read_text())
source = root / 'prismatic'
packaging = source / 'packaging/flatpak'
app_id = info['app_id']

# This is the isolated builder checkout, never a developer's sibling Iris.
# Keep About's branch indication useful while retaining the pinned commit.
subprocess.run(['git', '-C', 'iris', 'checkout', '-B', info['iris_branch']], check=True)
revision = subprocess.check_output(['git', '-C', 'iris', 'rev-parse', 'HEAD'], text=True).strip()
if revision != info['iris_revision']:
    raise SystemExit('Iris checkout does not match the recorded release revision')

# Older Flatpak Builder versions let CMake default to /app/lib64, which the
# runtime loader does not search. Catch a missing runtime dependency early.
if not Path('/app/lib/libpcap.so.1').is_file():
    raise SystemExit('libpcap must be installed in /app/lib before building Prismatic')

subprocess.run(['cargo', 'build', '--manifest-path', str(source / 'Cargo.toml'),
                '--release', '--frozen', '--features', info['features']], check=True)

def install(src, destination, mode=0o644):
    dest = Path('/app') / destination
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dest)
    dest.chmod(mode)

install(source / 'target/release/prismatic', 'libexec/prismatic', 0o755)
install(packaging / 'prismatic', 'bin/prismatic', 0o755)
install(packaging / f'{app_id}.desktop', f'share/applications/{app_id}.desktop')
install(packaging / f'{app_id}.png', f'share/icons/hicolor/512x512/apps/{app_id}.png')
metadata = ET.parse(packaging / f'{app_id}.metainfo.xml')
ET.SubElement(ET.SubElement(metadata.getroot(), 'releases'), 'release',
              version=info['version'], date=info['release_date'])
destination = Path('/app/share/metainfo')
destination.mkdir(parents=True, exist_ok=True)
metadata.write(destination / f'{app_id}.metainfo.xml', encoding='utf-8', xml_declaration=True)
install(root / 'build-info.json', 'share/prismatic/build-info.json')
install(source / 'LICENSE', 'share/licenses/prismatic/Prismatic-LICENSE')
install(root / 'iris/LICENSE', 'share/licenses/prismatic/Iris-LICENSE')
install(root / 'iris/LICENSE-libchdman-rs.txt', 'share/licenses/prismatic/libchdman-rs-LICENSE.txt')
install(root / 'iris/third_party/winit-0.30.13/LICENSE', 'share/licenses/prismatic/winit-LICENSE')
install(source / 'assets/README.md', 'share/licenses/prismatic/assets-credits.md')

# Keep the notices distributed with vendored dependencies, including the native
# codec sources nested in libchdman. They are not relicensed with the frontend.
for crate in (root / 'cargo/vendor').iterdir():
    for notice in crate.rglob('*'):
        if notice.is_file() and notice.name.upper().startswith(
                ('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE', 'UNLICENSE')):
            install(notice, Path('share/licenses/prismatic/cargo') / crate.name / notice.relative_to(crate))
