#!/usr/bin/env python3
"""Exercise the bundled UI and actual video playback through WebKit WebDriver."""
import base64
import json
import os
from pathlib import Path
import re
import socket
import sqlite3
from media_fixture import create_fixture
import shutil
import uuid
import zipfile
import subprocess
import tempfile
import time
import urllib.request

root = Path(tempfile.mkdtemp(prefix="owc-browser-"))
if Path("/tmp/.X123-lock").exists():
    raise SystemExit("Private display already in use")
with socket.socket() as check:
    check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    check.bind(("127.0.0.1", 4458))
env = os.environ.copy()
env.update(DISPLAY=":123", GDK_BACKEND="x11", LIBGL_ALWAYS_SOFTWARE="1", XDG_CONFIG_HOME=str(root / "config"))
processes = []
logs = []
def launch(args, name):
    log = (root / name).open("w")
    logs.append(log)
    proc = subprocess.Popen(args, env=env, stdout=log, stderr=log)
    processes.append(proc)
    return proc

def driver(path, data=None, method=None):
    request = urllib.request.Request("http://127.0.0.1:4458" + path, data=None if data is None else json.dumps(data).encode(), headers={"Content-Type":"application/json"}, method=method)
    try:
        response = json.load(urllib.request.urlopen(request, timeout=20))["value"]
    except urllib.error.HTTPError as error:
        raise AssertionError(error.read().decode()) from error
    if isinstance(response, dict) and "error" in response:
        raise AssertionError(response)
    return response

try:
    display = launch(["Xvfb", ":123", "-screen", "0", "1440x1600x24", "-nolisten", "tcp"], "xvfb.log")
    time.sleep(1)
    assert display.poll() is None
    launch([os.environ.get("OWC_BINARY", "target/debug/obs-wow-companion"), "--no-open", "--data-dir", str(root / "data")], "app.log")
    launch(["WebKitWebDriver", "--port=4458", "--host=127.0.0.1"], "driver.log")
    time.sleep(1)
    text = (root / "app.log").read_text(encoding="utf-8")
    url = re.search(r"http://127\.0\.0\.1:\d+/session/[a-f0-9-]+", text).group()
    session = driver("/session", {"capabilities":{"alwaysMatch":{"browserName":"MiniBrowser"}}})["sessionId"]
    prefix = "/session/" + session
    driver(prefix + "/window/rect", {"width":1280,"height":960})
    driver(prefix + "/url", {"url":url})
    def js(script, args=None):
        return driver(prefix + "/execute/sync", {"script":script, "args":args or []})
    def element(selector):
        found = driver(prefix + "/element", {"using":"css selector", "value":selector})
        return next(iter(found.values()))
    def click(selector):
        js("document.querySelector(arguments[0]).scrollIntoView({block:'center'});", [selector])
        previous = None
        for _ in range(20):
            bounds = js("const r=document.querySelector(arguments[0]).getBoundingClientRect();return [r.x,r.y,r.width,r.height]", [selector])
            if bounds == previous:
                break
            previous = bounds
            time.sleep(.1)
        driver(prefix + "/element/" + element(selector) + "/click", {})
    time.sleep(2)
    assert js("return document.getElementById('setup-wizard').open") is True, js("return document.getElementById('error').textContent")
    assert js("return document.getElementById('wizard-finish').closest('.wizard-footer') !== null") is True
    (root / "wizard.png").write_bytes(base64.b64decode(driver(prefix + "/screenshot")))
    obs_port = os.environ.get("OWC_TEST_OBS_PORT")
    if obs_port:
        field = element("#wizard-port")
        driver(prefix + "/element/" + field + "/clear", {})
        driver(prefix + "/element/" + field + "/value", {"text":obs_port})
        if os.environ.get("OWC_TEST_PRIVATE_KEYRING") != "1":
            click("#remember-password")
        driver(prefix + "/element/" + element("#wizard-password") + "/value", {"text":"incorrect-fixture"})
        click("#wizard-next")
        for _ in range(100):
            time.sleep(0.1)
            if js("return !wizardBusy && !document.getElementById('wizard-error').hidden"):
                break
        assert js("return wizardStep") == 0, "Next must stay on connection errors"
        assert js("return state.connected") is False
        driver(prefix + "/element/" + element("#wizard-password") + "/clear", {})
        driver(prefix + "/element/" + element("#wizard-password") + "/value", {"text":"owc-isolated-test"})
        click("#wizard-next")
        for _ in range(100):
            time.sleep(0.1)
            if js("return state.connected && !state.setup.checking"):
                break
        assert js("return state.connected") is True
        if os.environ.get("OWC_TEST_PRIVATE_KEYRING") == "1":
            assert js("return state.credential_status.code") == "saved"
        assert js("return document.getElementById('wizard-password').value") == ""
        assert js("return wizardStep") == 1, "connection should advance automatically"
        assert js("return document.getElementById('wizard-success').hidden") is True
        assert js("return document.querySelector('[data-step=\"0\"]').dataset.status") == "pass"
        click('[data-step="0"]')
        time.sleep(1.6)
        assert js("return wizardStep") == 0, "polling must not undo manual navigation"
        assert js("return document.getElementById('wizard-success').hidden") is False
    click('[data-step="1"]')
    if obs_port:
        (root / 'recording-setup.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
        assert js("return document.getElementById('wizard-apply').textContent") == 'Change folder'
        assert js("return document.getElementById('wizard-confirm-folder')") is None
        click('#wizard-apply')
        for _ in range(100):
            time.sleep(0.1)
            if js("return !state.setup.checking && state.setup.checks.some(c=>c.id==='directory' && c.status==='pass')"):
                break
        assert js("return state.setup.checks.some(c=>c.id==='directory' && c.status==='pass')") is True
        assert js("return document.getElementById('wizard-apply').hidden") is True
        assert js("return document.getElementById('folder-help').textContent") == 'Ready.'
    else:
        assert js("return document.getElementById('wizard-apply').disabled") is True
    click('[data-step="2"]')
    assert js("return document.getElementById('wizard-player')") is None
    assert js("return document.getElementById('wizard-diagnostic')") is None
    click('#wizard-next')
    assert js('return wizardStep') == 3, 'Picture/audio help must not block Next'
    click('[data-step="3"]')
    assert js("return document.getElementById('wizard-finish').closest('.wizard-footer') !== null") is True
    fixture = root / "game-fixtures"
    for relative in ["one/_retail_/Logs", "one/_classic_/Logs", "two/_retail_/Logs", "one/_classic_era_"]:
        (fixture / relative).mkdir(parents=True)
    for _ in range(40):
        time.sleep(0.1)
        if js("return !discovering"):
            break
    js("document.getElementById('discovery-folder').closest('details').open=true")
    driver(prefix + '/element/' + element('#discovery-folder') + '/value', {'text':str(fixture)})
    click('#search-installations button')
    for _ in range(80):
        time.sleep(0.1)
        if js("return !discovering && discovered.length===4"):
            break
    assert js("return discovered.length") == 4
    for count in range(1,4):
        click('#discovered-installations button:not(:disabled)')
        for _ in range(80):
            time.sleep(0.1)
            if js("return !wizardBusy && state.config.installations.length") == count:
                break
        assert js("return state.config.installations.length") == count
    assert js("return new Set(state.config.installations.map(i=>i.name)).size") == 3
    assert js("return document.querySelectorAll('#wizard-installations .installation-row').length") == 3
    click('#wizard-installations button')
    for _ in range(80):
        time.sleep(0.1)
        if js("return !wizardBusy && state.config.installations.length===2"):
            break
    assert js("return state.config.installations.length") == 2
    (root / 'installations.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
    if obs_port:
        click('[data-step="2"]')
        assert js("return document.getElementById('wizard-picture-sound')") is None
        click('[data-step="3"]')
        for _ in range(100):
            time.sleep(0.1)
            if not js("return document.getElementById('wizard-finish').disabled"):
                break
        assert js("return document.getElementById('wizard-finish').disabled") is False
        click('#wizard-finish')
        time.sleep(1)
        assert js("return document.getElementById('setup-wizard').open") is False
        assert js("return state.armed") is False
        click('#setup-tab')
        click('[data-settings-tab=obs]')
        click('#refresh-scenes')
        assert js("return document.getElementById('recording-scene').options.length") > 1
        click('[data-settings-tab=general]')
        click('#settings-wizard')
        assert js('return wizardStep') == 0
        assert js("return Number(document.getElementById('wizard-port').value)===state.config.obs_port") is True
        click('[data-step="3"]')
        assert js("return document.querySelectorAll('#wizard-installations .installation-row').length") == 2
        click('#wizard-close')
    else:
        click('#wizard-close')
    assert js("return document.getElementById('setup-wizard').open") is False
    click('#library-tab')
    assert js("const old=structuredClone(state);state.connected=false;state.automatic_paused=false;state.obs_connection_error={code:'connect_failed',diagnostic:'Authentication failed'};renderState(state);const ok=document.getElementById('obs-tooltip').textContent.includes('Authentication failed')&&document.getElementById('automatic-toggle').textContent==='Auto recording blocked'&&document.getElementById('automatic-toggle').getAttribute('aria-checked')==='true';renderState(old);return ok") is True
    assert js("const before=structuredClone(state),previous=selected;state.active=[{id:'automatic-fixture',name:'Current boss',kind:'raid',start_ms:Date.now(),markers:[],outcome:'unknown'}];selected='automatic-fixture';renderState(state);const ok=!document.getElementById('live')&&!document.getElementById('manual-end').hidden&&!document.getElementById('manual-end').disabled&&document.getElementById('manual').hidden&&stoppableRecording().id==='automatic-fixture';selected=previous;renderState(before);return ok") is True
    assert js("const w=document.getElementById('recording-workspace'),l=document.getElementById('recordings-pane'),r=w.querySelector('.review');return Math.abs(r.getBoundingClientRect().left-l.getBoundingClientRect().right-1)<1&&getComputedStyle(r).borderLeftWidth==='0px'") is True
    js("window.recoveryLayoutSnapshot=structuredClone(state);state.recovery={stage:'checking',total:4,completed:2,records:{},issues:[],waiting:false};renderRecovery()")
    assert js("const panel=document.getElementById('recovery-panel'),bar=document.getElementById('recovery-progress'),head=panel.querySelector('.recovery-header');return !bar.hidden&&bar.getBoundingClientRect().top>=head.getBoundingClientRect().bottom&&Math.abs(bar.getBoundingClientRect().width-head.getBoundingClientRect().width)<1") is True
    js("state.recovery={stage:'needs_attention',total:4,completed:4,records:{a:{stage:'complete'},b:{stage:'complete'},c:{stage:'needs_attention'}},issues:[{path:'/home/player/.local/share/obswowcompanion/spool/2026-10-26 15-34-53.mp4',reason:'timing_uncertain'}]};renderRecovery()")
    assert js("const panel=document.getElementById('recovery-panel'),review=document.getElementById('recovery-issues'),actions=panel.querySelector('.actions'),head=panel.querySelector('.recovery-header');return document.getElementById('recovery-progress').hidden&&review.open&&!review.hidden&&review.getBoundingClientRect().top>=head.getBoundingClientRect().bottom&&Math.abs(review.getBoundingClientRect().width-head.getBoundingClientRect().width)<1&&Math.abs(actions.getBoundingClientRect().top-head.getBoundingClientRect().top)<1") is True
    assert js("return !document.getElementById('recovery-events')&&!document.getElementById('recovery-dismiss')&&!document.querySelector('#recovery-issue-list button')&&document.getElementById('recovery-issue-list').textContent.includes('Not automatically cleaned up')") is True
    js("state.recovery.issues.push({path:'/home/player/.local/share/obswowcompanion/spool/empty.mp4',reason:'empty_source',fingerprint:[0,1]});renderRecovery()")
    assert js("return document.querySelectorAll('#recovery-issue-list button').length===1&&document.querySelector('#recovery-issue-list button').textContent==='Delete file'") is True
    (root/'recovery-panel.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    js("renderState(window.recoveryLayoutSnapshot)")
    # A previously hidden workspace gets its restored width in ResizeObserver.
    # Measure only after that layout pass, otherwise the pointer can miss the divider.
    for _ in range(50):
        if js("return Math.abs(document.getElementById('recordings-pane').getBoundingClientRect().width-Math.min(libraryWidth,maximumLibraryWidth()))<1"): break
        time.sleep(0.05)
    assert js("return Math.abs(document.getElementById('recordings-pane').getBoundingClientRect().width-Math.min(libraryWidth,maximumLibraryWidth()))<1") is True
    js("window.paneDragEvents=[];for(const type of ['pointerdown','pointermove','pointerup','lostpointercapture'])document.getElementById('pane-divider').addEventListener(type,e=>paneDragEvents.push({type:e.type,x:e.clientX,id:e.pointerId}))")
    geometry=js("const r=document.getElementById('pane-divider').getBoundingClientRect();return {x:r.x,y:r.y+100,to:r.x+70}")
    driver(prefix+'/actions',{'actions':[{'type':'pointer','id':'pane-mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'x':round(geometry['x']),'y':round(geometry['y'])},{'type':'pointerDown','button':0},{'type':'pointerMove','duration':250,'x':round(geometry['to']),'y':round(geometry['y'])},{'type':'pointerUp','button':0}]}]})
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.recordings_width>370"): break
    assert js("return state.config.recordings_width>370") is True, js("return {events:paneDragEvents,width:libraryWidth,saved:state.config.recordings_width,uiError:document.getElementById('error').textContent}")
    click('#collapse-recordings')
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.recordings_collapsed"): break
    assert js("return document.getElementById('recordings-pane').getBoundingClientRect().width") == 44
    assert js("return getComputedStyle(document.getElementById('recordings')).display") == 'none'
    assert js("return document.getElementById('collapse-recordings').getAttribute('aria-expanded')") == 'false'
    (root / 'collapsed-workspace.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
    click('#collapse-recordings')
    js("document.getElementById('pane-divider').dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}))")
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.recordings_width===240&&!state.config.recordings_collapsed"): break
    assert js("return state.config.recordings_width===240&&!state.config.recordings_collapsed") is True
    js("document.getElementById('pane-divider').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))")
    assert js("return updates.readyState") == 1, 'state event stream is not connected'
    assert js("return !document.getElementById('connection').hasAttribute('title')") is True
    assert js("return getComputedStyle(document.getElementById('obs-tooltip')).display") == 'none'
    click('#connection')
    assert js("return getComputedStyle(document.getElementById('obs-tooltip')).display") == 'block'
    assert js("const old=structuredClone(state);state.connected=false;state.obs_connection_error={code:'unreachable'};state.credential_status={code:'store_failed',diagnostic:'Old password error'};state.obs_retry_seconds=12;renderSetup();const text=document.getElementById('obs-tooltip').textContent;const ok=document.getElementById('connection').textContent==='OBS offline'&&text.includes('Cannot reach OBS')&&text.includes('12s')&&!text.includes('password')&&!text.includes('Old password error');state.obs_connection_error={code:'credential_store'};renderSetup();const store=document.getElementById('connection').textContent==='OBS · Password store';renderState(old);return ok&&store") is True
    js("window.tooltipSnapshot=structuredClone(state);state.connected=true;state.obs_connection_error=null;state.setup={checks:[],checking:false};state.credential_status={code:'reconnected'};renderSetup()")
    assert js("return document.getElementById('obs-tooltip').textContent") == 'Ready.'
    js("state.credential_status={code:'store_failed',diagnostic:'Password could not be saved.'};state.setup.checks=[{id:'directory',status:'fail',title:'Recording folder',detail:'Change the recording folder.'}];renderSetup()")
    assert js("return document.getElementById('obs-tooltip').textContent") == 'Change the recording folder.\nCould not use the system password store.\nPassword could not be saved.'
    js("state=window.tooltipSnapshot;renderSetup()")
    click('#connection')
    assert js("return getComputedStyle(document.getElementById('obs-tooltip')).display") == 'none'

    js("window.uiErrors=[]; window.addEventListener('error',e=>uiErrors.push(e.message));")
    assert js("return document.getElementById('recording-tools').hidden") is True
    assert js("return document.getElementById('disarm').textContent") == "Pause"
    assert js("return document.getElementById('manual').disabled") == (not bool(obs_port))
    assert js("return document.getElementById('connect-form')") is None
    click("#setup-tab")
    click("[data-settings-tab=obs]")
    field = element("#obs-port")
    driver(prefix + "/element/" + field + "/clear", {})
    driver(prefix + "/element/" + field + "/value", {"text":"4455"})
    assert js("return document.getElementById('obs-port').value") == "4455", "typing blocked by shortcuts"
    assert js("return document.getElementById('diagnostic')") is None
    click('[data-settings-tab=export]')
    js("document.getElementById('obs-port').value=state.config.obs_port")
    assert js("return document.getElementById('export-mode').value") == 'fast'
    assert js("return document.getElementById('export-encoder').value") == 'cpu'
    js("document.getElementById('export-mode').value='fast';document.getElementById('export-encoder').value='vaapi';document.getElementById('export-encoder').dispatchEvent(new Event('change'))")
    assert js("return document.getElementById('vaapi-device-label').hidden") is False
    click('#save-preferences')
    for _ in range(60):
        time.sleep(0.1)
        if js("return state.config.export_mode==='fast'&&state.config.media.encoder==='vaapi'"): break
    assert js("return state.config.export_mode==='fast'&&state.config.media.encoder==='vaapi'") is True
    js("loadConfig(state.config)")
    assert js("return document.getElementById('export-mode').value==='fast'&&document.getElementById('export-encoder').value==='vaapi'") is True
    (root / 'export-settings.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
    js("document.getElementById('export-mode').value='precise';document.getElementById('export-encoder').value='cpu';document.getElementById('export-encoder').dispatchEvent(new Event('change'))")
    click('#save-preferences')
    for _ in range(60):
        time.sleep(0.1)
        if js("return state.config.export_mode==='precise'&&state.config.media.encoder==='cpu'"): break
    assert js("return document.getElementById('vaapi-device-label').hidden") is True
    assert js("""
      installation();let card=document.getElementById('installations').lastElementChild;
      const defaults=installationConfig(card),offByDefault=!defaults.categories.includes('delve');
      const set=(key,value)=>{const input=card.querySelector(`[data-key="${key}"]`);input.value=value;input.dispatchEvent(new Event('change'));};
      set('raid-mode','heroic');set('key-mode','minimum');set('minimum_key','17');set('delve-mode','on');
      const chosen=installationConfig(card);card.remove();installation(chosen);card=document.getElementById('installations').lastElementChild;
      const restored=installationConfig(card),roundtrip=restored.minimum_raid_difficulty==='heroic'&&restored.minimum_key===17&&restored.categories.includes('delve');
      set('raid-mode','off');set('key-mode','off');set('delve-mode','off');
      const disabled=installationConfig(card),allOff=['raid','mythic_plus','delve'].every(k=>!disabled.categories.includes(k));card.remove();
      installation({name:'Legacy',logs:'.',flavour:'retail',categories:['raid'],difficulties:[15],minimum_key:0});card=document.getElementById('installations').lastElementChild;
      const legacy=installationConfig(card).difficulties.join(',')==='15';set('raid-mode','normal');
      const replaced=installationConfig(card).difficulties.length===0;card.remove();
      return offByDefault&&roundtrip&&allOff&&legacy&&replaced;
    """) is True, 'Automatic recording controls preserve and round-trip policies'
    click('[data-settings-tab=obs]')
    assert js("""
      const input=document.getElementById('recording-scene').getBoundingClientRect(),button=document.getElementById('refresh-scenes').getBoundingClientRect();
      return Math.abs(input.height-button.height)<1&&Math.abs(input.top-button.top)<1;
    """) is True, 'Scene selector and Refresh share height and alignment'
    (root/'aligned-obs-settings.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    click('[data-settings-tab=wow]')
    (root/'compact-installations.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    click('[data-settings-tab=recording]')
    (root/'compact-recording-settings.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    click('#library-tab')
    click("#favourites-only")
    assert js("return document.getElementById('favourites-only').getAttribute('aria-pressed')") == "true"
    # Reset the filter fixture without a second rapid native click becoming a double-click.
    js("document.getElementById('favourites-only').click()")
    assert js("return document.getElementById('favourites-only').getAttribute('aria-pressed')") == "false"
    clip_id = str(uuid.uuid4())
    clip_path = root / "data/videos" / (clip_id + ".mkv")
    create_fixture(clip_path)
    activity = {"id":clip_id,"installation":"UI test","kind":"manual","name":"Playback test","start_ms":0,"end_ms":2000,"outcome":"success","markers":[{"at_ms":50,"kind":"combat_activity","label":"Combat activity (estimated)"},{"at_ms":500,"kind":"death","label":"Test Player"},{"at_ms":200,"kind":"boss_start","label":"Test boss"},{"at_ms":1500,"kind":"boss_end","label":"Test boss: kill"}],"warnings":[]}
    clip_path.with_suffix(".json").write_text(json.dumps({"schema_version":1,"activity":activity,"plan":None,"media":{"duration_ms":2000}}), encoding="utf-8")
    with sqlite3.connect(root / "data/library.sqlite") as database:
        database.execute("INSERT INTO activities(id,data,status,path,audio_tracks,protected) VALUES(?,?,'ready',?,2,1)", (clip_id,json.dumps(activity),str(clip_path)))
    for _ in range(40):
        time.sleep(0.25)
        if js("return document.querySelectorAll('.recording').length"):
            break
    click('.recording-select')
    for _ in range(60):
        time.sleep(0.25)
        if js("return state.preview?.key===selected && state.preview?.status==='ready' && document.getElementById('player').readyState>=2"):
            break
    assert js("return state.preview.key===selected && document.getElementById('player').readyState>=2") is True, "selecting a recording must prepare playback"
    click('#toggle-play')
    for _ in range(40):
        time.sleep(0.1)
        if js("return !reviewPlayer.paused()&&reviewPlayer.currentTime()>0"): break
    assert js("return !reviewPlayer.paused()&&document.getElementById('toggle-play').title==='Pause'") is True
    # Keep live OBS snapshots from replacing deliberately synthetic render states.
    js("window.holdFixtureUpdates=e=>{window.heldFixtureState=JSON.parse(e.data);e.stopImmediatePropagation()};updates.addEventListener('state',holdFixtureUpdates,true)")
    js("window.readySnapshot=structuredClone(state);state.activities.find(a=>a.id===selected).status='pending';state.preview=null;choose(recordingById(selected),true)")
    assert js("return !reviewPlayer.currentSrc()") is True
    assert js("return !reviewPlayer.controls()&&getComputedStyle(document.querySelector('.vjs-big-play-button')).display==='none'&&getComputedStyle(document.querySelector('.vjs-control-bar')).display==='none'") is True
    assert js("return document.getElementById('empty-player').textContent") == 'Encounter finished. The video clip is not ready yet.'
    js("recordingById(selected).processing={stage:'observed_source',title:'Waiting for OBS',detail:'OBS is still recording. Stop recording in OBS so the remaining footage can be checked.'};renderContext()")
    assert js("return document.getElementById('review-state').textContent") == 'Waiting for OBS'
    assert js("return document.getElementById('empty-player').textContent.includes('Stop recording in OBS')") is True
    js("recordingById(selected).status='exporting';recordingById(selected).processing={stage:'exporting',title:'Saving clip',progress:{stage:'encoding',completed:2,total:3},elapsed_seconds:65};renderContext()")
    assert js("return document.getElementById('empty-player').textContent") == 'Encoding footage · 2 of 3 · 1:05 elapsed'

    assert js("const old=uiLocale;uiLocale='de-DE';const valid=formatNumber(1234.5)==='1.234,5'&&searchText('E\\u0301')===searchText('É')&&message('not.a.real.key')===englishMessages.unknown;uiLocale=old;return valid") is True
    assert js("const old=englishMessages['credential.saved'];englishMessages['credential.saved']='[Saved test translation]';const result=noticeText('credential',{code:'saved'})==='[Saved test translation]';englishMessages['credential.saved']=old;return result") is True
    js("recordingById(selected).status='unavailable';delete recordingById(selected).processing;recordingById(selected).recovery={stage:'unrecoverable',reason:'missing_source'};renderContext()")
    assert js("return document.getElementById('review-state').textContent") == 'Unrecoverable'
    assert js("return document.getElementById('empty-player').textContent.includes('Restore the original footage')") is True
    assert js("return !reviewPlayer.controls()") is True
    js("recordingById(selected).recovery={stage:'needs_attention',reason:'timing_uncertain'};renderContext()")
    assert js("return document.getElementById('review-state').textContent.includes('could not be verified')") is True

    js("renderState(window.readySnapshot)")
    for _ in range(100):
        time.sleep(0.1)
        if js("return document.getElementById('player').readyState>=2"): break
    assert js("return document.getElementById('player').readyState>=2") is True
    assert js("return reviewPlayer.controls()") is True
    js("reviewPlayer.pause();reviewPlayer.currentTime(0.4);window.retainedSource=reviewPlayer.currentSrc();window.retainedLoads=0;reviewPlayer.on('loadstart',()=>window.retainedLoads++);state.preview.early=true;recordingById(selected).status='exporting';selectedStatus='exporting';renderState(structuredClone(state))")
    js("recordingById(selected).status='ready';renderState(structuredClone(state))")
    time.sleep(0.2)
    assert js("return reviewPlayer.currentSrc()===window.retainedSource&&Math.abs(reviewPlayer.currentTime()-0.4)<0.1&&window.retainedLoads===0&&selectedStatus==='ready'") is True, 'final export must not replace retained playback'
    js("state.preview.early=false")
    assert js("return [1,10,21].map(key_level=>recordingPrefix({kind:'mythic_plus',key_level}))") == [' 1','10','21']
    assert js("return [16,15,14,17].map(difficulty=>recordingPrefix({kind:'raid',difficulty}))") == [' M','HC','NM','RF']
    assert js("return recordingPrefix({kind:'raid',difficulty:999})") == '  '
    assert js("return recordingPrefix({kind:'manual'})") == '--'
    js("const a=recordingById(selected);a.kind='mythic_plus';a.key_level=21;renderList();renderRecordingTitle(document.getElementById('review-title'),a)")
    assert js("return document.querySelector('.recording.selected .recording-prefix').textContent") == '21'
    assert js("return document.querySelector('.recording.selected .recording-prefix').classList.contains('key')") is True
    assert js("return document.querySelector('.recording.selected .recording-name').textContent") == 'Playback test'
    assert js("return getComputedStyle(document.querySelector('.recording.selected .recording-prefix')).whiteSpace") == 'pre'
    assert js("const icons=document.querySelector('.recording.selected .recording-icons');const a=icons.children[0].getBoundingClientRect(),b=icons.children[1].getBoundingClientRect();return Math.abs(a.x-b.x)<1&&b.y>=a.bottom") is True
    (root / 'recording-prefixes.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
    js("const a=recordingById(selected);a.kind='manual';delete a.key_level;renderList();renderRecordingTitle(document.getElementById('review-title'),a)")
    assert js("return reviewPlayer.paused()&&document.getElementById('toggle-play').title==='Play'&&!document.getElementById('toggle-play').classList.contains('vjs-playing')") is True, "a replacement source must show Play when paused"
    click('#toggle-play')
    for _ in range(40):
        time.sleep(0.1)
        if js("return !reviewPlayer.paused()&&reviewPlayer.currentTime()>0"): break
    assert js("return !reviewPlayer.paused()&&document.getElementById('toggle-play').title==='Pause'") is True
    click('#toggle-play')
    assert js("return reviewPlayer.paused()&&document.getElementById('toggle-play').title==='Play'") is True

    js("updates.removeEventListener('state',holdFixtureUpdates,true);if(window.heldFixtureState)renderState(window.heldFixtureState)")
    assert js("return !document.querySelector('[data-event-kind=activity_end]')") is True
    assert js("const source=reviewPlayer.currentSrc();choose(reviewActivity());return source===reviewPlayer.currentSrc()") is True
    assert js("return document.getElementById('error').hidden") is True
    assert js("return document.getElementById('video-metadata').textContent.includes('H264')") is True
    assert js("return document.getElementById('playback-details').textContent.includes('Original file')||document.getElementById('playback-details').textContent.includes('compatibility copy')") is True
    assert js("return document.getElementById('toggle-markers').getAttribute('aria-pressed')") == 'true'
    click('#toggle-markers')
    click('#marker-menu input')
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.show_markers===false&&!markerVisibilitySaving"): break
    assert js("return getComputedStyle(document.getElementById('markers')).display") == 'none'
    click('#marker-menu input')
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.show_markers===true&&!markerVisibilitySaving"): break
    assert js("return getComputedStyle(document.getElementById('markers')).display!=='none'") is True
    js("[...document.querySelectorAll('#marker-menu label')].find(row=>row.textContent==='Death').querySelector('input').click()")
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.hidden_marker_types.includes('death')&&!markerVisibilitySaving"): break
    assert js("return !document.querySelector('.event-tick.death')&&document.querySelectorAll('#deaths tr').length>0") is True
    js("reviewActivity().markers.push({kind:'new_event_type',at_ms:reviewActivity().start_ms+100,label:'Future event'});renderMarkerMenu()")
    assert js("return document.getElementById('marker-menu').textContent.includes('New event type')") is True
    click('#marker-menu input')
    for _ in range(50):
        time.sleep(0.1)
        if js("return state.config.hidden_marker_types.length===0&&!markerVisibilitySaving"): break
    click('#toggle-markers')
    assert js("return document.getElementById('manual').closest('#recordings-pane')!==null&&document.getElementById('manual').textContent==='Record now'&&!document.getElementById('capture-state').getClientRects().length") is True
    assert js("return [0,59,60,31147,100800].map(elapsedLabel)") == ['0s','59s','1m','8h 39m','1d 4h']
    click('#open-events')
    assert js("return !document.getElementById('events-dialog').hidden") is True
    assert js("return document.getElementById('event-rows').children.length>0") is True
    js("document.getElementById('event-search').value='no-such-event-xyz';renderDiagnostics()")
    assert js("return document.getElementById('event-rows').textContent") == 'No matching events'
    js("document.getElementById('event-search').value='';renderDiagnostics()")
    assert js("return watcherDescription({active:true,last_event_ms:Date.now()-120000,inactivity_deadline_ms:Date.now()+180000}).includes('No exit or completion detected')") is True
    assert js("const panel=document.getElementById('events-dock').getBoundingClientRect(),workspace=document.getElementById('recording-workspace').getBoundingClientRect();return Math.abs(panel.left-workspace.left)<2&&Math.abs(panel.right-workspace.right)<2") is True
    assert js("return !document.querySelector('dialog[open]')&&document.getElementById('open-events').getAttribute('aria-expanded')==='true'") is True
    js("window.eventHeight=document.getElementById('events-dialog').getBoundingClientRect().height;document.getElementById('events-divider').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}))")
    assert js("return document.getElementById('events-dialog').getBoundingClientRect().height>window.eventHeight") is True
    point=js("const r=document.getElementById('events-divider').getBoundingClientRect();window.dragStartHeight=document.getElementById('events-dialog').getBoundingClientRect().height;return {x:Math.round(r.left+r.width/2),y:Math.round(r.top+3)}")
    driver(prefix + '/actions', {'actions':[{'type':'pointer','id':'divider-mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'x':point['x'],'y':point['y']},{'type':'pointerDown','button':0},{'type':'pointerMove','duration':200,'x':point['x'],'y':point['y']-30},{'type':'pointerUp','button':0}]}]})
    assert js("return document.getElementById('events-dialog').getBoundingClientRect().height>=window.dragStartHeight+25") is True
    js("document.getElementById('events-divider').dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}))")
    assert js("return Math.abs(document.getElementById('events-dialog').getBoundingClientRect().height-200)<1") is True
    (root / 'events-dock.png').write_bytes(base64.b64decode(driver(prefix + '/screenshot')))
    click('#close-events')
    assert js("return document.getElementById('events-dialog').hidden&&document.getElementById('open-events').getAttribute('aria-expanded')==='false'") is True
    print('Playback mode:', js("return state.preview.direct?'original file':'compatible copy'"))
    if js("return !!state.preview.direct"):
        js("reviewPlayer.error({code:4,message:'Simulated unsupported original format'})")
        for _ in range(100):
            time.sleep(0.1)
            if js("return !state.preview.direct&&state.preview.status==='ready'&&document.getElementById('player').readyState>=2"): break
        assert js("return !state.preview.direct&&state.preview.status==='ready'&&document.getElementById('player').readyState>=2") is True, js("return {preview:state.preview,selected,source:reviewPlayer.currentSrc(),ready:reviewPlayer.readyState(),playerError:reviewPlayer.error(),ui:document.getElementById('error').textContent}")
        assert js("return document.getElementById('playback-details').textContent.includes('video unchanged')&&document.getElementById('playback-details').textContent.includes('rejected')") is True


    def player_pixels(name):
        path=root/name
        path.write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
        bounds=js("const r=document.getElementById('player').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,viewport:innerWidth}")
        probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_entries','stream=width','-of','json',str(path)]))
        scale=probe['streams'][0]['width']/bounds['viewport']
        x,y,w,h=[bounds[k]*scale for k in ['x','y','width','height']]
        crop=f"crop={int(w*.6)}:{int(h*.4)}:{int(x+w*.2)}:{int(y+h*.2)}"
        return subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-vf',crop,'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-'])
    js("reviewPlayer.pause();reviewPlayer.currentTime(0.4);window.viewSource=reviewPlayer.currentSrc();window.viewLoads=0;reviewPlayer.on('loadstart',()=>window.viewLoads++)")
    time.sleep(0.3)
    before_tab_pixels=player_pixels('before-tab-video.png')
    click('#setup-tab')
    time.sleep(0.5)
    assert js("""
      const setup=document.getElementById('setup-view'),library=document.getElementById('library-view'),r=library.getBoundingClientRect();
      const points=[[.5,.4],[.8,.6],[.8,.9],[.5,.98]];
      return library.inert&&getComputedStyle(library).display!=='none'&&points.every(([x,y])=>setup.contains(document.elementFromPoint(r.x+r.width*x,r.y+r.height*y)));
    """) is True, 'Settings covers the player controls, markers and footer'
    (root/'settings-over-player.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    click('#library-tab')
    time.sleep(0.5)
    assert js("return reviewPlayer.currentSrc()===window.viewSource&&reviewPlayer.readyState()>=2&&Math.abs(reviewPlayer.currentTime()-0.4)<0.1&&!reviewPlayer.el().classList.contains('playback-unavailable')&&window.viewLoads===0") is True, js("return {src:reviewPlayer.currentSrc(),ready:reviewPlayer.readyState(),time:reviewPlayer.currentTime(),loads:window.viewLoads,classes:reviewPlayer.el().className}")
    pixels=player_pixels('returned-video-frame.png')
    coloured=sum(max(pixels[i:i+3])-min(pixels[i:i+3])>80 for i in range(0,len(pixels),3))
    assert coloured>len(pixels)/3*.1, 'Returned player visibly paints the video, not a black surface'
    assert len(pixels)==len(before_tab_pixels)
    difference=sum(abs(a-b) for a,b in zip(pixels,before_tab_pixels))/len(pixels)
    assert difference<8, f'Returned paused frame changed or became corrupted: mean difference {difference}'
    js("reviewPlayer.playbackRate(0.5);reviewPlayer.play().catch(()=>{})")
    time.sleep(0.1)
    click('#setup-tab')
    assert js("return reviewPlayer.paused()") is True
    paused_time=js("return reviewPlayer.currentTime()")
    click('#library-tab')
    time.sleep(0.2)
    assert abs(js("return reviewPlayer.currentTime()")-paused_time)<0.05
    assert js("return reviewPlayer.paused()&&reviewPlayer.playbackRate()===0.5&&window.viewLoads===0") is True
    js("reviewPlayer.playbackRate(1)")
    click('#audio-tracks')
    assert js("return !audioMenu.hidden&&audioMenu.querySelectorAll('input[type=radio]').length===2") is True
    js("window.focusedTrack=audioMenu.querySelector('input:checked');focusedTrack.focus();audioSelect.disabled=false")
    time.sleep(.1)
    assert js("return document.activeElement===focusedTrack") is True
    (root/'audio-tracks.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    js("audioMenu.querySelector('input').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))")
    assert js("return audioMenu.hidden&&document.activeElement.id==='audio-tracks'") is True
    click('#audio-tracks')
    click('#review-title')
    assert js("return audioMenu.hidden") is True
    js("reviewPlayer.pause();reviewPlayer.currentTime(.8);reviewPlayer.playbackRate(.5)")
    click('#audio-tracks')
    click('#audio-menu input[value="1"]')
    for _ in range(100):
        time.sleep(.1)
        if js("return !pendingAudioSwitch&&state.preview?.track===1&&reviewPlayer.readyState()>=2"):break
    assert js("return state.preview.track===1&&reviewPlayer.paused()&&reviewPlayer.playbackRate()===.5&&Math.abs(reviewPlayer.currentTime()-.8)<.15") is True, js("return {track:state.preview.track,paused:reviewPlayer.paused(),rate:reviewPlayer.playbackRate(),time:reviewPlayer.currentTime(),pending:pendingAudioSwitch}")
    click('#toggle-play')
    js("document.getElementById('audio').value='0';document.getElementById('audio').dispatchEvent(new Event('change'))")
    for _ in range(100):
        time.sleep(.1)
        if js("return !pendingAudioSwitch&&state.preview?.track===0&&!reviewPlayer.paused()"):break
    assert js("return state.preview.track===0&&!reviewPlayer.paused()&&reviewPlayer.playbackRate()===.5") is True
    js("reviewPlayer.pause();reviewPlayer.playbackRate(1)")

    js("const a=structuredClone(reviewActivity());a.markers.find(m=>m.kind==='death').recent_hits=[{offset_ms:-100,source:'Boss',ability:'Fireball'}];renderMarkers(a)")
    assert 'Fireball' in js("return document.querySelector('#deaths .death-recap').textContent")

    assert js("return document.getElementById('recording-tools').hidden") is False
    assert js("return document.getElementById('delete').disabled") is True
    assert js("return document.getElementById('clip-options').open") is False
    assert js("return document.getElementById('retry').hidden") is True
    assert js("return document.getElementById('play').hidden") is True
    assert js("return document.querySelectorAll('#deaths tr').length") == 1
    assert js("return document.querySelectorAll('#markers .event-tick').length") >= 3
    click('#deaths tr td:first-child')
    assert js("return document.getElementById('player').currentTime") < 0.05
    js("document.activeElement.blur();window.playerKey=(key,extra={})=>document.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...extra}));reviewPlayer.pause()")
    js("playerKey('k')")
    time.sleep(.1)
    assert js("return !reviewPlayer.paused()") is True
    js("playerKey(' ')")
    assert js("return reviewPlayer.paused()") is True
    js("window.beforeMute=reviewPlayer.muted();playerKey('m')")
    assert js("return reviewPlayer.muted()!==beforeMute") is True
    js("playerKey('m');reviewPlayer.volume(.5);playerKey('ArrowUp')")
    assert abs(js("return reviewPlayer.volume()")-.55)<.001
    js("playerKey('>')")
    assert js("return reviewPlayer.playbackRate()") == 1.25
    js("playerKey('<');playerKey('5')")
    assert abs(js("return reviewPlayer.currentTime()-reviewPlayer.duration()/2"))<.05
    js("playerKey('ArrowLeft')")
    assert js("return reviewPlayer.currentTime()") == 0
    js("playerKey('l')")
    assert abs(js("return reviewPlayer.currentTime()-reviewPlayer.duration()"))<.05
    js("playerKey('j')")
    assert js("return reviewPlayer.currentTime()") == 0
    js("document.getElementById('tags').focus();window.beforeMute=reviewPlayer.muted();document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'m',bubbles:true,cancelable:true}))")
    assert js("return reviewPlayer.muted()===beforeMute") is True
    js("document.activeElement.blur();show('setup');playerKey('m')")
    assert js("return reviewPlayer.muted()===beforeMute") is True
    js("show('library')")
    js("seekTo(0.5)")
    js("document.activeElement.blur();document.dispatchEvent(new KeyboardEvent('keydown',{key:'.',bubbles:true}))")
    assert abs(js("return document.getElementById('player').currentTime") - (0.5+1/30)) < 0.02
    js("const rect=reviewPlayer.el().getBoundingClientRect();document.getElementById('player').dispatchEvent(new WheelEvent('wheel',{deltaY:-100,clientX:rect.left+rect.width*.7,clientY:rect.top+rect.height*.4,bubbles:true,cancelable:true}))")
    assert js("return videoZoom>1&&!document.getElementById('reset-video-zoom').hidden&&videoPanX<0") is True
    assert js("return getComputedStyle(controlBar.el()).transform") == 'none'
    js("controlBar.el().dispatchEvent(new WheelEvent('wheel',{deltaY:-100,bubbles:true,cancelable:true}))")
    assert abs(js("return videoZoom")-1.2)<0.001
    click('#reset-video-zoom')
    assert js("return videoZoom===1&&videoPanX===0&&videoPanY===0") is True
    click('#add-marker')
    js("document.getElementById('marker-label').value='Review this';document.getElementById('marker-form').requestSubmit()")
    for _ in range(50):
        time.sleep(0.1)
        if js("return document.querySelectorAll('.event-tick.manual').length") == 1: break
    assert js("return document.querySelectorAll('.event-tick.manual').length") == 1
    assert js("return document.querySelectorAll('#combat-track .boss').length") == 1
    assert js("return document.querySelectorAll('#combat-track .trash').length") == 1
    sidecar=json.loads(clip_path.with_suffix('.json').read_text(encoding="utf-8"))
    assert any(m['kind']=='manual' and m['label']=='Review this' and abs(m['at_ms']-600)<150 for m in sidecar['activity']['markers'])
    assert js("""
      const track=seekTrack.getBoundingClientRect(),duration=document.getElementById('player').duration;
      return [...document.querySelectorAll('.event-tick')].every(marker=>{
        const rect=marker.getBoundingClientRect();
        return Math.abs((rect.left+rect.width/2)-(track.left+Number(marker.dataset.seconds)/duration*track.width))<1.5;
      });
    """) is True
    driver(prefix + "/window/rect", {"width":1024,"height":768})
    time.sleep(0.3)
    assert js("""
      const track=seekTrack.getBoundingClientRect(),duration=document.getElementById('player').duration;
      return [...document.querySelectorAll('.event-tick')].every(marker=>{const rect=marker.getBoundingClientRect();return Math.abs(rect.left+rect.width/2-track.left-Number(marker.dataset.seconds)/duration*track.width)<1.5;});
    """) is True
    driver(prefix + "/window/rect", {"width":1280,"height":960})
    time.sleep(0.3)
    position_before_fullscreen=js("return document.getElementById('player').currentTime")
    marker_rect=js("const r=document.querySelector('.event-tick.death').getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2}")
    driver(prefix+'/actions',{'actions':[{'type':'pointer','id':'hover-mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':100,'x':round(marker_rect['x']),'y':round(marker_rect['y'])}]}]})
    assert js("return getComputedStyle(document.querySelector('.event-tick.death .sign-label')).display") != 'none'
    assert js("return document.querySelector('.event-tick.death .sign-label').textContent.includes('Test Player')") is True
    assert js("return getComputedStyle(document.querySelector('.vjs-mouse-display')).display") == 'none'
    click('#fullscreen')
    for _ in range(30):
        time.sleep(0.1)
        if js("return !!document.fullscreenElement"): break
    assert js("return document.fullscreenElement?.classList.contains('video-js')") is True
    assert js("return document.fullscreenElement.contains(document.getElementById('toggle-play'))") is True
    assert js("""
      const track=seekTrack.getBoundingClientRect(),duration=document.getElementById('player').duration;
      return [...document.querySelectorAll('.event-tick')].every(marker=>{
        const rect=marker.getBoundingClientRect();return Math.abs(rect.left+rect.width/2-track.left-Number(marker.dataset.seconds)/duration*track.width)<1.5;
      });
    """) is True
    click('#fullscreen')
    time.sleep(0.3)
    assert js("return !!document.fullscreenElement") is False
    assert abs(js("return document.getElementById('player').currentTime")-position_before_fullscreen)<0.05
    click('.vjs-playback-rate .vjs-menu-button')
    rate_index=js("return [...document.querySelectorAll('.vjs-playback-rate .vjs-menu-item')].findIndex(item=>item.textContent.startsWith('0.25'))+1")
    assert rate_index > 0
    click(f'.vjs-playback-rate .vjs-menu-item:nth-child({rate_index})')
    assert js("return document.getElementById('player').playbackRate") == 0.25
    driver(prefix + '/element/' + element('#tags') + '/value', {'text':'progression'})
    js("document.getElementById('annotation').requestSubmit()")
    for _ in range(50):
        time.sleep(0.1)
        if js("return (state.activities.find(a=>a.id===selected)?.tags||[]).includes('progression')"): break
    assert js("return document.querySelector('#known-labels option')?.value") == 'progression'
    assert js("return document.querySelector('.recording-label')?.textContent") == 'progression'
    assert js("return document.getElementById('search').getAttribute('list')") == 'known-labels'
    js("document.getElementById('search').value='PROGR';renderList()")
    assert js("return document.querySelectorAll('.recording').length") == 1
    js("document.getElementById('search').value='never-create-this-label';renderList()")
    assert js("return document.querySelectorAll('.recording').length") == 0
    assert js("return [...document.querySelectorAll('#known-labels option')].map(o=>o.value)") == ['progression']
    js("document.getElementById('search').value='';renderList()")
    js("const strip=document.querySelector('.recording-labels');strip.style.width='5px';fitRecordingLabels()")
    assert js("return [...document.querySelectorAll('.recording-label')].every(label=>label.hidden)") is True
    js("document.querySelector('.recording-labels').style.width='';fitRecordingLabels()")
    click('#label-chips button')
    for _ in range(50):
        time.sleep(0.1)
        if js("return document.querySelectorAll('#known-labels option').length") == 0: break
    assert js("return document.querySelectorAll('#known-labels option').length") == 0
    click('.recording .heart')
    for _ in range(50):
        time.sleep(0.1)
        if js("return document.querySelector('.recording .heart').getAttribute('aria-pressed')") == 'true': break
    assert js("return document.querySelector('.recording .heart').getAttribute('aria-pressed')") == 'true'
    assert js("return getComputedStyle(document.querySelector('.header-controls')).display") == 'flex'
    assert js("return document.getElementById('connection').tagName") == 'BUTTON'
    assert js("return document.getElementById('player').controls") is False
    assert js("return window.uiErrors") == []
    driver(prefix + "/window/rect", {"width":1280,"height":960})
    time.sleep(0.3)
    js("document.querySelector('.review').scrollTop=0;window.scrollTo(0,0)")
    assert js("""
      const original=state, originalSort=document.getElementById('sort').value;
      const fixture=(id,start_ms)=>({id,start_ms,name:id,kind:'manual',installation:'Test',tags:[],status:'ready',end_ms:start_ms+1000});
      state={...state,active:[fixture('older-active',1000)],activities:[fixture('newer-manual',2000)]};
      document.getElementById('sort').value='new';renderList();
      const newest=[...document.querySelectorAll('.recording')].map(row=>row.dataset.id);
      document.getElementById('sort').value='old';renderList();
      const oldest=[...document.querySelectorAll('.recording')].map(row=>row.dataset.id);
      state=original;document.getElementById('sort').value=originalSort;renderList();
      return JSON.stringify([newest,oldest]);
    """) == '[["newer-manual","older-active"],["older-active","newer-manual"]]'
    assert js("return !document.getElementById('precise')") is True
    assert js("""
      const volume=document.querySelector('.vjs-volume-panel').getBoundingClientRect();
      const time=document.querySelector('.vjs-current-time').getBoundingClientRect();return volume.right<=time.left;
    """) is True
    assert js("return !document.getElementById('clip-options').open") is True
    click('#clip-options summary')
    click('#select-range')
    js("document.querySelector('.vjs-progress-control').scrollIntoView({block:'center'})")
    def drag_range(boundary, fraction):
        geometry=js("const h=document.querySelector('.range-handle.'+arguments[0]).getBoundingClientRect(),t=seekTrack.getBoundingClientRect();return {x:h.left+h.width/2,y:h.top+h.height/2,to:t.left+t.width*arguments[1]};",[boundary,fraction])
        driver(prefix+'/actions',{'actions':[{'type':'pointer','id':'range-mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'x':round(geometry['x']),'y':round(geometry['y'])},{'type':'pointerDown','button':0},{'type':'pointerMove','duration':250,'x':round(geometry['to']),'y':round(geometry['y'])},{'type':'pointerUp','button':0}]}]})
    drag_range('start',0.2)
    drag_range('end',0.7)
    duration=js("return reviewPlayer.duration()")
    assert abs(js("return Number(document.getElementById('trim-start').value)")-duration*0.2)<0.05
    assert abs(js("return Number(document.getElementById('trim-end').value)")-duration*0.7)<0.05
    assert js("return document.getElementById('trim').contains(document.getElementById('export-clip'))") is True
    click('#fullscreen')
    time.sleep(0.3)
    assert js("""
      const track=seekTrack.getBoundingClientRect(),duration=reviewPlayer.duration();return [...document.querySelectorAll('.range-handle')].every(handle=>{const rect=handle.getBoundingClientRect();return Math.abs(rect.left+rect.width/2-track.left-Number(handle.getAttribute('aria-valuenow'))/duration*track.width)<1.5;});
    """) is True
    click('#fullscreen')
    time.sleep(0.3)
    assert js("""
      const original=document.querySelector('.event-tick'),copies=Array.from({length:5},()=>{const copy=original.cloneNode(true);copy.style.left='50%';return copy;});
      document.getElementById('markers').replaceChildren(...copies);layoutSignposts();
      const result=copies.every((tick,index)=>Number(tick.style.getPropertyValue('--lane'))===index&&Number(tick.style.zIndex)===100-index);
      copies.forEach((tick,index)=>tick.style.left=`${index*20+5}%`);layoutSignposts();
      const separate=copies.every(tick=>Number(tick.style.getPropertyValue('--lane'))===0);
      renderMarkers(reviewActivity());return result&&separate;
    """) is True
    (root/'export-range.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    js("document.getElementById('share-path').value=arguments[0]",[str(root/'share.zip')])
    assert js("""
      const original=state.activities;const make=(n,kind='mythic_plus')=>({id:'test-'+n,name:'Fixture '+n,kind,start_ms:new Date(2026,0,n%28+1).getTime(),end_ms:1,status:'ready',difficulty:kind==='raid'?15:8,key_level:kind==='raid'?null:[17,19,21][n%3],tags:[]});
      state.activities=Array.from({length:300},(_,n)=>make(n));state.activities.push(make(301,'raid'));renderList();
      const keys=[...document.getElementById('key-filter').options].map(o=>o.value).join(',')===',17,19,21';
      document.getElementById('key-filter').value='19';renderList();const filtered=document.querySelectorAll('.recording').length===100;
      document.getElementById('key-filter').value='';document.getElementById('kind-filter').value='raid';renderList();const heroic=[...document.getElementById('difficulty-filter').options].map(o=>o.textContent).join(',')==='All difficulties,Heroic';
      document.getElementById('kind-filter').value='';renderList();const list=document.getElementById('recordings'),top=document.getElementById('recordings-filters').getBoundingClientRect().top;list.scrollTop=400;const scroll=list.scrollTop>0&&document.getElementById('recordings-filters').getBoundingClientRect().top===top;
      document.getElementById('date-from').value='2026-02-01';renderList();const dates=document.querySelectorAll('.recording').length===0;
      document.getElementById('date-from').value='';state.activities=original;renderList();
      const footer=document.getElementById('labels-panel').getBoundingClientRect(),body=document.querySelector('.review-scroll').getBoundingClientRect();
      return keys&&filtered&&heroic&&scroll&&dates&&body.bottom<=footer.top+1;
    """) is True, 'Dynamic filters and independent scroll areas'
    assert js("""
      const a=reviewActivity(),original=a.markers;a.markers=Array.from({length:300},(_,n)=>({kind:'death',at_ms:a.start_ms+n,label:'Player '+n}));renderMarkers(a);
      const scroll=document.querySelector('.death-scroll');scroll.scrollTop=200;const independent=scroll.scrollHeight>scroll.clientHeight&&scroll.scrollTop>0&&scroll.clientHeight>0;
      a.markers=original;renderMarkers(a);return independent;
    """) is True, 'Long death list scrolls within its section'
    driver(prefix + '/window/rect', {'width':1280,'height':1400})
    time.sleep(0.3)
    assert js("""
      const clip=document.getElementById('clip-options'),details=document.getElementById('video-details'),wasOpen=clip.open,wasDetailsOpen=details.open;
      clip.open=false;details.open=false;
      const scroll=document.querySelector('.death-scroll'),height=scroll.clientHeight;
      const tools=document.getElementById('recording-tools'),footer=document.getElementById('labels-panel');
      const pinned=Math.abs(tools.getBoundingClientRect().bottom-footer.getBoundingClientRect().top)<2;
      clip.open=true;const shrinks=scroll.clientHeight<height;
      clip.open=wasOpen;details.open=wasDetailsOpen;
      return height>220&&pinned&&shrinks;
    """) is True, 'Deaths use spare height and yield space to expanded export'
    driver(prefix + '/window/rect', {'width':1280,'height':960})
    assert js("""
      const clip=document.getElementById('clip-options'),wasOpen=clip.open,browse=document.getElementById('browse-share'),wasHidden=browse.hidden;
      clip.open=true;browse.hidden=false;
      const aligned=(a,b)=>{const x=document.querySelector(a).getBoundingClientRect(),y=document.querySelector(b).getBoundingClientRect();return x.height>0&&Math.abs(x.height-y.height)<1&&Math.abs(x.top-y.top)<1;};
      const result=aligned('#recording-tools #share-path','#browse-share')&&aligned('#select-range','#trim-start')&&aligned('#tags','#delete');
      clip.open=wasOpen;browse.hidden=wasHidden;return result;
    """) is True, 'Export and label actions align with adjacent inputs'
    # Hold a real child process so cancellation is exercised while media work is live.
    wrapper = root / 'held-ffmpeg.py'
    started = root / 'held-ffmpeg.pid'
    wrapper.write_text("#!/usr/bin/env python3\nimport os,pathlib,sys,time\n"
        + f"started=pathlib.Path({str(started)!r})\n"
        + "if any(arg.endswith(('part-0000.mkv','.partial.mp4')) for arg in sys.argv[1:]):\n"
        + " started.write_text(str(os.getpid()))\n while True: time.sleep(0.1)\n"
        + "os.execvp('ffmpeg',['ffmpeg',*sys.argv[1:]])\n", encoding="utf-8")
    wrapper.chmod(0o700)
    js("window.originalMedia=structuredClone(state.config.media);window.cancelConfigSaved=false;const config=structuredClone(state.config);config.media.ffmpeg=arguments[0];action('save_config',{config}).then(()=>window.cancelConfigSaved=true)",[str(wrapper)])
    for _ in range(50):
        time.sleep(0.1)
        if js("return window.cancelConfigSaved"): break
    assert js("return window.cancelConfigSaved") is True
    click('#export-clip')
    for _ in range(100):
        time.sleep(0.1)
        if started.exists() and js("return state.share_running&&!document.getElementById('cancel-share').hidden"): break
    assert started.exists(), 'held worker never started'
    worker_pid = int(started.read_text(encoding="utf-8"))
    assert Path(f'/proc/{worker_pid}').exists(), 'worker must be alive before cancellation'
    click('#cancel-share')
    for _ in range(100):
        time.sleep(0.1)
        if js("return !state.share_running&&state.share?.status==='cancelled'") and not Path(f'/proc/{worker_pid}').exists(): break
    assert js("return !state.share_running&&state.share?.status==='cancelled'") is True, js("return state.share")
    assert not Path(f'/proc/{worker_pid}').exists(), 'cancelled media child remains alive'
    assert not (root/'share.zip').exists(), 'cancelled export published a ZIP'
    assert not list(root.glob('.owc-share-*')), 'cancelled export leaked staging files'
    js("window.cancelConfigSaved=false;const config=structuredClone(state.config);config.media=window.originalMedia;action('save_config',{config}).then(()=>window.cancelConfigSaved=true)")
    for _ in range(50):
        time.sleep(0.1)
        if js("return window.cancelConfigSaved"): break
    assert js("return window.cancelConfigSaved") is True
    click('#export-clip')
    for _ in range(200):
        time.sleep(0.1)
        if js("return !state.share_running&&state.share?.status"): break
    assert js("return state.share?.status==='ready'") is True, js("return state.share")
    with zipfile.ZipFile(root/'share.zip') as archive:
        assert {'index.html','recording.json','video.mp4','poster.jpg','assets/viewer.js','assets/video.min.js'} <= set(archive.namelist())
        shared=json.loads(archive.read('recording.json'))
        assert shared['format']=='owc-share' and shared['video']['duration_ms']>0
        assert 'installation' not in json.dumps(shared) and str(root) not in json.dumps(shared)
        archive.extractall(root/'shared-viewer')
    assert js("return !!state.activities.find(a=>a.id===selected).latest_bundle?.video?.duration_ms") is True
    assert js("return document.getElementById('bundle-metadata').textContent.includes('share.zip')") is True
    assert js("return document.getElementById('share-audio').textContent.includes('Game test')&&document.getElementById('share-audio').textContent.includes('Voice test')") is True
    assert js("const input=document.getElementById('share-path'),old=input.value;bundlePathChosen({recording:'different',path:'/wrong.zip'});if(input.value!==old)return false;bundlePathChosen({recording:selected,path:null});return input.value===old;") is True
    assert js("return document.getElementById('open-video-folder').hidden && document.getElementById('open-bundle-folder').hidden") is True
    folder_actions = js("""
      window.companionDesktop=true;
      const previous=window.ipc,messages=[];
      window.ipc={postMessage:value=>messages.push(JSON.parse(value))};
      renderPlaybackDetails();
      document.getElementById('open-video-folder').click();
      document.getElementById('open-bundle-folder').click();
      window.ipc=previous;window.companionDesktop=false;renderPlaybackDetails();
      return {messages,original:recordingById(selected).path,bundle:recordingById(selected).latest_bundle.path};
    """)
    assert folder_actions["messages"] == [
        {"action":"open-containing-folder", "path":folder_actions["original"]},
        {"action":"open-containing-folder", "path":folder_actions["bundle"]},
    ]
    js("document.getElementById('video-details').open=true;document.getElementById('video-details').scrollIntoView({block:'center'})")
    assert js("return getComputedStyle(document.getElementById('video-metadata')).display==='table-row-group'") is True
    (root/'asset-details.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    # Exercise the durable automatic preparation path with a held encoder.
    started.unlink()
    wrapper.write_text(wrapper.read_text(encoding="utf-8").replace("('part-0000.mkv','.partial.mp4')", "('part-0000.mkv',)"), encoding="utf-8")
    js("window.originalRecording=selected;window.cancelConfigSaved=false;const config=structuredClone(state.config);config.media.ffmpeg=arguments[0];action('save_config',{config}).then(()=>window.cancelConfigSaved=true)",[str(wrapper)])
    for _ in range(50):
        time.sleep(0.1)
        if js("return window.cancelConfigSaved"): break
    assert js("return window.cancelConfigSaved") is True
    js("action('trim',{id:selected,start_ms:200,end_ms:1500,precise:true})")
    for _ in range(100):
        time.sleep(0.1)
        if started.exists() and js("return state.activities.some(a=>a.name==='Playback test: clip'&&a.status==='exporting')"): break
    assert started.exists(), 'automatic worker never started'
    worker_pid = int(started.read_text(encoding="utf-8"))
    js("choose(state.activities.find(a=>a.name==='Playback test: clip'))")
    for _ in range(50):
        time.sleep(0.1)
        if js("return !document.getElementById('cancel-export').hidden"): break
    click('#cancel-export')
    for _ in range(100):
        time.sleep(0.1)
        if js("return recordingById(selected)?.status==='failed'&&!state.export_running") and not Path(f'/proc/{worker_pid}').exists(): break
    assert not Path(f'/proc/{worker_pid}').exists(), 'automatic media child remains alive'
    assert js("return recordingById(selected)?.error?.includes('cancelled')&&!document.getElementById('retry').hidden") is True
    assert not list((root/'data/videos').glob('.*-export')), 'automatic cancellation leaked staging files'
    js("window.cancelConfigSaved=false;const config=structuredClone(state.config);config.media=window.originalMedia;action('save_config',{config}).then(()=>window.cancelConfigSaved=true)")
    for _ in range(50):
        time.sleep(0.1)
        if js("return window.cancelConfigSaved"): break
    assert js("return window.cancelConfigSaved") is True
    click('#retry')
    for _ in range(100):
        time.sleep(0.1)
        if js("return state.activities.some(a=>a.name==='Playback test: clip'&&a.status==='ready')"): break
    assert js("return state.activities.some(a=>a.name==='Playback test: clip'&&a.status==='ready')") is True
    js("choose(state.activities.find(a=>a.id===window.originalRecording))")
    assert js("return document.getElementById('labels-panel').contains(document.getElementById('delete'))") is True
    assert js("return document.querySelectorAll('.recording>.trash').length===document.querySelectorAll('.recording').length") is True
    assert js("return [1,2,3].every(n=>recordingResult({kind:'mythic_plus',keystone_upgrade:n}).text==='+'+n)&&recordingResult({kind:'mythic_plus',keystone_upgrade:0}).text==='D'&&recordingResult({kind:'raid',outcome:'success'}).text==='K'&&recordingResult({kind:'raid',outcome:'failure'}).text==='W'") is True
    js("window.confirmCalls=0;window.confirm=()=>{window.confirmCalls++;return false};window.deleteTarget=state.activities.find(a=>a.name==='Playback test: clip').id")
    js("document.querySelector('[data-id=\"'+window.deleteTarget+'\"]>.trash').click()")
    assert js("return window.confirmCalls===1&&state.activities.some(a=>a.id===window.deleteTarget)") is True
    js("document.querySelector('[data-id=\"'+window.deleteTarget+'\"]>.trash').dispatchEvent(new MouseEvent('click',{bubbles:true,shiftKey:true}))")
    for _ in range(50):
        time.sleep(0.1)
        if js("return !state.activities.some(a=>a.id===window.deleteTarget)"): break
    assert js("return window.confirmCalls===1&&!state.activities.some(a=>a.id===window.deleteTarget)") is True
    assert js("return window.uiErrors") == []
    image = driver(prefix + "/screenshot")
    (root / "browser.png").write_bytes(base64.b64decode(image))
    assert js("return document.getElementById('trim').contains(document.getElementById('export-clip'))") is True
    assert js("return document.getElementById('share-path').getBoundingClientRect().width > document.getElementById('trim').getBoundingClientRect().width * .9") is True
    assert js("return Math.abs(document.getElementById('connection').getBoundingClientRect().height-document.getElementById('automatic-toggle').getBoundingClientRect().height)<1") is True
    click('#date-range-open')
    js("calendarMonth=new Date(2026,9,1,12);renderCalendar()")
    click('#date-calendar [data-date="2026-10-20"]')
    click('#date-calendar [data-date="2026-10-10"]')
    assert js("return document.getElementById('date-from').value===''&&rangeDraftStart==='2026-10-10'&&rangeDraftEnd==='2026-10-20'") is True
    driver(prefix+'/element/'+element('#date-range-apply')+'/value',{'text':'\ue00c'})
    assert js("return !dateDialog.open&&document.getElementById('date-from').value===''") is True
    click('#date-range-open')
    js("calendarMonth=new Date(2026,9,1,12);renderCalendar()")
    click('#date-calendar [data-date="2026-10-10"]')
    click('#date-calendar [data-date="2026-10-20"]')
    click('#date-range-apply')
    assert js("return !dateDialog.open&&document.getElementById('date-from').value==='2026-10-10'&&!document.getElementById('date-range-reset').hidden") is True
    click('#date-range-open')
    click('#date-calendar [data-date="2026-10-12"]')
    driver(prefix+'/actions',{'actions':[{'type':'pointer','id':'date-outside','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'x':5,'y':5},{'type':'pointerDown','button':0},{'type':'pointerUp','button':0}]}]})
    assert js("return !dateDialog.open&&document.getElementById('date-from').value==='2026-10-10'") is True
    click('#date-range-reset')
    assert js("return document.getElementById('date-from').value===''&&document.getElementById('date-to').value===''&&document.getElementById('date-range-label').textContent==='All dates'") is True
    click('#date-range-open')
    (root/'date-range.png').write_bytes(base64.b64decode(driver(prefix+'/screenshot')))
    click('#date-range-all')
    assert js("return !dateDialog.open") is True
    main_window=driver(prefix+'/window')
    shared_window=driver(prefix+'/window/new',{'type':'tab'})['handle']
    driver(prefix+'/window',{'handle':shared_window})
    driver(prefix + '/url', {'url':(root/'shared-viewer/index.html').as_uri()})
    for _ in range(60):
        time.sleep(0.1)
        if js("return typeof player!=='undefined'&&player.readyState()>=2"): break
    if not js("return typeof player!=='undefined'&&player.readyState()>=2"):
        driver(prefix + '/element/' + element('#open-video') + '/value', {'text':str(root/'shared-viewer/video.mp4')})
        for _ in range(60):
            time.sleep(0.1)
            if js("return player.readyState()>=2"): break
    assert js("return document.getElementById('title').textContent==='Playback test'&&player.readyState()>=2") is True
    click('.vjs-big-play-button')
    time.sleep(0.5)
    assert js("return player.currentTime()>0") is True
    js("player.pause()")
    with socket.socket() as available:
        available.bind(('127.0.0.1',0));share_port=available.getsockname()[1]
    launch(['python3','-m','http.server',str(share_port),'--bind','127.0.0.1','--directory',str(root/'shared-viewer')],'share-host.log')
    time.sleep(0.3)
    driver(prefix+'/url',{'url':f'http://127.0.0.1:{share_port}/index.html'})
    for _ in range(60):
        time.sleep(0.1)
        if js("return typeof player!=='undefined'&&player.readyState()>=2"): break
    assert js("return player.readyState()>=2&&document.querySelectorAll('.marker').length>0") is True, 'Hosted bundle playback'
    js("window.posterImage=new Image();window.posterImage.src=player.poster()")
    for _ in range(30):
        time.sleep(0.1)
        if js("return window.posterImage.complete&&window.posterImage.naturalWidth>0"): break
    assert js("return player.poster()==='poster.jpg'&&window.posterImage.naturalWidth>0&&window.posterImage.naturalWidth<=640&&getComputedStyle(document.querySelector('.vjs-poster')).display!=='none'") is True, 'Shared poster loads before playback'
    click('.vjs-big-play-button');time.sleep(0.3)
    assert js("return player.currentTime()>0") is True
    driver(prefix+'/window',method='DELETE')
    driver(prefix+'/window',{'handle':main_window})
    started.unlink(missing_ok=True)
    js("window.cancelConfigSaved=false;const config=structuredClone(state.config);config.media.ffmpeg=arguments[0];action('save_config',{config}).then(()=>window.cancelConfigSaved=true)",[str(wrapper)])
    for _ in range(50):
        time.sleep(0.1)
        if js("return window.cancelConfigSaved"): break
    assert js("return window.cancelConfigSaved") is True
    source_before_quit = js("return recordingById(selected).path")
    js("action('trim',{id:selected,start_ms:200,end_ms:1500,precise:true})")
    for _ in range(100):
        time.sleep(0.1)
        if started.exists() and js("return state.export_running"): break
    assert started.exists(), 'quit fixture encoder did not start'
    worker_pid = int(started.read_text(encoding="utf-8"))
    pending_id = js("return state.activities.find(a=>a.name==='Playback test: clip'&&a.status==='exporting').id")
    click('#quit')
    assert processes[1].wait(timeout=15) == 0, "Quit button must stop the service with its UI stream connected"
    assert not Path(f'/proc/{worker_pid}').exists(), 'quit left encoder running'
    assert Path(source_before_quit).is_file(), 'quit lost source footage'
    with sqlite3.connect(root/'data/library.sqlite') as db:
        saved = db.execute("SELECT status,plan FROM activities WHERE id=?", (pending_id,)).fetchone()
        assert saved[0] == 'pending' and saved[1], saved
    assert not list((root/'data/videos').glob('.*-export')), 'quit leaked export staging'
    driver(prefix, method="DELETE")
    print(f"WebKit startup wizard, typing, playback speed and favourites passed: {root}")
finally:
    try:
        (root / "last-state.png").write_bytes(base64.b64decode(driver(prefix + "/screenshot")))
    except Exception:
        pass
    print(f"Browser artifacts: {root}")
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    held = root / 'held-ffmpeg.pid'
    if held.exists():
        try:
            pid = int(held.read_text(encoding="utf-8"))
            if str(root/'held-ffmpeg.py').encode() in Path(f'/proc/{pid}/cmdline').read_bytes():
                import signal
                os.kill(pid, signal.SIGKILL)
        except (FileNotFoundError, ProcessLookupError):
            pass
    for log in logs:
        log.close()
