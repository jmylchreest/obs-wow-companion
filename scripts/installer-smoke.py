#!/usr/bin/env python3
"""Install from a private loopback artifact server without changing user PATH/config."""
import functools
import http.server
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import tomllib

directory = Path(sys.argv[1]).resolve()
root = Path(tempfile.mkdtemp(prefix="owc-installer-"))
class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

served = root / "served"
served.mkdir()
archive_name = "owc-app-x86_64-pc-windows-msvc.zip" if os.name == "nt" else "owc-app-x86_64-unknown-linux-gnu.tar.xz"
archive = served / archive_name
shutil.copyfile(directory / archive_name, archive)
handler = functools.partial(QuietHandler, directory=str(served))
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
env = os.environ.copy()
env.update(OWC_APP_DOWNLOAD_URL=f"http://127.0.0.1:{server.server_port}",
           OWC_APP_UNMANAGED_INSTALL=str(root / "bin"), OWC_APP_NO_MODIFY_PATH="1")
try:
    if os.name == "nt":
        command = ["pwsh", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(directory / "owc-app-installer.ps1")]
        executable = "obs-wow-companion.exe"
    else:
        command = ["sh", str(directory / "owc-app-installer.sh")]
        executable = "obs-wow-companion"
    # A changed download must be rejected before anything is installed.
    with archive.open("ab") as altered:
        altered.write(b"tampered-download")
    rejected = subprocess.run(command, env=env, capture_output=True, text=True, timeout=120)
    rejection = rejected.stdout + rejected.stderr
    (root / "checksum-rejection.log").write_text(rejection, encoding="utf-8")
    assert rejected.returncode != 0, "Installer accepted a changed archive"
    assert "checksum" in rejection.lower(), "Failure must identify archive integrity"
    assert not list((root / "bin").rglob(executable)), "Changed archive installed a binary"
    shutil.copyfile(directory / archive_name, archive)
    with (root / "installer.log").open("w") as log:
        subprocess.run(command, env=env, stdout=log, stderr=log, check=True, timeout=120)
    matches = list((root / "bin").rglob(executable))
    assert len(matches) == 1, "Expected exactly one installed binary"
    binary = matches[0]
    expected = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    reported = subprocess.check_output([str(binary), "--version"], text=True, timeout=20).strip()
    assert reported.split()[-1] == expected
    env["OWC_BINARY"] = str(binary)
    subprocess.run([sys.executable, "scripts/service-smoke.py"], env=env, check=True, timeout=90)
    print(f"Installer and installed service passed: {root}")
finally:
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)
