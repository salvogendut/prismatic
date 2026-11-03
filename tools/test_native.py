"""Native source pinning, draft/source isolation and packaging failure paths."""
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import native
from build import BuildError


class NativeTests(unittest.TestCase):
    def test_cargo_metadata_text_is_utf8_even_with_a_windows_locale(self):
        # Cargo emits UTF-8, including dependency authors/descriptions; Python
        # 3.13 otherwise uses the Windows code page for subprocess output.
        with patch('subprocess._text_encoding', return_value='cp1252'):
            text = native.run([sys.executable, '-c',
                               'import sys; sys.stdout.buffer.write(bytes.fromhex("e38181"))'], capture=True)
        self.assertEqual(text, '\u3041')

    def test_release_plan_honors_makefile_and_checks_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Makefile').write_text('IRIS_REPO ?= https://example.invalid/iris.git\nIRIS_BRANCH ?= custom\n')
            (root / 'Cargo.toml').write_text('[package]\nversion="1.2.3"\n')
            def git(_root, *args):
                if args[0] == 'ls-remote':
                    self.assertIn('https://example.invalid/iris.git', args)
                    self.assertEqual(args[-1], 'refs/heads/custom')
                    return 'a' * 40 + '\trefs/heads/custom'
                return 'b' * 40
            with patch.object(native, 'ROOT', root), patch.dict(native.os.environ, {}, clear=True), patch.object(native, 'git', side_effect=git):
                info = native.plan('v1.2.3')
                self.assertEqual(info['iris_revision'], 'a' * 40)
                self.assertEqual(info['iris_branch'], 'custom')
                with self.assertRaisesRegex(BuildError, 'does not match'):
                    native.plan('v1.2.4')

    def test_checkout_keeps_planned_revision_when_remote_advances(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            remote = root / 'remote'
            remote.mkdir()
            def git(*args):
                return subprocess.check_output(['git', '-C', str(remote), *args], text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '--initial-branch=xiri')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'first')
            original = git('rev-parse', 'HEAD')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'second')
            latest = git('rev-parse', 'HEAD')
            destination = root / 'isolated'
            native.checkout_iris(destination, {'iris_repository': remote.as_uri(), 'iris_branch': 'xiri', 'iris_revision': original})
            self.assertEqual(native.git(destination, 'rev-parse', 'HEAD'), original)
            self.assertEqual(native.git(destination, 'branch', '--show-current'), 'xiri')
            self.assertEqual(git('rev-parse', 'HEAD'), latest)

    def test_snapshot_excludes_preferences_disks_and_other_workspaces(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain.toml'):
                (root / name).write_text('fixture\n')
            for name in ('src', 'assets'):
                (root / name).mkdir()
                (root / name / 'fixture').write_text('input')
            (root / 'prismatic.cfg').write_text('private')
            (root / 'guest.chd').write_text('private')
            with patch.object(native, 'ROOT', root):
                native.snapshot(root / 'snapshot')
            self.assertFalse((root / 'snapshot/prismatic.cfg').exists())
            self.assertFalse((root / 'snapshot/guest.chd').exists())
            self.assertIn('[workspace]', (root / 'snapshot/Cargo.toml').read_text())

    def test_sdl_archive_integrity_and_paths_are_checked_before_extraction(self):
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as archive:
            archive.writestr('../escape', 'bad')
        payload = data.getvalue()
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(native.urllib.request, 'urlopen', return_value=io.BytesIO(payload)):
                with self.assertRaisesRegex(BuildError, 'checksum'):
                    native.windows_sdl(Path(temporary))
            with patch.object(native.urllib.request, 'urlopen', return_value=io.BytesIO(payload)), patch.object(native, 'SDL_SHA256', hashlib.sha256(payload).hexdigest()):
                with self.assertRaisesRegex(BuildError, 'Unsafe path'):
                    native.windows_sdl(Path(temporary))

    def test_mismatched_frontend_plan_fails_before_creating_build_files(self):
        with patch.object(native, 'git', return_value='b' * 40):
            with self.assertRaisesRegex(BuildError, 'differs'):
                native.build({'prismatic_revision': 'a' * 40}, 'debian', Path('unused'))

    def test_deb_rejects_empty_dependency_scan(self):
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary)
            with patch.object(native, 'run', return_value=''):
                with self.assertRaisesRegex(BuildError, 'no runtime dependencies'):
                    native.debian_package(stage, stage, {'version': '1.2.3'})


if __name__ == '__main__':
    unittest.main()
