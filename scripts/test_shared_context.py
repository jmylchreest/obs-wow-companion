"""Exercise the pre-commit export boundary without touching a real aide store."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("shared_context", Path(__file__).with_name("shared-context.py"))
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)


class SharedContextTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.directory = self.root / ".aide/shared"
        self.directory.mkdir(parents=True)
        self.patches = [patch.object(shared, "ROOT", self.root), patch.object(shared, "SHARED", self.directory)]
        for item in self.patches:
            item.start()
            self.addCleanup(item.stop)

    def test_rejects_private_paths_without_echoing_them(self):
        for path in ("/home/someone/private", "/Users/someone/file", "/root/private", r"C:\Users\someone\private"):
            with self.subTest(path=path):
                (self.directory / "decision.md").write_text(path)
                with self.assertRaises(ValueError) as error:
                    shared.check()
                self.assertNotIn(path, str(error.exception))

    def test_accepts_portable_references(self):
        (self.directory / "decision.md").write_text("Use $HOME/.config or ~/.config; src/main.rs")
        shared.check()

    def test_without_local_store_checks_but_does_not_export(self):
        with patch.object(shared.subprocess, "run") as run:
            shared.export()
            run.assert_not_called()
        (self.directory / "decision.md").write_text("/home/someone/private")
        with self.assertRaises(ValueError):
            shared.export()

    def prepare_store(self):
        (self.root / ".aide/memory").mkdir()

    def test_exports_only_decisions_and_stages_only_shared_directory(self):
        self.prepare_store()
        with patch.object(shared.shutil, "which", return_value="aide"), patch.object(shared.subprocess, "run") as run:
            shared.export()
            self.assertEqual([call.args[0] for call in run.call_args_list], [
                ["aide", "share", "export", "--decisions"], ["git", "add", "--", ".aide/shared"]])

    def test_failed_export_or_private_path_never_stages(self):
        self.prepare_store()
        with patch.object(shared.shutil, "which", return_value="aide"), patch.object(shared.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "aide")) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                shared.export()
            self.assertEqual(run.call_count, 1)
        (self.directory / "decision.md").write_text("/home/someone/private")
        with patch.object(shared.shutil, "which", return_value="aide"), patch.object(shared.subprocess, "run") as run:
            with self.assertRaises(ValueError):
                shared.export()
            self.assertEqual(run.call_count, 1)

    def test_rejects_symlinks(self):
        try:
            (self.directory / "linked.md").symlink_to(self.root / "absent.md")
        except OSError:
            self.skipTest("Creating symlinks is not permitted")
        with self.assertRaisesRegex(ValueError, "symlinks"):
            shared.check()
