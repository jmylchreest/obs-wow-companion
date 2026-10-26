#!/usr/bin/env python3
"""Compare a native OBS death-marker frame with the known looping source."""
import json
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
reference = json.loads((root / "companion/marker-reference.json").read_text(encoding="utf-8"))
state = json.loads((root / "companion/evidence.json").read_text(encoding="utf-8"))
clip = next(a for a in state["activities"] if a["kind"] == "manual")
offset = (reference["event_ms"] - clip["origin_ms"]) / 1000
assert offset >= 0, "marker is before playable footage"

def pixels(path, filters, seek=None):
    args = ["ffmpeg", "-nostdin", "-v", "error"]
    if seek is not None:
        args += ["-ss", str(seek)]
    args += ["-i", str(path), "-an", "-vf", filters]
    if seek is not None:
        args += ["-frames:v", "1"]
    return subprocess.check_output(args + ["-pix_fmt", "rgb24", "-f", "rawvideo", "-"], timeout=30)

# The private profile is 1280x720; the 640x360 source is placed at (0,0)
# without scaling. Compare in RGB using the fixture's BT.601 and OBS profile's
# BT.709 matrices; raw luma values differ even for matching displayed colours.
# This checks encoded pixels, independently of clip timestamps.
frame_size = 80 * 45 * 3
observed = pixels(clip["path"], "crop=640:360:0:0,scale=80:45:in_color_matrix=bt709", offset)
source = pixels(root / "videos/pattern.mp4", "fps=30,scale=80:45:in_color_matrix=bt601")
assert len(observed) == frame_size and len(source) % frame_size == 0
def match_frame(frame):
    assert len(frame) == frame_size
    scores = [sum(abs(a-b) for a, b in zip(frame, source[i:i+frame_size])) / frame_size
              for i in range(0, len(source), frame_size)]
    best = min(range(len(scores)), key=scores.__getitem__)
    return best, scores[best]

best, pixel_error = match_frame(observed)
matched_ms = best * 1000 / 30
period = reference["source_duration_ms"]
assert period > 0
delta = abs(matched_ms - reference["source_cursor_ms"]) % period
error_ms = min(delta, period - delta)
report = dict(reference, matched_source_ms=matched_ms, marker_offset_seconds=offset,
              alignment_error_ms=error_ms, mean_pixel_error=pixel_error,
              method="Minimum RGB frame error against the 30fps looping source",
              scope="Private Linux OBS synthetic scene; not gameplay or Windows qualification")
(root / "companion/marker-alignment.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
assert pixel_error < 12, f"Captured picture does not match source: {report}"
assert error_ms <= 1000, f"Marker alignment exceeds one second: {report}"
sidecar = json.loads(Path(clip["path"]).with_suffix(".json").read_text(encoding="utf-8"))
parts = sidecar["plan"]["sources"]
assert len(parts) >= 2, "Native split fixture must retain multiple fragments"
boundaries = []
for left, right in zip(parts, parts[1:]):
    assert left["segment"]["session"] == right["segment"]["session"]
    before = pixels(left["segment"]["path"], "crop=640:360:0:0,scale=80:45:in_color_matrix=bt709")
    after = pixels(right["segment"]["path"], "crop=640:360:0:0,scale=80:45:in_color_matrix=bt709")
    last, last_error = match_frame(before[-frame_size:])
    first, first_error = match_frame(after[:frame_size])
    step = (first-last) % (len(source)//frame_size)
    boundary = dict(last_source_frame=last, first_source_frame=first, frame_step=step,
                    last_pixel_error=last_error, first_pixel_error=first_error)
    boundaries.append(boundary)
    assert max(last_error, first_error) < 12, f"Split pictures do not match source: {boundary}"
    assert step == 1, f"Split boundary is not consecutive source frames: {boundary}"
report["video_split_boundaries"] = boundaries
(root / "companion/marker-alignment.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
print(f"Native marker alignment passed: {error_ms:.1f} ms; evidence: {root}")
