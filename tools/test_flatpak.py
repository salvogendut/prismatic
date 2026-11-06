"""Packaging source isolation, lockfile integrity and offline build inputs."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import flatpak
from build import BuildError


class FlatpakTests(unittest.TestCase):
    def test_failed_bundle_export_preserves_previous_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            destination = output / 'Prismatic-x86_64.flatpak'
            destination.write_bytes(b'previous bundle')
            (output / 'SHA256SUMS').write_text('previous checksum')
            def fail(args, **_kwargs):
                Path(args[-2]).write_bytes(b'partial bundle')
                raise BuildError('export failed')
            with patch.object(flatpak, 'execute', side_effect=fail):
                with self.assertRaises(BuildError):
                    flatpak.bundle(output)
            self.assertEqual(destination.read_bytes(), b'previous bundle')
            self.assertEqual((output / 'SHA256SUMS').read_text(), 'previous checksum')
            self.assertFalse(list(output.glob('.bundle-*')))

    def test_successful_bundle_export_updates_artifact_and_checksum(self):
        import hashlib
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            def export(args, **_kwargs):
                Path(args[-2]).write_bytes(b'new bundle')
            with patch.object(flatpak, 'execute', side_effect=export):
                destination = flatpak.bundle(output)
            checksum = hashlib.sha256(destination.read_bytes()).hexdigest()
            self.assertEqual((output / 'SHA256SUMS').read_text(),
                             f'{checksum}  Prismatic-x86_64.flatpak\n')

    def test_crates_are_versioned_checksummed_and_path_dependencies_are_skipped(self):
        checksum = 'a' * 64
        lock = {'package': [
            {'name': 'iris', 'version': '0.1.0'},
            *[{'name': 'sample', 'version': version,
               'source': 'registry+https://github.com/rust-lang/crates.io-index',
               'checksum': checksum} for version in ('1.0.0', '2.0.0')],
        ]}
        sources = flatpak.crate_sources(lock)
        self.assertEqual(sources[0]['dest'], 'cargo/vendor/sample-1.0.0')
        self.assertEqual(sources[2]['dest'], 'cargo/vendor/sample-2.0.0')
        self.assertEqual(sources[0]['sha256'], checksum)
        self.assertEqual(json.loads(sources[1]['contents'])['package'], checksum)
        self.assertIn('offline = true', sources[-1]['contents'])
        self.assertEqual(len(sources), 5)

    def test_unknown_or_unchecksummed_sources_fail_before_build(self):
        for source, checksum in [('git+https://example.invalid/core', 'a' * 64),
                                 ('registry+https://example.invalid', 'a' * 64),
                                 ('registry+https://github.com/rust-lang/crates.io-index', '')]:
            with self.assertRaises(BuildError):
                flatpak.crate_sources({'package': [{'name': 'sample', 'version': '1.0.0',
                                                    'source': source, 'checksum': checksum}]})

    def test_resolving_branch_does_not_checkout_or_modify_remote(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(root), *args],
                                               text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '--initial-branch=xiri')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'Fixture')
            revision = git('rev-parse', 'HEAD')
            (root / 'keep-local.txt').write_text('unsaved local work')
            before = git('status', '--porcelain')
            self.assertEqual(flatpak.resolve_iris(root.as_uri(), 'xiri'), revision)
            self.assertEqual(git('status', '--porcelain'), before)
            self.assertEqual(git('branch', '--show-current'), 'xiri')
            with self.assertRaises(BuildError):
                flatpak.resolve_iris(root.as_uri(), 'missing')

    def test_preparation_pins_iris_and_excludes_personal_runtime_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'project'
            root.mkdir()
            for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'build.rs', 'README.md', 'LICENSE'):
                shutil.copyfile(flatpak.ROOT / name, root / name)
            for name in ('assets', 'src'):
                (root / name).mkdir()
                (root / name / 'fixture').write_text('build input')
            shutil.copytree(flatpak.PACKAGING, root / 'packaging/flatpak')
            output = root / 'target/flatpak'
            output.mkdir(parents=True)
            (root / 'prismatic.cfg').write_text('private preferences')
            (root / 'private-disk.raw').write_bytes(b'private disk')
            def git(_directory, *args):
                return {'log': '1793664000', 'rev-parse': 'b' * 40, 'status': ''}[args[0]]
            with patch.multiple(flatpak, ROOT=root, OUTPUT=output,
                                PACKAGING=root / 'packaging/flatpak'), \
                    patch.object(flatpak, 'resolve_iris', return_value='a' * 40), \
                    patch.object(flatpak, 'git', side_effect=git), \
                    patch.object(flatpak, 'prepare_instigator', return_value=({'instigator_revision': 'c' * 40}, {'name': 'instigator'})):
                manifest_path = flatpak.prepare('https://example.invalid/iris.git', 'xiri', 'v1.2.3', 2)
                manifest = json.loads(manifest_path.read_text())
                source = manifest['modules'][-1]['sources'][1]
                self.assertEqual(source['commit'], 'a' * 40)
                self.assertEqual(source['branch'], 'xiri')
                self.assertEqual(source['url'], 'https://example.invalid/iris.git')
                info = json.loads((output / 'build-info.json').read_text())
                self.assertEqual(info['version'], '1.2.3')
                self.assertEqual(info['instigator_revision'], 'c' * 40)
                self.assertEqual(manifest['modules'][-2]['name'], 'instigator')
                self.assertFalse(info['prismatic_dirty'])
                self.assertTrue((output / 'prismatic/src/fixture').exists())
                self.assertTrue((output / 'prismatic/LICENSE').exists())
                self.assertFalse((output / 'prismatic/prismatic.cfg').exists())
                self.assertFalse((output / 'prismatic/private-disk.raw').exists())
                self.assertFalse((output / 'prismatic/target').exists())
                (root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="nightly-2000-01-01"\n')
                with self.assertRaisesRegex(BuildError, 'Rust archives'):
                    flatpak.prepare('https://example.invalid/iris.git', 'xiri', None, 2)


if __name__ == '__main__':
    unittest.main()
