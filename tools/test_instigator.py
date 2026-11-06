"""Helper source pinning, archive safety and preservation of usable builds."""
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import instigator
from build import BuildError, git


class InstigatorTests(unittest.TestCase):
    def test_pinned_checkout_does_not_follow_newer_branch_tip(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            remote = root / 'remote'
            remote.mkdir()
            git(remote, 'init', '--initial-branch=main')
            def commit():
                git(remote, '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                    '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'fixture')
                return git(remote, 'rev-parse', 'HEAD')
            first = commit()
            info = instigator.resolve(remote.as_uri(), 'main')
            latest = commit()
            self.assertNotEqual(first, latest)
            instigator.checkout(root / 'checkout', info)
            self.assertEqual(git(root / 'checkout', 'rev-parse', 'HEAD'), first)
            self.assertEqual(git(remote, 'rev-parse', 'HEAD'), latest)

    def test_tar_rejects_traversal_and_symlinks_before_extracting(self):
        for name, kind in [('../escape', tarfile.REGTYPE), ('escape', tarfile.SYMTYPE)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / 'bad.tar'
                with tarfile.open(archive, 'w') as stream:
                    entry = tarfile.TarInfo(name)
                    entry.type = kind
                    entry.linkname = '/tmp'
                    stream.addfile(entry)
                with self.assertRaisesRegex(BuildError, 'Unsafe archive'):
                    instigator.extract_tar(archive, root / 'output')
                self.assertFalse((root / 'output').exists())

    def test_go_checksum_failure_does_not_publish_toolchain(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(instigator.platform, 'machine', return_value='x86_64'), \
                    patch.object(instigator.platform, 'system', return_value='Linux'), \
                    patch.object(instigator.urllib.request, 'urlopen', return_value=io.BytesIO(b'bad archive')):
                with self.assertRaisesRegex(BuildError, 'checksum mismatch'):
                    instigator.toolchain(root)
            self.assertFalse(list(root.iterdir()))

    def test_failed_go_build_preserves_previous_helper(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'bin/instigator'
            output.parent.mkdir()
            output.write_bytes(b'last working helper')
            with patch.object(instigator, 'vendor'), patch.object(instigator, 'toolchain', return_value=Path('/go')), \
                 patch.object(instigator, 'go_environment', return_value={}), patch.object(instigator, 'execute', side_effect=BuildError('failed build')):
                with self.assertRaisesRegex(BuildError, 'failed build'):
                    instigator.build(root, output, root / 'cache')
            self.assertEqual(output.read_bytes(), b'last working helper')
            self.assertFalse(list((root / 'cache').glob('.instigator-build-*')))

    def test_linux_payload_contains_private_helper_and_notices(self):
        import native
        import json
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'prismatic').write_bytes(b'app')
            (root / 'instigator').write_bytes(b'helper')
            licenses = root / 'licenses/instigator'
            licenses.mkdir(parents=True)
            (licenses / 'LICENSE').write_text('notice')
            native.linux_payload(root / 'stage', root / 'prismatic', licenses.parent,
                                 {'instigator_revision': 'a' * 40})
            self.assertEqual((root / 'stage/usr/libexec/prismatic/instigator').read_bytes(), b'helper')
            self.assertTrue((root / 'stage/usr/share/licenses/prismatic/instigator/LICENSE').is_file())
            info = json.loads((root / 'stage/usr/share/doc/prismatic/build-info.json').read_text())
            self.assertEqual(info['instigator_revision'], 'a' * 40)


if __name__ == '__main__':
    unittest.main()
