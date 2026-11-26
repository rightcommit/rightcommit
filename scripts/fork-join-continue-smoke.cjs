// Real browser regression: Continue run from a Join, from a step inside a parallel branch and from a Fork,
// with the Fork as the first step of the workflow and with skipped Tasks inside the branches.
// An offline fixture stands in for Claude Code: no models or external services.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('node:assert/strict'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const port=process.env.SMOKE_PORT||'18353',dir=fs.mkdtempSync(path.join(os.tmpdir(),'fork-join-continue-')),base='http://127.0.0.1:'+port,bin=path.join(dir,'bin'),counts=path.join(dir,'counts'),flags=path.join(dir,'flags');
 for(const d of [bin,counts,flags])fs.mkdirSync(d);
 // Each turn takes a few seconds so its running state, and the edge shown with it, can be observed.
 // A step's summary counts its turns; the step after the Join (tag ending in SYN) reports the branch outputs
// it received, those whose tag has the same prefix.
 fs.writeFileSync(path.join(bin,'claude'),`#!/usr/bin/env python3
import sys,json,re,time,pathlib
prompt=sys.stdin.read();args=sys.argv
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
tag=re.search(r'TAG:(\\w+)',prompt).group(1)
count=pathlib.Path(${JSON.stringify(counts)})/tag
n=int(count.read_text())+1 if count.exists() else 1
count.write_text(str(n))
time.sleep(2.5)
failed=pathlib.Path(${JSON.stringify(flags)}).joinpath('fail-'+tag).exists()
summary='OUT-%s-%d'%(tag,n)
if tag.endswith('SYN'):summary+=' SAW '+' '.join(sorted(set(re.findall(r'OUT-%s(?:A1|A2|B1|B2|B)-\\d+'%tag[:-3],prompt))))
print(json.dumps(dict(type='system',subtype='init',session_id=sid)),flush=True)
print(json.dumps(dict(type='result',result='<nodo_result>'+json.dumps(dict(status='failed' if failed else 'completed',summary=summary,artifacts=[],questions=[]))+'</nodo_result>',is_error=False)),flush=True)
`,{mode:0o755});
 const server=spawn(process.env.NODO_TEST_BINARY||path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:'+port,NODO_SOURCES:path.resolve('sources'),PATH:bin+':'+process.env.PATH,ANTHROPIC_AUTH_TOKEN:'offline-fork-join-fixture'},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>localStorage.setItem('rightcommit.hideAgentSetupNotice','true'));
  const call=async(url,body,status=200)=>{const r=await page.request.fetch(base+'/api'+url,{method:body?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:body});assert.equal(r.status(),status,url+': '+await r.text());return r.json()};
  await call('/auth/register',{email:'fork-join-continue@test.local',password:'temporary-fork-join-password'});
  const agent=(id,x)=>({id,kind:'Agent',title:id,runtime:'claude',x,y:380}),task=(id,agent,tag,x,y)=>({id,kind:'Task',title:id,agent,prompt:'TAG:'+tag,x,y});
  const control=(from,to)=>({from,to,kind:'control'}),context=(from,to)=>({from,to,kind:'context'});
  const w={id:'fork-join-continue',name:'Fork Join continue',description:'',entry:'plan',nodes:[
   {id:'source',kind:'Source',title:'Source',source_type:'example',x:60,y:100},agent('coord',260),agent('aa',660),agent('ab',860),
   task('plan','coord','PLAN',260,100),{id:'fork',kind:'Fork',title:'fork',target:'join',x:460,y:100},
   task('a1','aa','A1',660,0),task('a2','aa','A2',860,0),task('b','ab','B',760,200),
   {id:'join',kind:'Join',title:'join',x:1060,y:100},task('syn','coord','SYN',1260,100)],
   edges:[context('source','coord'),context('coord','plan'),context('coord','syn'),context('aa','a1'),context('aa','a2'),context('ab','b'),
    control('plan','fork'),control('fork','a1'),control('fork','b'),control('a1','a2'),control('a2','join'),control('b','join'),control('join','syn')]};
  await call('/workflows',w);
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  // The edges that carry an animated signal while `node` runs, read from the rendered graph.
  const signal=async node=>{
   await page.waitForFunction(id=>app.run?.status==='running'&&['running','waiting_external'].includes(app.run.states[id]),node,{timeout:30000});await page.waitForTimeout(500);
   return page.evaluate(id=>{if(!['running','waiting_external'].includes(app.run.states[id]))return id+' no longer running';
    return [...document.querySelectorAll('#edges path.edge.signal')].map(p=>{const s=getComputedStyle(p);return `${p.dataset.from}->${p.dataset.to}:${p.dataset.kind}`+(s.animationName==='flow'&&p.getTotalLength()>0&&s.display!=='none'&&s.visibility!=='hidden'&&+s.opacity>0?'':' (not animated)')}).join(' ')},node)};
  const settled=()=>page.waitForFunction(()=>['completed','failed'].includes(app.run?.status),null,{timeout:60000});
  const started=()=>page.waitForFunction(()=>['starting','running'].includes(app.run?.status),null,{timeout:30000});
  const continueFrom=async node=>{await page.locator('#fit').click();await page.locator(`.node-main[data-id="${node}"]`).click({button:'right'});await page.locator('[data-action=continue-run-node]:enabled').click();await page.locator('.app-confirm [data-confirm-accept]').click();await started()};
  let run=await call('/runs',{workflow_id:w.id});await page.evaluate(id=>loadRun(id),run.id);
  const state=async()=>{const d=await call('/runs/'+run.id);assert.equal(d.run.status,'completed',d.run.error);return d};
  const attempts=(d,expected)=>assert.deepEqual(Object.fromEntries(Object.keys(expected).map(id=>[id,d.run.attempts[id]])),expected);
  // First run: while the branches run, the link into the Fork shows the signal.
  assert.equal(await signal('a1'),'plan->fork:control','first run');await settled();
  let d=await state();assert.equal(d.run.outputs.syn.summary,'OUT-SYN-1 SAW OUT-A1-1 OUT-A2-1 OUT-B-1');
  for(const id of ['fork','a1','a2','b','join','syn'])assert(d.continue_nodes.includes(id),id);
  // From the Join: every branch keeps its output, the Join and the step after it run again.
  await continueFrom('join');assert.equal(await signal('syn'),'join->syn:control');await settled();
  d=await state();attempts(d,{plan:1,a1:1,a2:1,b:1,join:2,syn:2});assert.equal(d.run.outputs.syn.summary,'OUT-SYN-2 SAW OUT-A1-1 OUT-A2-1 OUT-B-1');
  // From a step inside a branch: that step and the rest of its branch run again, the other branch is kept.
  await continueFrom('a2');assert.equal(await signal('a2'),'a1->a2:control','continue inside a branch');await settled();
  d=await state();attempts(d,{plan:1,a1:1,a2:2,b:1,join:3,syn:3});assert.equal(d.run.outputs.syn.summary,'OUT-SYN-3 SAW OUT-A1-1 OUT-A2-2 OUT-B-1');
  await continueFrom('b');assert.equal(await signal('b'),'fork->b:control','continue from the first step of a branch');await settled();
  d=await state();attempts(d,{plan:1,a1:1,a2:2,b:2,join:4,syn:4});assert.equal(d.run.outputs.syn.summary,'OUT-SYN-4 SAW OUT-A1-1 OUT-A2-2 OUT-B-2');
  // From the Fork: every branch runs again.
  await continueFrom('fork');assert.equal(await signal('a1'),'plan->fork:control','continue from the Fork');await settled();
  d=await state();attempts(d,{plan:1,a1:2,a2:3,b:3,join:5,syn:5});assert.equal(d.run.outputs.syn.summary,'OUT-SYN-5 SAW OUT-A1-2 OUT-A2-3 OUT-B-3');
  // A failed branch: the Join is refused with a message that names what to do, the failed branch runs again.
  fs.writeFileSync(path.join(flags,'fail-B'),'');await continueFrom('b');await settled();
  d=await call('/runs/'+run.id);assert.equal(d.run.status,'failed');assert.equal(d.run.states.b,'failed');
  assert(!d.continue_nodes.includes('join'));assert(!d.continue_nodes.includes('syn'));for(const id of ['fork','a1','a2','b'])assert(d.continue_nodes.includes(id),id);
  const refused=await call('/runs/'+run.id+'/continue',{node:'join',acknowledge:true},400);assert.match(JSON.stringify(refused),/continua dal Fork/);
  fs.rmSync(path.join(flags,'fail-B'));await continueFrom('b');await settled();
  d=await state();attempts(d,{plan:1,a1:2,a2:3,b:5,syn:6});assert.equal(d.run.outputs.syn.summary,'OUT-SYN-6 SAW OUT-A1-2 OUT-A2-3 OUT-B-5');
  // The Fork as the first step: nothing runs before it, so a branch step runs again through the Fork
  // without a previous node to arrive from.
  const fork1={id:'fork-first-continue',name:'Fork first continue',description:'',entry:'fork',nodes:[
   {id:'source',kind:'Source',title:'Source',source_type:'example',x:60,y:100},agent('coord',260),agent('aa',660),agent('ab',860),
   {id:'fork',kind:'Fork',title:'fork',target:'join',x:460,y:100},
   task('a1','aa','FA1',660,0),task('a2','aa','FA2',860,0),task('b','ab','FB',760,200),
   {id:'join',kind:'Join',title:'join',x:1060,y:100},task('syn','coord','FSYN',1260,100)],
   edges:[context('source','coord'),context('coord','syn'),context('aa','a1'),context('aa','a2'),context('ab','b'),
    control('fork','a1'),control('fork','b'),control('a1','a2'),control('a2','join'),control('b','join'),control('join','syn')]};
  await call('/workflows',fork1);
  run=await call('/runs',{workflow_id:fork1.id});await page.evaluate(id=>loadRun(id),run.id);
  await signal('a1');await settled();
  d=await state();assert.equal(d.run.outputs.syn.summary,'OUT-FSYN-1 SAW OUT-FA1-1 OUT-FA2-1 OUT-FB-1');
  for(const id of ['fork','a1','a2','b','join','syn'])assert(d.continue_nodes.includes(id),'Fork first: '+id);
  await continueFrom('a2');assert.equal(await signal('a2'),'a1->a2:control','Fork first, continue inside a branch');await settled();
  d=await state();attempts(d,{a1:1,a2:2,b:1,join:2,syn:2});assert.equal(d.run.outputs.syn.summary,'OUT-FSYN-2 SAW OUT-FA1-1 OUT-FA2-2 OUT-FB-1');
  await continueFrom('b');assert.equal(await signal('b'),'fork->b:control','Fork first, continue from the first step of a branch');await settled();
  d=await state();attempts(d,{a1:1,a2:2,b:2,join:3,syn:3});assert.equal(d.run.outputs.syn.summary,'OUT-FSYN-3 SAW OUT-FA1-1 OUT-FA2-2 OUT-FB-2');
  await continueFrom('join');assert.equal(await signal('syn'),'join->syn:control','Fork first, continue from the Join');await settled();
  d=await state();attempts(d,{a1:1,a2:2,b:2,join:4,syn:4});assert.equal(d.run.outputs.syn.summary,'OUT-FSYN-4 SAW OUT-FA1-1 OUT-FA2-2 OUT-FB-2');
  // Skipped Tasks inside the branches: they count as done for the Join, are never run and contribute nothing.
  const skipped=(id,agent,tag,x,y)=>({...task(id,agent,tag,x,y),skip:true});
  const skips={id:'skipped-branches-continue',name:'Skipped branches continue',description:'',entry:'plan',nodes:[
   {id:'source',kind:'Source',title:'Source',source_type:'example',x:60,y:100},agent('coord',260),agent('aa',660),agent('ab',860),
   task('plan','coord','SPLAN',260,100),{id:'fork',kind:'Fork',title:'fork',target:'join',x:460,y:100},
   skipped('a1','aa','SA1',660,0),task('a2','aa','SA2',860,0),task('b1','ab','SB1',660,200),skipped('b2','ab','SB2',860,200),
   {id:'join',kind:'Join',title:'join',x:1060,y:100},task('syn','coord','SSYN',1260,100)],
   edges:[context('source','coord'),context('coord','plan'),context('coord','syn'),context('aa','a1'),context('aa','a2'),context('ab','b1'),context('ab','b2'),
    control('plan','fork'),control('fork','a1'),control('fork','b1'),control('a1','a2'),control('a2','join'),control('b1','b2'),control('b2','join'),control('join','syn')]};
  await call('/workflows',skips);
  run=await call('/runs',{workflow_id:skips.id});await page.evaluate(id=>loadRun(id),run.id);
  await signal('a2');await settled();
  const bypassed=d=>{for(const id of ['a1','b2'])assert.equal(d.run.states[id],'bypassed',id);assert(!fs.existsSync(path.join(counts,'SA1'))&&!fs.existsSync(path.join(counts,'SB2')),'a skipped Task ran')};
  d=await state();bypassed(d);assert.equal(d.run.outputs.syn.summary,'OUT-SSYN-1 SAW OUT-SA2-1 OUT-SB1-1');
  for(const id of ['fork','a1','a2','b1','b2','join','syn'])assert(d.continue_nodes.includes(id),'skipped: '+id);
  await continueFrom('join');assert.equal(await signal('syn'),'join->syn:control','skipped, continue from the Join');await settled();
  d=await state();bypassed(d);attempts(d,{plan:1,a1:undefined,a2:1,b1:1,b2:undefined,join:2,syn:2});assert.equal(d.run.outputs.syn.summary,'OUT-SSYN-2 SAW OUT-SA2-1 OUT-SB1-1');
  await continueFrom('a2');assert.equal(await signal('a2'),'a1->a2:control','skipped, continue from the step after a skipped Task');await settled();
  d=await state();bypassed(d);attempts(d,{plan:1,a1:undefined,a2:2,b1:1,b2:undefined,join:3,syn:3});assert.equal(d.run.outputs.syn.summary,'OUT-SSYN-3 SAW OUT-SA2-2 OUT-SB1-1');
  await continueFrom('b1');assert.equal(await signal('b1'),'fork->b1:control','skipped, continue from the step before a skipped Task');await settled();
  d=await state();bypassed(d);attempts(d,{plan:1,a1:undefined,a2:2,b1:2,b2:undefined,join:4,syn:4});assert.equal(d.run.outputs.syn.summary,'OUT-SSYN-4 SAW OUT-SA2-2 OUT-SB1-2');
  assert.deepEqual(errors,[]);
  console.log('PASS: Continue run from a Join keeps every branch output, from a branch step runs that branch and joins the kept ones, from a Fork runs every branch, the Join is refused after a failed branch, with the edge signal in each case; also with the Fork as the first step and with skipped Tasks in the branches');
 }finally{await browser?.close();server.kill('SIGINT');await new Promise(r=>server.exitCode!==null?r():server.once('exit',r));fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
