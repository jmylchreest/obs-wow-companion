"use strict";
const reviewPlayer=videojs('video-player',{
  controls:true,autoplay:false,preload:'metadata',fluid:true,inactivityTimeout:2500,
  playbackRates:[0.1,0.25,0.5,0.75,1,1.25,1.5,2],
  controlBar:{children:['playToggle','volumePanel','currentTimeDisplay','timeDivider','durationDisplay','progressControl','customControlSpacer','playbackRateMenuButton','fullscreenToggle']},
  userActions:{hotkeys:false,doubleClick:true}
});
reviewPlayer.tech(true).el().id='player';
reviewPlayer.on('playerreset',()=>{reviewPlayer.tech(true).el().id='player';});
const controlBar=reviewPlayer.controlBar;
controlBar.playToggle.el().id='toggle-play';
function syncPlayToggle(){
  const toggle=controlBar.playToggle,ended=reviewPlayer.ended(),paused=reviewPlayer.paused();
  for(const [name,active] of [['vjs-ended',ended],['vjs-paused',!ended&&paused],['vjs-playing',!ended&&!paused]])toggle.el().classList.toggle(name,active);
  toggle.setIcon(ended?'replay':paused?'play':'pause');
  toggle.controlText(ended?'Replay':paused?'Play':'Pause');
}
// Replacing the tech can discard its queued pause event while keeping the control bar.
reviewPlayer.on(['playerreset','loadstart','loadedmetadata','loadeddata','emptied','play','playing','pause','ended','seeked','error'],syncPlayToggle);
syncPlayToggle();
controlBar.fullscreenToggle.el().id='fullscreen';
// Keep the track model shared with playback/export while presenting a player menu.
const audioSelect=document.createElement('select');audioSelect.id='audio';audioSelect.hidden=true;audioSelect.add(new Option('Track 1','0'));reviewPlayer.el().append(audioSelect);
const audioToggle=controlBar.addChild('button',{},controlBar.children().length-1);
audioToggle.el().id='audio-tracks';audioToggle.el().classList.add('vjs-audio-toggle');audioToggle.controlText('Audio tracks');
audioToggle.el().querySelector('.vjs-icon-placeholder').innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 17h16M8 4v6M16 14v6"/><circle cx="8" cy="7" r="2"/><circle cx="16" cy="17" r="2"/></svg>';
const audioMenu=document.createElement('div');audioMenu.id='audio-menu';audioMenu.hidden=true;audioMenu.setAttribute('role','group');audioMenu.setAttribute('aria-label','Audio tracks');reviewPlayer.el().append(audioMenu);
audioToggle.el().setAttribute('aria-haspopup','true');audioToggle.el().setAttribute('aria-controls','audio-menu');audioToggle.el().setAttribute('aria-expanded','false');
function closeAudioMenu(){audioMenuKey='';audioMenu.hidden=true;audioToggle.el().setAttribute('aria-expanded','false');reviewPlayer.el().classList.remove('audio-menu-open');}
let audioMenuKey='';
function renderAudioMenu(){
  audioToggle.el().disabled=audioSelect.disabled||audioSelect.options.length<2;
  audioToggle.el().title='Audio: '+(audioSelect.selectedOptions[0]?.textContent||'Track 1');
  if(audioMenu.hidden){audioMenuKey='';return;}
  const key=JSON.stringify([audioSelect.disabled,audioSelect.value,[...audioSelect.options].map(option=>[option.value,option.textContent])]);
  if(key===audioMenuKey)return;audioMenuKey=key;
  audioMenu.replaceChildren();
  for(const option of audioSelect.options){
    const row=document.createElement('label'),radio=document.createElement('input');
    radio.type='radio';radio.name='playback-audio-track';radio.value=option.value;radio.checked=option.selected;radio.disabled=audioSelect.disabled;
    radio.onchange=()=>{audioSelect.value=radio.value;closeAudioMenu();audioToggle.el().focus();audioSelect.dispatchEvent(new Event('change'));};
    row.append(radio,document.createTextNode(option.textContent));audioMenu.append(row);
  }
}
audioSelect.onchange=()=>{switchPlaybackAudio().catch(error).finally(renderAudioMenu);renderAudioMenu();};
new MutationObserver(renderAudioMenu).observe(audioSelect,{childList:true,subtree:true,attributes:true,characterData:true});
audioToggle.on('click',()=>{if(!audioMenu.hidden){closeAudioMenu();return;}closeMarkerMenu();audioMenu.hidden=false;audioToggle.el().setAttribute('aria-expanded','true');reviewPlayer.el().classList.add('audio-menu-open');renderAudioMenu();reviewPlayer.userActive(true);audioMenu.querySelector('input:checked')?.focus();});
document.addEventListener('pointerdown',event=>{if(!audioMenu.contains(event.target)&&!audioToggle.el().contains(event.target))closeAudioMenu();});
audioMenu.onkeydown=event=>{if(event.key==='Escape'){event.preventDefault();closeAudioMenu();audioToggle.el().focus();}};
for(const type of ['pointerdown','click','keydown'])audioMenu.addEventListener(type,event=>event.stopPropagation());
reviewPlayer.on('playerreset',closeAudioMenu);
renderAudioMenu();
const markControl=controlBar.addChild('button',{},controlBar.children().length-1);markControl.controlText('Add marker');markControl.el().id='add-marker';markControl.el().title='Add marker';markControl.el().classList.add('vjs-marker-button');markControl.el().querySelector('.vjs-icon-placeholder').textContent='⚑';
const markerToggle=controlBar.addChild('button',{},controlBar.children().length-1);
markerToggle.el().id='toggle-markers';markerToggle.el().classList.add('vjs-marker-toggle');
markerToggle.el().querySelector('.vjs-icon-placeholder').innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/><path class="eye-slash" d="m3 3 18 18"/></svg>';
let markerVisibilitySaving=false;
const markerMenu=document.createElement('div');markerMenu.id='marker-menu';markerMenu.hidden=true;markerMenu.setAttribute('role','group');markerMenu.setAttribute('aria-label','Visible markers');reviewPlayer.el().append(markerMenu);
markerToggle.el().setAttribute('aria-haspopup','true');markerToggle.el().setAttribute('aria-expanded','false');markerToggle.el().setAttribute('aria-controls','marker-menu');
function markerCategory(kind){return kind.replace(/_(start|end)$/,'');}
function markerTypeVisible(kind){return !(state.config?.hidden_marker_types||[]).includes(markerCategory(kind));}
function markerCategories(){
  const a=typeof reviewActivity==='function'?reviewActivity():null;if(!a)return [];
  return [...new Set([a.kind==='raid'?'boss':'activity',...(a.markers||[]).map(m=>markerCategory(m.kind))])].sort();
}
function renderMarkerMenu(){
  const kinds=markerCategories(),hidden=state.config?.hidden_marker_types||[],enabled=state.config?.show_markers!==false;
  markerMenu.replaceChildren();
  function option(text,checked,change){const row=document.createElement('label'),input=document.createElement('input');input.type='checkbox';input.checked=checked;input.disabled=markerVisibilitySaving;input.onchange=()=>change(input.checked);row.append(input,document.createTextNode(text));markerMenu.append(row);return input;}
  const all=option('Toggle all',enabled&&kinds.every(kind=>!hidden.includes(kind)),checked=>saveMarkerFilters(checked,[]));
  all.indeterminate=enabled&&kinds.some(kind=>hidden.includes(kind))&&kinds.some(kind=>!hidden.includes(kind));
  for(const kind of kinds)option(label(kind),enabled&&!hidden.includes(kind),checked=>{
    const next=new Set(enabled?hidden:kinds);
    if(checked)next.delete(kind);else next.add(kind);
    saveMarkerFilters(true,[...next]);
  });
}
function renderMarkerVisibility(){
  if(markerVisibilitySaving)return;
  const visible=state.config?.show_markers!==false;
  reviewPlayer.el().classList.toggle('markers-hidden',!visible);
  markerToggle.el().setAttribute('aria-pressed',String(visible));
  markerToggle.controlText('Marker filters');markerToggle.el().title='Marker filters';
  if(!visible){seekTrack.classList.remove('marker-hover');reviewPlayer.el().classList.remove('marker-focused');}
  if(!markerMenu.hidden)renderMarkerMenu();
}
async function saveMarkerFilters(visible,hidden){
  if(markerVisibilitySaving)return;markerVisibilitySaving=true;renderMarkerMenu();
  try{await action('set_marker_visibility',{visible,hidden_types:hidden});state.config.show_markers=visible;state.config.hidden_marker_types=hidden;}
  catch(e){error(e);}
  finally{markerVisibilitySaving=false;renderMarkerVisibility();renderMarkers(reviewActivity());}
}
function closeMarkerMenu(){markerMenu.hidden=true;markerToggle.el().setAttribute('aria-expanded','false');reviewPlayer.el().classList.remove('marker-menu-open');}
markerToggle.on('click',()=>{if(!markerMenu.hidden){closeMarkerMenu();return;}closeAudioMenu();markerMenu.hidden=false;markerToggle.el().setAttribute('aria-expanded','true');reviewPlayer.el().classList.add('marker-menu-open');renderMarkerMenu();reviewPlayer.userActive(true);});
document.addEventListener('pointerdown',event=>{if(!markerMenu.contains(event.target)&&!markerToggle.el().contains(event.target))closeMarkerMenu();});
markerMenu.onkeydown=event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();closeMarkerMenu();markerToggle.el().focus();}};
for(const type of ['pointerdown','click','keydown'])markerMenu.addEventListener(type,event=>event.stopPropagation());
const seekTrack=controlBar.progressControl.seekBar.el();
seekTrack.setAttribute("aria-description","Red: boss fights. Orange: combat activity estimated from damage events.");
const combatTrack=document.createElement('div');combatTrack.id='combat-track';combatTrack.setAttribute('aria-hidden','true');seekTrack.append(combatTrack);
const markerLayer=document.createElement('div');markerLayer.id='markers';markerLayer.setAttribute('aria-label','Recording events');seekTrack.append(markerLayer);
reviewPlayer.el().append($('marker-form'),$('empty-player'));
reviewPlayer.on('fullscreenchange',()=>{const video=$('player');if(video.paused&&video.readyState>=2)video.currentTime=Math.min(video.duration,video.currentTime+0.0001);});
reviewPlayer.on('ratechange',()=>{const rate=reviewPlayer.playbackRate();$('speed').replaceChildren(new Option(String(rate),String(rate)));$('speed').value=String(rate);});

let rangeEnabled=false,rangeRecording=null,rangeStart=0,rangeEnd=0;
const rangeLayer=document.createElement('div');rangeLayer.id='export-range';rangeLayer.hidden=true;
const rangeFill=document.createElement('div');rangeFill.className='range-fill';rangeLayer.append(rangeFill);
const rangeHandles=['start','end'].map(boundary=>{const handle=document.createElement('button');handle.type='button';handle.className='range-handle '+boundary;handle.dataset.boundary=boundary;handle.setAttribute('role','slider');handle.setAttribute('aria-label',boundary==='start'?'Export start':'Export end');handle.textContent=boundary==='start'?'In':'Out';rangeLayer.append(handle);return handle;});
seekTrack.append(rangeLayer);
function exportDuration(){const value=reviewPlayer.duration(),source=state.preview?.key===selected?state.preview.duration_ms/1000:NaN;return Number.isFinite(value)?Math.floor(Math.min(value,Number.isFinite(source)?source:value)*1000)/1000:0;}
function clearExportRange(){reviewPlayer.el().classList.remove('range-editing');seekTrack.classList.remove('range-selecting');rangeEnabled=false;rangeRecording=null;rangeLayer.hidden=true;$('select-range').setAttribute('aria-pressed','false');$('select-range').textContent='Select range';}
function initializeExportRange(){const duration=exportDuration();if(rangeRecording!==selected){rangeStart=0;rangeEnd=duration;rangeRecording=selected;}rangeEnd=Math.min(rangeEnd,duration);updateExportRange();}
function updateExportRange(){
  const duration=exportDuration();$('select-range').disabled=!duration;
  $('trim-start').value=rangeStart.toFixed(3);$('trim-end').value=rangeEnd.toFixed(3);$('trim-start').max=duration;$('trim-end').max=duration;
  $('range-duration').textContent=duration?`${(rangeEnd-rangeStart).toFixed(2)}s selected`:'';
  rangeLayer.hidden=!rangeEnabled||!duration;reviewPlayer.el().classList.toggle('range-editing',rangeEnabled);seekTrack.classList.toggle('range-selecting',rangeEnabled);
  const timelineDuration=Number.isFinite(reviewPlayer.duration())?reviewPlayer.duration():duration;
  rangeFill.style.left=`${timelineDuration?rangeStart/timelineDuration*100:0}%`;rangeFill.style.width=`${timelineDuration?(rangeEnd-rangeStart)/timelineDuration*100:0}%`;
  rangeHandles.forEach((handle,index)=>{const value=index?rangeEnd:rangeStart;handle.style.left=`${timelineDuration?value/timelineDuration*100:0}%`;handle.setAttribute('aria-valuemin',index?rangeStart+0.01:0);handle.setAttribute('aria-valuemax',index?duration:Math.max(0,rangeEnd-0.01));handle.setAttribute('aria-valuenow',String(value));handle.setAttribute('aria-valuetext',`${value.toFixed(2)} seconds`);handle.title=`${index?'End':'Start'}: ${value.toFixed(2)}s`;});
  $('select-range').setAttribute('aria-pressed',String(rangeEnabled));$('select-range').textContent=rangeEnabled?'Clear range':'Select range';
}
function setRangeBoundary(boundary,value){const duration=exportDuration();if(!Number.isFinite(value)||!duration)return;if(boundary==='start')rangeStart=Math.min(Math.max(0,value),Math.max(0,rangeEnd-0.01));else rangeEnd=Math.max(Math.min(duration,value),rangeStart+0.01);updateExportRange();}
function pointerTime(event){const rect=seekTrack.getBoundingClientRect();return Math.max(0,Math.min(exportDuration(),(event.clientX-rect.left)/rect.width*reviewPlayer.duration()));}
$('select-range').onclick=()=>{if(rangeEnabled){rangeEnabled=false;rangeStart=0;rangeEnd=exportDuration();}else{rangeEnabled=true;reviewPlayer.pause();reviewPlayer.userActive(true);}updateExportRange();};
for(const [id,boundary] of [['trim-start','start'],['trim-end','end']])$(id).onchange=()=>{rangeEnabled=true;setRangeBoundary(boundary,Number($(id).value));};
for(const handle of rangeHandles){
  let dragging=false;
  handle.onpointerdown=event=>{if(event.button!==0)return;event.preventDefault();event.stopPropagation();dragging=true;handle.setPointerCapture(event.pointerId);reviewPlayer.pause();};
  handle.onpointermove=event=>{event.stopPropagation();if(dragging)setRangeBoundary(handle.dataset.boundary,pointerTime(event));};
  handle.onpointerup=handle.onpointercancel=handle.onlostpointercapture=event=>{event.stopPropagation();dragging=false;};
  handle.onkeydown=event=>{const step=event.shiftKey?1:0.1,current=handle.dataset.boundary==='start'?rangeStart:rangeEnd;const value=event.key==='ArrowLeft'?current-step:event.key==='ArrowRight'?current+step:event.key==='Home'?0:event.key==='End'?exportDuration():null;if(value!==null){event.preventDefault();event.stopPropagation();setRangeBoundary(handle.dataset.boundary,value);}};
}
let rangeAnchor=null;
rangeLayer.onpointerdown=event=>{if(event.button!==0||event.target.closest('.range-handle'))return;event.preventDefault();event.stopPropagation();rangeAnchor=pointerTime(event);rangeLayer.setPointerCapture(event.pointerId);reviewPlayer.pause();};
rangeLayer.onpointermove=event=>{event.stopPropagation();if(rangeAnchor===null)return;const at=pointerTime(event),start=Math.min(rangeAnchor,at),end=Math.max(rangeAnchor,at);if(end-start<0.01)return;rangeStart=start;rangeEnd=end;updateExportRange();};
rangeLayer.onpointerup=rangeLayer.onpointercancel=rangeLayer.onlostpointercapture=event=>{event.stopPropagation();rangeAnchor=null;};
for(const type of ['mousedown','touchstart','click'])rangeLayer.addEventListener(type,event=>event.stopPropagation());

function resetPlayback(){lastPreview='';const volume=reviewPlayer.volume(),muted=reviewPlayer.muted(),rates=[...reviewPlayer.playbackRates()];reviewPlayer.pause();reviewPlayer.reset();reviewPlayer.volume(volume);reviewPlayer.muted(muted);reviewPlayer.playbackRates(rates);}

function updatePlaybackAvailability(){
  const preview=state.preview, matches=preview?.status==='ready'&&(!selected||preview.key===selected);
  const playable=!!(matches&&reviewPlayer.currentSrc()&&reviewPlayer.readyState()>=2&&!reviewPlayer.error());
  reviewPlayer.el().classList.toggle('playback-unavailable',!playable);
  reviewPlayer.controls(playable);$('empty-player').hidden=playable;
  if(playable)return;
  const recording=recordingById(selected);
  if(recording?.processing){
    const progress=recording.processing;
    $('empty-player').textContent=progress.detail+(progress.elapsed_seconds!=null?' · '+timeLabel(progress.elapsed_seconds)+' elapsed':'');
    return;
  }
  $('empty-player').textContent=recording?.status==='pending'?'Encounter finished. The video clip is not ready yet.':recording?.status==='exporting'?'Exporting video…':recording?.status==='unavailable'?'No playable video is available for this encounter.':recording?.status==='failed'?'Video export failed.':matches?'Loading video…':preview?.key===selected&&preview?.status==='generating'?'Preparing playback…':selected?'No playable video yet.':'Select a recording';
}
reviewPlayer.on(['loadeddata','canplay','emptied','error'],updatePlaybackAvailability);

// Transform only the picture; timeline, event markers and controls retain their geometry.
let videoZoom=1,videoPanX=0,videoPanY=0,videoDrag=null,suppressVideoClick=false;
const zoomReset=document.createElement('button');zoomReset.id='reset-video-zoom';zoomReset.type='button';zoomReset.hidden=true;zoomReset.title='Reset video zoom';reviewPlayer.el().append(zoomReset);
function applyVideoZoom(){
  videoPanX=Math.max(1-videoZoom,Math.min(0,videoPanX));videoPanY=Math.max(1-videoZoom,Math.min(0,videoPanY));
  const video=$('player');video.style.transformOrigin='0 0';video.style.transform=`translate(${videoPanX*100}%,${videoPanY*100}%) scale(${videoZoom})`;
  reviewPlayer.el().classList.toggle('video-zoomed',videoZoom>1);zoomReset.hidden=videoZoom===1;
  zoomReset.textContent=`${videoZoom.toFixed(1)}× Reset`;zoomReset.setAttribute('aria-label',`Video zoom ${videoZoom.toFixed(1)} times. Reset zoom`);
}
function resetVideoZoom(){videoZoom=1;videoPanX=0;videoPanY=0;videoDrag=null;suppressVideoClick=false;applyVideoZoom();}
zoomReset.onclick=event=>{event.stopPropagation();resetVideoZoom();};
function isVideoSurface(target){return target===$('player')||!!target.closest('.vjs-big-play-button');}
reviewPlayer.el().addEventListener('wheel',event=>{
  if(!isVideoSurface(event.target)||reviewPlayer.readyState()<2||event.deltaY===0)return;
  const rect=reviewPlayer.el().getBoundingClientRect();if(!rect.width||!rect.height)return;
  event.preventDefault();event.stopPropagation();
  const factor=event.deltaY<0?1.2:1/1.2,next=Math.max(1,Math.min(6,videoZoom*factor));
  const x=(event.clientX-rect.left)/rect.width,y=(event.clientY-rect.top)/rect.height;
  videoPanX=x-(x-videoPanX)*next/videoZoom;videoPanY=y-(y-videoPanY)*next/videoZoom;videoZoom=next;
  if(videoZoom<1.01)videoZoom=1;
  applyVideoZoom();reviewPlayer.userActive(true);
},{passive:false});
reviewPlayer.el().addEventListener('pointerdown',event=>{
  if(videoZoom<=1||event.button!==0||event.target!==$('player'))return;
  const rect=reviewPlayer.el().getBoundingClientRect();
  videoDrag={id:event.pointerId,x:event.clientX,y:event.clientY,panX:videoPanX,panY:videoPanY,width:rect.width,height:rect.height};
  reviewPlayer.el().setPointerCapture(event.pointerId);
});
reviewPlayer.el().addEventListener('pointermove',event=>{
  if(!videoDrag||videoDrag.id!==event.pointerId)return;
  const dx=event.clientX-videoDrag.x,dy=event.clientY-videoDrag.y;
  if(Math.hypot(dx,dy)>3)suppressVideoClick=true;
  videoPanX=videoDrag.panX+dx/videoDrag.width;videoPanY=videoDrag.panY+dy/videoDrag.height;applyVideoZoom();
});
for(const kind of ['pointerup','pointercancel','lostpointercapture'])reviewPlayer.el().addEventListener(kind,()=>{videoDrag=null;});
reviewPlayer.el().addEventListener('click',event=>{if(suppressVideoClick){event.preventDefault();event.stopImmediatePropagation();suppressVideoClick=false;}},true);
reviewPlayer.on('loadstart',resetVideoZoom);
