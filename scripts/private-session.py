#!/usr/bin/env python3
"""Run native probes with private D-Bus activation and runtime/config directories."""
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time

if len(sys.argv) < 2 or sys.platform != "linux":
    raise SystemExit("Usage on Linux: python3 scripts/private-session.py COMMAND [ARGS...]")
with tempfile.TemporaryDirectory(prefix="owc-session-") as directory:
    root = Path(directory)
    env = os.environ.copy()
    for variable, name in [("XDG_RUNTIME_DIR", "runtime"), ("XDG_CONFIG_HOME", "config"),
                           ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")]:
        path = root / name
        path.mkdir(mode=0o700)
        env[variable] = str(path)
    # Activation inherits this environment before any portal/secret service starts.
    process = subprocess.Popen(["dbus-run-session", "--", *sys.argv[1:]],
                               env=env, start_new_session=True)
    try:
        result = process.wait()
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        # Portals may leave FUSE mounts behind after their private bus exits.
        mounts = []
        for line in Path("/proc/self/mountinfo").read_text().splitlines():
            fields = line.split()
            path = Path(fields[4])
            if path.is_relative_to(root) and fields[fields.index("-") + 1].startswith("fuse"):
                mounts.append(path)
        for path in sorted(mounts, key=lambda item: len(item.parts), reverse=True):
            unmount = shutil.which("fusermount3") or shutil.which("fusermount")
            if not unmount:
                raise RuntimeError(f"Cannot unmount private test filesystem: {path}")
            outcome = subprocess.run([unmount, "-uz", str(path)], capture_output=True, text=True, timeout=10)
            # The portal can unmount concurrently after receiving SIGTERM.
            still_mounted = any(line.split()[4] == str(path)
                                for line in Path("/proc/self/mountinfo").read_text().splitlines())
            if outcome.returncode and still_mounted:
                raise RuntimeError(outcome.stderr)
        for attempt in range(10):
            try:
                shutil.rmtree(root)
                break
            except OSError:
                if attempt == 9:
                    raise
                time.sleep(0.1)
    raise SystemExit(result)
