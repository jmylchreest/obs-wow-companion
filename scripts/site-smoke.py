#!/usr/bin/env python3
"""Render the static site on desktop/mobile and reject broken local resources."""
import base64
import functools
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import urllib.request

root=Path(tempfile.mkdtemp(prefix="owc-site-"))
env=dict(os.environ,DISPLAY=":125",GDK_BACKEND="x11",LIBGL_ALWAYS_SOFTWARE="1")
if Path("/tmp/.X125-lock").exists():
    raise SystemExit("Private display already in use")
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass
server=http.server.ThreadingHTTPServer(("127.0.0.1",0),
    functools.partial(Quiet,directory=str(Path("site").resolve())))
thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
processes=[];logs=[]
def launch(args,name):
    log=(root/name).open("w");logs.append(log)
    p=subprocess.Popen(args,env=env,stdout=log,stderr=log);processes.append(p);return p
def driver(path,data=None,method=None):
    request=urllib.request.Request("http://127.0.0.1:4460"+path,
        data=None if data is None else json.dumps(data).encode(),
        headers={"Content-Type":"application/json"},method=method)
    return json.load(urllib.request.urlopen(request,timeout=20))["value"]
try:
    launch(["Xvfb",":125","-screen","0","1500x1100x24","-nolisten","tcp"],"xvfb.log")
    time.sleep(1)
    launch(["WebKitWebDriver","--port=4460","--host=127.0.0.1"],"driver.log")
    time.sleep(1)
    session=driver("/session",{"capabilities":{"alwaysMatch":{"browserName":"MiniBrowser"}}})["sessionId"]
    prefix="/session/"+session
    def js(script):return driver(prefix+"/execute/sync",{"script":script,"args":[]})
    driver(prefix+"/url",{"url":f"http://127.0.0.1:{server.server_port}/"})
    for name,width,height in [("desktop",1440,1000),("mobile",390,844)]:
        driver(prefix+"/window/rect",{"width":width,"height":height})
        time.sleep(.5)
        assert js("return document.documentElement.scrollWidth<=innerWidth"), "horizontal overflow"
        assert js("return [...document.images].filter(i=>i.loading!=='lazy').every(i=>i.complete&&i.naturalWidth>0)"), "broken image"
        resources=js("return [...document.querySelectorAll('[src],link[href],a[href]')].map(e=>e.getAttribute('src')||e.getAttribute('href')).filter(v=>v&&!v.startsWith('#')&&!/^https?:/.test(v))")
        for resource in resources:
            urllib.request.urlopen(f"http://127.0.0.1:{server.server_port}/"+resource).read()
        (root/(name+".png")).write_bytes(base64.b64decode(driver(prefix+"/screenshot")))
        js("document.getElementById('start').scrollIntoView()")
        time.sleep(.5)
        (root/(name+"-setup.png")).write_bytes(base64.b64decode(driver(prefix+"/screenshot")))
        js("window.scrollTo(0,0)")
    for guide in sorted(Path("site/guide").glob("*.html")):
        driver(prefix+"/url",{"url":f"http://127.0.0.1:{server.server_port}/guide/{guide.name}"})
        for name,width,height in [("desktop",1440,1000),("mobile",390,844)]:
            driver(prefix+"/window/rect",{"width":width,"height":height})
            time.sleep(.2)
            assert js("return document.documentElement.scrollWidth<=innerWidth"), f"{guide}: horizontal overflow"
            assert js("return document.querySelector('main')&&document.querySelector('[aria-current=page]')"), f"{guide}: missing navigation"
            resources=js("return [...document.querySelectorAll('[src],link[href],a[href]')].map(e=>e.src||e.href).filter(v=>v&&v.startsWith(location.origin))")
            for resource in resources:
                urllib.request.urlopen(resource).read()
            (root/(guide.stem+"-"+name+".png")).write_bytes(base64.b64decode(driver(prefix+"/screenshot")))
    driver(prefix,method="DELETE")
    print(f"Desktop/mobile site and local resources passed: {root}")
finally:
    for p in reversed(processes):
        if p.poll() is None:p.terminate();p.wait(timeout=5)
    for log in logs:log.close()
    server.shutdown();server.server_close();thread.join(timeout=5)
