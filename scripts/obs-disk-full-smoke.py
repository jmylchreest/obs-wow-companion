#!/usr/bin/env python3
"""Real OBS disk exhaustion in a private Linux mount and user namespace."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

if len(sys.argv) == 1:
    root = Path(tempfile.mkdtemp(prefix="owc-obs-full-"))
    subprocess.run(["unshare", "--user", "--map-root-user", "--mount", "--",
                    sys.executable, str(Path(__file__).resolve()), str(root)], check=True)
    print("Disk-full evidence:", root)
else:
    root = Path(sys.argv[1]).resolve()
    limited = root / "limited"
    limited.mkdir()
    subprocess.run(["mount", "--make-rprivate", "/"], check=True)
    subprocess.run(["mount", "-t", "tmpfs", "-o", "size=128m,nosuid,nodev", "owc-obs-test", str(limited)], check=True)
    try:
        env = dict(os.environ, OWC_NATIVE_OBS_LIMITED=str(limited))
        subprocess.run(["dbus-run-session", "--", "python3", "scripts/isolated-obs.py"],
                       env=env, check=True, timeout=120)
    finally:
        subprocess.run(["umount", str(limited)], check=True)
