'use strict';
const finopsState={data:null,view:'overview',run:null,workflow:'',coverageOpen:false};
try{finopsState.coverageOpen=localStorage.getItem('nodo.finops.coverageOpen')==='true'}catch{}
const fnum=value=>value===null||value===undefined?'—':new Intl.NumberFormat(nodoLocale(),{maximumFractionDigits:0}).format(value);
const fpercent=value=>new Intl.NumberFormat(nodoLocale(),{style:'percent',minimumFractionDigits:1,maximumFractionDigits:1}).format(value);
const fusd=value=>value===null||value===undefined?'—':new Intl.NumberFormat(nodoLocale(),{style:'currency',currency:'USD',minimumFractionDigits:2,maximumFractionDigits:4}).format(value);
function ftime(value){if(value===null||value===undefined)return '—';if(value<1000)return Math.round(value)+' ms';const s=Math.floor(value/1000);return s<60?s+' s':s<3600?Math.floor(s/60)+' m '+s%60+' s':Math.floor(s/3600)+' h '+Math.floor(s%3600/60)+' m'}
function finopsCard(label,value,note=''){return `<div class="finops-card"><span class="small">${esc(uiText(label))}</span><strong>${esc(value)}</strong>${note?`<small>${esc(uiText(note))}</small>`:''}</div>`}
function finopsKnown(m){const values=[m.usd,m.estimated_usd].filter(v=>typeof v==='number'&&Number.isFinite(v));return values.length?values.reduce((a,b)=>a+b,0):null}
function finopsCost(m){return `${esc(fusd(m.usd))} <small>${esc(uiText('riportati'))}</small><br>${esc(fusd(m.estimated_usd))} <small>${esc(uiText('stimati'))}</small>`}
function finopsPricing(p){if(!p?.available)return `<p class="note">${esc(uiText('Tariffe di riferimento non disponibili: le stime senza tariffa restano mancanti.'))}</p>`;return `<p class="note"><a href="https://openrouter.ai/api/v1/models" target="_blank" rel="noopener noreferrer">${esc(uiText('Tariffe di riferimento'))}</a> · <span>${esc(uiText('Tariffe acquisite il:'))}</span> <strong data-no-i18n>${esc(new Date(p.fetched_at*1000).toLocaleString(nodoLocale()))}</strong>${p.stale?' · '+esc(uiText('Aggiornamento tariffe non riuscito: uso l’ultima copia disponibile, acquisita nella data indicata.')):''}${p.persisted===false?' · '+esc(uiText('Salvataggio delle tariffe non riuscito: questa copia è disponibile solo fino al riavvio.')):''}</p>`}
function finopsCostCards(m){return `${finopsCard('Costo LLM riportato',fusd(m.usd),'USD · dati disponibili')}${finopsCard('Costo LLM stimato',fusd(m.estimated_usd),'Equivalente API · tariffe di riferimento')}${finopsCard('Totale noto con stime',fusd(finopsKnown(m)),'Costi riportati + stimati · copertura parziale')}`}
function finopsMetricCards(m){return `<div class="finops-grid">${finopsCostCards(m)}${finopsCard('Token osservati',fnum(m.records?m.total:null),'Input + output, cache inclusa una sola volta')}${finopsCard('Chiamate LLM note',fnum(m.llm_calls))}${finopsCard('Invocazioni tool note',fnum(m.records?m.tools:null))}${finopsCard('Token input',fnum(m.records?m.input:null))}${finopsCard('Token output',fnum(m.records?m.output:null))}${finopsCard('Cache letta',fnum(m.cache_read))}${finopsCard('Cache scritta',fnum(m.cache_write))}${finopsCard('Reasoning token',fnum(m.reasoning))}${finopsCard('Tempo risposte LLM',ftime(m.llm_ms))}${finopsCard('Tempo turni agente',ftime(m.agent_ms))}${finopsCard('Costo ripetizioni riportato',fusd(m.repeated_usd))}</div>`}
function finopsModelName(row){
 return row.model===null ? row.runtime+' · '+uiText('Modello non attribuito') : row.name;
}
// "3 giorni fa" / "3 days ago", from seconds up to years; a missing date stays missing.
function finopsAgo(ms){
 if(typeof ms!=='number'||!Number.isFinite(ms)||ms<=0)return '—';
 const seconds=Math.max(0,Math.floor((Date.now()-ms)/1000));
 const [value,unit]=seconds<60?[seconds,'second']:seconds<3600?[Math.floor(seconds/60),'minute']:seconds<86400?[Math.floor(seconds/3600),'hour']:seconds<2592000?[Math.floor(seconds/86400),'day']:seconds<31536000?[Math.floor(seconds/2592000),'month']:[Math.floor(seconds/31536000),'year'];
 const date=new Date(ms);
 return `<time datetime="${esc(date.toISOString())}" title="${esc(date.toLocaleString(nodoLocale()))}" data-no-i18n>${esc(new Intl.RelativeTimeFormat(nodoLocale(),{numeric:'always'}).format(-value,unit))}</time>`;
}
// `when` adds a last column with a relative date: {label, at: row => milliseconds or null}.
function finopsRows(rows,onClick='',extended=false,when=null){
 const labels=['Nome','Chiamate LLM note','Token input / output','Cache letta',
  ...(extended?['Cache scritta','Reasoning token']:[]),'Costi LLM riportati / stimati','Tempo risposte LLM',...(when?[when.label]:[])];
 const body=rows.map(r=>{
  const m=r.metrics;
  const name=`<span data-no-i18n>${esc(finopsModelName(r))}</span>`;
  const label=onClick?`<button class="text-button" data-finops-workflow="${esc(r.id)}">${name}</button>`:name;
  const runs=r.runs===undefined?'':`<small class="finops-row-note">${fnum(r.completed)} / ${fnum(r.runs)} ${esc(uiText('run completati'))}</small>`;
  const extra=extended?`<td>${fnum(m.cache_write)}</td><td>${fnum(m.reasoning)}</td>`:'';
  return `<tr><td>${label}${runs}</td><td>${fnum(m.llm_calls)}</td><td>${m.records?fnum(m.input)+' / '+fnum(m.output):'—'}</td><td>${fnum(m.cache_read)}</td>${extra}<td>${finopsCost(m)}</td><td>${ftime(m.llm_ms)}</td>${when?`<td>${finopsAgo(when.at(r))}</td>`:''}</tr>`;
 }).join('');
 return `<div class="finops-table"><table><thead><tr>${labels.map(l=>`<th scope="col">${esc(uiText(l))}</th>`).join('')}</tr></thead><tbody>${body}</tbody></table>${rows.length?'':'<p class="note">Nessun dato disponibile.</p>'}</div>`;
}
function finopsCostGroups(values,limit){
 if(values.length<=limit+1)return values;
 const others=values.slice(limit);
 return [...values.slice(0,limit),{name:uiText('Altri'),value:others.reduce((sum,row)=>sum+row.value,0),others}];
}
function finopsGroupLabel(row,panel){
 if(!row.others?.length||!panel)return `<span data-no-i18n>${esc(row.label||row.name)}</span>`;
 return `<button type="button" class="text-button finops-other-toggle" aria-expanded="false" aria-controls="${panel}" data-finops-other data-no-i18n>${esc(row.name)} (${fnum(row.others.length)})</button>`;
}
function finopsOtherPanel(rows,id,content){
 const others=rows.at(-1)?.others;
 return others?.length?`<div id="${id}" class="finops-other-panel" hidden>${content(others)}</div>`:'';
}
// The first agents are listed; the rest stay behind one more row with their count and total, so a long tail does not fill the card.
const finopsOtherAgentsShown=10;
function finopsOtherAgents(agents){
 const row=agent=>`<details class="finops-agent-models">
  <summary><span data-no-i18n>${esc(agent.name)}</span> · <b>${esc(fusd(agent.value))}</b></summary>
  ${finopsRows(agent.models||[])}
 </details>`;
 const rest=agents.slice(finopsOtherAgentsShown);
 const more=rest.length?`<details class="finops-agent-models finops-agent-more">
  <summary><span data-no-i18n>${esc(fnum(rest.length))} ${esc(uiText('altri agenti'))}</span> · <b>${esc(fusd(rest.reduce((sum,agent)=>sum+agent.value,0)))}</b></summary>
  ${rest.map(row).join('')}
 </details>`:'';
 return agents.slice(0,finopsOtherAgentsShown).map(row).join('')+more;
}
const finopsCreated={label:'Creato',at:row=>row.created?row.created*1000:null};
const finopsFirstUsed={label:'Primo utilizzo',at:row=>row.metrics?.first_at??null};
const finopsChartColors=['#8b7bf1','#38bfa7','#eeae54','#5aa9e6','#e17ca6','#9aac6e','#b795d5'];
function finopsChartCard(title,content,note=''){return `<section class="finops-chart"><h3>${esc(uiText(title))}</h3>${content}${note?`<p class="small">${esc(uiText(note))}</p>`:''}</section>`}
function finopsChartEmpty(zero=false){return `<p class="finops-chart-empty">${esc(uiText(zero?'Il valore riportato è zero.':'Nessun dato disponibile.'))}</p>`}
function finopsChartValues(rows){return rows.filter(r=>typeof r.value==='number'&&Number.isFinite(r.value)&&r.value>=0).sort((a,b)=>b.value-a.value)}
function finopsChartDonut(rows,title){
 const values=finopsChartValues(rows),total=values.reduce((n,r)=>n+r.value,0);if(!total)return finopsChartCard(title,finopsChartEmpty(values.length>0));
 const slices=finopsCostGroups(values,5);
 const panel='finops-other-models';
 let offset=0;
 const circles=slices.filter(s=>s.value>0).map((s,i)=>{const percent=100*s.value/total,tip=s.name+' · '+fusd(s.value)+' · '+fpercent(s.value/total),part=`<circle cx="100" cy="100" r="68" pathLength="100" fill="none" stroke="${finopsChartColors[i%finopsChartColors.length]}" stroke-width="24" stroke-dasharray="${percent} ${100-percent}" stroke-dashoffset="${-offset}" transform="rotate(-90 100 100)" tabindex="0" role="img" aria-label="${esc(tip)}" data-tip="${esc(tip)}"><title>${esc(tip)}</title></circle>`;offset+=percent;return part}).join('');
 return finopsChartCard(title,`<div class="finops-donut-layout"><svg viewBox="0 0 200 200" role="img" aria-label="${esc(uiText(title))}"><title>${esc(uiText(title))}</title>${circles}<text x="100" y="98" text-anchor="middle" class="finops-donut-total">${esc(new Intl.NumberFormat(nodoLocale(),{maximumFractionDigits:2}).format(total))}</text><text x="100" y="119" text-anchor="middle" class="finops-chart-label">USD</text></svg><ul class="finops-chart-legend">${slices.map((s,i)=>`<li><i style="background:${finopsChartColors[i%finopsChartColors.length]}"></i>${finopsGroupLabel(s,panel)}<b>${fusd(s.value)}</b><small>${fpercent(s.value/total)}</small></li>`).join('')}</ul></div>${finopsOtherPanel(slices,panel,finopsRows)}`,'Costi riportati + stimati · copertura parziale');
}
function finopsChartBars(rows,title,format=fusd,expandAgents=false){
 const values=finopsChartValues(rows);if(!values.length)return finopsChartCard(title,finopsChartEmpty());const display=finopsCostGroups(values,8);
 const panel=expandAgents?'finops-other-agents':'';
 const max=Math.max(...display.map(r=>r.value),.000001);
 return finopsChartCard(title,`<ul class="finops-bars" aria-label="${esc(uiText(title))}">${display.map((r,i)=>`<li><div class="finops-bar-heading"><div class="finops-bar-name">${finopsGroupLabel(r,panel)}</div><b>${esc(format(r.value))}</b></div>${r.detail?`<div class="finops-bar-context" data-no-i18n>${esc(r.detail)}</div>`:''}<div class="finops-bar-track" aria-hidden="true"><span style="width:${100*r.value/max}%;background:${finopsChartColors[i%finopsChartColors.length]}"></span></div></li>`).join('')}</ul>${panel?finopsOtherPanel(display,panel,finopsOtherAgents):''}`,'Costi riportati + stimati; valori mancanti esclusi');
}
function finopsTokenParts(m){
 if(!m.records)return [];let parts=[];
 if(m.cache_read!==null&&m.cache_write!==null&&m.cache_read+m.cache_write<=m.input){parts.push({name:'Input restante',value:m.input-m.cache_read-m.cache_write},{name:'Cache letta',value:m.cache_read},{name:'Cache scritta',value:m.cache_write})}else parts.push({name:'Token input',value:m.input});
 if(m.reasoning!==null&&m.reasoning<=m.output)parts.push({name:'Output restante',value:m.output-m.reasoning},{name:'Reasoning token',value:m.reasoning});else parts.push({name:'Token output',value:m.output});
 return parts.filter(p=>p.value>0);
}
function finopsTokenColor(part){return {'Input restante':'#8b7bf1','Token input':'#8b7bf1','Cache letta':'#38bfa7','Cache scritta':'#eeae54','Output restante':'#5aa9e6','Token output':'#5aa9e6','Reasoning token':'#e17ca6'}[part.name]}
function finopsChartTokens(m,title='Composizione dei token'){
 const parts=finopsTokenParts(m),total=parts.reduce((n,p)=>n+p.value,0);if(!total)return finopsChartCard(title,finopsChartEmpty(m.records>0));let x=0;
 const segments=parts.map((p,i)=>{const width=640*p.value/total,tip=uiText(p.name)+' · '+fnum(p.value)+' · '+fpercent(p.value/total),segment=`<rect x="${x}" y="8" width="${width}" height="32" fill="${finopsTokenColor(p)}" tabindex="0" role="img" aria-label="${esc(tip)}" data-tip="${esc(tip)}"><title>${esc(tip)}</title></rect>`;x+=width;return segment}).join('');
 return finopsChartCard(title,`<svg viewBox="0 0 640 50" role="img" aria-label="${esc(uiText(title))}"><title>${esc(uiText(title))}</title>${segments}</svg><ul class="finops-chart-legend finops-token-legend">${parts.map((p,i)=>`<li><i style="background:${finopsTokenColor(p)}"></i><span>${esc(uiText(p.name))}</span><b>${fnum(p.value)}</b><small>${fpercent(p.value/total)}</small></li>`).join('')}</ul>`,'Cache e reasoning sono parti del totale, non consumi aggiuntivi');
}
function finopsMergeDays(runs){const days=new Map();for(const r of runs)for(const day of r.daily){const previous=days.get(day.at)??null,value=finopsKnown(day.metrics);days.set(day.at,typeof value==='number'&&Number.isFinite(value)?(previous??0)+value:previous)}return [...days].sort((a,b)=>a[0]-b[0]).map(([at,value])=>({at,value}))}
function finopsChartLine(days,title='Costi riportati e stimati nel tempo'){
 const points=days.filter(d=>typeof d.value==='number'&&Number.isFinite(d.value)&&d.value>=0).sort((a,b)=>a.at-b.at);if(!points.length)return finopsChartCard(title,finopsChartEmpty());
 const min=points[0].at,maxAt=points.at(-1).at,max=Math.max(...points.map(p=>p.value),.000001),x=at=>points.length===1?285:70+430*(at-min)/(maxAt-min||1),y=value=>178-148*value/max;let previous=null;
 const path=points.map(p=>{const type=previous!==null&&p.at-previous<=86400000?'L':'M';previous=p.at;return `${type}${x(p.at)},${y(p.value)}`}).join(' ');
 const date=at=>new Date(at).toLocaleDateString(nodoLocale(),{timeZone:'UTC',day:'2-digit',month:'short'});
 return finopsChartCard(title,`<svg class="finops-line-chart" viewBox="0 0 520 230" role="img" aria-label="${esc(uiText(title))}"><title>${esc(uiText(title))}</title>${[0,.5,1].map(f=>`<line x1="70" y1="${y(max*f)}" x2="500" y2="${y(max*f)}" class="finops-chart-grid"/><text x="62" y="${y(max*f)+4}" text-anchor="end" class="finops-chart-axis">${esc(fusd(max*f))}</text>`).join('')}<path d="${path}" class="finops-chart-line"/>${points.map((p,i)=>{const tip=new Date(p.at).toLocaleDateString(nodoLocale(),{timeZone:'UTC'})+' · '+fusd(p.value);return `<circle cx="${x(p.at)}" cy="${y(p.value)}" r="5" class="finops-chart-point" tabindex="0" role="img" aria-label="${esc(tip)}" data-tip="${esc(tip)}"><title>${esc(tip)}</title></circle>${i%Math.max(1,Math.ceil(points.length/5))===0||i===points.length-1?`<text x="${x(p.at)}" y="211" text-anchor="middle" class="finops-chart-axis" data-no-i18n>${esc(date(p.at))}</text>`:''}`}).join('')}</svg>`,points.length===1?'Un solo giorno disponibile: nessuna tendenza stimata':'Giorni UTC. I giorni senza dati non sono stimati');
}
function finopsOverviewCharts(d){return `<div class="finops-chart-grid-layout">${finopsChartDonut(d.models.map(r=>({...r,name:finopsModelName(r),value:finopsKnown(r.metrics)})),'Ripartizione costi per modello')}${finopsChartLine(finopsMergeDays([...d.runs,...(d.manager?[d.manager]:[])]))}${finopsChartTokens(d.summary.metrics)}${finopsChartBars(d.workflows.map(r=>({name:r.name,value:finopsKnown(r.metrics)})),'Confronto costi per workflow')}</div>`}

function finopsFamily(r,d){if(!r.parent&&!r.children?.length)return '';const ids=[...(r.parent?[r.parent.run]:[]),...(r.children||[]).map(c=>c.run)],family=ids.map(id=>d.runs.find(x=>x.id===id)).filter(Boolean);return `<section class="finops-family"><h3>Workflow collegati</h3><p class="note">I costi diretti sono conteggiati una sola volta nel globale. Il totale inclusivo comprende i workflow figli, con costi riportati e stimati.</p>${r.children?.length?finopsCard('Costo inclusivo dei workflow figli',fusd(r.tree_total_usd)):''}${family.map(c=>`<button type="button" class="child-run-link" data-finops-run="${esc(c.id)}">${icon('workflow')}<span data-no-i18n>${esc(c.name)} · ${esc(c.id.slice(0,8))}</span><b>${fusd(c.tree_total_usd)}</b></button>`).join('')}</section>`}
function finopsBody(){
 const d=finopsState.data,s=d.summary,m=s.metrics;let body='';
 const nav=`<div class="finops-heading"><div><span class="eyebrow">AGENT ECONOMICS</span><p class="small">Vista globale del tuo account · Workflow · Agenti · Manager · Run</p></div><button type="button" class="icon-button" data-finops-refresh aria-label="Aggiorna statistiche" data-tip="Aggiorna statistiche">${icon('rotate-cw')}</button></div><div class="tabs finops-tabs">${[['overview','Globale'],['workflows','Workflow'],['models','Agenti e modelli'],['manager','Manager'],['runs','Esecuzioni']].map(([id,label])=>`<button type="button" class="${finopsState.view===id?'active':''}" data-finops-view="${id}" aria-pressed="${finopsState.view===id}">${esc(uiText(label))}</button>`).join('')}</div>`;
 const coverage=finopsState.view==='overview'?`<details class="finops-coverage" data-finops-coverage${finopsState.coverageOpen?' open':''}><summary><b>${fnum(s.sessions_with_usage)} / ${fnum(s.sessions)}</b> ${esc(uiText('sessioni workflow con consumi disponibili'))} · <b>${fnum(m.costed_records)} / ${fnum(m.records)}</b> ${esc(uiText('record con costo riportato'))}</summary><p class="note">${esc(uiText('I costi reported sono quelli comunicati dagli agenti. I costi estimated sono stime dei consumi per cui manca un costo reported. I reported non includono i estimated: le due voci si sommano nel totale noto con stime, senza doppio conteggio. Le metriche mancanti sono indicate con — e non valgono zero.'))}</p><p class="small">${fnum(m.estimated_records)} ${esc(uiText('record con costo stimato'))} · ${fnum(m.approximate_records)} ${esc(uiText('stime approssimate da riepiloghi o cache incompleta'))} · ${fnum(m.historical_codex_records)} ${esc(uiText('richieste Codex recuperate dai log'))}</p>${finopsPricing(d.pricing)}</details>`:'';
 if(finopsState.view==='overview'){
  body=`${finopsOverviewCharts(d)}${finopsMetricCards(m)}<h3>Esiti ed esecuzione</h3><p class="note" data-finops-outcomes-note>${esc(uiText('Riepilogo degli stati e delle attività dei workflow. Le ripetizioni sono tentativi aggiuntivi dei passi; i cicli di rework indicano i ritorni previsti dal workflow per correggere o rivedere il lavoro.'))}</p><div class="finops-grid finops-outcomes">${finopsCard('Esecuzioni',fnum(s.runs))}${finopsCard('Completate',fnum(s.completed))}${finopsCard('In esecuzione',fnum(s.active))}${finopsCard('In errore',fnum(s.failed))}${finopsCard('Passaggi eseguiti',fnum(s.steps))}${finopsCard('Ripetizioni dei passi',fnum(s.repetitions))}${finopsCard('Cicli di rework',fnum(s.reworks))}${finopsCard('Decisioni umane',fnum(s.decisions))}</div><h3>Efficienza dei consumi osservati</h3><p class="note" data-finops-efficiency-note>${esc(uiText('Il costo medio divide la spesa nota di tutti i workflow per i run completati. Le quote mostrano il riuso della cache e il peso delle ripetizioni sui costi riportati; le attese misurano il tempo in attesa di interventi umani.'))}</p><div class="finops-grid">${finopsCard('Costo noto / run completato',s.completed&&finopsKnown(s.workflow_metrics||m)!==null?fusd(finopsKnown(s.workflow_metrics||m)/s.completed):'—','Costi riportati + stimati · copertura parziale')}${finopsCard('Quota cache letta',m.input&&m.cache_read!==null?fpercent(m.cache_read/m.input):'—')}${finopsCard('Quota costo delle ripetizioni',m.usd&&m.repeated_usd!==null?fpercent(m.repeated_usd/m.usd):'—','Ripetizione non significa spreco')}${finopsCard('Tempo attese umane',ftime(d.runs.reduce((n,r)=>n+r.human_wait_ms,0)))}</div><h3>Distribuzione per workflow</h3>${finopsRows(d.workflows,'workflows',false,finopsCreated)}`;
 }else if(finopsState.view==='workflows')body=finopsChartBars(d.workflows.map(r=>({name:r.name,value:finopsKnown(r.metrics)})),'Confronto costi per workflow')+finopsRows(d.workflows,'workflows',false,finopsCreated);
 else if(finopsState.view==='manager')body=d.manager?`<section data-finops-manager><h3>Manager</h3><p class="note">${esc(uiText('Costi della chat del project manager, già inclusi nel totale globale. Le voci reported e estimated si sommano nel totale noto con stime; non aggiungere nuovamente questo totale a quello globale.'))}</p><div class="finops-manager-layout"><div class="finops-manager-costs">${finopsCostCards(d.manager.metrics)}</div>${finopsChartLine(d.manager.daily.map(day=>({at:day.at,value:finopsKnown(day.metrics)})),'Costi del Manager nel tempo')}</div></section>`:'<p class="note">Nessun dato disponibile.</p>';
 else if(finopsState.view==='models')body=`<div class="finops-chart-grid-layout">${finopsChartDonut(d.models.map(r=>({...r,name:finopsModelName(r),value:finopsKnown(r.metrics)})),'Ripartizione costi per modello')}${finopsChartBars(d.agents.map(r=>({...r,value:finopsKnown(r.metrics)})),'Confronto costi per agente',fusd,true)}</div><h3>Modelli e runtime</h3>${finopsRows(d.models,'',false,finopsFirstUsed)}<h3>Agenti per workflow</h3>${finopsRows(d.agents,'',false,finopsFirstUsed)}`;
 else if(finopsState.run){
  const r=d.runs.find(r=>r.id===finopsState.run);if(!r){finopsState.run=null;return finopsBody()}
  body=`<button type="button" data-finops-back>${icon('arrow-left')}Tutte le esecuzioni</button><div class="finops-run-title"><h3 data-no-i18n>${esc(r.name)}</h3><span class="pill">${esc(states[r.status]||r.status)}</span><code>${esc(r.id)}</code></div><div class="finops-grid">${finopsCard('Passaggi eseguiti',fnum(r.steps))}${finopsCard('Ripetizioni dei passi',fnum(r.repetitions))}${finopsCard('Cicli di rework',fnum(r.reworks))}${finopsCard('Durata totale incluse attese',ftime(r.elapsed_ms))}${finopsCard('Tempo attese umane',ftime(r.human_wait_ms))}${finopsCard('Decisioni umane',fnum(r.decisions))}${finopsCard('Review approvate / rifiutate',r.approvals+' / '+r.rejections)}${finopsCard('Versioni artefatti',fnum(r.artifacts))}</div>${finopsMetricCards(r.metrics)}<h3>Costi per modello</h3>${finopsRows(r.models||[],'',true)}${finopsFamily(r,d)}<div class="finops-chart-grid-layout">${finopsChartBars(r.nodes.map(n=>({name:n.name,value:n.kind==='Workflow'?n.child_total_usd:finopsKnown(n.metrics)})),'Confronto costi per nodo')}${finopsChartTokens(r.metrics)}${finopsChartLine(r.daily.map(day=>({at:day.at,value:finopsKnown(day.metrics)})))}</div><h3>Dettaglio per nodo</h3><div class="finops-table"><table><thead><tr>${['Passo','Stato','Tentativi','Token input / output','Chiamate LLM note','Invocazioni tool note','Costi LLM riportati / stimati','Tempo turni agente'].map(l=>`<th>${esc(uiText(l))}</th>`).join('')}</tr></thead><tbody>${r.nodes.map(n=>`<tr><td><span data-no-i18n>${esc(n.name)}</span><small class="finops-row-note">${esc(n.kind)}</small></td><td>${esc(states[n.status]||n.status||'—')}</td><td>${n.attempts}</td><td>${n.metrics.records?fnum(n.metrics.input)+' / '+fnum(n.metrics.output):'—'}</td><td>${fnum(n.metrics.llm_calls)}</td><td>${n.metrics.records||n.metrics.agent_ms!==null?fnum(n.metrics.tools):'—'}</td><td>${finopsCost(n.metrics)}</td><td>${ftime(n.metrics.agent_ms)}</td></tr>`).join('')}</tbody></table></div>`;
 }else{
  const filtered=d.runs.filter(r=>!finopsState.workflow||r.workflow_id===finopsState.workflow);
  body=`${select('Workflow','finops_workflow',finopsState.workflow,[['','Tutti'],...d.workflows.map(w=>[w.id,w.name])])}<div class="finops-table"><table><thead><tr>${['Esecuzione','Stato','Passaggi / ripetizioni','Token osservati','Costi LLM riportati / stimati','Data di creazione'].map(l=>`<th>${esc(uiText(l))}</th>`).join('')}</tr></thead><tbody>${filtered.map(r=>`<tr><td><button type="button" class="text-button" data-finops-run="${r.id}"><span data-no-i18n>${esc(r.name)}</span></button><small class="finops-row-note" data-no-i18n>${r.id.slice(0,8)} · ${new Date(r.created*1000).toLocaleString(nodoLocale())}</small></td><td>${esc(states[r.status]||r.status)}</td><td>${r.steps} / ${r.repetitions}</td><td>${r.metrics.records?fnum(r.metrics.total):'—'}</td><td>${finopsCost(r.metrics)}</td><td data-no-i18n>${new Date(r.created*1000).toLocaleDateString(nodoLocale())}</td></tr>`).join('')}</tbody></table>${filtered.length?'':'<p class="note">Nessun dato disponibile.</p>'}</div>`;
 }
 return nav+coverage+body+`<details class="finops-limits"><summary>Copertura e interpretazione</summary><p>Input e output non duplicano i token di cache o reasoning. La somma in USD comprende solo i costi disponibili. Per i dati storici Pi e Codex, il nodo e il tentativo sono attribuiti tramite la timeline del run.</p><p>Le stime per richiesta includono cache e fasce di contesto. I riepiloghi aggregati usano le tariffe base e possono omettere scritture cache o maggiorazioni. I costi dei tool sono esclusi. Lo storico usa le tariffe correnti. I riepiloghi manager esistenti sono conservati quando non è possibile associarli con certezza ai log.</p><p>${esc(uiText('Per Claude, il tempo risposte LLM somma le durate API, inclusi i retry, escludendo i tool. Lo storico usa le differenze dei contatori di sessione associabili ai turni. Per Codex, i log disponibili non separano il tempo LLM dai tool: il valore resta —.'))}</p><p>La durata totale include le attese umane; il tempo dei turni agente è disponibile solo per le nuove esecuzioni. Le chiamate LLM e tool sono mostrate solo quando osservabili.</p><p>Costi di infrastruttura, tool a pagamento, budget, TAF e ROI non sono calcolati senza tariffe, token del prompt iniziale o baseline esplicite. Un workflow completato non attesta l'assenza di regressioni future.</p><p>${fnum(s.historical_sessions_unavailable)} ${esc(uiText('sessioni Pi storiche non recuperabili'))} · ${fnum(s.historical_codex_sessions_unavailable)} ${esc(uiText('sessioni Codex storiche non recuperabili'))}</p></details>`;
}
function renderFinops(){
 modal('FinOps',finopsBody(),`<span class="small">${esc(uiText('Aggiornato:'))} <span data-no-i18n>${new Date(finopsState.data.generated*1000).toLocaleTimeString(nodoLocale())}</span></span><div class="spacer"></div><button type="button" data-action="close-dialog">Chiudi</button>`,true);
 const coverage=$('[data-finops-coverage]');if(coverage)coverage.ontoggle=()=>{if(!coverage.isConnected)return;finopsState.coverageOpen=coverage.open;try{localStorage.setItem('nodo.finops.coverageOpen',String(coverage.open))}catch{}};
 $$('[data-finops-other]').forEach(button=>button.onclick=()=>{
  const panel=document.getElementById(button.getAttribute('aria-controls'));
  panel.hidden=!panel.hidden;
  button.setAttribute('aria-expanded',String(!panel.hidden));
 });
 $$('[data-finops-view]').forEach(b=>b.onclick=()=>{finopsState.view=b.dataset.finopsView;finopsState.run=null;renderFinops()});
 $$('[data-finops-workflow]').forEach(b=>b.onclick=()=>{finopsState.view='runs';finopsState.run=null;finopsState.workflow=b.dataset.finopsWorkflow;renderFinops()});
 $$('[data-finops-run]').forEach(b=>b.onclick=()=>{finopsState.run=b.dataset.finopsRun;renderFinops()});
 const back=$('[data-finops-back]');if(back)back.onclick=()=>{finopsState.run=null;renderFinops()};
 const filter=$('[name="finops_workflow"]');if(filter)filter.onchange=()=>{finopsState.workflow=filter.value;renderFinops()};
 $('[data-finops-refresh]').onclick=async e=>{const button=e.currentTarget;button.disabled=true;try{finopsState.data=await api('/finops');if($('#dialog h2')?.textContent==='FinOps')renderFinops()}catch(error){toast(error.message)}finally{if(button.isConnected)button.disabled=false}};
}
$('#finops').onclick=async()=>{finopsState.data=null;const owner=app.user?.id;modal('FinOps','<p role="status">Caricamento statistiche…</p>');try{const data=await api('/finops');if(app.user?.id!==owner||$('#app').hidden)return;finopsState.data=data;finopsState.view='overview';finopsState.run=null;finopsState.workflow='';if($('#dialog h2')?.textContent==='FinOps'&&$('#dialog').open)renderFinops()}catch(e){toast(e.message);if($('#dialog h2')?.textContent==='FinOps')closeModal()}};

function refreshFinopsLanguage(){if(finopsState.data&&$('#dialog').open&&$('#dialog h2')?.textContent==='FinOps')renderFinops()}
window.addEventListener('storage',e=>{if(e.key==='nodo.language')refreshFinopsLanguage()});
document.addEventListener('change',e=>{if(e.target.matches('[data-language]'))refreshFinopsLanguage()});
