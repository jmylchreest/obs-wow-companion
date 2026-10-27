#!/usr/bin/env python3
"""CI artifact verification and publication. Importing this module has no side effects."""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.request
import zipfile

TARGETS = {"x86_64-unknown-linux-gnu": ".tar.xz", "x86_64-pc-windows-msvc": ".zip"}

def artifact_names():
    return ["owc-app-installer.sh", "owc-app-installer.ps1", "source.tar.gz", "source.tar.gz.sha256"] + [
        "owc-app-" + target + suffix + tail
        for target, suffix in TARGETS.items() for tail in ["", ".sha256"]]

def version():
    return tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]

def release_tag(tag):
    expected = "v" + version()
    if tag != expected or not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", tag):
        raise ValueError(f"Release tag must match workspace version: {expected}")
    return tag

def checksum(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()

def verified_archive(directory, target):
    path = directory / ("owc-app-" + target + TARGETS[target])
    expected = Path(str(path) + ".sha256").read_text(encoding="utf-8").split()[0]
    if checksum(path) != expected:
        raise ValueError(f"Checksum mismatch: {path.name}")
    return path

def package(directory, target):
    archive = verified_archive(directory, target)
    unpack = Path(tempfile.mkdtemp(prefix="owc-package-"))
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as source:
            for item in source.infolist():
                path = Path(item.filename.replace("\\", "/"))
                if path.is_absolute() or ".." in path.parts or ":" in item.filename:
                    raise ValueError("Unsafe archive path")
            source.extractall(unpack)
    else:
        with tarfile.open(archive) as source:
            source.extractall(unpack, filter="data")
    executable = "obs-wow-companion.exe" if target.endswith("msvc") else "obs-wow-companion"
    matches = list(unpack.rglob(executable))
    if len(matches) != 1:
        raise ValueError("Expected exactly one application binary")
    binary = matches[0].resolve()
    for required in ["LICENSE", "README.md", "THIRD-PARTY-NOTICES.txt", "docs/obs-setup.md",
                     "docs/images/obs-recording.png", "templates/obs/manifest.json",
                     "templates/obs/OWC-Starter/basic.ini", "templates/obs/OWC-Starter-scenes.json"]:
        if not (binary.parent / required).is_file():
            raise ValueError(f"Package is missing {required}")
    subprocess.run([str(binary), "--help"], check=True, timeout=20)
    reported = subprocess.check_output([str(binary), "--version"], text=True, timeout=20).strip()
    if reported.split()[-1] != version():
        raise ValueError("Packaged binary version differs from workspace")
    if os.environ.get("GITHUB_ENV"):
        with Path(os.environ["GITHUB_ENV"]).open("a") as env:
            env.write(f"OWC_BINARY={binary}\n")
    print(f"Verified package: {binary}")
    return binary

def secure_windows_installer(directory):
    # cargo-dist 0.32's PowerShell template omits the shell installer's checksum check.
    archive = verified_archive(directory, "x86_64-pc-windows-msvc")
    installer = directory / "owc-app-installer.ps1"
    text = installer.read_text(encoding="utf-8")
    anchor = "  Invoke-DownloadFile -client $wc -url $url -path $dir_path\n"
    check = (f"  # Companion archive integrity check\n"
             f"  if ($artifact_name -ne '{archive.name}' -or "
             f"(Get-FileHash -LiteralPath $dir_path -Algorithm SHA256).Hash -ne '{checksum(archive)}') {{\n"
             '    throw "Archive checksum mismatch; installation cancelled"\n'
             "  }\n")
    if anchor + check in text:
        return
    if text.count(anchor) != 1 or "# Companion archive integrity check" in text:
        raise ValueError("Unrecognised PowerShell installer template; review checksum placement")
    installer.write_text(text.replace(anchor, anchor + check), encoding="utf-8")


def bundle(directory):
    for target in TARGETS:
        verified_archive(directory, target)
    required = ["owc-app-installer.sh", "owc-app-installer.ps1", "source.tar.gz", "source.tar.gz.sha256"]
    for filename in required:
        if not (directory / filename).is_file():
            raise ValueError(f"Missing release artifact: {filename}")
    if checksum(directory / "source.tar.gz") != (directory / "source.tar.gz.sha256").read_text(encoding="utf-8").split()[0]:
        raise ValueError("Source archive checksum mismatch")
    secure_windows_installer(directory)
    for target, installer in [("x86_64-unknown-linux-gnu", "owc-app-installer.sh"),
                              ("x86_64-pc-windows-msvc", "owc-app-installer.ps1")]:
        text = (directory / installer).read_text(encoding="utf-8")
        url = "https://github.com/jmylchreest/obs-wow-companion/releases/download/v" + version()
        if url not in text or checksum(verified_archive(directory, target)) not in text:
            raise ValueError(f"Installer lacks the release URL or embedded archive checksum: {installer}")
    destination = Path("target/release-bundle")
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    artifacts = artifact_names()
    digests = {}
    for filename in artifacts:
        shutil.copy2(directory / filename, destination / filename)
        digests[filename] = checksum(destination / filename)
    metadata = {"schema_version": 1, "version": version(), "tag": "v" + version(), "files": digests}
    (destination / "bundle.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    (destination / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(digests.items())), encoding="utf-8")
    notes = Path("docs/releases") / (version() + ".md")
    if not notes.is_file():
        notes = Path("CHANGELOG.md")
    (destination / "release-notes.md").write_text(notes.read_text(encoding="utf-8"), encoding="utf-8")
    print(f"Verified both platform archives and installers: {destination}")

def publish(directory, tag):
    release_tag(tag)
    metadata = json.loads((directory / "bundle.json").read_text(encoding="utf-8"))
    if metadata["tag"] != tag or metadata["version"] != version():
        raise ValueError("Bundle version does not match release tag")
    if metadata.get("schema_version") != 1 or set(metadata["files"]) != set(artifact_names()):
        raise ValueError("Incomplete or unsupported release bundle")
    for filename, digest in metadata["files"].items():
        if Path(filename).name != filename or checksum(directory / filename) != digest:
            raise ValueError(f"Invalid release artifact: {filename}")
    for target in TARGETS:
        verified_archive(directory, target)
    checksums = "".join(f"{digest}  {name}\n" for name, digest in sorted(metadata["files"].items()))
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != checksums:
        raise ValueError("Release checksum manifest differs from verified bundle")
    command = ["gh", "release", "create", tag, "--verify-tag", "--title", tag,
               "--notes-file", str(directory / "release-notes.md")]
    if "-" in version():
        command.append("--prerelease")
    command.extend(str(directory / filename) for filename in sorted(metadata["files"]))
    command.append(str(directory / "SHA256SUMS"))
    subprocess.run(command, check=True)

def evidence():
    destination = Path("target/ci-evidence")
    destination.mkdir(parents=True, exist_ok=True)
    count = 0
    for prefix in ["owc-desktop-", "owc-browser-", "owc-recovery-", "owc-obs-", "owc-service-", "owc-installer-"]:
        for root in Path(tempfile.gettempdir()).glob(prefix + "*"):
            candidates = list(root.iterdir())
            if prefix == "owc-obs-":
                candidates.extend(root / name for name in [
                    "companion/evidence.json", "companion/delayed-start.json",
                    "companion/partial-start.json", "companion/closed-history.json",
                    "companion/queue-latency.json", "companion/marker-reference.json",
                    "companion/marker-alignment.json",
                    "companion/events.jsonl", "videos/readiness.json",
                ])
            for source in candidates:
                if source.is_file() and source.suffix in [".log", ".png", ".json", ".jsonl"]:
                    output = destination / root.name / source.relative_to(root)
                    output.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, output)
                    count += 1
    (destination / "summary.txt").write_text(f"Collected {count} private test logs/screenshots.\n", encoding="utf-8")

def actionlint():
    url = "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_amd64.tar.gz"
    data = urllib.request.urlopen(url, timeout=60).read()
    if hashlib.sha256(data).hexdigest() != "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8":
        raise ValueError("actionlint checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="owc-actionlint-") as root:
        binary = Path(root) / "actionlint"
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            binary.write_bytes(archive.extractfile("actionlint").read())
        binary.chmod(0o755)
        subprocess.run([str(binary)], check=True)

def main():
    command = sys.argv[1]
    if command == "release-tag":
        release_tag(os.environ["RELEASE_TAG"])
    elif command == "package":
        package(Path(sys.argv[2]), os.environ["DIST_TARGET"])
    elif command == "bundle":
        bundle(Path(sys.argv[2]))
    elif command == "publish":
        publish(Path(sys.argv[2]), os.environ["RELEASE_TAG"])
    elif command == "evidence":
        evidence()
    elif command == "actionlint":
        actionlint()
    else:
        raise ValueError(f"Unknown command: {command}")

if __name__ == "__main__":
    main()
