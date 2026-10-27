"""Release-boundary tests: failed prerequisites and incomplete artifacts must not publish."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import yaml

spec = importlib.util.spec_from_file_location("ci_tools", Path(__file__).with_name("ci-tools.py"))
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)

class ReleaseTests(unittest.TestCase):
    def test_composite_steps_use_supported_timeout_mechanism(self):
        action = yaml.safe_load(Path(".github/actions/build-environment/action.yml").read_text(encoding="utf-8"))
        for step in action["runs"]["steps"]:
            self.assertNotIn("timeout-minutes", step)
        dependencies = next(step for step in action["runs"]["steps"] if step.get("name") == "Linux dependencies")
        self.assertIn("timeout 420s apt-get", dependencies["run"])

    def test_version_tag_is_exact_and_not_shell_text(self):
        self.assertEqual(ci.release_tag("v" + ci.version()), "v" + ci.version())
        for tag in ["v9.9.9", "main", "v" + ci.version() + ";echo bad"]:
            with self.assertRaises(ValueError):
                ci.release_tag(tag)

    def test_corrupt_archive_is_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "owc-app-x86_64-unknown-linux-gnu.tar.xz"
            path.write_bytes(b"corrupt")
            Path(str(path) + ".sha256").write_text("0" * 64, encoding="utf-8")
            with self.assertRaises(ValueError):
                ci.verified_archive(Path(root), "x86_64-unknown-linux-gnu")

    def test_incomplete_bundle_cannot_call_github(self):
        with tempfile.TemporaryDirectory() as root:
            metadata = dict(schema_version=1, tag="v"+ci.version(), version=ci.version(), files={})
            (Path(root)/"bundle.json").write_text(json.dumps(metadata), encoding="utf-8")
            with patch.object(ci.subprocess, "run") as run:
                with self.assertRaises(ValueError):
                    ci.publish(Path(root), metadata["tag"])
                run.assert_not_called()

    def test_missing_windows_package_blocks_bundle(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            archive = directory / "owc-app-x86_64-unknown-linux-gnu.tar.xz"
            archive.write_bytes(b"linux fixture")
            Path(str(archive)+".sha256").write_text(ci.checksum(archive), encoding="utf-8")
            with self.assertRaises(FileNotFoundError):
                ci.bundle(directory)

    def test_publication_requires_successful_quality_and_packages(self):
        release = yaml.safe_load(Path(".github/workflows/release.yml").read_text(encoding="utf-8"))
        publish = release["jobs"]["publish"]
        self.assertEqual(set(publish["needs"]), {"validate", "quality", "packages"})
        self.assertNotIn("if", publish)  # Default success() rejects failed/skipped prerequisites.
        self.assertEqual(release["permissions"], {"contents":"read"})
        self.assertEqual(publish["permissions"], {"contents":"write"})
        packages = yaml.safe_load(Path(".github/workflows/packages.yml").read_text(encoding="utf-8"))
        self.assertEqual(packages["jobs"]["bundle"]["needs"], "local")
        self.assertNotIn("if", packages["jobs"]["bundle"])
        self.assertEqual(packages["jobs"]["installers"]["needs"], "bundle")
        self.assertNotIn("if", packages["jobs"]["installers"])

    def test_complete_bundle_publishes_only_untampered_artifacts(self):
        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as root:
            try:
                os.chdir(root)
                Path("Cargo.toml").write_text('[workspace.package]\nversion="0.1.0-dev.1"\n', encoding="utf-8")
                Path("CHANGELOG.md").write_text("Fixture release notes", encoding="utf-8")
                directory = Path("input")
                directory.mkdir()
                for name in ci.artifact_names():
                    (directory/name).write_text(name, encoding="utf-8")
                for target, extension in ci.TARGETS.items():
                    archive = directory / ("owc-app-" + target + extension)
                    Path(str(archive)+".sha256").write_text(ci.checksum(archive), encoding="utf-8")
                    installer = "owc-app-installer.ps1" if target.endswith("msvc") else "owc-app-installer.sh"
                    (directory/installer).write_text("https://github.com/jmylchreest/obs-wow-companion/releases/download/v0.1.0-dev.1\n" + ("  Invoke-DownloadFile -client $wc -url $url -path $dir_path\n" if target.endswith("msvc") else ci.checksum(archive)), encoding="utf-8")
                (directory/"source.tar.gz.sha256").write_text(ci.checksum(directory/"source.tar.gz"), encoding="utf-8")
                ci.bundle(directory)
                patched = (directory/"owc-app-installer.ps1").read_text()
                self.assertIn("Get-FileHash -LiteralPath $dir_path -Algorithm SHA256", patched)
                self.assertIn("Archive checksum mismatch; installation cancelled", patched)
                self.assertEqual(patched.count("# Companion archive integrity check"), 1)
                output = Path("target/release-bundle")
                self.assertEqual((output/"release-notes.md").read_text(), "Fixture release notes")
                notes = Path("docs/releases/0.1.0-dev.1.md")
                notes.parent.mkdir(parents=True)
                notes.write_text("Player-facing preview notes", encoding="utf-8")
                ci.bundle(directory)
                self.assertEqual((output/"release-notes.md").read_text(), "Player-facing preview notes")
                with patch.object(ci.subprocess, "run") as run:
                    ci.publish(output, "v0.1.0-dev.1")
                    command = run.call_args.args[0]
                    self.assertIn("--verify-tag", command)
                    self.assertIn("--prerelease", command)
                (output/"owc-app-installer.ps1").write_text("tampered", encoding="utf-8")
                with patch.object(ci.subprocess, "run") as run:
                    with self.assertRaises(ValueError):
                        ci.publish(output, "v0.1.0-dev.1")
                    run.assert_not_called()
            finally:
                os.chdir(previous)

    def test_windows_template_changes_fail_closed(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            archive = directory / "owc-app-x86_64-pc-windows-msvc.zip"
            archive.write_bytes(b"fixture")
            Path(str(archive)+".sha256").write_text(ci.checksum(archive))
            installer = directory / "owc-app-installer.ps1"
            installer.write_text("Unexpected upstream download template")
            with self.assertRaises(ValueError):
                ci.secure_windows_installer(directory)
            self.assertEqual(installer.read_text(), "Unexpected upstream download template")

    def test_external_actions_are_immutable(self):
        for path in [*Path(".github/workflows").glob("*.yml"), *Path(".github/actions").rglob("action.yml")]:
            for line in path.read_text(encoding="utf-8").splitlines():
                if "uses:" in line:
                    action = line.split("uses:", 1)[1].strip().split()[0]
                    if not action.startswith("./"):
                        self.assertRegex(action, r"^[^@]+@[0-9a-f]{40}$", str(path))

if __name__ == "__main__":
    unittest.main()
