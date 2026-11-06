"""Bootstrap regression tests use disposable local Git repositories, no network."""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import build
import instigator


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="prismatic-build-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.remote = self.root / "origin with spaces.git"
        self.git(self.root, "init", "--bare", "--initial-branch=xiri", str(self.remote))
        self.seed = self.root / "seed"
        self.seed.mkdir()
        self.git(self.seed, "init", "--initial-branch=xiri")
        self.identity(self.seed)
        self.commit(self.seed, "first")
        self.git(self.seed, "remote", "add", "origin", self.remote.as_uri())
        self.git(self.seed, "push", "origin", "xiri")
        self.directory = self.root / "cache"
        self.enterContext(patch.dict(os.environ, INSTIGATOR_REPO=self.remote.as_uri(), INSTIGATOR_BRANCH="xiri"))
        self.enterContext(patch.object(instigator, "build"))

    def git(self, directory, *args):
        return subprocess.check_output(
            ["git", *args], cwd=directory, stderr=subprocess.PIPE, text=True,
        ).strip()

    def identity(self, directory):
        self.git(directory, "config", "user.name", "Bootstrap test")
        self.git(directory, "config", "user.email", "bootstrap@example.invalid")
        self.git(directory, "config", "commit.gpgsign", "false")

    def commit(self, directory, text):
        (directory / "source.txt").write_text(text)
        self.git(directory, "add", "source.txt")
        self.git(directory, "commit", "-m", text)
        return self.git(directory, "rev-parse", "HEAD")

    def fetch(self):
        return build.fetch_iris(self.directory, self.remote.as_uri(), "xiri")

    def frontend(self):
        root = self.root / "frontend"
        root.mkdir()
        (root / "src").mkdir()
        (root / "assets").mkdir()
        (root / "src/main.rs").write_text("fn main() {}\n")
        (root / "Cargo.toml").write_text('[package]\nname="prismatic"\nversion="0.1.0"\nedition="2024"\n[dependencies]\niris={path="../iris"}\n')
        (root / "Cargo.lock").write_text("# locked inputs\n")
        for name in ("build.rs", "rust-toolchain.toml", "README.md"):
            (root / name).write_text("test input\n")
        return root

    def fake_cargo(self):
        script = self.root / "fake cargo.py"
        script.write_text(
            "import json,os,sys\n"
            "from pathlib import Path\n"
            "with Path(os.environ['BOOTSTRAP_LOG']).open('a') as f:\n"
            " f.write(json.dumps({'args':sys.argv[1:],'cwd':os.getcwd(),'target':os.environ['CARGO_TARGET_DIR']})+'\\n')\n"
            "sys.exit(int(os.environ.get('FAIL_CARGO','0')))\n"
        )
        return shlex.join([sys.executable, str(script)])

    def test_clone_then_fast_forward_shallow_checkout(self):
        source, revision = self.fetch()
        self.assertEqual(revision, self.git(self.seed, "rev-parse", "HEAD"))
        self.assertEqual(self.git(source, "rev-parse", "--is-shallow-repository"), "true")
        newest = self.commit(self.seed, "second")
        self.git(self.seed, "push", "origin", "xiri")
        source, revision = self.fetch()
        self.assertEqual(revision, newest)
        self.assertEqual((source / "source.txt").read_text(), "second")

    def test_dirty_checkout_is_preserved(self):
        source, revision = self.fetch()
        for staged in (False, True):
            (source / "source.txt").write_text("local work")
            if staged:
                self.git(source, "add", "source.txt")
            with self.assertRaisesRegex(build.BuildError, "local changes"):
                self.fetch()
            self.assertEqual((source / "source.txt").read_text(), "local work")
            self.assertEqual(self.git(source, "rev-parse", "HEAD"), revision)

    def test_untracked_work_is_preserved(self):
        source, _ = self.fetch()
        (source / "notes.txt").write_text("keep me")
        with self.assertRaisesRegex(build.BuildError, "local changes"):
            self.fetch()
        self.assertEqual((source / "notes.txt").read_text(), "keep me")

    def test_local_and_divergent_commits_are_never_reset(self):
        source, _ = self.fetch()
        self.identity(source)
        local = self.commit(source, "local commit")
        for advance_remote in (False, True):
            if advance_remote:
                self.commit(self.seed, "remote commit")
                self.git(self.seed, "push", "origin", "xiri")
            with self.assertRaisesRegex(build.BuildError, "local or divergent"):
                self.fetch()
            self.assertEqual(self.git(source, "rev-parse", "HEAD"), local)

    def test_changed_branch_or_remote_is_rejected(self):
        source, _ = self.fetch()
        self.git(source, "switch", "-c", "local-topic")
        with self.assertRaisesRegex(build.BuildError, "branch has changed"):
            self.fetch()
        self.git(source, "switch", "xiri")
        self.git(source, "remote", "set-url", "origin", "another-repository")
        with self.assertRaisesRegex(build.BuildError, "origin has changed"):
            self.fetch()

    def test_failed_clone_leaves_no_partial_checkout(self):
        with self.assertRaises(build.BuildError):
            build.fetch_iris(self.directory, self.remote.as_uri(), "missing-branch")
        self.assertFalse((self.directory / "iris").exists())
        self.assertEqual(list(self.directory.glob(".clone-*")), [])

    def test_source_cache_separates_repository_and_branch(self):
        a = build.source_directory(self.directory, "repo-a", "xiri")
        b = build.source_directory(self.directory, "repo-a", "main")
        c = build.source_directory(self.directory, "repo-b", "xiri")
        self.assertEqual(len({a, b, c}), 3)

    def test_generated_workspace_uses_live_sources_and_preserves_root_lock(self):
        source, _ = self.fetch()
        root = self.frontend()
        manifest = build.prepare_frontend(root, self.directory)
        self.assertEqual((manifest.parent / "../iris").resolve(), source)
        self.assertEqual((manifest.parent / "src").resolve(), root / "src")
        (root / "src/main.rs").write_text("// latest edit\n")
        self.assertEqual((manifest.parent / "src/main.rs").read_text(), "// latest edit\n")
        (manifest.parent / "Cargo.lock").write_text("changed copy")
        self.assertEqual((root / "Cargo.lock").read_text(), "# locked inputs\n")
        build.prepare_frontend(root, self.directory)
        self.assertEqual((manifest.parent / "Cargo.lock").read_text(), "# locked inputs\n")

    def test_unexpected_workspace_files_are_not_overwritten(self):
        self.directory.mkdir()
        root = self.frontend()
        frontend = self.directory / "prismatic"
        frontend.mkdir()
        (frontend / "src").write_text("user file")
        with self.assertRaisesRegex(build.BuildError, "Unexpected file"):
            build.prepare_frontend(root, self.directory)
        self.assertEqual((frontend / "src").read_text(), "user file")

    def test_makefile_repo_and_branch_overrides_reach_git_and_cargo(self):
        self.git(self.seed, "switch", "-c", "topic/custom")
        newest = self.commit(self.seed, "custom branch")
        self.git(self.seed, "push", "origin", "topic/custom")
        root = self.frontend()
        shutil.copy(build.ROOT / "Makefile", root)
        (root / "tools").mkdir()
        shutil.copy(Path(build.__file__), root / "tools/build.py")
        (root / "tools/instigator.py").write_text(Path(instigator.__file__).read_text() + "\ndef build(*args, **kwargs): pass\n")
        log = self.root / "cargo.jsonl"
        env = dict(os.environ, BOOTSTRAP_LOG=str(log))
        subprocess.run([
            "make", "run", f"IRIS_REPO={self.remote.as_uri()}", "IRIS_BRANCH=topic/custom",
            f"INSTIGATOR_REPO={self.remote.as_uri()}", "INSTIGATOR_BRANCH=topic/custom",
            f"CARGO={self.fake_cargo()}", f"PYTHON={sys.executable}", "FEATURES=", "PROFILE=dev",
            'ARGS=--config "settings with spaces.cfg"',
        ], cwd=root, env=env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        call = json.loads(log.read_text())
        self.assertEqual(call["cwd"], str(root))
        self.assertEqual(call["args"][-3:], ["--", "--config", "settings with spaces.cfg"])
        self.assertNotIn("--features", call["args"])
        self.assertIn("--locked", call["args"])
        info = json.loads(next((root / "target/bootstrap").glob("*/build-info.json")).read_text())
        self.assertEqual(info["iris_revision"], newest)
        self.assertEqual(info["iris_branch"], "topic/custom")
        self.assertEqual(info["instigator_revision"], newest)
        self.assertEqual(info["instigator_branch"], "topic/custom")

    def test_cargo_failure_propagates_and_does_not_publish_success(self):
        root = self.frontend()
        with patch.object(build, "ROOT", root), patch.dict(os.environ, {
            "CARGO": self.fake_cargo(), "FAIL_CARGO": "17", "BOOTSTRAP_LOG": str(self.root / "cargo.jsonl"),
        }):
            with self.assertRaisesRegex(build.BuildError, "status 17"):
                build.main(["build", "--repo", self.remote.as_uri(), "--branch", "xiri"])
        self.assertEqual(list((root / "target/bootstrap").glob("*/build-info.json")), [])

    def test_checks_preserve_build_metadata_and_run_required_commands(self):
        root = self.frontend()
        log = self.root / "cargo.jsonl"
        with patch.object(build, "ROOT", root), patch.dict(os.environ, {
            "CARGO": self.fake_cargo(), "BOOTSTRAP_LOG": str(log),
        }):
            args = ["--repo", self.remote.as_uri(), "--branch", "xiri"]
            build.main(["build", *args])
            info_path = next((root / "target/bootstrap").glob("*/build-info.json"))
            original = info_path.read_bytes()
            build.main(["check", *args, "--features", ""])
        self.assertEqual(info_path.read_bytes(), original)
        calls = [json.loads(line)["args"] for line in log.read_text().splitlines()][1:]
        self.assertEqual([call[0] for call in calls], ["fmt", "clippy", "test"])
        self.assertNotIn("--all", calls[0])
        self.assertEqual(calls[0][-3:], ["-p", "prismatic", "--check"])
        self.assertIn("--locked", calls[1])
        self.assertIn("--no-deps", calls[1])
        self.assertEqual(calls[1][-3:], ["--", "-D", "warnings"])
        self.assertIn("--locked", calls[2])
        self.assertTrue((info_path.parent / "check-info.json").exists())

    def test_default_repository_branch_and_features(self):
        with patch.dict(os.environ, {}, clear=True):
            args = build.arguments([])
        self.assertEqual(args.repo, "https://github.com/salvogendut/iris.git")
        self.assertEqual(args.branch, "xiri")
        self.assertEqual(args.features, "pcap,idle-pause,jitv2")
        self.assertEqual(args.action, "build")


if __name__ == "__main__":
    unittest.main()
