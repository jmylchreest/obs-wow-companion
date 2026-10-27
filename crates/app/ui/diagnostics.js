"use strict";
let eventRowsKey='';
function elapsedLabel(seconds){
  let remaining=Math.max(0,Math.floor(seconds));
  const parts=[];
  for(const [size,unit] of [[86400,'d'],[3600,'h'],[60,'m'],[1,'s']]){
    const value=Math.floor(remaining/size);remaining%=size;
    if(value)parts.push(message('duration.'+unit,{count:value}));
    if(parts.length===2)break;
  }
  return parts.join(' ')||message('duration.s',{count:0});
}
function watcherDescription(log){
  const now=Date.now(),last=log.active?(log.last_activity_ms??log.last_event_ms):log.last_event_ms,age=last==null?null:Math.max(0,Math.floor((now-last)/1000));
  const ageText=age===null?message('watcher.empty'):message(log.active?'watcher.activity':'watcher.event',{age:elapsedLabel(age)});
  if(log.dungeon_exit_ms!=null)return message('watcher.left',{status:ageText,remaining:elapsedLabel(Math.max(0,Math.ceil((log.dungeon_exit_ms+60000-now)/1000)))});
  if(log.active&&age!==null&&age>=60)return message('watcher.inactive',{status:ageText,remaining:elapsedLabel(Math.max(0,(log.inactivity_deadline_ms-now)/1000))});
  return ageText;
}
function eventText(entry){
  if(entry.detail?.kind==='connection')return noticeText('connection',entry.detail.notice);
  if(entry.detail?.kind==='media')return progressText(entry.detail.progress);
  return entry.message||'';
}
function renderDiagnostics(){
  const logs=(state.log_status||[]).filter(log=>log.name!=='manual');
  const quiet=logs.find(log=>log.active&&log.last_event_ms!=null&&Date.now()-(log.last_activity_ms??log.last_event_ms)>=60000);
  $('footer-status').textContent=quiet?`${quiet.name}: ${watcherDescription(quiet)}`:$('capture-state').textContent;
  $('footer-status').title=$('footer-status').textContent;
  if($('events-dialog').hidden)return;
  $('watcher-status').replaceChildren();
  for(const log of logs){const row=document.createElement('p');row.textContent=`${log.name}: ${watcherDescription(log)}`;row.title=log.log_file||'';$('watcher-status').append(row);}
  $('event-log-error').hidden=!state.event_log_error;$('event-log-error').textContent=state.event_log_error||'';
  const query=$('event-search').value.toLowerCase(),entries=(state.event_log||[]).filter(entry=>`${entry.source} ${entry.level} ${eventText(entry)}`.toLowerCase().includes(query));
  const key=JSON.stringify(entries);if(key===eventRowsKey)return;eventRowsKey=key;
  const scroll=$('event-scroll'),top=scroll.scrollTop;
  $('event-rows').replaceChildren();
  for(const entry of entries){const row=document.createElement('tr');row.className=entry.level==='warning'?'event-warning':'';for(const value of [new Date(entry.at_ms).toLocaleString(),entry.source,eventText(entry)]){const cell=document.createElement('td');cell.textContent=value;row.append(cell);}$('event-rows').append(row);}
  if(!entries.length){const row=document.createElement('tr'),cell=document.createElement('td');cell.colSpan=3;cell.textContent=query?'No matching events':'No events recorded yet';row.append(cell);$('event-rows').append(row);}
  scroll.scrollTop=$('event-follow').checked?scroll.scrollHeight:top;
}
let eventsHeight=null,eventDrag=null;
function sizeEventsDock(){
  const main=document.querySelector('main'),bounds=main.getBoundingClientRect(),style=getComputedStyle(main);
  const dock=$('events-dock'),left=bounds.left+parseFloat(style.paddingLeft),right=bounds.right-parseFloat(style.paddingRight);
  dock.style.left=`${left}px`;dock.style.width=`${Math.max(0,right-left)}px`;
  const expanded=!$('events-dialog').hidden,max=Math.max(100,window.innerHeight-160);
  let height=eventsHeight;
  if(height===null){const video=reviewPlayer.el().getBoundingClientRect();height=video.width&&video.bottom>0?window.innerHeight-video.bottom-48:window.innerHeight*0.35;}
  height=Math.min(max,Math.max(Math.min(200,max),height));
  $('events-dialog').style.height=`${height}px`;
  document.documentElement.style.setProperty('--events-space',`${expanded?height+48:34}px`);
  $('events-divider').setAttribute('aria-valuemin',String(Math.min(200,max)));$('events-divider').setAttribute('aria-valuemax',String(max));$('events-divider').setAttribute('aria-valuenow',String(Math.round(height)));
}
function toggleEvents(open){
  $('events-dialog').hidden=!open;$('events-divider').hidden=!open;
  $('open-events').setAttribute('aria-expanded',String(open));
  const label=message(open?'events.hide':'events.show');
  $('open-events').setAttribute('aria-label',label);$('open-events').title=label;
  sizeEventsDock();eventRowsKey='';renderDiagnostics();
}
$('open-events').onclick=()=>toggleEvents($('events-dialog').hidden);
$('close-events').onclick=()=>{toggleEvents(false);$('open-events').focus();};
const eventsDivider=$('events-divider');
eventsDivider.onpointerdown=event=>{if(event.button!==0)return;event.preventDefault();eventDrag={y:event.clientY,height:$('events-dialog').getBoundingClientRect().height};eventsDivider.setPointerCapture(event.pointerId);document.body.classList.add('resizing-events');};
eventsDivider.onpointermove=event=>{if(!eventDrag)return;eventsHeight=eventDrag.height+eventDrag.y-event.clientY;sizeEventsDock();eventsHeight=$('events-dialog').getBoundingClientRect().height;};
function stopEventResize(){eventDrag=null;document.body.classList.remove('resizing-events');}
eventsDivider.onpointerup=eventsDivider.onpointercancel=eventsDivider.onlostpointercapture=stopEventResize;
eventsDivider.onkeydown=event=>{if(!['ArrowUp','ArrowDown','Home','End'].includes(event.key))return;event.preventDefault();eventsHeight=event.key==='Home'?200:event.key==='End'?window.innerHeight:$('events-dialog').getBoundingClientRect().height+(event.key==='ArrowUp'?20:-20);sizeEventsDock();};
eventsDivider.ondblclick=()=>{eventsHeight=null;sizeEventsDock();};
window.addEventListener('resize',sizeEventsDock);
new ResizeObserver(sizeEventsDock).observe(document.querySelector('main'));
sizeEventsDock();
$('event-search').oninput=renderDiagnostics;
$('event-follow').onchange=()=>{eventRowsKey='';renderDiagnostics();};
$('save-events').onclick=()=>{const blob=new Blob([(state.event_log||[]).map(entry=>JSON.stringify(entry)).join('\n')+'\n'],{type:'application/x-ndjson'}),url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download='companion-events.jsonl';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
setInterval(renderDiagnostics,1000);
renderDiagnostics();
