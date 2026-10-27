"use strict";
const dateDialog=$('date-range-dialog');
let rangeDraftStart='',rangeDraftEnd='',calendarMonth=new Date();
function calendarDate(date){return `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`;}
function calendarParse(value){return new Date(value+'T12:00:00');}
function rangeDateLabel(value){return calendarParse(value).toLocaleDateString(uiLocale,{day:'numeric',month:'short',year:'numeric'});}
function updateDateRangeLabel(){
  const from=$('date-from').value,to=$('date-to').value;
  $('date-range-label').textContent=from&&to?(from===to?rangeDateLabel(from):message('date.range',{from:rangeDateLabel(from),to:rangeDateLabel(to)})):from?message('date.from',{date:rangeDateLabel(from)}):to?message('date.until',{date:rangeDateLabel(to)}):message('date.all');
  $('date-range-reset').hidden=!from&&!to;
}
function closeDateRange(){dateDialog.close();$('date-range-open').focus();}
function clearDateRange(){
  $('date-from').value='';$('date-to').value='';updateDateRangeLabel();renderList();
  if(dateDialog.open)closeDateRange();else $('date-range-open').focus();
}
function renderCalendar(){
  const year=calendarMonth.getFullYear(),month=calendarMonth.getMonth();
  $('date-month').textContent=calendarMonth.toLocaleDateString(uiLocale,{month:'long',year:'numeric'});
  const first=new Date(year,month,1,12);const weekStart=weekInfo().firstDay%7;first.setDate(1-(first.getDay()-weekStart+7)%7);
  const weekdays=document.querySelector('.calendar-weekdays');weekdays.replaceChildren();
  for(let i=0;i<7;i++){const cell=document.createElement('span'),day=new Date(2024,0,7+weekStart+i,12);cell.textContent=new Intl.DateTimeFormat(uiLocale,{weekday:'short'}).format(day);weekdays.append(cell);}
  const today=calendarDate(new Date()),grid=$('date-calendar');grid.replaceChildren();
  for(let i=0;i<42;i++){
    const date=new Date(first);date.setDate(first.getDate()+i);const value=calendarDate(date),button=document.createElement('button');
    button.type='button';button.dataset.date=value;button.textContent=formatNumber(date.getDate(),0);
    button.setAttribute('aria-label',date.toLocaleDateString(uiLocale,{weekday:'long',day:'numeric',month:'long',year:'numeric'}));
    const endpoint=value===rangeDraftStart||value===rangeDraftEnd;
    button.setAttribute('aria-pressed',String(endpoint));
    button.classList.toggle('other-month',date.getMonth()!==month);
    button.classList.toggle('in-range',!!rangeDraftStart&&!!rangeDraftEnd&&value>rangeDraftStart&&value<rangeDraftEnd);
    if(value===today)button.setAttribute('aria-current','date');
    button.onclick=()=>{
      if(!rangeDraftStart||rangeDraftEnd){rangeDraftStart=value;rangeDraftEnd='';}
      else {rangeDraftEnd=value;if(rangeDraftEnd<rangeDraftStart)[rangeDraftStart,rangeDraftEnd]=[rangeDraftEnd,rangeDraftStart];}
      renderCalendar();grid.querySelector(`[data-date="${value}"]`)?.focus();
    };
    grid.append(button);
  }
  $('date-range-prompt').textContent=rangeDraftEnd?`${rangeDateLabel(rangeDraftStart)} – ${rangeDateLabel(rangeDraftEnd)}`:rangeDraftStart?message('date.end'):message('date.start');
  $('date-range-apply').disabled=!rangeDraftStart||!rangeDraftEnd;
}
$('date-range-open').onclick=()=>{
  rangeDraftStart=$('date-from').value;rangeDraftEnd=$('date-to').value;
  const anchor=rangeDraftStart?calendarParse(rangeDraftStart):new Date();calendarMonth=new Date(anchor.getFullYear(),anchor.getMonth(),1,12);
  renderCalendar();dateDialog.showModal();
  $('date-calendar').querySelector(`[data-date="${calendarDate(anchor)}"]`)?.focus();
};
$('date-month-prev').onclick=()=>{calendarMonth.setMonth(calendarMonth.getMonth()-1);renderCalendar();};
$('date-month-next').onclick=()=>{calendarMonth.setMonth(calendarMonth.getMonth()+1);renderCalendar();};
$('date-calendar').onkeydown=event=>{
  const value=event.target.dataset.date;if(!value)return;
  const offset={ArrowLeft:-1,ArrowRight:1,ArrowUp:-7,ArrowDown:7}[event.key];if(offset===undefined)return;
  event.preventDefault();const date=calendarParse(value);date.setDate(date.getDate()+offset);
  calendarMonth=new Date(date.getFullYear(),date.getMonth(),1,12);renderCalendar();
  $('date-calendar').querySelector(`[data-date="${calendarDate(date)}"]`)?.focus();
};
$('date-range-apply').onclick=()=>{
  if(!rangeDraftStart||!rangeDraftEnd)return;
  $('date-from').value=rangeDraftStart;$('date-to').value=rangeDraftEnd;updateDateRangeLabel();renderList();closeDateRange();
};
$('date-range-reset').onclick=clearDateRange;$('date-range-all').onclick=clearDateRange;$('date-range-cancel').onclick=closeDateRange;
dateDialog.addEventListener('cancel',event=>{event.preventDefault();closeDateRange();});
function outsideCalendar(event){const r=dateDialog.getBoundingClientRect();return event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom;}
let dateBackdropDown=false;
dateDialog.addEventListener('pointerdown',event=>{dateBackdropDown=outsideCalendar(event);});
dateDialog.addEventListener('pointerup',event=>{if(dateBackdropDown&&outsideCalendar(event))closeDateRange();dateBackdropDown=false;});
updateDateRangeLabel();
