"use strict";
const englishMessages={
  "duration.d": "{count}d",
  "duration.h": "{count}h",
  "duration.m": "{count}m",
  "duration.s": "{count}s",
  "watcher.empty": "No log events yet",
  "watcher.activity": "Last run activity {age} ago",
  "watcher.event": "Last log event {age} ago",
  "watcher.left": "{status} · Left dungeon; abandoning in {remaining}",
  "watcher.inactive": "{status} · No exit or completion detected; inactivity timeout in {remaining}",
  "events.show": "Show events",
  "events.hide": "Hide events",
  "unknown": "Unknown status",
  "connection.retry": "Retrying automatically in {seconds}s.",
  "connection.pill_store": "OBS · Password store",
  "connection.pill_password": "OBS · Password needed",
  "connection.unreachable": "Cannot reach OBS. Check that OBS and its WebSocket server are running.",
  "connection.password_missing": "OBS needs a password. Enter it in setup.",
  "connection.credential_store": "Unlock your system password store or enter the OBS password in setup.",
  "connection.connect_failed": "Could not connect to OBS. Check the connection details.",
  "connection.disconnected": "The OBS connection was lost. Reconnecting automatically.",
  "connection.status_failed": "OBS stopped responding. Reconnecting automatically.",
  "credential.saved": "Password saved securely.",
  "credential.session": "Connected for this session.",
  "credential.reconnected": "Reconnected to OBS.",
  "credential.removed": "Saved password removed. Your current connection stays open.",
  "credential.store_failed": "Could not use the system password store.",
  "processing.cancelling": "Cancelling clip preparation",
  "processing.exporting": "Saving clip",
  "processing.post_roll": "Post-roll",
  "processing.reconnecting": "Waiting for OBS",
  "processing.observed_source": "Waiting for OBS",
  "processing.uncertain_source": "Check recording timing",
  "processing.source_open": "Waiting for OBS to finish saving",
  "processing.queued": "Queued",
  "processing.preparing": "Preparing clip",
  "processing_detail.post_roll": "Saving the last {remaining_seconds} seconds after the encounter.",
  "processing_detail.reconnecting": "Reconnecting to OBS before checking the remaining footage.",
  "processing_detail.observed_source": "Stop recording in OBS so the remaining footage can be checked. Companion does not control this recording.",
  "processing_detail.uncertain_source": "Recording timing was interrupted. Stop recording in OBS so the footage can be checked.",
  "processing_detail.source_open": "OBS is still writing the last part of this recording.",
  "processing_detail.queued": "Waiting for the current clip to finish saving.",
  "processing_detail.preparing": "Checking recorded footage before saving this clip.",
  "processing_detail.exporting": "Saving the recording.",
  "processing_detail.cancelling": "Stopping clip preparation and retaining source footage.",
  "recovery.empty_source": "Unrecoverable: this file is empty.",
  "recovery.kept_for_review": "Not automatically cleaned up. Kept for review.",
  "recovery.delete_file": "Delete file",
  "recovery.confirm_delete": "Permanently delete this empty source file?\n{path}\nOBS must have stopped recording. Encounter details will be kept.",
  "recovery.unrecoverable": "Unrecoverable",
  "recovery.unrecoverable_missing_source": "Unrecoverable: no matching footage was found in the recording folder. The encounter details and markers are still available. Restore the original footage and check again if you have a backup.",
  "recovery.unrecoverable_gapped": "Unrecoverable as a complete clip: parts of the source footage are missing. The encounter details and markers are still available. Restore the missing files and check again if you have a backup.",
  "recovery.idle": "Recovery",
  "recovery.scanning": "Looking for recordings",
  "recovery.checking": "Checking footage",
  "recovery.recovering": "Recovering recordings",
  "recovery.complete": "Recovery complete",
  "recovery.needs_attention": "Attention needed",
  "recovery.count": "{completed} of {total} files checked",
  "recovery.unknown_files": {"one":"One file needs review","other":"{count} files need review"},
  "recovery.results": "{recovered} recovered · {unrecoverable} unrecoverable · {attention} need attention",
  "recovery.review_files": "Files needing review",
  "recovery.unknown": "Recovery status unavailable",
  "recovery.retry": "Check recordings",
  "recovery.partial": "Partial footage",
  "recovery.no_source": "No matching source footage found",
  "recovery.uncertain": "Source timing could not be verified",
  "recovery.gapped": "Source footage has gaps",
  "recovery.busy": "Waiting for OBS to stop recording",
  "auto.on": "Auto recording on",
  "auto.paused": "Auto recording paused",
  "auto.obs_blocked": "Auto recording blocked",
  "auto.setup_blocked": "Auto recording blocked",
  "recording.pending": "Pending",
  "recording.exporting": "Saving",
  "recording.ready": "Ready",
  "recording.failed": "Failed",
  "recording.unavailable": "Unavailable",
  "status.elapsed": "{duration} elapsed",
  "date.all": "All dates",
  "date.from": "From {date}",
  "date.until": "Until {date}",
  "audio.track": "Track {number}",
  "audio.mixed": {
    "one": "One track included",
    "other": "{count} tracks mixed into one audio track",
    "zero": "No audio included"
  },
  "discovery.found": {
    "one": "Found one installation.",
    "other": "Found {count} installations.",
    "zero": "No installations found here. Search your game folder or add one manually."
  }
};
Object.assign(englishMessages,{
'media.preparing':'Preparing footage','media.reading':'Reading footage','media.copying':'Copying footage','media.encoding':'Encoding footage','media.cpu_fallback':'Retrying with CPU encoding','media.joining':'Joining footage','media.converting':'Converting footage','media.checking':'Checking video and audio','media.poster':'Creating preview image','media.playback':'Preparing playback','media.bundle':'Preparing sharing bundle','media.writing_zip':'Writing ZIP',
'progress.count':'{completed} of {total}','progress.processed':'{seconds} seconds processed','progress.speed':'{speed}× speed', 'date.range':'{from} – {to}', 'date.start':'Choose a start date','date.end':'Choose an end date','recovery.confirm_closed':'Confirm that OBS has stopped recording before checking existing video files.'
});

let uiLocale=navigator.language||'en';
try{uiLocale=localStorage.getItem('ui.locale')||uiLocale;}catch{/* Standalone bundles may run with storage disabled. */}
try{new Intl.Locale(uiLocale);}catch{uiLocale='en';}
document.documentElement.lang=uiLocale;
document.documentElement.dir=/^(ar|fa|he|ur)(-|$)/i.test(uiLocale)?'rtl':'ltr';
function weekInfo(){const locale=new Intl.Locale(uiLocale);return typeof locale.getWeekInfo==='function'?locale.getWeekInfo():locale.weekInfo||{firstDay:1};}

function message(key,params={}){
  let value=englishMessages[key]??englishMessages.unknown;
  if(typeof value==='object')value=value[params.count===0?'zero':new Intl.PluralRules(uiLocale).select(params.count)]??value.other;
  return value.replace(/\{(\w+)\}/g,(_,name)=>typeof params[name]==='number'?new Intl.NumberFormat(uiLocale).format(params[name]):String(params[name]??''));
}
function noticeText(group,notice){
  if(!notice)return '';
  if(typeof notice==='string')return notice; // Historical snapshots/logs.
  return [message(group+'.'+notice.code),notice.diagnostic].filter(Boolean).join('\n');
}
function processingText(progress,detail=false){
  if(!progress)return '';
  return detail?(progress.progress?progressText(progress.progress):message('processing_detail.'+progress.stage,progress)):message('processing.'+progress.stage,progress);
}
function searchText(value){return String(value).normalize('NFC').toLocaleLowerCase(uiLocale);}
function formatNumber(value,maximumFractionDigits=1){return new Intl.NumberFormat(uiLocale,{maximumFractionDigits}).format(value);}

function progressText(progress){
  if(!progress)return '';
  if(typeof progress==='string')return progress;
  return [message('media.'+progress.stage),progress.total!=null?message('progress.count',progress):'',progress.processed_seconds!=null?message('progress.processed',{seconds:progress.processed_seconds}):'',progress.speed!=null?message('progress.speed',{speed:progress.speed}):''].filter(Boolean).join(' · ');
}
