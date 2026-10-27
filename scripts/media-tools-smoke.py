#!/usr/bin/env python3
"""Exercise the Rust media adapter with isolated executable lookup on each OS."""
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tempfile

from media_fixture import create_fixture


def main():
    tools = {name: Path(shutil.which(name)).resolve() for name in ("ffmpeg", "ffprobe")}
    # Chocolatey's PATH entry can be a shim, which cannot be relocated on its own.
    if os.name == "nt":
        chocolatey = Path(os.environ.get("ChocolateyInstall", r"C:\ProgramData\chocolatey"))
        for name, source in tools.items():
            if source.parent == chocolatey / "bin":
                candidates = list((chocolatey / "lib" / "ffmpeg").rglob(name + ".exe"))
                assert len(candidates) == 1, f"Cannot locate actual {name} behind Chocolatey shim: {candidates}"
                tools[name] = candidates[0]
    report = {"platform": platform.platform(), "tools": {}, "passed": []}
    for name, executable in tools.items():
        report["tools"][name] = subprocess.check_output([str(executable), "-version"], text=True).splitlines()[0]
    build = subprocess.run(
        ["cargo", "test", "-p", "owc-media", "--locked", "--test", "tool_paths",
         "--no-run", "--message-format=json"], capture_output=True, text=True,
    )
    if build.returncode:
        print(build.stdout)
        print(build.stderr)
        build.check_returncode()
    executables = [item["executable"] for line in build.stdout.splitlines()
                   if (item := json.loads(line)).get("reason") == "compiler-artifact"
                   and item.get("executable")]
    assert len(executables) == 1, executables
    with tempfile.TemporaryDirectory(prefix="owc media café ") as directory:
        root = Path(directory)
        create_fixture(root / "source.mkv")
        bin_dir = root / "tools with spaces – 音声"
        bin_dir.mkdir()
        copied = {}
        for name, source in tools.items():
            destination = bin_dir / (name + (".exe" if os.name == "nt" else ""))
            if os.name == "nt":
                shutil.copy2(source, destination)
                # Shared builds need their adjacent runtime DLLs as well.
                for dll in source.parent.glob("*.dll"):
                    shutil.copy2(dll, bin_dir / dll.name)
            else:
                destination.symlink_to(source)
            copied[name] = str(destination)
        cases = [
            ("system PATH", "ffmpeg", "ffprobe", os.environ["PATH"], "ready"),
            ("Unicode PATH", "ffmpeg", "ffprobe", str(bin_dir), "ready"),
            ("explicit paths without PATH", copied["ffmpeg"], copied["ffprobe"], "", "ready"),
            ("missing ffmpeg", str(root / "absent-ffmpeg"), copied["ffprobe"], "", "missing_encoder"),
            ("missing ffprobe", copied["ffmpeg"], str(root / "absent-ffprobe"), "", "missing_probe"),
            ("neither installed", "ffmpeg", "ffprobe", "", "missing_probe"),
        ]
        for label, ffmpeg, ffprobe, path, expected in cases:
            environment = dict(os.environ, PATH=path, OWC_TEST_FFMPEG=ffmpeg,
                               OWC_TEST_FFPROBE=ffprobe, OWC_MEDIA_EXPECT=expected,
                               OWC_MEDIA_FIXTURE=str(root))
            subprocess.run([executables[0], "--ignored", "--exact", "configured_tools"],
                           env=environment, cwd=root, check=True, timeout=120)
            report["passed"].append(label)
            print(f"Passed: {label}", flush=True)
    output = Path("target/media-tools-report.json")
    output.parent.mkdir(exist_ok=True)
    output.write_text(json.dumps(report, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
