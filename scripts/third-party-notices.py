#!/usr/bin/env python3
"""Generate notices from fetched registry sources, retaining original license text."""
import json
import hashlib
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root))
included = set()
for target in ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"):
    graph = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target], cwd=root))
    nodes = {n["id"]: n for n in graph["resolve"]["nodes"]}
    queue = [p["id"] for p in graph["packages"] if p["name"] == "owc-app"]
    visited = set()
    while queue:
        key = queue.pop()
        if key in visited:
            continue
        visited.add(key)
        queue.extend(d["pkg"] for d in nodes[key]["deps"] if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    included.update(visited)
sections = ["Third-party dependency notices\nGenerated from Cargo.lock and fetched registry source packages.\nSystem and separately installed applications are not bundled.\n"]
missing = []
upstream = root / "licenses/upstream"
fallbacks = json.loads((upstream / "manifest.json").read_text(encoding="utf-8"))
for package in sorted(metadata["packages"], key=lambda p:(p["name"],p["version"])):
    if package["source"] is None or package["id"] not in included:
        continue
    directory = Path(package["manifest_path"]).parent
    licenses = []
    for path in directory.iterdir():
        if path.is_file() and path.name.lower().startswith(("license", "licence", "copying", "notice", "copyright")):
            licenses.append(path)
        elif path.is_dir() and path.name.lower() in ("licenses", "licences"):
            licenses.extend(p for p in path.rglob("*") if p.is_file())
    declared = package.get("license_file")
    if declared and (directory / declared).is_file():
        licenses.append(directory / declared)
    sections.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nLicense: {package.get('license')}\nSource: {package.get('repository') or package['source']}\n")
    if not licenses:
        fallback = fallbacks.get(f"{package['name']}@{package['version']}")
        if fallback:
            data = (upstream / fallback["file"]).read_bytes()
            assert hashlib.sha256(data).hexdigest() == fallback["sha256"], "license source hash mismatch"
            sections.append(f"License retrieved from {fallback['source']}\n" + data.decode())
        else:
            missing.append(f"{package['name']} {package['version']}: {directory}")
    for path in sorted(set(licenses)):
        sections.append(f"\n--- {path.relative_to(directory)} ---\n" + path.read_text(errors="replace", encoding="utf-8"))
sections.append("\n" + "=" * 72 + "\nVideo.js classic 8.24.1 (HTML5 core player)\nLicense: Apache-2.0\nSource: https://github.com/videojs/video.js/tree/v8.24.1\n" + (root / "crates/app/ui/vendor/VIDEOJS-LICENSE").read_text(encoding="utf-8"))
(root / "THIRD-PARTY-NOTICES.txt").write_text("\n".join(sections), encoding="utf-8")
if missing:
    sys.exit("Missing bundled license files; review these packages:\n" + "\n".join(missing))
print("Third-party notices generated with license files for every registry package")
