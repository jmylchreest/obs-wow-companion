#!/usr/bin/env python3
"""Development-only OBS probe, using a fresh private config and virtual display."""
import os, subprocess, time, json, tempfile, shutil, configparser, socket
from pathlib import Path
if Path("/tmp/.X121-lock").exists():raise SystemExit("Private test display is already in use")
with socket.socket() as check:check.bind(("127.0.0.1",4457))
root=Path(tempfile.mkdtemp(prefix="owc-obs-"))
for d in ["config","cache","data","videos"]: (root/d).mkdir()
base=root/"config/obs-studio"
base.mkdir(parents=True)
(base/"user.ini").write_text("[General]\nFirstRun=true\n[Basic]\nConfigOnNewProfile=false\nProfile=Untitled\nProfileDir=Untitled\nSceneCollection=OWC Starter\nSceneCollectionFile=Untitled.json\n")
profile=base/"basic/profiles/Untitled"
profile.mkdir(parents=True)
starter=Path("templates/obs/OWC-Starter")
settings=configparser.ConfigParser(interpolation=None);settings.optionxform=str
settings.read(starter/"basic.ini")
settings["General"]["Name"]="Untitled"
settings["AdvOut"]["RecFilePath"]=str(root/"videos")
with (profile/"basic.ini").open("w") as f:settings.write(f,space_around_delimiters=False)
shutil.copyfile(starter/"recordEncoder.json",profile/"recordEncoder.json")
scenes=base/"basic/scenes";scenes.mkdir(parents=True)
shutil.copyfile("templates/obs/OWC-Starter-scenes.json",scenes/"Untitled.json")
config=base/"plugin_config/obs-websocket"
config.mkdir(parents=True)
(config/"config.json").write_text(json.dumps(dict(server_enabled=True,server_port=4457,server_password="owc-isolated-test",auth_required=True,first_load=False,alerts_enabled=False)))
subprocess.run(["ffmpeg","-nostdin","-v","error","-y","-f","lavfi","-i","testsrc2=size=640x360:rate=30","-f","lavfi","-i","sine=frequency=440:sample_rate=48000","-t","12","-c:v","libx264","-g","30","-c:a","aac",str(root/"videos/pattern.mp4")],check=True)
env=os.environ.copy();env.update(TZ="UTC",DISPLAY=":121",QT_QPA_PLATFORM="xcb",XDG_CONFIG_HOME=str(root/"config"),XDG_CACHE_HOME=str(root/"cache"),XDG_DATA_HOME=str(root/"data"),LIBGL_ALWAYS_SOFTWARE="1",PULSE_SERVER="unix:/tmp/owc-no-audio-server")
xlog=open(root/"xvfb.log","w");olog=open(root/"obs.log","w")
x=subprocess.Popen(["Xvfb",":121","-screen","0","1280x720x24","-nolisten","tcp"],stdout=xlog,stderr=xlog)
obs=None
try:
 time.sleep(1)
 if x.poll() is not None:raise RuntimeError("Xvfb failed")
 obs=subprocess.Popen(["obs","--multi","--disable-shutdown-check","--skip-plugin-load-warning"],env=env,stdout=olog,stderr=olog)
 time.sleep(6)
 subprocess.run(["ffmpeg","-nostdin","-v","error","-y","-f","x11grab","-video_size","1280x720","-i",":121","-frames:v","1",str(root/"screen.png")],check=True)
 out=subprocess.run(["target/debug/examples/obs_probe",str(root/"videos")],capture_output=True,text=True,timeout=65,env=env)
 (root/"probe.log").write_text(out.stdout+out.stderr)
 subprocess.run(["ffmpeg","-nostdin","-v","error","-y","-f","x11grab","-video_size","1280x720","-i",":121","-frames:v","1",str(root/"after.png")],check=True)
 print("Evidence:",root);print(out.stdout[-9000:]);print(out.stderr[-1000:])
 if out.returncode: raise SystemExit(out.returncode)
 if os.environ.get("OWC_RECONNECT_TEST") == "1":
  reconnect_env=os.environ.copy();reconnect_env["OWC_RECOVERY_OBS_PORT"]="4457"
  subprocess.run(["cargo","nextest","run","-p","owc-app","--run-ignored","only","-E","test(native_obs_reconnect_keeps_owned_fragment_and_closes_it)"],env=reconnect_env,check=True,timeout=90)
 if os.environ.get("OWC_BROWSER_SETUP_TEST") == "1":
  browser_env=os.environ.copy();browser_env["OWC_TEST_OBS_PORT"]="4457"
  subprocess.run(["python3","scripts/browser-smoke.py"],env=browser_env,check=True,timeout=90)
finally:
 if obs:
  obs.terminate()
  try:obs.wait(timeout=5)
  except subprocess.TimeoutExpired:obs.kill();obs.wait()
 x.terminate();x.wait(timeout=5)
