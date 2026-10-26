#!/usr/bin/env python3
"""Verify an extracted Linux/Windows binary's local service and media tools."""
from media_fixture import create_fixture
import http.cookiejar
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

root = Path(tempfile.mkdtemp(prefix="owc-service-"))
binary = os.environ.get("OWC_BINARY", "target/debug/obs-wow-companion")
with (root / "app.log").open("w") as log:
    app = subprocess.Popen([binary, "--no-open", "--data-dir", str(root / "data")], stdout=log, stderr=log)
    try:
        url = None
        for _ in range(100):
            time.sleep(0.1)
            text = (root / "app.log").read_text(encoding="utf-8")
            match = re.search(r"http://127\.0\.0\.1:\d+/session/[a-f0-9-]+", text)
            if match:
                url = match.group()
                break
            assert app.poll() is None, text
        assert url, "No service URL"
        base = url.split("/session/")[0]
        try:
            urllib.request.urlopen(base + "/api/status", timeout=10)
            raise AssertionError("Unauthenticated access accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 401
        client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
        client.open(url, timeout=10).read()
        def action(data):
            request = urllib.request.Request(base + "/api/action", data=json.dumps(data).encode(),
                headers={"Content-Type":"application/json", "X-OWC-Request":"1"})
            return json.load(client.open(request, timeout=60))
        action({"action":"set_marker_visibility","visible":True})
        result = create_fixture(root / "fixture.mkv")
        assert sum(stream['codec_type']=='audio' for stream in result['streams']) == 2
        assert float(result['format']['duration']) >= 2.9
        try:
            action({"action":"diagnostic"})
            raise AssertionError("Removed media-test action is still available")
        except urllib.error.HTTPError as error:
            assert error.code == 400
        request = urllib.request.Request(base + "/api/action", data=b'{"action":"set_marker_visibility","visible":true}',
            headers={"Content-Type":"application/json", "X-OWC-Request":"1", "Origin":"http://untrusted.invalid"})
        try:
            client.open(request, timeout=10)
            raise AssertionError("Cross-origin mutation accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 403
        events = client.open(base + "/api/events", timeout=15)
        assert events.readline().startswith(b"event: state")
        action({"action":"quit"})
        assert app.wait(timeout=10) == 0
        events.close()
        (root / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(f"Packaged service authentication, two-track media and shutdown passed: {root}")
    finally:
        if app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
