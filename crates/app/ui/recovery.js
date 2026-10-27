"use strict";
function recoveryText(record,detail=false){
  if(!record)return '';
  if(record.stage==='unrecoverable')return message(detail?'recovery.unrecoverable_'+record.reason:'recovery.unrecoverable');
  if(record.reason==='partial'&&record.stage==='complete')return message('recovery.partial');
  const reasons={missing_source:'no_source',timing_uncertain:'uncertain',gapped:'gapped',export_failed:'needs_attention'};
  return record.stage==='needs_attention'?message('recovery.'+(reasons[record.reason]||'needs_attention')):message('recovery.'+record.stage);
}
function renderRecovery(){
  const job=state.recovery,panel=$('recovery-panel');
  panel.hidden=!job||['idle','complete'].includes(job.stage);
  if(panel.hidden)return;
  const waiting=job.waiting;
  $('recovery-title').textContent=waiting?message('recovery.busy'):message('recovery.'+job.stage);
  const records=Object.values(job.records||{}),finished=['complete','needs_attention'].includes(job.stage);
  $('recovery-summary').textContent=finished?message('recovery.results',{recovered:records.filter(r=>r.stage==='complete').length,unrecoverable:records.filter(r=>r.stage==='unrecoverable').length,attention:records.filter(r=>r.stage==='needs_attention').length}):message('recovery.count',job);
  const review=finished&&!!job.issues?.length;
  const issues=$('recovery-issues');
  if(review&&issues.hidden)issues.open=true;
  issues.hidden=!review;
  $('recovery-issue-list').replaceChildren();
  for(const issue of job.issues||[]){
    const item=document.createElement('li'),info=document.createElement('div'),path=document.createElement('span'),reason=document.createElement('span'),cleanup=document.createElement('small');
    path.className='recovery-file-path';path.textContent=issue.path;
    reason.textContent=issue.reason==='empty_source'?message('recovery.empty_source'):recoveryText({stage:'needs_attention',reason:issue.reason});
    cleanup.textContent=message('recovery.kept_for_review');
    info.append(path,reason,cleanup);item.append(info);
    if(issue.reason==='empty_source'&&issue.fingerprint){
      const remove=document.createElement('button');remove.type='button';remove.className='danger';remove.textContent=message('recovery.delete_file');
      remove.disabled=state.export_running||state.share_running||state.recording_open;
      remove.onclick=run(async()=>{if(!confirm(message('recovery.confirm_delete',{path:issue.path})))return;await action('delete_recovery_file',{path:issue.path,confirm_closed:!state.connected});});
      item.append(remove);
    }
    $('recovery-issue-list').append(item);
  }
  const progress=$('recovery-progress');progress.hidden=finished;progress.max=Math.max(1,job.total);
  if(waiting||job.stage==='recovering')progress.removeAttribute('value');else progress.value=job.completed;
  $('recovery-retry').disabled=state.export_running||state.share_running||state.recording_open;
}
const checkRecovery=run(async()=>{
  const offline=!state.connected;
  if(offline&&!confirm(message('recovery.confirm_closed')))return;
  await action('recover_sources',{confirm_closed:offline});
});
$('recovery-retry').onclick=checkRecovery;
$('settings-recovery').onclick=checkRecovery;

