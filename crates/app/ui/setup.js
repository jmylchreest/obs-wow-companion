'use strict';
let wizardStep=0,wizardAutoOpened=false,wizardBusy=false;
let discoveryStarted=false,discovering=false,discovered=[],installationRenderKey='';
const wizardTitles=['OBS connection','Recording settings','Picture and sound','WoW installations'];
const connectionLabels={
  checking:['Checking for OBS…','Checking the configured local WebSocket port.'],
  unreachable:['OBS not reachable','Open OBS and enable its WebSocket server, then check the port below.'],
  password_required:['OBS is running: password needed','OBS answered on the configured port. Enter its WebSocket password to inspect settings.'],
  available:['OBS is running: connect to continue','OBS answered without requiring a password. Enable authentication in OBS for the recommended setup.'],
  connected:['OBS connected','Read-only checks are available below. Picture and sound still need your verification.'],
  unsupported:['This port did not identify as supported OBS','Check the OBS WebSocket port and use a supported OBS Studio version.'],
  changed:['OBS settings changed','The OBS profile or scene collection changed. Check the settings again before capturing.']
};
function wizardPage(step){
  wizardStep=Math.max(0,Math.min(3,step));
  document.querySelectorAll('[data-wizard-panel]').forEach(panel=>panel.hidden=Number(panel.dataset.wizardPanel)!==wizardStep);
  document.querySelectorAll('[data-step]').forEach(button=>{const active=Number(button.dataset.step)===wizardStep;button.classList.toggle('active',active);button.setAttribute('aria-current',active?'step':'false');});
  $('wizard-title').textContent=wizardTitles[wizardStep];
  $('wizard-progress').textContent=`Step ${wizardStep+1} of 4`;
  $('wizard-back').disabled=wizardStep===0;
  $('wizard-next').hidden=wizardStep===3;
  $('wizard-finish').hidden=wizardStep!==3;
  $('wizard-error').hidden=true;
  renderSetup();
  $('wizard-title').focus();
  if(wizardStep===3&&!discoveryStarted)findInstallations();
}
function openWizard(){
  $('wizard-port').value=state.config?.obs_port||4455;
  $('wizard-password').value='';
  $('wizard-manual-only').checked=!!state.setup_completed&&(!!state.automatic_paused||!state.config?.installations.length);
  if(!$('setup-wizard').open)$('setup-wizard').showModal();
  wizardPage(0);
}
function closeWizard(){$('setup-wizard').close(); }
function wizardRun(fn){return async event=>{
  event?.preventDefault();if(wizardBusy)return;
  wizardBusy=true;$('wizard-error').hidden=true;renderSetup();
  try{await fn();}catch(e){$('wizard-error').textContent=e.message||String(e);$('wizard-error').hidden=false;}
  finally{wizardBusy=false;renderSetup();}
};}
function checksInto(id,checks){
  $(id).replaceChildren();
  for(const check of checks){
    const row=document.createElement('div');row.className=`readiness-check ${check.status}`;
    const title=document.createElement('strong');title.textContent=`${check.status==='pass'?'✓ ':check.status==='fail'?'! ':''}${check.id==='splitting'?'2. Save clips while you keep playing':check.title}`;
    const detail=document.createElement('p');detail.textContent=check.detail;
    row.append(title,detail);
    if(check.id==='splitting'&&check.status==='warn'){
      detail.textContent='In OBS, open Settings → Output, then:';
      const steps=document.createElement('ol');
      for(const instruction of ['Set Output Mode to Advanced.','Under Recording, set Type to Standard.','Enable Automatic File Splitting → Only split manually.','Click Apply, then Check again below.']){
        const item=document.createElement('li');item.textContent=instruction;steps.append(item);
      }
      row.append(steps);
    }
    $(id).append(row);
  }
}
function renderSetup(){
  if(!state.config)return;
  const report=state.setup||{},checks=report.checks||[];
  const connectionCode=state.obs_connection_error?.code;
  const unreachable=!state.connected&&connectionCode==='unreachable';
  const [title,detail]=connectionLabels[unreachable?'unreachable':report.connection]||connectionLabels.checking;
  const failed=checks.some(c=>c.status==='fail');
  $('credential-status').textContent=noticeText('credential',state.credential_status);
  const passwordSaved=state.credential_status?.code==='saved';
  $('credential-status').hidden=unreachable||wizardStep!==0||!state.credential_status||passwordSaved||state.credential_status?.code==='session';
  $('wizard-success').textContent=passwordSaved?'✓ Connected to OBS. Password saved securely.':'✓ Connected to OBS successfully.';
  $('wizard-success').hidden=wizardStep!==0||!state.connected;
  const stepChecks=[
    state.connected?'pass':'pending',
    checks.some(c=>['directory','splitting','outputs','obs_check'].includes(c.id)&&c.status==='fail')?'fail':state.connected&&!report.checking&&checks.some(c=>c.id==='directory')?(checks.some(c=>['directory','splitting','outputs'].includes(c.id)&&c.status==='warn')?'warn':'pass'):'pending',
    checks.some(c=>c.id==='media_tools'&&c.status==='fail')?'fail':'optional',
    checks.some(c=>c.id==='logs'&&c.status==='fail')?'fail':state.config.installations.length||$('wizard-manual-only').checked?'pass':'pending'
  ];
  const stepNames=['Connect','Recording','Picture & sound','WoW installs'];
  document.querySelectorAll('[data-step]').forEach(button=>{
    const index=Number(button.dataset.step),status=stepChecks[index];
    button.dataset.status=status;
    button.textContent=`${status==='pass'?'✓':status==='optional'?'':['fail','warn'].includes(status)?'!':index+1} ${stepNames[index]}`;
    button.setAttribute('aria-label',`${stepNames[index]}: ${status==='pass'?'checks passed':status==='fail'?'needs attention':status==='warn'?'review warnings':status==='optional'?'optional help':'not verified'}`);
  });
  $('connection').textContent=title;
  const notes=(state.connected?checks:[]).filter(check=>['fail','warn'].includes(check.status)).map(check=>check.detail||check.title);
  if(state.obs_connection_error)notes.unshift(noticeText('connection',state.obs_connection_error));
  const credential=noticeText('credential',state.credential_status);
  const routineCredentials=['reconnected','session','saved'];
  if(state.connected&&credential&&!routineCredentials.includes(state.credential_status?.code))notes.push(credential);
  if(!state.connected&&state.obs_retry_seconds!=null)notes.push(message('connection.retry',{seconds:state.obs_retry_seconds}));
  const tooltip=[...new Set(notes.filter(Boolean))].join('\n')||(report.checking?'Checking OBS…':state.connected?'Ready.':'Open OBS and check its WebSocket connection.');
  $('connection').textContent=state.connected?(failed?'OBS · Check setup':'OBS connected'):unreachable?'OBS offline':connectionCode==='credential_store'?message('connection.pill_store'):connectionCode==='password_missing'?message('connection.pill_password'):state.obs_connection_error?'OBS · Connection problem':'OBS offline';
  $('connection').dataset.status=unreachable?'offline':failed||state.obs_connection_error?'error':state.connected?'connected':'offline';
  $('connection').removeAttribute('title');
  $('obs-tooltip').textContent=tooltip;
  $('wizard-connection').textContent=title;$('wizard-connection-detail').textContent=detail;
  $('wizard-target-folder').textContent=state.config.spool;
  const splitting=checks.filter(c=>c.id==='splitting');
  checksInto('wizard-recording-checks',splitting.length?splitting:[{id:'splitting',title:'',status:'pending',detail:'Connect to OBS to check this setting.'}]);
  checksInto('wizard-extra-checks',checks.filter(c=>['obs_check','connection'].includes(c.id)&&c.status==='fail'));
  const folder=checks.find(c=>c.id==='directory');
  $('folder-title').textContent=folder?.status==='pass'?'1. Recording folder ✓':'1. Set recording folder';
  $('folder-help').textContent=folder?.status==='pass'?'Ready.':report.outputs_active?'Stop recording, streaming and replay buffer in OBS, then click Check again.':state.connected?'Click Change folder to let the companion save temporary footage here.':'Connect to OBS first.';
  $('folder-destination').hidden=folder?.status==='pass';
  $('recording-recheck').disabled=wizardBusy||report.checking||!state.connected;
  checksInto('wizard-media-checks',checks.filter(c=>c.id==='media_tools'&&c.status==='fail'));
  checksInto('wizard-scene-checks',checks.filter(c=>c.id==='scene'&&c.status!=='pass'));
  checksInto('wizard-log-checks',checks.filter(c=>c.id==='logs'&&c.status==='fail'));
  renderInstallations();
  $('wizard-apply').hidden=checks.some(c=>c.id==='directory'&&c.status==='pass');
  $('wizard-apply').disabled=wizardBusy||!state.connected||report.checking||report.outputs_active||!report.recording_directory;
  for(const id of ['wizard-recheck','wizard-save-logs','forget-password'])$(id).disabled=wizardBusy;
  $('wizard-next').disabled=wizardBusy;
  $('wizard-recheck').disabled=wizardBusy||report.checking;
  $('wizard-recheck').textContent=wizardBusy?'Checking…':'Check again';
  $('wizard-finish').disabled=wizardBusy||report.checking;
  if(!wizardAutoOpened){
    if(!report.checking&&!state.setup_completed){wizardAutoOpened=true;openWizard();}
    else if(!report.checking)wizardAutoOpened=true;
  }
}
$('settings-wizard').onclick=openWizard;
$('wizard-close').onclick=closeWizard;
document.querySelectorAll('[data-step]').forEach(button=>button.onclick=()=>wizardPage(Number(button.dataset.step)));
$('wizard-back').onclick=()=>wizardPage(wizardStep-1);
$('wizard-next').onclick=()=>{
  if(wizardBusy)return;
  if(wizardStep===0)$('wizard-connect').requestSubmit();
  else wizardPage(wizardStep+1);
};
$('get-obs').onclick=wizardRun(()=>action('open_obs_download'));
async function saveWizardPort(){
  const port=Number($('wizard-port').value);
  if(!Number.isInteger(port)||port<1||port>65535)throw Error('Enter a port between 1 and 65535.');
  if(port!==state.config.obs_port){const config=structuredClone(state.config);config.obs_port=port;await action('save_config',{config});configLoaded=false;return true;}
  return false;
}
$('wizard-connect').onsubmit=wizardRun(async()=>{
  const changed=await saveWizardPort();
  if(changed||!state.connected||$('wizard-password').value){
    await action('connect',{password:$('wizard-password').value,remember:$('remember-password').checked,use_saved:$('remember-password').checked});
    $('wizard-password').value='';
  }else await action('check_setup');
  const response=await fetch('/api/status');
  if(!response.ok)throw Error('Could not check the OBS connection. Try again.');
  state=await response.json();
  if(!state.connected)throw Error('OBS is not connected. Check the port and password, then try again.');
  renderSetup();
  if(wizardStep===0)wizardPage(1);
});
$('forget-password').onclick=wizardRun(async()=>{await saveWizardPort();await action('forget_password');$('wizard-password').value='';});
$('recording-recheck').onclick=wizardRun(()=>action('check_setup'));
$('wizard-apply').onclick=wizardRun(()=>action('configure_output'));
$('wizard-logs').onsubmit=wizardRun(async()=>{
  const name=$('wizard-install-name').value.trim(),logs=$('wizard-log-path').value.trim();
  if(!name||!logs)throw Error('Enter an installation name and its Logs directory.');
  const config=structuredClone(state.config);
  if(config.installations.some(i=>i.name===name))throw Error('That installation name already exists. Edit it in Setup or choose another name.');
  config.installations.push({name,logs,flavour:$('wizard-flavour').value,minimum_key:0,difficulties:[],categories:kinds.filter(k=>k!=='manual')});
  await saveInstallations(config);$('wizard-log-path').value='';$('manual-installation').open=false;
});
$('wizard-manual-only').onchange=renderSetup;
$('wizard-finish').onclick=wizardRun(async()=>{
  const report=state.setup||{},checks=report.checks||[];
  let missing;
  if(!state.connected||report.connection!=='connected')missing=[0,'Connect to OBS to finish setup.'];
  else if(checks.some(c=>['directory','splitting','outputs','obs_check','connection'].includes(c.id)&&c.status==='fail'))missing=[1,'Check the recording settings below.'];
  else if(checks.some(c=>c.id==='media_tools'&&c.status==='fail'))missing=[2,'Install the missing media tools shown below.'];
  else if(checks.some(c=>c.id==='logs'&&c.status==='fail')||(!state.config.installations.length&&!$('wizard-manual-only').checked))missing=[3,'Add an installation or select manual clips only.'];
  if(missing){wizardPage(missing[0]);throw Error(missing[1]);}
  await action('finish_setup',{manual_only:$('wizard-manual-only').checked});
  closeWizard();show('library');
});

async function saveInstallations(config){
  await action('save_config',{config});configLoaded=false;
  const response=await fetch('/api/status');if(!response.ok)throw Error('Could not refresh installations.');
  state=await response.json();$('wizard-manual-only').checked=false;renderSetup();
}
async function findInstallations(folder){
  if(discovering)return;
  discoveryStarted=true;discovering=true;
  $('discovery-status').textContent='Looking for WoW installations…';renderInstallations();
  try{
    const result=await action('discover_installations',folder?{folder}:{});
    discovered=result.installations;
    $('discovery-status').textContent=discovered.length?message('discovery.found',{count:discovered.length}):'No installations found here. Search your game folder or add one manually.';
    if(result.limited)$('discovery-status').textContent+=' Search another folder if yours is missing.';
  }catch(error){$('discovery-status').textContent=error.message;}
  finally{discovering=false;renderInstallations();}
}
function renderInstallations(){
  if(!state.config)return;
  $('find-installations').disabled=discovering||wizardBusy;
  $('discovery-status').hidden=!discovering&&discovered.length>0&&discovered.every(candidate=>state.config.installations.some(i=>i.logs===candidate.logs));
  const key=JSON.stringify([state.config.installations,discovered,wizardBusy,discovering]);
  if(key===installationRenderKey)return;installationRenderKey=key;
  $('wizard-installations').replaceChildren();$('discovered-installations').replaceChildren();
  for(const install of state.config.installations){
    const row=document.createElement('div');row.className='installation-row';
    const text=document.createElement('div'),title=document.createElement('strong'),path=document.createElement('small');
    title.textContent=`✓ ${install.name}`;path.textContent=install.logs;text.append(title,path);
    const remove=document.createElement('button');remove.textContent='Remove';remove.disabled=wizardBusy;
    remove.onclick=wizardRun(async()=>{const config=structuredClone(state.config);config.installations=config.installations.filter(i=>i.name!==install.name);await saveInstallations(config);});
    row.append(text,remove);$('wizard-installations').append(row);
  }
  for(const candidate of discovered){
    const added=state.config.installations.some(i=>i.logs===candidate.logs);
    if(added)continue;
    const row=document.createElement('div');row.className='installation-row';
    const text=document.createElement('div'),title=document.createElement('strong'),path=document.createElement('small');
    title.textContent=candidate.name;path.textContent=candidate.logs;text.append(title,path);
    if(!candidate.available){const hint=document.createElement('p');hint.textContent='Open this game and enable /combatlog, then search again.';text.append(hint);}
    const add=document.createElement('button');add.textContent=added?'Added':'Add';add.disabled=added||!candidate.available||wizardBusy||discovering;
    add.onclick=wizardRun(async()=>{
      const config=structuredClone(state.config);let name=candidate.name,index=2;
      while(config.installations.some(i=>i.name===name))name=`${candidate.name} (${index++})`;
      config.installations.push({name,logs:candidate.logs,flavour:candidate.flavour,minimum_key:0,difficulties:[],categories:kinds.filter(k=>k!=='manual')});
      await saveInstallations(config);
    });
    row.append(text,add);$('discovered-installations').append(row);
  }
}
$('find-installations').onclick=()=>findInstallations();
$('search-installations').onsubmit=event=>{event.preventDefault();findInstallations($('discovery-folder').value.trim());};

renderSetup();

$('connection').onclick=()=>{$('connection').setAttribute('aria-expanded',String($('connection').getAttribute('aria-expanded')!=='true'));};
document.addEventListener('click',event=>{if(!event.target.closest('.obs-status'))$('connection').setAttribute('aria-expanded','false');});
document.addEventListener('keydown',event=>{if(event.key==='Escape')$('connection').setAttribute('aria-expanded','false');});
