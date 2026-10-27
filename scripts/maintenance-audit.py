#!/usr/bin/env python3
"""Check pins outside Dependabot's Cargo, Actions and pip coverage (read-only)."""
import json
import os
from pathlib import Path
import re
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "obs-wow-companion-maintenance"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def pins():
    found = {}
    for path in (ROOT / ".github/workflows").glob("*.yml"):
        for package, version in re.findall(
                r'package: ([\w-]+)\s+executable: [\w-]+\s+version: "([\d.]+)"',
                path.read_text(encoding="utf-8")):
            found.setdefault(package, set()).add(version)
    return found


def main():
    rows = []
    for package, versions in sorted(pins().items()):
        latest = fetch(f"https://crates.io/api/v1/crates/{package}")["crate"]["max_stable_version"]
        rows.append((package, ", ".join(sorted(versions)), latest, versions == {latest}))
    source = (ROOT / "scripts/ci-tools.py").read_text(encoding="utf-8")
    version = re.search(r"actionlint/releases/download/v([\d.]+)/", source).group(1)
    latest = fetch("https://api.github.com/repos/rhysd/actionlint/releases/latest")["tag_name"].removeprefix("v")
    rows.append(("actionlint", version, latest, version == latest))
    source = (ROOT / "scripts/third-party-notices.py").read_text(encoding="utf-8")
    version = re.search(r"Video.js classic ([\d.]+)", source).group(1)
    latest = fetch("https://registry.npmjs.org/-/package/video.js/dist-tags")["latest-8"]
    rows.append(("Video.js classic (maintained v8 channel)", version, latest, version == latest))
    report = "# Maintenance pins\n\n| Tool | Pinned | Latest | Status |\n|---|---|---|---|\n"
    report += "\n".join(f"| {name} | {pin} | {latest} | {'Current' if ok else 'Review update'} |"
                        for name, pin, latest, ok in rows) + "\n"
    print(report)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as output:
            output.write(report)
    if not all(row[3] for row in rows):
        raise SystemExit("Updates need review; this audit does not change pins or merge dependencies.")


if __name__ == "__main__":
    main()
