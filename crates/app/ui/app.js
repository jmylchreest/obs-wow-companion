'use strict';
const $=id=>document.getElementById(id);
const kinds=['raid','mythic_plus','arena','solo_shuffle','battleground','manual'];
let state={},selected=null,configLoaded=false,lastPreview='',manual=false,lastListData='';
const label=x=>String(x||'').replaceAll('_',' ').replace(/^./,c=>c.toUpperCase());
function error(e){$('error').textContent=e.message||String(e);$('error').hidden=false;}
async function action(action,data={}){ $('error').hidden=true;const r=await fetch('/api/action',{method:'POST',headers:{'Content-Type':'application/json','X-OWC-Request':'1'},body:JSON.stringify({action,...data})});const result=await r.json();if(!r.ok)throw Error(result.error||r.statusText);return result;}
function run(fn){return async e=>{e?.preventDefault();try{await fn(e);}catch(e){error(e);}};}
function show(tab){
  if(tab==='setup')reviewPlayer.pause();
  $('library-view').hidden=tab!=='library';$('setup-view').hidden=tab!=='setup';
  $('library-tab').classList.toggle('active',tab==='library');$('setup-tab').classList.toggle('active',tab==='setup');
  for(const [id,active] of [['library-tab',tab==='library'],['setup-tab',tab==='setup']]){$(id).setAttribute('aria-selected',String(active));$(id).tabIndex=active?0:-1;}

  if(tab==='library')requestAnimationFrame(()=>{
    // WebKit can discard the displayed paused frame while its ancestor is hidden.
    const video=$('player');
    if(video.paused&&video.readyState>=2&&Number.isFinite(video.currentTime))video.currentTime=video.currentTime;
    updatePlaybackAvailability();syncPlayToggle();layoutSignposts();
  });
}
$('quit').onclick=()=>requestDesktopQuit();
$('library-tab').onclick=()=>show('library');$('setup-tab').onclick=run(async()=>{show('setup');if(state.connected)await refreshScenes();});
$('automatic-toggle').onclick=run(()=>action(state.automatic_paused?'resume_automatic':'pause_automatic'));
for(const id of ['library-tab','setup-tab'])$(id).onkeydown=event=>{if(['ArrowLeft','ArrowRight','Home','End'].includes(event.key)){event.preventDefault();const target=event.key==='Home'?'library-tab':event.key==='End'?'setup-tab':id==='library-tab'?'setup-tab':'library-tab';$(target).click();$(target).focus();}};
$('arm').onclick=run(()=>action('resume_automatic'));$('disarm').onclick=run(()=>action('pause_automatic'));
async function startManualClip(){await action('manual_start',{name:'Manual recording',installation_id:$('manual-installation-source').value||undefined});manual=true;}
$('manual').onclick=run(startManualClip);
$('manual-end').onclick=run(async()=>{await action('manual_stop');manual=false;});
$('live').onclick=run(async()=>{selected=null;renderMarkers(null);await action('live_preview',{track:0});show('library');$('review-title').textContent='Live review: last closed segment';});
$('autostart').onchange=run(()=>action('autostart',{enabled:$('autostart').checked}));

$('play').onclick=run(()=>{if(!selected)throw Error('Select a recording first');return requestPlayback(selected,Number($('audio').value),false);});

$('abandon-key').onclick=run(async()=>{if(selected)await action('abandon_key',{id:selected});});
$('retry').onclick=run(()=>{if(!selected)throw Error('Select a recording first');return action('retry',{id:selected});});
$('delete').onclick=run(event=>deleteRecording(selected,event.shiftKey));
$('trim').onsubmit=run(()=>{if(!selected)throw Error('Select a recording first');return action('share_export',{id:selected,path:$('share-path').value,tracks:[...$('share-audio').querySelectorAll('input:checked')].map(input=>Number(input.value)),origin_ms:state.preview?.key===selected?state.preview.origin_ms:undefined,...(rangeEnabled?{start_ms:Math.round(Number($('trim-start').value)*1000),end_ms:Math.round(Number($('trim-end').value)*1000)}:{})});});
for(const id of ['search','kind-filter','outcome-filter','difficulty-filter','key-filter','date-from','date-to','sort'])$(id).oninput=()=>renderList();
const settings={ 'obs-port':'obs_port',spool:'spool','library-dir':'library',pre:'pre_seconds',post:'post_seconds',segment:'segment_seconds',history:'history_seconds','spool-budget':'spool_gib','library-budget':'library_gib','free-budget':'minimum_free_gib',timeout:'activity_timeout_minutes'};
function installation(value={name:'Retail',logs:'',flavour:'retail',categories:kinds,difficulties:[],minimum_key:0}){const card=document.createElement('div');card.className='installation';card.dataset.id=value.id||'';function field(title,key,type='text'){const l=document.createElement('label');l.textContent=title;const i=document.createElement('input');i.type=type;i.dataset.key=key;i.value=value[key]??'';l.append(i);card.append(l);}field('Name','name');field('Logs directory','logs');const l=document.createElement('label');l.textContent='Game flavour';const s=document.createElement('select');s.dataset.key='flavour';for(const f of ['retail','classic','era']){const o=document.createElement('option');o.value=f;o.textContent=label(f);s.append(o);}s.value=value.flavour;l.append(s);card.append(l);field('Minimum key level','minimum_key','number');const filters=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Activity filters';filters.append(summary,card.lastElementChild);card.append(filters);const d={...value,difficulties:(value.difficulties||[]).join(',')};const dl=document.createElement('label');dl.textContent='Difficulty IDs (empty = all supported)';const di=document.createElement('input');di.dataset.key='difficulties';di.value=d.difficulties;dl.append(di);filters.append(dl);const cats=document.createElement('div');cats.className='category-list';for(const k of kinds){const l=document.createElement('label');l.className='inline';const i=document.createElement('input');i.type='checkbox';i.dataset.kind=k;i.checked=value.categories.includes(k);l.append(i,document.createTextNode(label(k)));cats.append(l);}filters.append(cats);const remove=document.createElement('button');remove.type='button';remove.textContent='Remove';remove.onclick=()=>card.remove();card.append(remove);$('installations').append(card);}
$('add-install').onclick=()=>installation();
function loadConfig(c){setSceneOptions([],c.recording_scene||'');for(const [id,key]of Object.entries(settings))$(id).value=c[key];$('ffmpeg').value=c.media.ffmpeg;$('ffprobe').value=c.media.ffprobe;$('export-mode').value=c.export_mode||'fast';$('export-encoder').value=c.media.encoder||'cpu';$('vaapi-device').value=c.media.vaapi_device||'/dev/dri/renderD128';$('vaapi-device-label').hidden=$('export-encoder').value!=='vaapi';$('installations').replaceChildren();for(const i of c.installations)installation(i);configLoaded=true;}
$('settings-form').onsubmit=run(async()=>{const c=structuredClone(state.config);for(const [id,key]of Object.entries(settings))c[key]=$(id).type==='number'?Number($(id).value):$(id).value;c.recording_scene=$('recording-scene').value;c.export_mode=$('export-mode').value;c.media={...c.media,ffmpeg:$('ffmpeg').value,ffprobe:$('ffprobe').value,encoder:$('export-encoder').value,vaapi_device:$('vaapi-device').value};c.installations=[...$('installations').children].map(card=>{const get=k=>card.querySelector(`[data-key="${k}"]`).value;return{id:card.dataset.id||undefined,name:get('name'),logs:get('logs'),flavour:get('flavour'),minimum_key:Number(get('minimum_key')),difficulties:get('difficulties').split(',').map(x=>x.trim()).filter(Boolean).map(Number),categories:[...card.querySelectorAll('[data-kind]:checked')].map(i=>i.dataset.kind)};});await action('save_config',{config:c});$('preferences-status').textContent='Preferences saved.';});
function renderState(next){state=next;if(typeof renderLibraryLayout==='function')renderLibraryLayout();renderMarkerVisibility();$('obs-stats').textContent=state.obs_stats?('OBS: '+Number(state.obs_stats.activeFps||0).toFixed(1)+' FPS · '+(state.obs_stats.renderSkippedFrames||0)+' render frames skipped · '+(state.obs_stats.outputSkippedFrames||0)+' output frames skipped'):'';$('autostart').checked=!!state.autostart;$('message').textContent=state.message||'';$('connection').textContent=state.connected?'OBS connected':'OBS disconnected';$('capture-state').textContent=state.armed?'Recording':state.automatic_paused?'Paused':state.recording_open?'OBS recording (observe only)':state.connected?'Waiting for encounter':'Waiting for OBS';$('active-count').textContent=(state.active||[]).length;$('recording-count').textContent=(state.activities||[]).length;$('export-state').textContent=state.export_running?'Exporting':'Idle';$('arm').disabled=false;$('disarm').disabled=false;manual=(state.active||[]).some(a=>a.kind==='manual');$('manual').disabled=!state.connected||manual;$('manual-end').disabled=!manual;$('log-status').replaceChildren();for(const l of state.log_status||[]){const p=document.createElement('p');p.textContent=`${l.name}: ${l.last_event_ms?new Date(l.last_event_ms).toLocaleString():'waiting for new log events'}`;$('log-status').append(p);}if(state.config&&!configLoaded)loadConfig(state.config);if(state.preview?.status==='ready'&&(!selected||state.preview.key===selected)&&state.preview.url!==lastPreview){lastPreview=state.preview.url;if(pendingAudioSwitch?.id===selected)reviewPlayer.defaultPlaybackRate(pendingAudioSwitch.rate);reviewPlayer.src({src:lastPreview,type:state.preview.direct?'video/x-matroska':'video/mp4'});$('empty-player').hidden=true;const a=(state.activities||[]).find(a=>a.id===selected);if(a)renderMarkers(a);}if(state.preview?.status==='generating'&&(!selected||state.preview.key===selected))$('review-state').textContent='Preparing compatible playback…';if(state.preview?.status==='failed')$('review-state').textContent=state.preview.error;const listData=JSON.stringify([state.activities,state.active,state.armed,state.connected,state.automatic_paused,state.ownership,state.recording_activity_ids]);if(listData!==lastListData){lastListData=listData;renderList();renderMarkers(reviewActivity());}const current=recordingById(selected);if(current?.status==='ready'&&selectedStatus&&selectedStatus!=='ready'){if(state.preview?.key===selected&&['ready','generating'].includes(state.preview.status)){selectedStatus='ready';}else choose(current,true).catch(error);}else if(current?.review_available&&retainedRequest!==selected&&state.preview?.key!==selected){retainedRequest=selected;requestPlayback(selected,0,false).catch(error);}renderContext();if(typeof renderPlaybackDetails==='function')renderPlaybackDetails();if(typeof renderLabels==='function')renderLabels();if(typeof renderSetup==='function')renderSetup();}
const updates=new EventSource('/api/events');
updates.addEventListener('state',event=>{try{renderState(JSON.parse(event.data));}catch(e){error(e);}});
updates.onerror=()=>{$('connection').textContent='Reconnecting to companion…';};


$('favourites-only').onclick=()=>{const button=$('favourites-only');button.setAttribute('aria-pressed',String(button.getAttribute('aria-pressed')!=='true'));renderList();};
document.addEventListener('keydown',async e=>{try{if($('setup-wizard').open)return;if(e.repeat||e.ctrlKey||e.metaKey||e.altKey||['INPUT','TEXTAREA','SELECT','BUTTON'].includes(document.activeElement?.tagName))return;if(e.key.toLowerCase()==='m'&&state.armed){e.preventDefault();if(manual)await action('manual_stop');else await startManualClip();}if(e.key===' '&&$('player').src){e.preventDefault();if($('player').paused)await $('player').play();else $('player').pause();}}catch(e){error(e);}});

let desktopQuitPending=false;
async function requestDesktopQuit(){
  if(desktopQuitPending)return;
  desktopQuitPending=true;
  $('quit').disabled=true;
  $('quit').textContent='Closing…';
  try{await action('quit');updates.close();reviewPlayer.pause();if(window.companionDesktop)window.ipc.postMessage('quit-accepted');document.body.textContent=window.companionDesktop?'Closing…':'Companion stopped. You can close this window.';}
  catch(e){
    if($('setup-wizard').open)$('setup-wizard').close();
    error(e);$('error').scrollIntoView({block:'center'});
  }
  finally{desktopQuitPending=false;if($('quit')){$('quit').disabled=false;$('quit').textContent='Quit';}}
}
if(window.pendingDesktopQuit)requestDesktopQuit();

function renderContext(){
  $('automatic-toggle').setAttribute('aria-checked',String(!state.automatic_paused));$('automatic-toggle').textContent=state.automatic_paused?'Auto recording paused':'Auto recording on';

  $('arm').hidden=!state.automatic_paused;
  $('disarm').hidden=!!state.automatic_paused;
  $('manual').hidden=manual;
  $('manual-end').hidden=!manual;
  const source=$('manual-installation-source'), installs=state.config?.installations||[];
  const optionsKey=JSON.stringify(installs.map(i=>[i.id,i.name]));
  if(source.dataset.options!==optionsKey){const previous=source.value;source.replaceChildren();for(const i of installs)source.add(new Option(i.name,i.id));if(installs.some(i=>i.id===previous))source.value=previous;source.dataset.options=optionsKey;}
  source.hidden=installs.length<2||!state.connected||manual;
  $('live').hidden=!state.armed;
  const recording=(state.activities||[]).find(a=>a.id===selected);
  $('add-marker').disabled=!((state.active||[]).some(a=>a.id===selected)||recording?.status==='ready');
  $('abandon-key').hidden=!(state.active||[]).some(a=>a.id===selected&&a.kind==='mythic_plus');
  $('abandon-key').parentElement.hidden=$('abandon-key').hidden;
  if(recording&&['unavailable','failed'].includes(recording.status))$('review-state').textContent=recording.error||label(recording.status);
  if(recording&&['pending','exporting'].includes(recording.status))$('review-state').textContent=recording.processing?.title||'Preparing clip';
  updatePlaybackAvailability();
  $('recording-tools').hidden=!recording;
  $('labels-panel').hidden=!recording;

  $('clip-options').hidden=recording?.status!=='ready';
  $('export-clip').hidden=recording?.status!=='ready';
  $('export-clip').disabled=!!state.share_running;
  $('cancel-share').hidden=!state.share_running;
  $('cancel-share').disabled=state.share?.status==='cancelling';
  $('share-status').textContent=state.share_running?(state.share?.status==='cancelling'?'Cancelling export…':state.share_progress||'Preparing bundle…'):state.share?.status==='ready'?'Saved: '+state.share.path:state.share?.error||'';
  if(!recording)clearExportRange();
  $('cancel-export').hidden=!['exporting','cancelling'].includes(recording?.processing?.stage);
  $('cancel-export').disabled=recording?.processing?.stage==='cancelling';
  $('retry').hidden=recording?.status!=='failed';$('retry').title='Retry failed export';
  $('delete').disabled=!!recording?.protected;
  $('delete').title=recording?.protected?'Unlock this recording before deleting.':'Delete recording (Shift-click skips confirmation)';
  if(recording?.status==='ready'&&$('audio').options.length!==Math.max(recording.audio_tracks||0,1)){const track=$('audio').value;$('audio').replaceChildren();for(let i=0;i<Math.max(recording.audio_tracks||0,1);i++)$('audio').add(new Option(`Track ${i+1}`,String(i)));$('audio').value=track||'0';}
  if(recording?.status==='ready')renderShareAudio(recording);
  if(pendingAudioSwitch&&state.preview?.key===selected&&state.preview.status==='failed')cancelAudioSwitch();
  $('audio').disabled=!!pendingAudioSwitch;
  $('audio').parentElement.hidden=!recording||recording.status!=='ready'||(recording.audio_tracks||0)<2;
  $('play').hidden=!recording||!(recording.status==='ready'||recording.review_available)||state.preview?.status!=='failed';
}
$('settings-form').addEventListener('input',()=>{$('preferences-status').textContent='Unsaved changes';});

function setSceneOptions(scenes,selected){
  const select=$('recording-scene');select.replaceChildren(new Option('Use the current OBS scene',''));
  const names=new Set(scenes.map(scene=>scene.sceneName));if(selected)names.add(selected);
  for(const name of names)select.add(new Option(name,name));select.value=selected;
}
async function refreshScenes(){const result=await action('list_scenes');setSceneOptions(result.scenes||[],$('recording-scene').value);}
$('refresh-scenes').onclick=run(refreshScenes);

$('export-encoder').onchange=()=>{$('vaapi-device-label').hidden=$('export-encoder').value!=='vaapi';};

$('cancel-share').onclick=run(()=>action('cancel_share'));

$('cancel-export').onclick=run(()=>action('cancel_export',{id:selected}));
