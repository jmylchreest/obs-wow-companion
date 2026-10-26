#!/usr/bin/env python3
"""Test credentials in a disposable D-Bus session and GNOME keyring."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

if len(sys.argv) == 1:
    root = Path(tempfile.mkdtemp(prefix="owc-keyring-"))
    (root / "runtime").mkdir(mode=0o700)
    env = os.environ.copy()
    env.pop("DBUS_SESSION_BUS_ADDRESS", None)
    env.pop("GNOME_KEYRING_CONTROL", None)
    env.update(XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"), XDG_RUNTIME_DIR=str(root / "runtime"), OWC_TEST_PRIVATE_KEYRING="1")
    subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--inside"], env=env, check=True)
    print(f"Secure save, retrieval, endpoint isolation and deletion passed: {root}")
else:
    assert os.environ.get("OWC_TEST_PRIVATE_KEYRING") == "1"
    subprocess.run(["gnome-keyring-daemon", "--unlock", "--components=secrets"], input=b"temporary-test-keyring-password", check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["cargo", "test", "-p", "owc-app", "--locked", "credentials::tests::system_store_round_trip", "--", "--ignored"], check=True)
    if os.environ.get("OWC_KEYRING_OBS_TEST") == "1":
        subprocess.run([sys.executable, "scripts/isolated-obs.py"], check=True)
