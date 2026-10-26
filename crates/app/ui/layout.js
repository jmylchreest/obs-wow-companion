"use strict";
const libraryWorkspace=$('recording-workspace'),paneDivider=$('pane-divider'),paneToggle=$('collapse-recordings');
let libraryLayoutLoaded=false,libraryWidth=320,libraryCollapsed=false,layoutWrite=Promise.resolve(),layoutTimer;
const narrowWorkspace=window.matchMedia('(max-width:700px)');
function maximumLibraryWidth(){return Math.max(240,Math.min(720,libraryWorkspace.clientWidth-361));}
function applyLibraryLayout(){
  const width=Math.max(240,Math.min(libraryWidth,maximumLibraryWidth()));
  libraryWorkspace.style.setProperty('--library-width',width+'px');
  libraryWorkspace.classList.toggle('library-collapsed',libraryCollapsed);
  paneToggle.setAttribute('aria-expanded',String(!libraryCollapsed));
  paneToggle.title=libraryCollapsed?'Show recordings':'Collapse recordings';paneToggle.setAttribute('aria-label',paneToggle.title);
  paneDivider.tabIndex=libraryCollapsed||narrowWorkspace.matches?-1:0;
  paneDivider.setAttribute('aria-valuemin','240');paneDivider.setAttribute('aria-valuemax',String(maximumLibraryWidth()));
  paneDivider.setAttribute('aria-valuenow',String(Math.round(width)));paneDivider.setAttribute('aria-valuetext',Math.round(width)+' pixels');
  paneDivider.setAttribute('aria-disabled',String(libraryCollapsed));
}
function renderLibraryLayout(){
  if(!libraryLayoutLoaded&&state.config){
    libraryWidth=state.config.recordings_width||320;libraryCollapsed=!!state.config.recordings_collapsed;libraryLayoutLoaded=true;applyLibraryLayout();
  }
}
function saveLibraryLayout(){
  clearTimeout(layoutTimer);
  const width=Math.round(libraryWidth),collapsed=libraryCollapsed;
  layoutWrite=layoutWrite.catch(()=>{}).then(()=>action('set_library_layout',{width,collapsed})).catch(error);
}
paneToggle.onclick=()=>{libraryCollapsed=!libraryCollapsed;applyLibraryLayout();saveLibraryLayout();};
let resizePointer=null;
paneDivider.onpointerdown=event=>{
  if(event.button!==0||libraryCollapsed||narrowWorkspace.matches)return;
  event.preventDefault();resizePointer=event.pointerId;paneDivider.setPointerCapture(resizePointer);document.body.classList.add('resizing-panes');
};
paneDivider.onpointermove=event=>{
  if(event.pointerId!==resizePointer)return;
  libraryWidth=Math.round(Math.max(240,Math.min(event.clientX-libraryWorkspace.getBoundingClientRect().left,maximumLibraryWidth())));applyLibraryLayout();
};
function finishPaneResize(event){
  if(event.pointerId!==resizePointer)return;
  resizePointer=null;document.body.classList.remove('resizing-panes');saveLibraryLayout();
}
paneDivider.onpointerup=paneDivider.onpointercancel=paneDivider.onlostpointercapture=finishPaneResize;
paneDivider.ondblclick=()=>{if(libraryCollapsed)return;libraryWidth=320;applyLibraryLayout();saveLibraryLayout();};
paneDivider.onkeydown=event=>{
  if(libraryCollapsed||narrowWorkspace.matches)return;
  const step=event.shiftKey?50:10;
  const width=event.key==='ArrowLeft'?libraryWidth-step:event.key==='ArrowRight'?libraryWidth+step:event.key==='Home'?240:event.key==='End'?maximumLibraryWidth():null;
  if(width===null)return;
  event.preventDefault();libraryWidth=Math.round(Math.max(240,Math.min(width,maximumLibraryWidth())));applyLibraryLayout();
  clearTimeout(layoutTimer);layoutTimer=setTimeout(saveLibraryLayout,250);
};
new ResizeObserver(applyLibraryLayout).observe(libraryWorkspace);
narrowWorkspace.addEventListener('change',applyLibraryLayout);
applyLibraryLayout();

function showSettingsCategory(category,focus=false){
  for(const button of document.querySelectorAll('[data-settings-tab]')){
    const selected=button.dataset.settingsTab===category;
    button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;
    $(button.getAttribute('aria-controls')).hidden=!selected;
    if(selected&&focus)button.focus();
  }
  document.querySelector('.settings-scroll').scrollTop=0;
}
for(const button of document.querySelectorAll('[data-settings-tab]'))button.onclick=()=>showSettingsCategory(button.dataset.settingsTab);
$('settings-nav').onkeydown=event=>{
  const buttons=[...document.querySelectorAll('[data-settings-tab]')],index=buttons.indexOf(document.activeElement);
  const next=event.key==='ArrowDown'?(index+1)%buttons.length:event.key==='ArrowUp'?(index+buttons.length-1)%buttons.length:event.key==='Home'?0:event.key==='End'?buttons.length-1:null;
  if(next!==null){event.preventDefault();showSettingsCategory(buttons[next].dataset.settingsTab,true);}
};
$('settings-form').addEventListener('invalid',event=>{
  const section=event.target.closest('[role="tabpanel"]');
  if(section)showSettingsCategory(section.id.replace('settings-',''));
},true);
