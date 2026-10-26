#!/usr/bin/env python3
"""Kill a private app during export, restart, and release its orphan media worker."""
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time
import uuid

root = Path(tempfile.mkdtemp(prefix="owc-recovery-"))
binary = str(Path(os.environ.get("OWC_BINARY", "target/debug/obs-wow-companion")).resolve())
processes = []
logs = []

def launch():
    log = (root / f"app-{len(processes)}.log").open("w")
    logs.append(log)
    app = subprocess.Popen([binary, "--no-open", "--data-dir", str(root)],
                           stdout=log, stderr=log, start_new_session=True)
    processes.append(app)
    return app

def wait_for(predicate, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError(f"Timed out; evidence: {root}")

try:
    initial = launch()
    database = root / "library.sqlite"
    wait_for(lambda: database.exists() and (root / "preview").exists())
    initial.terminate()
    initial.wait(timeout=5)
    source = root / "source.mkv"
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i",
                    "testsrc2=size=320x180:rate=30", "-f", "lavfi", "-i",
                    "sine=sample_rate=48000", "-t", "3", "-c:v", "libx264", "-c:a", "aac", str(source)], check=True)
    wrapper = root / "media-worker.py"
    wrapper.write_text("#!/usr/bin/env python3\n" +
        "import os, pathlib, sys, time\n" +
        f"root=pathlib.Path({str(root)!r})\n" +
        "part=next((arg for arg in sys.argv[1:] if arg.endswith('part-0000.mkv')),None)\n" +
        "if part:\n with (root/'worker-paths').open('a') as log: log.write(part+'\\n')\n" +
        "held=False\nif part:\n try:\n  (root/'hold-once').unlink()\n  held=True\n except FileNotFoundError:\n  pass\n" +
        "if held:\n (root/'worker-started').write_text(str(os.getpid()))\n while not (root/'release-worker').exists(): time.sleep(0.05)\n" +
        "os.execvp('ffmpeg',['ffmpeg',*sys.argv[1:]])\n", encoding="utf-8")
    wrapper.chmod(0o700)
    (root / "hold-once").touch()
    identity = str(uuid.uuid4())
    now = int(time.time() * 1000)
    activity = dict(id=identity, installation="manual", kind="manual", name="Crash recovery",
                    start_ms=now, end_ms=now+2000, outcome="unknown", difficulty=None,
                    key_level=None, zone=None, encounter=None, character=None, markers=[], warnings=[])
    segment = dict(id=identity, session=identity, path=str(source), start_ms=now,
                   end_ms=now+3000, closed=True, managed=False)
    plan = dict(activity_id=identity, requested_start_ms=now, requested_end_ms=now+2000,
                actual_start_ms=now, coverage="complete",
                sources=[dict(segment=segment, offset_ms=0, duration_ms=2000)])
    # Seed the same durable transaction an interrupted export leaves behind.
    with sqlite3.connect(database) as db:
        config = dict(obs_port=4455, spool=str(root/'spool'), library=str(root/'videos'),
                      installations=[], pre_seconds=5, post_seconds=10, segment_seconds=60,
                      history_seconds=120, spool_gib=30, library_gib=100, minimum_free_gib=1,
                      activity_timeout_minutes=30, media=dict(ffmpeg=str(wrapper), ffprobe="ffprobe"))
        db.execute("INSERT OR REPLACE INTO settings VALUES('config',?)", (json.dumps(config),))
        db.execute("INSERT INTO activities(id,data,status,plan,precise) VALUES(?,?,'exporting',?,1)",
                   (identity,json.dumps(activity),json.dumps(plan)))
    interrupted = launch()
    wait_for(lambda: (root/'worker-started').exists())
    interrupted.kill()
    interrupted.wait(timeout=5)
    recovered = launch()
    def ready():
        with sqlite3.connect(database) as db:
            status, error = db.execute("SELECT status,error FROM activities WHERE id=?",(identity,)).fetchone()
        assert status != "failed", error
        return status == "ready"
    wait_for(ready)
    attempts = [path for path in (root/'worker-paths').read_text(encoding="utf-8").splitlines() if path.endswith('part-0000.mkv')]
    assert len(attempts) == 2 and attempts[0] != attempts[1], "retry reused orphan worker output path"
    destination = root / "videos" / f"{identity}.mkv"
    before = destination.read_bytes()
    (root/'release-worker').touch()
    time.sleep(0.5)
    assert destination.read_bytes() == before, "orphan worker changed published output"
    assert source.exists()
    assert len(list((root/'videos').glob('*.mkv'))) == 1
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-xerror", "-i", str(destination),
                    "-f", "null", "-"], check=True)
    print(f"Process crash, orphan worker isolation and single-output recovery passed: {root}")
finally:
    for process in reversed(processes):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        if process.poll() is None:
            process.wait(timeout=5)
    for log in logs:
        log.close()
