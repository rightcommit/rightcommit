const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('web/finops.js','utf8');
const context={Intl,Date,uiText:s=>s,nodoLocale:()=> 'it-IT',esc:s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))};
vm.createContext(context);vm.runInContext(source.slice(0,source.indexOf('function finopsBody()')),context);
const m={records:1,input:100,output:20,cache_read:30,cache_write:40,reasoning:10};
const parts=context.finopsTokenParts(m);assert.equal(parts.reduce((n,p)=>n+p.value,0),120);assert.equal(parts.find(p=>p.name==='Input restante').value,30);
const unknown=context.finopsTokenParts({...m,cache_read:null,cache_write:null,reasoning:null});assert.equal(unknown.length,2);assert.equal(unknown.reduce((n,p)=>n+p.value,0),120);
const zero=context.finopsChartDonut([{name:'free',value:0},{name:'unknown',value:null}],'Cost');assert(zero.includes('Il valore riportato è zero.'));assert(!zero.includes('<circle'));
const missing=context.finopsChartDonut([{name:'unknown',value:null}],'Cost');assert(missing.includes('Nessun dato disponibile.'));
const single=context.finopsChartDonut([{name:'one',value:10}],'Cost');assert(single.includes('stroke-dasharray="100 0"'));assert(!single.includes('NaN'));
const hostile=context.finopsChartBars([{name:'<script>alert(1)</script>',value:2},{name:'unknown',value:null}],'Cost');assert(!hostile.includes('<script>'));assert(hostile.includes('&lt;script&gt;'));
const days=context.finopsMergeDays([{daily:[{at:0,metrics:{usd:2}},{at:86400000,metrics:{usd:null}}]},{daily:[{at:0,metrics:{usd:3}}]}]);assert.equal(days[0].value,5);assert.equal(days[1].value,null);
const one=context.finopsChartLine([{at:0,value:5}]);assert(one.includes('Un solo giorno disponibile'));assert(!one.includes('L285'));
const gap=context.finopsChartLine([{at:0,value:5},{at:3*86400000,value:3}]);const path=gap.match(/<path d="([^"]+)"/)[1];assert.equal((path.match(/M/g)||[]).length,2);assert(!path.includes('L'));
for(const html of [single,hostile,one,gap,context.finopsChartTokens(m)])assert(!/NaN|Infinity/.test(html));
console.log('PASS: token partition, unknown/zero distinction, single-category donut, escaping, daily totals, one-day chart, missing-day gaps, finite SVG geometry');
assert.equal(context.finopsKnown({usd:null,estimated_usd:3}),3);
assert.equal(context.finopsKnown({usd:2,estimated_usd:3}),5);
assert.equal(context.finopsKnown({usd:null,estimated_usd:null}),null);
assert.equal(context.finopsKnown({usd:0,estimated_usd:null}),0);
const costs=context.finopsCost({usd:2,estimated_usd:3});assert(costs.includes('riportati'));assert(costs.includes('stimati'));
const estimatedDays=context.finopsMergeDays([{daily:[{at:0,metrics:{usd:null,estimated_usd:3}}]},{daily:[{at:0,metrics:{usd:2,estimated_usd:null}}]}]);assert.equal(estimatedDays[0].value,5);
assert(context.finopsMetricCards({records:0,usd:null,estimated_usd:3}).includes('Costo LLM stimato'));
assert(context.finopsPricing({available:false}).includes('Tariffe di riferimento non disponibili'));
assert(context.finopsPricing({available:true,stale:true,fetched_at:1}).includes('ultima copia disponibile'));
console.log('PASS: reported/estimated distinction, combined charts, zero versus missing, unavailable/stale catalog');

assert(context.finopsPricing({available:true,stale:true,persisted:true,fetched_at:1}).includes('Tariffe acquisite il:'));
assert(context.finopsPricing({available:true,stale:false,persisted:false,fetched_at:1}).includes('solo fino al riavvio'));

// "Other" can be expanded into the rows it groups, and run details list their models.
{
 const cost=(usd,estimated_usd=null)=>({records:1,input:100,output:20,cache_read:null,cache_write:null,reasoning:null,llm_calls:2,llm_ms:null,total:120,usd,estimated_usd});
 const tags=html=>html.replace(/<[^>]+>/g,'|').replace(/\|+/g,'|');
 const model=(name,usd,est)=>({name:name===null?'pi · unknown':'pi · '+name,runtime:'pi',model:name,metrics:cost(usd,est)});
 const slices=list=>list.map(r=>({...r,name:context.finopsModelName(r),value:context.finopsKnown(r.metrics)}));
 const models=[7,6,5,4,3,2,1].map(n=>model('m'+n,n)).concat([model(null,.5,.25)]);
 const groups=context.finopsCostGroups(context.finopsChartValues(slices(models)),5),other=groups.at(-1);
 assert.equal(groups.length,6);assert.equal(other.others.length,3);
 // The expanded rows add up to "Other" and expanding does not change the page total.
 assert.equal(other.others.reduce((n,r)=>n+r.value,0),other.value);
 assert.equal(groups.reduce((n,r)=>n+r.value,0),slices(models).reduce((n,r)=>n+r.value,0));
 // "Other" is never offered for a single grouped entry.
 assert(!context.finopsCostGroups(context.finopsChartValues(slices(models.slice(0,6))),5).some(r=>r.others));
 assert.equal(context.finopsCostGroups(context.finopsChartValues(slices(models.slice(0,7))),5).at(-1).others.length,2);
 const donut=context.finopsChartDonut(slices(models),'Ripartizione costi per modello');
 const toggle=donut.match(/<button[^>]*data-finops-other[^>]*>([^<]*)<\/button>/);
 assert(toggle&&toggle[0].includes('aria-expanded="false"')&&toggle[0].includes('aria-controls="finops-other-models"'));
 assert.equal(toggle[1],'Altri (3)');
 const panel=donut.slice(donut.indexOf('<div id="finops-other-models"'));
 assert(panel.startsWith('<div id="finops-other-models" class="finops-other-panel" hidden>'));
 assert.equal((panel.match(/<tr><td>/g)||[]).length,3);
 assert(panel.includes('pi · m2')&&panel.includes('pi · m1')&&panel.includes('pi · Modello non attribuito')&&!panel.includes('pi · m7'));
 assert.equal((context.finopsChartDonut(slices(models.slice(0,6)),'t').match(/data-finops-other/g)||[]).length,0);
 // A model that was not recorded is named as unattributed, and missing values are not zeros.
 const missing=tags(context.finopsRows([{...model(null,null),metrics:{...cost(null),records:0,llm_calls:null}}],'',true));
 assert(missing.includes('pi · Modello non attribuito')&&!/\|0[,|]/.test(missing.slice(missing.indexOf('pi · Modello'))));
 // The run detail table adds cache write and reasoning; the summary tables keep their columns.
 const extended=context.finopsRows([model('m1',1,2)],'',true),plain=context.finopsRows([model('m1',1,2)]);
 assert(extended.includes('Cache scritta')&&extended.includes('Reasoning token')&&!plain.includes('Cache scritta'));
 assert.equal((extended.match(/<th /g)||[]).length,8);assert.equal((extended.match(/<td>/g)||[]).length,8);assert.equal((plain.match(/<td>/g)||[]).length,6);
 // Agents: "Other" opens one block per agent with that agent's models; other charts keep a plain label.
 const agents=[...Array(11)].map((_,i)=>({name:`Workflow · abcd1234 · agent${i}`,label:'agent'+i,detail:'Workflow · abcd1234',metrics:cost(i+1),models:[model('a',i+.5),model(null,.5)]}));
 const rows=agents.map(r=>({...r,value:context.finopsKnown(r.metrics)}));
 const bars=context.finopsChartBars(rows,'Confronto costi per agente',undefined,true);
 assert(bars.includes('aria-controls="finops-other-agents"')&&bars.includes('>Altri (3)<'));
 const details=bars.slice(bars.indexOf('<div id="finops-other-agents"'));
 assert.equal((details.match(/<details/g)||[]).length,3);
 for(const name of ['agent0','agent1','agent2'])assert(details.includes('Workflow · abcd1234 · '+name));
 assert(!details.includes('agent10')&&details.includes('pi · a')&&details.includes('pi · Modello non attribuito'));
 assert(bars.includes('<div class="finops-bar-context" data-no-i18n>Workflow · abcd1234</div>'));
 assert(!context.finopsChartBars(rows.map(r=>({name:r.name,value:r.value})),'Confronto costi per workflow').includes('data-finops-other'));
 assert(!details.includes('finops-agent-more'));
 // A long "Other" lists its first ten agents; the rest sit in one more row with their count and total.
 const many=[...Array(42)].map((_,i)=>({name:`Workflow · abcd1234 · agent${i}`,label:'agent'+i,detail:'Workflow · abcd1234',value:i+1,metrics:cost(i+1),models:[model('a',i+1)]}));
 const long=context.finopsChartBars(many,'Confronto costi per agente',undefined,true);
 assert(long.includes('>Altri (34)<'));
 const tail=long.slice(long.indexOf('<details class="finops-agent-models finops-agent-more">'));
 const head=long.slice(long.indexOf('<div id="finops-other-agents"'),long.indexOf('<details class="finops-agent-models finops-agent-more">'));
 assert.equal((head.match(/<details/g)||[]).length,10);assert.equal((tail.match(/<details/g)||[]).length,25);
 // Largest first: agents 34..25 are listed, 24..1 are behind the row, and the two parts add up to "Other".
 assert(head.includes('agent33<')&&head.includes('agent24<')&&!head.includes('agent23<')&&tail.includes('agent23<')&&tail.includes('agent0<'));
 assert(tail.includes('24 altri agenti')&&tail.includes(vm.runInContext('fusd(300)',context)));
 assert.equal(many.slice(0,34).reduce((n,r)=>n+r.value,0),295+300);
 assert(!context.finopsRows([{...model('<img src=x>',1),name:'<img src=x>'}]).includes('<img'));
 console.log('PASS: expandable Other for models and agents, reconciliation, no single-entry Other, unattributed model, run model columns, escaping');
}

// Relative dates in the workflow, model and agent tables.
{
 const ago=seconds=>context.finopsAgo(Date.now()-seconds*1000).replace(/<[^>]+>/g,'');
 const cases=[[5,'5 secondi fa'],[59,'59 secondi fa'],[60,'1 minuto fa'],[3599,'59 minuti fa'],[3600,'1 ora fa'],[86399,'23 ore fa'],[86400,'1 giorno fa'],[29*86400,'29 giorni fa'],[30*86400,'1 mese fa'],[364*86400,'12 mesi fa'],[365*86400,'1 anno fa'],[800*86400,'2 anni fa']];
 for(const [seconds,text] of cases)assert.equal(ago(seconds),text);
 // A date in the future (clock skew) is not shown as negative, and a missing date is not a date.
 assert.equal(ago(-30),'0 secondi fa');
 for(const missing of [null,undefined,0,NaN,'x'])assert.equal(context.finopsAgo(missing),'—');
 const at=Date.now()-3*86400*1000,cell=context.finopsAgo(at);
 assert(cell.includes(`datetime="${new Date(at).toISOString()}"`)&&cell.includes('title="')&&cell.includes('data-no-i18n'));
 const metrics={records:1,input:1,output:1,cache_read:null,llm_calls:1,usd:1,estimated_usd:null,llm_ms:null,first_at:at};
 const created=vm.runInContext('finopsCreated',context),firstUsed=vm.runInContext('finopsFirstUsed',context);
 const workflows=context.finopsRows([{id:'w',name:'W',runs:1,completed:1,created:Math.floor(at/1000),metrics},{id:'x',name:'X',runs:0,completed:0,created:null,metrics}],'workflows',false,created);
 assert(workflows.includes('>Creato</th>')&&workflows.includes('3 giorni fa')&&/<td>—<\/td><\/tr><\/tbody>/.test(workflows));
 const models=context.finopsRows([{name:'pi · m',runtime:'pi',model:'m',metrics}],'',false,firstUsed);
 assert(models.includes('>Primo utilizzo</th>')&&models.includes('3 giorni fa'));
 assert.equal((models.match(/<th /g)||[]).length,7);assert.equal((models.match(/<td>/g)||[]).length,7);
 // Tables without the option keep their columns.
 assert(!context.finopsRows([{name:'pi · m',runtime:'pi',model:'m',metrics}]).includes('giorni fa'));
 console.log('PASS: relative dates from seconds to years, missing dates, created and first used columns');
}
