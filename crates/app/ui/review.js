"use strict";
let selectedStatus=null, retainedRequest=null, pendingAudioSwitch=null;
let labelRenderKey='', annotationQueue=Promise.resolve();
const iconPaths={trash:'<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>',heart:'<path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z"/>',lock:'<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>',unlock:'<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0"/>'};
function iconButton(icon,title,pressed){const button=document.createElement('button');button.className='icon-button '+icon;button.type='button';button.title=title;button.setAttribute('aria-label',title);button.setAttribute('aria-pressed',String(pressed));button.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true">'+iconPaths[icon]+'</svg>';return button;}
function timeLabel(seconds){if(!Number.isFinite(seconds))return '0:00';seconds=Math.max(0,seconds);return `${Math.floor(seconds/60)}:${Math.floor(seconds%60).toString().padStart(2,'0')}`;}
function recordingById(id){return (state.activities||[]).find(a=>a.id===id);}
function reviewActivity(){return recordingById(selected)||(state.active||[]).find(a=>a.id===selected);}
function recordingPrefix(a){
  if(a.kind==='manual')return '--';
  if(a.kind==='delve')return 'DV';
  if(a.kind==='mythic_plus'&&Number.isInteger(a.key_level)&&a.key_level>0)return String(a.key_level).padStart(2,' ');
  if(a.kind==='raid')return ({14:'NM',15:'HC',16:' M',17:'RF'})[a.difficulty]||'  ';
  return '  ';
}
function renderRecordingTitle(element,a){
  element.classList.add('recording-heading');
  const prefix=document.createElement('span');prefix.className='recording-prefix '+(a.kind==='mythic_plus'?'key':a.kind==='manual'?'manual':a.kind==='delve'?'delve':a.kind==='raid'?({14:'normal',15:'heroic',16:'mythic',17:'raid-finder'})[a.difficulty]||'unknown':'unknown');prefix.textContent=recordingPrefix(a);
  const name=document.createElement('span');name.className='recording-name';name.textContent=a.name;
  element.replaceChildren(prefix,name);
}
async function choose(a,force=false){
  if(!force&&selected===a.id&&state.preview?.status!=='failed')return;
  pendingAudioSwitch=null;
  selectedStatus=a.status;retainedRequest=null;
  clearExportRange();selected=a.id;$('marker-form').hidden=true;$('tags').value='';labelRenderKey='';
  if(state.preview?.key!==a.id){resetPlayback();$('empty-player').textContent=a.status==='pending'?'Waiting for recording to finish':a.status==='exporting'?'Exporting recording…':'Select a recording';$('empty-player').hidden=false;}
  renderRecordingTitle($('review-title'),a);$('review-state').textContent=a.status==='ready'?'':a.error||label(a.status);
  $('review-state').title=[a.error,...(a.warnings||[])].filter(Boolean).join('\n');
  $('trim-start').value=0;$('trim-end').value=0;
  $('audio').replaceChildren();for(let i=0;i<Math.max(a.audio_tracks||0,1);i++)$('audio').add(new Option(`Track ${i+1}`,String(i)));
  $('share-path').value=(state.share_directory||'').replace(/[\\/]$/,'')+'/'+a.id+'.zip';
  $('clip-options').open=false;
  renderShareAudio(a,true);

  renderMarkers(a);renderList();renderContext();renderLabels();renderPlaybackDetails();
  if(a.status==='ready'||a.review_available){retainedRequest=a.id;lastPreview='';$('review-state').textContent='Loading…';if(state.preview?.status==='ready'&&state.preview.key===a.id)renderState(state);else await requestPlayback(a.id,0,true);}
}
function recordingDateMatches(a){
  const from=$('date-from').value,to=$('date-to').value;
  const start=from?new Date(from+'T00:00:00').getTime():-Infinity;
  const end=to?new Date(to+'T00:00:00'):null;if(end)end.setDate(end.getDate()+1);
  return a.start_ms>=start&&(!end||a.start_ms<end.getTime());
}
function updateRecordingFilters(){
  const all=[...(state.active||[]),...(state.activities||[])];
  function options(id,values,empty,name){const input=$(id),previous=input.value,key=JSON.stringify(values);if(input.dataset.values!==key){input.dataset.values=key;input.replaceChildren(new Option(empty,''));for(const value of values)input.add(new Option(name(value),String(value)));input.value=values.some(v=>String(v)===previous)?previous:'';}}
  options('kind-filter',[...new Set(all.map(a=>a.kind))].sort(),'All activities',label);
  options('outcome-filter',[...new Set(all.map(a=>a.outcome).filter(Boolean))].sort(),'All outcomes',label);
  const relevant=all.filter(a=>!$('kind-filter').value||a.kind===$('kind-filter').value);
  const difficulties=[...new Set(relevant.map(a=>a.difficulty).filter(v=>v!=null))].sort((a,b)=>a-b);
  options('difficulty-filter',difficulties,'All difficulties',id=>({1:'Normal dungeon',2:'Heroic dungeon',8:'Mythic+',14:'Normal',15:'Heroic',16:'Mythic',17:'Raid Finder',23:'Mythic dungeon',208:'Delve'})[id]||`Difficulty ${id}`);
  const keys=[...new Set(relevant.filter(a=>a.kind==='mythic_plus').map(a=>a.key_level).filter(v=>v>0))].sort((a,b)=>a-b);
  options('key-filter',keys,'All keys',key=>String(key));
  $('difficulty-filter').hidden=!difficulties.length;$('key-filter').hidden=!keys.length;
}
function renderList(){
  updateRecordingFilters();
  const query=searchText($('search').value.trim());
  let all=[...(state.active||[]).map(a=>({...a,status:(state.ownership==='managed'||state.ownership==='observed')&&((state.recording_activity_ids||[]).includes(a.id)||a.kind==='manual')?'recording':'active'})),...(state.activities||[])].filter(a=>($('favourites-only').getAttribute('aria-pressed')!=='true'||(a.tags||[]).includes('favourite'))&&($('protected-only').getAttribute('aria-pressed')!=='true'||a.protected)&&(!$('kind-filter').value||a.kind===$('kind-filter').value)&&(!$('outcome-filter').value||a.outcome===$('outcome-filter').value)&&(!$('difficulty-filter').value||a.difficulty===Number($('difficulty-filter').value))&&(!$('key-filter').value||a.key_level===Number($('key-filter').value))&&recordingDateMatches(a)&&[a.name,a.character,a.installation,new Date(a.start_ms).toLocaleString(),...(a.tags||[])].map(searchText).join(' ').includes(query));
  const byTime=(a,b)=>Number(a.start_ms)-Number(b.start_ms)||String(a.id).localeCompare(String(b.id));
  all.sort($('sort').value==='name'?(a,b)=>a.name.localeCompare(b.name)||byTime(b,a):$('sort').value==='old'?byTime:(a,b)=>byTime(b,a));
  $('result-count').textContent=String(all.length);$('recordings').replaceChildren();
  if(!all.length){const empty=document.createElement('p');empty.className='empty';empty.textContent=query?'No matching recordings':state.automatic_paused?'Automatic recording paused':'Waiting for your next encounter';$('recordings').append(empty);}
  for(const a of all){
    const row=document.createElement('div');row.className='recording'+(a.id===selected?' selected':'');row.dataset.id=a.id;
    const persisted=!!recordingById(a.id),heart=(a.tags||[]).includes('favourite');
    const controls=document.createElement('div');controls.className='recording-icons';
    const favourite=iconButton('heart',heart?'Remove favourite':'Favourite',heart);favourite.disabled=!persisted;
    favourite.onclick=()=>annotateRecording(a.id,current=>({...current,tags:heart?current.tags.filter(t=>t!=='favourite'):[...current.tags,'favourite']}));
    const lock=iconButton(a.protected?'lock':'unlock',a.protected?'Unlock recording':'Protect recording',!!a.protected);lock.disabled=!persisted;
    lock.onclick=()=>annotateRecording(a.id,current=>({...current,protected:!a.protected}));controls.append(favourite,lock);
    const select=document.createElement('button');select.className='recording-select';select.setAttribute('aria-pressed',String(a.id===selected));select.onclick=run(()=>choose(a));
    const title=document.createElement('strong');renderRecordingTitle(title,a);
    const sub=document.createElement('small');sub.textContent=`${new Date(a.start_ms).toLocaleDateString(undefined,{month:'short',day:'numeric'})} ${new Date(a.start_ms).toLocaleTimeString(undefined,{hour:'2-digit',minute:'2-digit'})} · ${a.character||a.installation}`;
    const duration=document.createElement('span');duration.className='recording-duration';duration.textContent=a.status==='ready'?timeLabel((a.end_ms-a.start_ms)/1000):recoveryText(a.recovery)||processingText(a.processing)||message('recording.'+a.status);duration.title=recoveryText(a.recovery,true)||processingText(a.processing,true)||duration.textContent;
    select.title=[(recordingPrefix(a).trim()+' '+a.name).trim(),label(a.kind),label(a.outcome),a.error,...(a.warnings||[])].filter(Boolean).join('\n');const result=recordingResult(a),badge=document.createElement('span');badge.className='recording-result '+result.style;badge.textContent=result.text;badge.title=result.title;badge.setAttribute('aria-label',result.title);select.append(title,sub,duration,badge);
    const labels=(a.tags||[]).filter(tag=>tag!=='favourite');
    if(labels.length){const strip=document.createElement('span');strip.className='recording-labels';strip.title=labels.join(', ');strip.setAttribute('aria-label','Labels: '+labels.join(', '));for(const tag of labels){const chip=document.createElement('span');chip.className='recording-label';chip.textContent=tag;strip.append(chip);}select.append(strip);}
    const trash=iconButton('trash',a.protected?'Unlock recording before deleting':'Delete recording (Shift-click skips confirmation)',false);trash.disabled=!persisted||!!a.protected;trash.onclick=run(event=>deleteRecording(a.id,event.shiftKey));
    row.append(controls,select,trash);$('recordings').append(row);
  }
  requestAnimationFrame(fitRecordingLabels);
}
function fitRecordingLabels(){
  for(const strip of document.querySelectorAll('.recording-labels')){
    let used=0,full=false;const available=strip.clientWidth;
    for(const chip of strip.children){chip.hidden=false;const width=chip.getBoundingClientRect().width+(used?4:0);if(full||used+width>available){chip.hidden=true;full=true;}else used+=width;}
  }
}
new ResizeObserver(fitRecordingLabels).observe($('recordings'));
function annotateRecording(id,change){
  $('label-status').textContent='Saving…';
  annotationQueue=annotationQueue.catch(()=>{}).then(async()=>{
    const response=await fetch('/api/status');if(!response.ok)throw Error('Could not read recording labels.');
    const current=(await response.json()).activities.find(a=>a.id===id);if(!current)throw Error('Recording no longer exists.');
    const next=change({tags:current.tags||[],protected:!!current.protected});
    await action('annotate',{id,tags:[...new Set(next.tags)],protected:next.protected});
    const updated=await fetch('/api/status');if(!updated.ok)throw Error('Saved, but could not refresh labels.');
    renderState(await updated.json());$('label-status').textContent='';
  }).catch(e=>{$('label-status').textContent='Not saved';error(e);});
  return annotationQueue;
}
function renderLabels(){
  const current=recordingById(selected);
  const known=[...new Set((state.activities||[]).flatMap(a=>a.tags||[]).filter(t=>t!=='favourite'))].sort((a,b)=>a.localeCompare(b));
  const key=JSON.stringify([selected,current?.tags,known]);if(key===labelRenderKey)return;labelRenderKey=key;
  $('known-labels').replaceChildren();for(const value of known){const option=document.createElement('option');option.value=value;$('known-labels').append(option);}
  $('label-chips').replaceChildren();
  for(const tag of (current?.tags||[]).filter(t=>t!=='favourite')){
    const chip=document.createElement('span');chip.className='label-chip';const name=document.createElement('span');name.textContent=tag;
    const remove=document.createElement('button');remove.type='button';remove.textContent='×';remove.setAttribute('aria-label',`Remove label ${tag}`);const id=current.id;
    remove.onclick=()=>annotateRecording(id,recording=>({...recording,tags:recording.tags.filter(t=>t!==tag)}));chip.append(name,remove);$('label-chips').append(chip);
  }
}
function addLabel(event){
  event?.preventDefault();const id=selected,value=$('tags').value.trim();if(!id||!value)return;
  if(value==='favourite'){error(Error('Use the heart to favourite a recording.'));return;}
  annotateRecording(id,current=>({...current,tags:[...current.tags,value]})).then(()=>{if(selected===id&&$('tags').value.trim()===value&&recordingById(id)?.tags?.includes(value))$('tags').value='';});
}
$('annotation').onsubmit=addLabel;$('tags').onchange=addLabel;
function seekTo(seconds,pause=true){const player=$('player');if(!Number.isFinite(player.duration)||player.readyState<1)return;if(pause)reviewPlayer.pause();reviewPlayer.hasStarted(true);player.currentTime=Math.min(player.duration,Math.max(0,seconds));updatePlayhead();}
function updatePlayhead(){const player=$('player');$('toggle-play').disabled=!Number.isFinite(player.duration);}
function renderMarkers(a){
  seekTrack.classList.remove('marker-hover');reviewPlayer.el().classList.remove('marker-focused');
  $('markers').replaceChildren();$('combat-track').replaceChildren();$('deaths').replaceChildren();$('deaths-panel').hidden=true;if(!a)return;
  const origin=(state.preview?.key===a.id?state.preview.origin_ms:null)??a.origin_ms??a.start_ms;
  const ready=state.preview?.key===a.id&&$('player').readyState>=1;
  const duration=ready&&Number.isFinite($('player').duration)?$('player').duration:Math.max(0,((a.end_ms??a.start_ms)-origin)/1000);
  if(!duration)return;
  const boundary=a.kind==='raid'?'boss':'activity';
  const events=[{at_ms:a.start_ms,kind:boundary+'_start',label:a.kind==='raid'?a.name+' starts':'Recording start'},...(a.markers||[]),...(a.end_ms!=null?[{at_ms:a.end_ms,kind:boundary+'_end',label:a.kind==='raid'?a.name+' ends':'Recording end'}]:[])].sort((a,b)=>a.at_ms-b.at_ms);
  function band(start,end,kind,title){if(!markerTypeVisible(kind==='boss'?'boss':'combat_activity'))return;start=Math.max(0,start);end=Math.min(duration,end);if(end<=start)return;const span=document.createElement('span');span.className='combat-span '+kind;span.style.left=`${start/duration*100}%`;span.style.width=`${(end-start)/duration*100}%`;span.title=title;span.dataset.start=String(start);span.dataset.end=String(end);$('combat-track').append(span);}
  const pulses=events.filter(event=>event.kind==='combat_activity').map(event=>(event.at_ms-origin)/1000);
  let from=null,last=null;
  for(const at of pulses){if(last!==null&&at-last>5){band(from,last+1,'trash','Combat activity (estimated)');from=null;}if(from===null)from=at;last=at;}
  if(last!==null)band(from,last+1,'trash','Combat activity (estimated)');
  let boss=null;
  for(const event of events){
    const seconds=(event.at_ms-origin)/1000,inRange=seconds>=0&&seconds<=duration;
    if(event.kind==='boss_start')boss=event;
    if(event.kind==='boss_end'&&boss){band((boss.at_ms-origin)/1000,seconds,'boss',boss.label);boss=null;}
    if(event.kind==='combat_activity'||event.kind==='activity_end')continue;
    if(inRange&&markerTypeVisible(event.kind)){
      const tick=document.createElement('button');tick.className='event-tick '+(event.kind==='death'?'death':event.kind==='manual'?'manual':event.kind.startsWith('boss_')?'boss':event.kind.includes('round')?'round':'boundary');
      tick.style.left=`${seconds/duration*100}%`;tick.dataset.seconds=String(seconds);tick.dataset.eventKind=event.kind;
      const head=document.createElement('span');head.className='sign-head';
      head.textContent=event.kind==='manual'?'⚑':event.kind==='death'?'†':event.kind.startsWith('boss_')?'⚔':event.kind.endsWith('_end')?'⏹':'▶';
      if(event.kind==='boss_end')head.classList.add('combat-ended');
      const caption=document.createElement('span');caption.className='sign-label';caption.textContent=`${timeLabel(seconds)} · ${event.label}`;
      tick.setAttribute('aria-label',caption.textContent);tick.disabled=!ready;tick.append(head,caption);
      for(const type of ['mousedown','pointerdown','touchstart'])tick.addEventListener(type,e=>e.stopPropagation());
      tick.onpointerenter=tick.onfocus=()=>{seekTrack.classList.add('marker-hover');reviewPlayer.el().classList.add('marker-focused');reviewPlayer.userActive(true);};
      tick.onpointerleave=tick.onblur=()=>{seekTrack.classList.remove('marker-hover');reviewPlayer.el().classList.remove('marker-focused');};
      tick.onmousemove=e=>e.stopPropagation();
      tick.onclick=e=>{e.stopPropagation();seekTo(event.kind==='death'?Math.max(0,seconds-5):seconds);};$('markers').append(tick);
    }
    if(event.kind==='death'){
      const row=document.createElement('tr');row.tabIndex=ready&&inRange?0:-1;row.setAttribute('aria-disabled',String(!ready||!inRange));row.title=inRange?'Review 5 seconds before death':'Outside the retained footage';
      const time=document.createElement('td');time.textContent=inRange?timeLabel(seconds):'Not captured';const player=document.createElement('td');player.textContent=event.label;const recap=document.createElement('td');recap.className='death-recap';
      const hits=(event.recent_hits||[]).slice(-3);
      if(!hits.length)recap.textContent='Not recorded';
      for(const hit of [...hits].reverse()){
        const entry=document.createElement('div');
        entry.textContent=`${hit.ability} · ${hit.source} (${Math.abs(hit.offset_ms/1000).toFixed(1)}s before)`;
        recap.append(entry);
      }

      row.append(time,player,recap);
      const jump=()=>{if(ready&&inRange)seekTo(Math.max(0,seconds-5));};row.onclick=jump;row.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();jump();}};$('deaths').append(row);
    }
  }
  // An unmatched boss start is not evidence that combat continued to the end of the file.
  const count=$('deaths').children.length;$('deaths-panel').hidden=count===0;$('death-count').textContent=String(count);
  layoutSignposts();requestAnimationFrame(layoutSignposts);
}
function layoutSignposts(){
  const width=seekTrack.getBoundingClientRect().width;if(width<=0)return;
  const laneEnds=[];
  const ticks=[...$('markers').children].sort((a,b)=>parseFloat(a.style.left)-parseFloat(b.style.left));
  for(const tick of ticks){
    const x=parseFloat(tick.style.left)*width/100,half=(tick.offsetWidth||20)/2;
    let lane=laneEnds.findIndex(end=>x-half>=end+5);if(lane<0)lane=laneEnds.length;
    laneEnds[lane]=x+half;tick.style.setProperty('--lane',lane);tick.style.zIndex=String(100-lane);tick.classList.toggle('edge-left',x<100);tick.classList.toggle('edge-right',x>width-100);
  }
}
new ResizeObserver(layoutSignposts).observe(seekTrack);
for(const event of ['timeupdate','play','pause','durationchange','emptied'])reviewPlayer.on(event,updatePlayhead);
reviewPlayer.on('loadedmetadata',()=>{controlBar.playbackRateMenuButton.updateVisibility();controlBar.playbackRateMenuButton.updateLabel();initializeExportRange();renderMarkers(reviewActivity());updatePlayhead();renderPlaybackDetails();});
updatePlayhead();

async function toggleFullscreen(){if(reviewPlayer.isFullscreen())reviewPlayer.exitFullscreen();else await reviewPlayer.requestFullscreen();}
let markerTarget=null;
$('add-marker').onclick=()=>{const active=(state.active||[]).some(a=>a.id===selected);markerTarget={id:selected,origin_ms:state.preview?.key===selected?state.preview.origin_ms:undefined,at_ms:active?Date.now():undefined,position_ms:active?undefined:Math.round($('player').currentTime*1000)};reviewPlayer.pause();reviewPlayer.hasStarted(true);$('marker-form').hidden=false;$('marker-label').focus();$('marker-label').select();};
$('cancel-marker').onclick=()=>{$('marker-form').hidden=true;};
$('marker-form').onsubmit=run(async()=>{if(!markerTarget)return;await action('add_marker',{...markerTarget,label:$('marker-label').value});$('marker-form').hidden=true;renderState(await (await fetch('/api/status')).json());});

async function switchPlaybackAudio(){
  if(!selected||pendingAudioSwitch)return;
  const track=Number($('audio').value),preview=state.preview;
  pendingAudioSwitch={id:selected,track,previousTrack:preview?.track||0,wallTime:(preview?.origin_ms||0)+reviewPlayer.currentTime()*1000,rate:reviewPlayer.tech(true).el().playbackRate,playing:!reviewPlayer.paused()};
  reviewPlayer.pause();$('audio').disabled=true;
  try{await requestPlayback(selected,track,false);}
  catch(e){cancelAudioSwitch();throw e;}
}
function cancelAudioSwitch(){
  if(pendingAudioSwitch?.id===selected)$('audio').value=String(pendingAudioSwitch.previousTrack);
  pendingAudioSwitch=null;$('audio').disabled=false;
}
reviewPlayer.on('loadeddata',()=>{
  const pending=pendingAudioSwitch,preview=state.preview;
  if(!pending||pending.id!==selected||preview?.key!==pending.id||preview.track!==pending.track)return;
  pendingAudioSwitch=null;$('audio').disabled=false;
  reviewPlayer.currentTime(Math.min(Math.max(0,(pending.wallTime-(preview.origin_ms||0))/1000),reviewPlayer.duration()));
  reviewPlayer.playbackRate(pending.rate);
  if(pending.playing)reviewPlayer.play().catch(error);
});
let playbackRequest=0;
async function requestPlayback(id,track,direct=false,reason){
  const request=++playbackRequest;
  while(request===playbackRequest&&selected===id){
    if(state.preview?.status==='generating'){
      if(state.preview.key===id&&state.preview.track===track)return;
      $('review-state').textContent='Waiting for playback preparation…';
      await new Promise(resolve=>setTimeout(resolve,150));continue;
    }
    const result=await action('preview',{id,track,direct,reason});
    if(!result.busy)return;
    await new Promise(resolve=>setTimeout(resolve,150));
  }
}
reviewPlayer.on('error',()=>{
  if(state.preview?.direct&&state.preview.key===selected){
    const failure=reviewPlayer.error();const reason=`Original file was rejected by the player (${failure?.code||'unknown'}): ${failure?.message||'unsupported container or codec'}`;
    resetPlayback();lastPreview='';$('review-state').textContent='Preparing compatible playback…';
    requestPlayback(selected,Number($('audio').value),false,reason).catch(error);
  }
});

function renderPlaybackDetails(){
  const preview=state.preview?.key===selected?state.preview:null,info=preview?.source;
  $('video-details').hidden=recordingById(selected)?.status!=='ready'&&!preview?.early;
  $('open-video-folder').hidden=!window.companionDesktop||!recordingById(selected)?.path||recordingById(selected)?.status!=='ready';
  $('video-metadata').replaceChildren();
  renderBundleDetails(recordingById(selected)?.latest_bundle);
  if(info){renderShareAudio(recordingById(selected));for(const option of $('audio').options)option.textContent=audioTrackLabel(Number(option.value),info);}
  if(!info){$('playback-details').textContent='Reading video details…';return;}
  const mode=preview.mode==='original'?'Original file':preview.mode==='remux'?'MP4 compatibility copy (video unchanged)':'H.264 compatibility copy (video converted)';
  $('playback-details').textContent=mode+(preview.reason?'. '+preview.reason:'');
  const rate=info.frame_rate?.split('/').map(Number),fps=rate?.[1]?Number((rate[0]/rate[1]).toFixed(3))+' fps':info.frame_rate||'Unknown';
  const rows=[['Container',info.container],['Video',info.video_codec?.toUpperCase()],['Resolution',`${info.width} × ${info.height}`],['Frame rate',fps],['Pixel format',info.pixel_format],['Audio',(info.audio_codecs||[]).map((codec,i)=>`${audioTrackLabel(i,info)}: ${codec.toUpperCase()}`).join(', ')||'No audio'],['Duration',timeLabel(info.duration_ms/1000)],['File size',`${(info.size_bytes/1048576).toFixed(1)} MiB`]];
  if(info.export){rows.push(['Saved using',info.export.encoder==='copy'?'Lossless copy':info.export.encoder]);if(info.export.fallback)rows.push(['Export fallback',info.export.fallback]);}
  renderAssetTable($('video-metadata'),rows);
  if(preview.status==='generating')$('review-state').textContent=preview.mode==='remux'?'Preparing MP4 (video unchanged)…':'Converting video for playback…';
  else if(preview.status==='ready')$('review-state').textContent=preview.early?(recordingById(selected)?.status==='ready'?'Retained footage':'Playing retained footage · Saving clip'):preview.direct?'Original playback':'Compatible playback';
  else if(preview.status==='failed')$('review-state').textContent=preview.error;
  $('review-state').title=$('playback-details').textContent;
}

function recordingResult(a){
  if(a.kind==='raid'&&a.outcome==='success')return {text:'K',style:'kill',title:'Boss killed'};
  if(a.kind==='raid'&&a.outcome==='failure')return {text:'W',style:'wipe',title:'Wipe'};
  if(a.kind==='mythic_plus'){
    const upgrade=a.keystone_upgrade;
    if(upgrade===0)return {text:'D',style:'deplete',title:'Depleted: completed over time'};
    if([1,2,3].includes(upgrade))return {text:`+${upgrade}`,style:`upgrade-${upgrade}`,title:`Timed: +${upgrade} keystone upgrade`};
    if(a.outcome==='failure')return {text:'D',style:'deplete',title:'Depleted'};
    if(a.outcome==='abandoned')return {text:'D',style:'deplete',title:'Abandoned run'};
    if(a.outcome==='success')return {text:'?',style:'unknown',title:'Completed; upgrade timing was not recorded'};
  }
  return {text:'',style:'',title:''};
}
async function deleteRecording(id,skipConfirmation=false){
  const recording=recordingById(id);if(!recording)return;
  if(recording.protected)throw Error('Unlock this recording before deleting.');
  if(!skipConfirmation&&!confirm(`Delete “${recording.name}” and its local video?`))return;
  await action('delete',{id});
  if(selected===id){selected=null;lastPreview='';resetPlayback();clearExportRange();$('empty-player').hidden=false;$('review-title').textContent='Review';$('review-state').textContent='';renderMarkers(null);renderContext();}
}

function renderShareAudio(recording,reset=false){
  const container=$('share-audio'),count=recording?.audio_tracks||0,info=state.preview?.key===selected?state.preview.source:null;
  const key=JSON.stringify([count,info?.audio_titles]);
  if(!reset&&container.dataset.key===key)return;
  const previous=[...container.querySelectorAll('input:checked')].map(input=>Number(input.value));
  container.dataset.key=key;container.replaceChildren();
  for(let track=0;track<count;track++){
    const label=document.createElement('label'),input=document.createElement('input');
    label.className='audio-track';input.type='checkbox';input.value=String(track);input.checked=reset?track===0:previous.includes(track);
    label.append(input,document.createTextNode(audioTrackLabel(track,info)));container.append(label);
    input.onchange=updateShareAudioHint;
  }
  updateShareAudioHint();
}
function updateShareAudioHint(){
  const count=$('share-audio').querySelectorAll('input:checked').length;
  $('share-audio-hint').textContent=count>1?`${count} tracks mixed into one audio track`:count===1?'One audio track included':'No audio included';
}

function audioTrackLabel(track,info){
  const title=info?.audio_titles?.[track]?.trim();
  return `Track ${track+1}`+(title?` · ${title}`:'');
}
function renderAssetTable(body,rows){
  body.replaceChildren();
  for(const [name,value] of rows){
    const row=document.createElement('tr'),heading=document.createElement('th'),cell=document.createElement('td');
    heading.scope='row';heading.textContent=name;cell.textContent=value||'Unknown';row.append(heading,cell);body.append(row);
  }
}
function renderBundleDetails(bundle){
  $('open-bundle-folder').hidden=!window.companionDesktop||!bundle?.path;
  $('bundle-empty').hidden=!!bundle;
  if(!bundle){$('bundle-metadata').replaceChildren();return;}
  const info=bundle.video||{},tracks=bundle.source_tracks||[];
  const rows=[['Saved to',bundle.path],['Exported',new Date(bundle.exported_at_ms).toLocaleString()],['ZIP size',`${(bundle.size_bytes/1048576).toFixed(1)} MiB`],['Range',bundle.range?`${timeLabel(bundle.range[0]/1000)} to ${timeLabel(bundle.range[1]/1000)}`:'Whole recording'],['Duration',timeLabel(info.duration_ms/1000)],['Video',`${info.video_codec?.toUpperCase()||'Unknown'} · ${info.width} × ${info.height}`],['Audio',info.audio_tracks?`${info.audio_codecs?.join(', ').toUpperCase()} · ${info.audio_tracks} track`:'No audio'],['Source tracks',tracks.map((track,i)=>`Track ${track}`+(bundle.source_track_names?.[i]?` · ${bundle.source_track_names[i]}`:'')).join(', ')||'None']];
  renderAssetTable($('bundle-metadata'),rows);
}
$('browse-share').hidden=!window.companionDesktop;
$('browse-share').onclick=()=>{
  $('browse-share').disabled=true;
  window.ipc.postMessage(JSON.stringify({action:'save-bundle-path',recording:selected,path:$('share-path').value}));
};
window.bundlePathChosen=({recording,path})=>{
  $('browse-share').disabled=false;
  if(recording===selected&&path)$('share-path').value=path;
};


function openAssetFolder(path){
  if(!path||!window.companionDesktop)return;
  window.ipc.postMessage(JSON.stringify({action:'open-containing-folder',path}));
}
$('open-video-folder').onclick=()=>openAssetFolder(recordingById(selected)?.path);
$('open-bundle-folder').onclick=()=>openAssetFolder(recordingById(selected)?.latest_bundle?.path);
window.folderOpened=({error:message})=>{if(message)error(new Error(message));};

// Handle each shortcut once, before Video.js applies its own focused-control keys.
document.addEventListener('keydown',async event=>{
  const target=event.target instanceof Element?event.target:document.activeElement;
  if(event.defaultPrevented||event.isComposing||event.ctrlKey||event.altKey||event.metaKey||
    $('library-view').inert||document.querySelector('dialog[open]')||
    target?.closest('input,select,textarea,[contenteditable]:not([contenteditable="false"])')||
    reviewPlayer.el().classList.contains('playback-unavailable'))return;
  const key=event.key.toLowerCase(),inside=reviewPlayer.el().contains(target);
  const widget=target?.closest('button,a,summary,[role="button"],[role="slider"],[role="menuitem"],[role="checkbox"],[role="separator"],[role="tab"]');
  if(widget&&(!inside||key===' '||key.startsWith('arrow')||key==='home'||key==='end'))return;
  const supported=[' ','k','j','l','m','f','arrowleft','arrowright','arrowup','arrowdown','home','end',',','.','<','>'];
  if(!supported.includes(key)&&!/^\d$/.test(key))return;
  event.preventDefault();event.stopPropagation();
  if(event.repeat&&[' ','k','m','f'].includes(key))return;
  try{
    const time=reviewPlayer.currentTime(),duration=reviewPlayer.duration();
    if(key===' '||key==='k'){if(reviewPlayer.paused())await reviewPlayer.play();else reviewPlayer.pause();}
    else if(key==='m')reviewPlayer.muted(!reviewPlayer.muted());
    else if(key==='f')await toggleFullscreen();
    else if(key==='arrowup'||key==='arrowdown')reviewPlayer.volume(Math.max(0,Math.min(1,reviewPlayer.volume()+(key==='arrowup'?.05:-.05))));
    else if(key==='<'||key==='>'){
      const rates=[...reviewPlayer.playbackRates()].sort((a,b)=>a-b),current=reviewPlayer.playbackRate();
      const rate=key==='>'?rates.find(rate=>rate>current):rates.reverse().find(rate=>rate<current);
      if(rate!==undefined)reviewPlayer.playbackRate(rate);
    }else if(key===','||key==='.'){
      if(!reviewPlayer.paused())return;
      const parts=state.preview?.source?.frame_rate?.split('/').map(Number)||[],fps=parts.length===2?parts[0]/parts[1]:parts[0];
      seekTo(time+(key==='.'?1:-1)/(Number.isFinite(fps)&&fps>0?fps:30));
    }else if(key==='home'||key==='end'||/^\d$/.test(key))seekTo(key==='home'?0:key==='end'?duration:duration*Number(key)/10,false);
    else {
      const amount={j:-10,l:10,arrowleft:event.shiftKey?-.1:-5,arrowright:event.shiftKey?.1:5}[key];
      seekTo(time+amount,false);
    }
  }catch(e){error(e);}
},true);
