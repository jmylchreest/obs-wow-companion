#!/usr/bin/env python3
"""Exercise real ENOSPC inside a private Linux mount/user namespace."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

if len(sys.argv) == 1:
    with tempfile.TemporaryDirectory(prefix="owc-disk-full-") as root:
        subprocess.run(["unshare", "--user", "--map-root-user", "--mount", "--",
                        sys.executable, str(Path(__file__).resolve()), root], check=True)
else:
    root = Path(sys.argv[1]).resolve()
    subprocess.run(["mount", "--make-rprivate", "/"], check=True)
    subprocess.run(["mount", "-t", "tmpfs", "-o", "size=4m,nosuid,nodev", "owc-test", str(root)], check=True)
    try:
        env = dict(os.environ, OWC_LIMITED_FILESYSTEM=str(root))
        subprocess.run(["cargo", "test", "-p", "owc-media", "disk_full_export_preserves_source",
                        "--lib", "--", "--ignored"], env=env, check=True, timeout=120)
        subprocess.run(["cargo", "test", "-p", "owc-app", "disk_full_zip_preserves_source",
                        "--lib", "--", "--ignored"], env=env, check=True, timeout=120)
        (root / "clip.mkv").unlink()
        (root / "share.zip").unlink()
        subprocess.run(["cargo", "test", "-p", "owc-library", "disk_full_checkpoint_rolls_back",
                        "--lib", "--", "--ignored"], env=env, check=True, timeout=120)
        for database_file in root.glob("library.sqlite*"):
            database_file.unlink()
        subprocess.run(["cargo", "test", "-p", "owc-app", "disk_full_log_checkpoint_retains_ack",
                        "--lib", "--", "--ignored"], env=env, check=True, timeout=120)
    finally:
        subprocess.run(["umount", str(root)], check=True)
    print("Private tmpfs ENOSPC, source preservation and retry passed")
