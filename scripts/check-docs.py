#!/usr/bin/env python3
"""Check repository Markdown links and reject machine-specific OBS starter data."""
import json
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parent.parent
errors = []
for source in [root / "README.md", *(root / "docs").rglob("*.md"), *(root / "templates").rglob("*.md")]:
    for target in re.findall(r"\]\(([^)]+)\)", source.read_text()):
        target = target.split("#")[0]
        if not target or re.match(r"[a-z]+://", target):
            continue
        if not (source.parent / target).exists():
            errors.append(f"{source.relative_to(root)}: missing {target}")
for source in (root / "templates/obs").rglob("*"):
    if not source.is_file() or source.suffix not in (".ini", ".json"):
        continue
    text = source.read_text()
    if source.suffix == ".json":
        json.loads(text)
    if re.search(r"/home/|/tmp/|[A-Z]:\\\\Users\\\\|server_password|device_id", text):
        errors.append(f"{source.relative_to(root)}: machine-specific configuration")
if errors:
    sys.exit("\n".join(errors))
print("Documentation links and OBS template data passed")
