'use strict';
const $=id=>document.getElementById(id);
const kinds=['raid','mythic_plus','delve','arena','solo_shuffle','battleground','manual'];
let state={},selected=null,configLoaded=false,lastPreview='',manual=false,lastListData='';
const label=x=>x==='delve'?'Delves':String(x||'').replaceAll('_',' ').replace(/^./,c=>c.toUpperCase());
function error(e){$('error').textContent=e.message||String(e);$('error').hidden=false;}
async function action(action,data={}){ $('error').hidden=true;const r=await fetch('/api/action',{method:'POST',headers:{'Content-Type':'application/json','X-OWC-Request':'1'},body:JSON.stringify({action,...data})});const result=await r.json();if(!r.ok)throw Error(result.error||r.statusText);return result;}
function run(fn){return async e=>{e?.preventDefault();try{await fn(e);}catch(e){error(e);}};}
function show(tab){
  if(tab==='setup')reviewPlayer.pause();
  $('library-view').inert=tab!=='library';$('library-view').setAttribute('aria-hidden',String(tab!=='library'));$('setup-view').hidden=tab!=='setup';
  $('library-tab').classList.toggle('active',tab==='library');$('setup-tab').classList.toggle('active',tab==='setup');
  for(const [id,active] of [['library-tab',tab==='library'],['setup-tab',tab==='setup']]){$(id).setAttribute('aria-selected',String(active));$(id).tabIndex=active?0:-1;}

  if(tab==='library')requestAnimationFrame(()=>{
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
function stoppableRecording(){
  const active=state.active||[];
  return active.find(a=>a.id===selected)||active.find(a=>a.kind==='manual')||active[0];
}
$('manual-end').onclick=run(async()=>{
  const current=stoppableRecording();if(!current)return;
  if(current.kind==='manual'){await action('manual_stop');manual=false;}
  else await action('stop_activity',{id:current.id});
});
$('autostart').onchange=run(()=>action('autostart',{enabled:$('autostart').checked}));

$('play').onclick=run(()=>{if(!selected)throw Error('Select a recording first');return requestPlayback(selected,Number($('audio').value),false);});

$('retry').onclick=run(()=>{if(!selected)throw Error('Select a recording first');return action('retry',{id:selected});});
$('delete').onclick=run(event=>deleteRecording(selected,event.shiftKey));
$('trim').onsubmit=run(()=>{if(!selected)throw Error('Select a recording first');return action('share_export',{id:selected,path:$('share-path').value,tracks:[...$('share-audio').querySelectorAll('input:checked')].map(input=>Number(input.value)),origin_ms:state.preview?.key===selected?state.preview.origin_ms:undefined,...(rangeEnabled?{start_ms:Math.round(Number($('trim-start').value)*1000),end_ms:Math.round(Number($('trim-end').value)*1000)}:{})});});
for(const id of ['search','kind-filter','outcome-filter','difficulty-filter','key-filter','date-from','date-to','sort'])$(id).oninput=()=>renderList();
const settings={ 'obs-port':'obs_port',spool:'spool','library-dir':'library',pre:'pre_seconds',post:'post_seconds',segment:'segment_seconds',history:'history_seconds','spool-budget':'spool_gib','library-budget':'library_gib','free-budget':'minimum_free_gib',timeout:'activity_timeout_minutes'};
function installation(value={name:'Retail',logs:'',flavour:'retail',categories:kinds.filter(k=>k!=='delve'),difficulties:[],minimum_key:0}){
  const card=document.createElement('div');card.className='installation';card.dataset.id=value.id||'';
  card.originalInstallation=structuredClone(value);
  function field(title,key,type='text',parent=card){
    const l=document.createElement('label');l.textContent=title;l.className='installation-field-'+key;
    const input=document.createElement('input');input.type=type;input.dataset.key=key;input.value=value[key]??'';
    l.append(input);parent.append(l);return input;
  }
  function choice(title,key,options,selected,parent=card){
    const l=document.createElement('label');l.textContent=title;l.className='installation-field-'+key;
    const select=document.createElement('select');select.dataset.key=key;
    for(const [value,text] of options)select.add(new Option(text,value));
    select.value=selected;l.append(select);parent.append(l);return select;
  }
  field('Name','name');field('Logs directory','logs');
  choice('Game flavour','flavour',['retail','classic','era'].map(f=>[f,label(f)]),value.flavour);
  const filters=document.createElement('details'),summary=document.createElement('summary');
  summary.textContent='Automatic recording';filters.className='automatic-recording-options';filters.append(summary);card.append(filters);
  const raidOptions=[['off','Off'],['all','All raids'],['normal','Normal and above'],['heroic','Heroic and above'],['mythic','Mythic only']];
  const legacy=(value.difficulties||[]).length>0;
  if(legacy)raidOptions.push(['custom','Custom (existing filter)']);
  choice('Raids','raid-mode',raidOptions,!value.categories.includes('raid')?'off':legacy?'custom':value.minimum_raid_difficulty||'all',filters);
  if(legacy){const note=document.createElement('p');note.className='hint';note.textContent='Your existing difficulty filter is kept until you choose a new raid setting.';filters.append(note);}
  const key=choice('Mythic+','key-mode',[['off','Off'],['all','Any key'],['minimum','Minimum key level']],!value.categories.includes('mythic_plus')?'off':value.minimum_key>0?'minimum':'all',filters);
  const minimum=field('Minimum key level','minimum_key','number',filters);minimum.min='1';minimum.step='1';minimum.value=Math.max(1,value.minimum_key||2);
  const updateKey=()=>{minimum.parentElement.hidden=key.value!=='minimum';minimum.disabled=key.value!=='minimum';minimum.required=key.value==='minimum';};key.onchange=updateKey;updateKey();
  choice('Delves (experimental)','delve-mode',[['off','Off'],['on','On']],value.categories.includes('delve')?'on':'off',filters);
  const cats=document.createElement('div');cats.className='category-list';
  for(const kind of ['arena','solo_shuffle','battleground']){
    const l=document.createElement('label');l.className='inline';const input=document.createElement('input');input.type='checkbox';input.dataset.kind=kind;input.checked=value.categories.includes(kind);l.append(input,document.createTextNode(label(kind)));cats.append(l);
  }
  filters.append(cats);
  const remove=document.createElement('button');remove.type='button';remove.className='remove-installation danger';remove.textContent='Remove';remove.onclick=()=>card.remove();card.append(remove);$('installations').append(card);
}
function installationConfig(card){
  const get=key=>card.querySelector(`[data-key="${key}"]`).value;
  const original=card.originalInstallation,raid=get('raid-mode'),key=get('key-mode');
  const categories=[...card.querySelectorAll('[data-kind]:checked')].map(input=>input.dataset.kind);
  if(original.categories.includes('manual'))categories.push('manual');
  if(raid!=='off')categories.push('raid');
  if(key!=='off')categories.push('mythic_plus');
  if(get('delve-mode')==='on')categories.push('delve');
  const keepLegacy=raid==='custom'||(raid==='off'&&!original.categories.includes('raid'));
  return {id:card.dataset.id||undefined,name:get('name'),logs:get('logs'),flavour:get('flavour'),categories,
    minimum_key:key==='minimum'?Number(get('minimum_key')):0,
    minimum_raid_difficulty:raid==='custom'?original.minimum_raid_difficulty||null:['normal','heroic','mythic'].includes(raid)?raid:null,
    difficulties:keepLegacy?original.difficulties||[]:[]};
}
$('add-install').onclick=()=>installation();
function loadConfig(c){setSceneOptions([],c.recording_scene||'');for(const [id,key]of Object.entries(settings))$(id).value=c[key];$('ffmpeg').value=c.media.ffmpeg;$('ffprobe').value=c.media.ffprobe;$('export-mode').value=c.export_mode||'fast';$('export-encoder').value=c.media.encoder||'cpu';$('vaapi-device').value=c.media.vaapi_device||'/dev/dri/renderD128';$('vaapi-device-label').hidden=$('export-encoder').value!=='vaapi';$('installations').replaceChildren();for(const i of c.installations)installation(i);configLoaded=true;}
$('settings-form').onsubmit=run(async()=>{const c=structuredClone(state.config);for(const [id,key]of Object.entries(settings))c[key]=$(id).type==='number'?Number($(id).value):$(id).value;c.recording_scene=$('recording-scene').value;c.export_mode=$('export-mode').value;c.media={...c.media,ffmpeg:$('ffmpeg').value,ffprobe:$('ffprobe').value,encoder:$('export-encoder').value,vaapi_device:$('vaapi-device').value};c.installations=[...$('installations').children].map(installationConfig);await action('save_config',{config:c});$('preferences-status').textContent='Preferences saved.';});
function renderState(next){state=next;if(typeof renderRecovery==="function")renderRecovery();if(typeof renderLibraryLayout==='function')renderLibraryLayout();renderMarkerVisibility();$('obs-stats').textContent=state.obs_stats?('OBS: '+Number(state.obs_stats.activeFps||0).toFixed(1)+' FPS · '+(state.obs_stats.renderSkippedFrames||0)+' render frames skipped · '+(state.obs_stats.outputSkippedFrames||0)+' output frames skipped'):'';$('autostart').checked=!!state.autostart;$('message').textContent=state.message||'';$('connection').textContent=state.connected?'OBS connected':'OBS disconnected';$('capture-state').textContent=state.armed?'Recording':state.automatic_paused?'Paused':state.recording_open?'OBS recording (observe only)':state.connected?'Waiting for encounter':'Waiting for OBS';$('active-count').textContent=(state.active||[]).length;$('recording-count').textContent=(state.activities||[]).length;$('export-state').textContent=state.export_running?'Exporting':'Idle';$('arm').disabled=false;$('disarm').disabled=false;manual=(state.active||[]).some(a=>a.kind==='manual');$('manual').disabled=!state.connected||manual;$('manual-end').disabled=!stoppableRecording();$('log-status').replaceChildren();for(const l of state.log_status||[]){const p=document.createElement('p');p.textContent=`${l.name}: ${l.last_event_ms?new Date(l.last_event_ms).toLocaleString():'waiting for new log events'}`;$('log-status').append(p);}if(state.config&&!configLoaded)loadConfig(state.config);if(state.preview?.status==='ready'&&(!selected||state.preview.key===selected)&&state.preview.url!==lastPreview){lastPreview=state.preview.url;if(pendingAudioSwitch?.id===selected)reviewPlayer.defaultPlaybackRate(pendingAudioSwitch.rate);reviewPlayer.src({src:lastPreview,type:state.preview.direct?'video/x-matroska':'video/mp4'});$('empty-player').hidden=true;const a=(state.activities||[]).find(a=>a.id===selected);if(a)renderMarkers(a);}if(state.preview?.status==='generating'&&(!selected||state.preview.key===selected))$('review-state').textContent='Preparing compatible playback…';if(state.preview?.status==='failed')$('review-state').textContent=state.preview.error;const listData=JSON.stringify([state.activities,state.active,state.armed,state.connected,state.automatic_paused,state.ownership,state.recording_activity_ids]);if(listData!==lastListData){lastListData=listData;renderList();renderMarkers(reviewActivity());}const current=recordingById(selected);if(current?.status==='ready'&&selectedStatus&&selectedStatus!=='ready'){if(state.preview?.key===selected&&['ready','generating'].includes(state.preview.status)){selectedStatus='ready';}else choose(current,true).catch(error);}else if(current?.review_available&&retainedRequest!==selected&&state.preview?.key!==selected){retainedRequest=selected;requestPlayback(selected,0,false).catch(error);}renderContext();if(typeof renderPlaybackDetails==='function')renderPlaybackDetails();if(typeof renderLabels==='function')renderLabels();if(typeof renderSetup==='function')renderSetup();}
const updates=new EventSource('/api/events');
updates.addEventListener('state',event=>{try{renderState(JSON.parse(event.data));}catch(e){error(e);}});
updates.onerror=()=>{$('connection').textContent='Reconnecting to companion…';};


for(const id of ['favourites-only','protected-only'])$(id).onclick=()=>{const button=$(id);button.setAttribute('aria-pressed',String(button.getAttribute('aria-pressed')!=='true'));renderList();};

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
  const failedCheck=state.setup?.checks?.find(check=>check.status==='fail');
  const blocked=!state.connected?(noticeText('connection',state.obs_connection_error)||'Waiting for OBS to connect.'):!state.setup_completed?'Finish setup to enable automatic recording.':failedCheck?(failedCheck.detail||failedCheck.title):'';
  $('automatic-toggle').setAttribute('aria-checked',String(!state.automatic_paused));
  $('automatic-toggle').textContent=message('auto.'+(state.automatic_paused?'paused':blocked?(!state.connected?'obs_blocked':'setup_blocked'):(state.automatic_status||'on')));
  $('automatic-toggle').dataset.blocked=String(!state.automatic_paused&&!!blocked);
  $('automatic-toggle').title=state.automatic_paused?'Click to enable automatic recording.':blocked+' '+(blocked?'It will resume when ready. Click to pause.':'Click to pause automatic recording.');
  if(blocked&&!state.automatic_paused&&!state.armed)$('capture-state').textContent=!state.connected?'Automatic recording blocked: OBS disconnected':'Automatic recording blocked: check setup';

  $('arm').hidden=!state.automatic_paused;
  $('disarm').hidden=!!state.automatic_paused;
  const stopping=stoppableRecording();
  $('manual').hidden=!!stopping;
  $('manual-end').hidden=!stopping;
  $('manual-end').title=stopping?`End ${stopping.name} and keep the captured footage`:'';
  const source=$('manual-installation-source'), installs=state.config?.installations||[];
  const optionsKey=JSON.stringify(installs.map(i=>[i.id,i.name]));
  if(source.dataset.options!==optionsKey){const previous=source.value;source.replaceChildren();for(const i of installs)source.add(new Option(i.name,i.id));if(installs.some(i=>i.id===previous))source.value=previous;source.dataset.options=optionsKey;}
  source.hidden=installs.length<2||!state.connected||!!stopping;
  const recording=(state.activities||[]).find(a=>a.id===selected);
  $('add-marker').disabled=!((state.active||[]).some(a=>a.id===selected)||recording?.status==='ready');
  if(recording&&['unavailable','failed'].includes(recording.status))$('review-state').textContent=recoveryText(recording.recovery)||recording.error||label(recording.status);
  if(recording&&['pending','exporting'].includes(recording.status))$('review-state').textContent=processingText(recording.processing)||message('processing.preparing');
  updatePlaybackAvailability();
  $('recording-tools').hidden=!recording;
  $('labels-panel').hidden=!recording;

  $('clip-options').hidden=recording?.status!=='ready';
  $('export-clip').hidden=recording?.status!=='ready';
  $('export-clip').disabled=!!state.share_running;
  $('cancel-share').hidden=!state.share_running;
  $('cancel-share').disabled=state.share?.status==='cancelling';
  $('share-status').textContent=state.share_running?(state.share?.status==='cancelling'?'Cancelling export…':progressText(state.share_progress)||message('media.bundle')):state.share?.status==='ready'?'Saved: '+state.share.path:state.share?.error||'';
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
  audioToggle.el().hidden=!recording||recording.status!=='ready'||(recording.audio_tracks||0)<2;
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
