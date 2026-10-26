#!/usr/bin/env python3
"""Create a small OBS-style recording: full mix, game tone, and voice tone."""
import argparse
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
args.output.parent.mkdir(parents=True, exist_ok=True)
subprocess.run([
    "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-n",
    "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30",
    "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
    "-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000",
    "-filter_complex", "[1:a]asplit=2[game][g];[2:a]asplit=2[voice][v];[g][v]amix=inputs=2:normalize=1[mix]",
    "-map", "0:v", "-map", "[mix]", "-map", "[game]", "-map", "[voice]",
    "-t", "12", "-c:v", "libx264", "-preset", "veryfast", "-crf", "25",
    "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k",
    "-metadata:s:a:0", "title=Full mix",
    "-metadata:s:a:1", "title=Game demo (440 Hz)",
    "-metadata:s:a:2", "title=Voice demo (880 Hz)",
    str(args.output),
], check=True)
print(args.output.resolve())
