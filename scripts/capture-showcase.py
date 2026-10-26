#!/usr/bin/env python3
"""Capture the actual app with generated media and fictional public demo data."""
import base64
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import tempfile
import time
import urllib.request
import uuid

root = Path(tempfile.mkdtemp(prefix="owc-showcase-"))
assets = Path("site/assets")
assets.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, DISPLAY=":124", GDK_BACKEND="x11", LIBGL_ALWAYS_SOFTWARE="1",
           XDG_CONFIG_HOME=str(root / "config"))
if Path("/tmp/.X124-lock").exists():
    raise SystemExit("Private display already in use")
processes, logs = [], []
def launch(args, name):
    log = (root / name).open("w")
    logs.append(log)
    process = subprocess.Popen(args, env=env, stdout=log, stderr=log)
    processes.append(process)
    return process
def driver(path, data=None, method=None):
    request = urllib.request.Request("http://127.0.0.1:4459"+path,
        data=None if data is None else json.dumps(data).encode(),
        headers={"Content-Type":"application/json"}, method=method)
    return json.load(urllib.request.urlopen(request, timeout=20))["value"]
try:
    launch(["Xvfb",":124","-screen","0","1440x1000x24","-nolisten","tcp"],"xvfb.log")
    time.sleep(1)
    app = launch([os.environ.get("OWC_BINARY","target/debug/obs-wow-companion"),
                  "--no-open","--data-dir",str(root/"data")],"app.log")
    launch(["WebKitWebDriver","--port=4459","--host=127.0.0.1"],"driver.log")
    time.sleep(2)
    url = re.search(r"http://127\.0\.0\.1:\d+/session/[a-f0-9-]+",(root/"app.log").read_text()).group()
    session = driver("/session",{"capabilities":{"alwaysMatch":{"browserName":"MiniBrowser"}}})["sessionId"]
    prefix = "/session/"+session
    def js(script,args=None):
        return driver(prefix+"/execute/sync",{"script":script,"args":args or []})
    def click(selector):
        js("document.querySelector(arguments[0]).click()",[selector])
    def screenshot(name):
        (assets/name).write_bytes(base64.b64decode(driver(prefix+"/screenshot")))
    driver(prefix+"/window/rect",{"width":1440,"height":1000})
    driver(prefix+"/url",{"url":url})
    time.sleep(2)
    screenshot("setup.png")
    click("#wizard-close")
    source = root/"demo.mp4"
    subprocess.run(["ffmpeg","-nostdin","-v","error","-y","-f","lavfi","-i",
        "testsrc2=size=960x540:rate=30","-f","lavfi","-i","anullsrc=r=48000:cl=stereo",
        "-t","30","-map","0:v","-map","1:a","-map","1:a",
        "-vf","hue=s=0.18,eq=brightness=-0.12:contrast=0.85,drawbox=x=0:y=190:w=iw:h=130:color=0x081216@0.95:t=fill,drawtext=text='DEMO FOOTAGE':fontsize=44:fontcolor=white:x=(w-tw)/2:y=215,drawtext=text='Generated test video • Fictional encounter data':fontsize=23:fontcolor=white:x=(w-tw)/2:y=275",
        "-c:v","libx264","-preset","fast","-crf","25","-g","30","-pix_fmt","yuv420p",
        "-c:a","aac","-metadata:s:a:0","title=Game","-metadata:s:a:1","title=Voice",str(source)],check=True)
    start = 1792944000000
    records = [
        ("Ruby Life Pools","mythic_plus",20,"success",2,["Weekly keys","Review"]),
        ("Den of Nalorakk","mythic_plus",21,"success",0,["Practice"]),
        ("Kith'ix","raid",15,"success",None,["Raid night"]),
        ("Kith'ix","raid",15,"failure",None,["Raid night"]),
        ("Manual recording","manual",0,"unknown",None,["Favourite moment"]),
    ]
    with sqlite3.connect(root/"data/library.sqlite") as db:
        for i,(name,kind,difficulty,outcome,upgrade,tags) in enumerate(records):
            identifier=str(uuid.uuid4())
            path=root/"data/videos"/(identifier+".mp4")
            shutil.copyfile(source,path)
            at=start-i*3600000
            markers=[{"at_ms":at+3000,"kind":"combat_activity","label":"Combat"},
                {"at_ms":at+7000,"kind":"boss_start","label":"Demo boss"},
                {"at_ms":at+18000,"kind":"death","label":"Demohealer",
                 "recent_hits":[{"offset_ms":-200,"source":"Demo boss","ability":"Flame burst"},
                                {"offset_ms":-800,"source":"Demo boss","ability":"Burning ground"}]},
                {"at_ms":at+26000,"kind":"boss_end","label":"Demo boss: kill"}]
            activity={"id":identifier,"installation":"Demo Retail","kind":kind,"name":name,
                "start_ms":at,"end_ms":at+30000,"outcome":outcome,"markers":markers,
                "character":"Demomage","realm":"Demo realm","warnings":[]}
            if kind=="mythic_plus":
                activity.update(key_level=difficulty,keystone_upgrade=upgrade,difficulty=8)
            elif kind=="raid":
                activity["difficulty"]=difficulty
            path.with_suffix(".json").write_text(json.dumps({"schema_version":1,"activity":activity,
                "plan":None,"media":{"duration_ms":30000}}))
            db.execute("INSERT INTO activities(id,data,status,path,audio_tracks,protected,tags,origin_ms) VALUES(?,?,'ready',?,2,?,?,?)",
                (identifier,json.dumps(activity),str(path),int(i==0),json.dumps(tags),at))
    for _ in range(50):
        if js("return document.querySelectorAll('.recording-select').length")==5:
            break
        time.sleep(.2)
    click(".recording-select")
    for _ in range(100):
        if js("return reviewPlayer.readyState()>=2"):
            break
        time.sleep(.2)
    assert js("return reviewPlayer.readyState()>=2")
    js("reviewPlayer.currentTime(12);reviewPlayer.pause();reviewPlayer.userActive(true)")
    time.sleep(.5)
    screenshot("review.png")
    click("#clip-options > summary")
    js("document.getElementById('share-path').value='raid-review.zip';document.getElementById('clip-options').scrollIntoView({block:'center'})")
    screenshot("export.png")
    click("#setup-tab")
    click("[data-settings-tab=export]")
    screenshot("settings.png")
    click("#library-tab")
    js("document.querySelector('.review-scroll').scrollTop=0") if js("return !!document.querySelector('.review-scroll')") else None
    js("document.getElementById('clip-options').open=false;document.getElementById('player').scrollIntoView({block:'start'});reviewPlayer.currentTime(5)")
    capture=launch(["ffmpeg","-nostdin","-v","error","-y","-f","x11grab","-video_size","1440x1000",
        "-framerate","20","-i",":124","-t","10","-vf","scale=1152:-2","-c:v","libx264",
        "-preset","fast","-crf","25","-pix_fmt","yuv420p","-movflags","+faststart",str(assets/"review-demo.mp4")],"capture.log")
    js("reviewPlayer.play();reviewPlayer.userActive(true)")
    time.sleep(3)
    js("reviewPlayer.currentTime(13);reviewPlayer.playbackRate(.5);reviewPlayer.userActive(true)")
    time.sleep(3)
    js("reviewPlayer.currentTime(23);reviewPlayer.userActive(true)")
    capture.wait(timeout=20)
    driver(prefix,method="DELETE")
    print(f"Public synthetic screenshots and recording: {assets}; private evidence: {root}")
finally:
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait()
    for log in logs: log.close()
