#!/usr/bin/env python3
"""Check repository Markdown/HTML links and reject machine-specific OBS starter data."""
from html.parser import HTMLParser
from urllib.parse import urlsplit, unquote
import json
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parent.parent
errors = []
class Page(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.links, self.ids = [], set()
        self.feed(source.read_text(encoding="utf-8"))

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        self.links.extend(attrs[key] for key in ("href", "src", "poster") if attrs.get(key))

pages = {source.resolve(): Page(source) for source in (root / "site").rglob("*.html")}
sources = [root / "README.md", *(root / "docs").rglob("*.md"), *(root / "templates").rglob("*.md"), *pages]
for source in sources:
    links = pages[source].links if source in pages else re.findall(r"\]\(([^)]+)\)", source.read_text(encoding="utf-8"))
    for target in links:
        url = urlsplit(target)
        if url.scheme or url.netloc:
            continue
        destination = (source.parent / unquote(url.path)).resolve() if url.path else source.resolve()
        if not destination.exists():
            errors.append(f"{source.relative_to(root)}: missing {target}")
        elif url.fragment and destination in pages and unquote(url.fragment) not in pages[destination].ids:
            errors.append(f"{source.relative_to(root)}: missing anchor {target}")
for source in (root / "templates/obs").rglob("*"):
    if not source.is_file() or source.suffix not in (".ini", ".json"):
        continue
    text = source.read_text(encoding="utf-8")
    if source.suffix == ".json":
        json.loads(text)
    if re.search(r"/home/|/tmp/|[A-Z]:\\\\Users\\\\|server_password|device_id", text):
        errors.append(f"{source.relative_to(root)}: machine-specific configuration")
if errors:
    sys.exit("\n".join(errors))
print("Documentation links and OBS template data passed")
