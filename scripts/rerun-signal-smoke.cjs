// Real browser regression: the animated edge signal shows when part of an executed run runs again,
// on the same edge as in the first run. Local test Connection steps only: no models or external services.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('node:assert/strict'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'rerun-signal-')),flags=path.join(dir,'flags'),base='http://127.0.0.1:18312';fs.mkdirSync(flags);
 const server=spawn(process.env.NODO_TEST_BINARY||path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18312',NODO_SOURCES:path.resolve('sources')},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>localStorage.setItem('rightcommit.hideAgentSetupNotice','true'));
  const call=async(url,body)=>{const r=await page.request.fetch(base+'/api'+url,{method:body?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:body});assert(r.ok(),url+': '+await r.text());return r.json()};
  await call('/auth/register',{email:'rerun-signal@test.local',password:'temporary-rerun-signal-password'});
  // Each step takes a few seconds so its running state, and the edge entering it, can be observed.
  const command=id=>`sh -c 'sleep 2.5; test ! -f ${flags}/fail-${id}'`;
  const step=(id,x)=>({id,kind:'Connection',title:id,provider:'local',operation:'test',test_command:command(id),x,y:100});
  const w={id:'rerun-signal',name:'Rerun signal',description:'',entry:'a',nodes:[{id:'source',kind:'Source',title:'Source',source_type:'example',x:60,y:100},step('a',260),step('b',460),step('c',660)],edges:[{from:'source',to:'a',kind:'context'},{from:'a',to:'b',kind:'control'},{from:'b',to:'c',kind:'control'}]};
  await call('/workflows',w);
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  // The edges that carry an animated signal while `node` runs, read from the rendered graph.
  const signal=async node=>{
   await page.waitForFunction(id=>app.run?.status==='running'&&['running','waiting_external'].includes(app.run.states[id]),node,{timeout:30000});await page.waitForTimeout(500);
   return page.evaluate(id=>{if(!['running','waiting_external'].includes(app.run.states[id]))return id+' no longer running';
    return [...document.querySelectorAll('#edges path.edge.signal')].map(p=>{const s=getComputedStyle(p);return `${p.dataset.from}->${p.dataset.to}:${p.dataset.kind}`+(s.animationName==='flow'&&p.getTotalLength()>0&&s.display!=='none'&&s.visibility!=='hidden'&&+s.opacity>0?'':' (not animated)')}).join(' ')},node)};
  const settled=()=>page.waitForFunction(()=>['completed','failed'].includes(app.run?.status),null,{timeout:60000});
  // Each action returns once the client shows the run started again, so the previous end state is not mistaken for the new one.
  const started=()=>page.waitForFunction(()=>['starting','running'].includes(app.run?.status),null,{timeout:30000});
  const continueFrom=async node=>{await page.locator('#fit').click();await page.locator(`.node-main[data-id="${node}"]`).click({button:'right'});await page.locator('[data-action=continue-run-node]:enabled').click();await page.locator('.app-confirm [data-confirm-accept]').click();await started()};
  const resume=async()=>{await page.locator('#resume').click();await page.locator('.app-confirm [data-confirm-accept]').click();await started()};
  // First run: the context edge from the Source enters the first step, then each control edge in turn.
  const run=await call('/runs',{workflow_id:w.id});await page.evaluate(id=>loadRun(id),run.id);
  assert.equal(await signal('a'),'source->a:context');assert.equal(await signal('b'),'a->b:control');assert.equal(await signal('c'),'b->c:control');await settled();
  // Continue a completed run from a middle step: its incoming edge, then the next one.
  await continueFrom('b');assert.equal(await signal('b'),'a->b:control');assert.equal(await signal('c'),'b->c:control');await settled();
  // Restart from the Source or the first step: the same edge as the first run.
  await continueFrom('source');assert.equal(await signal('a'),'source->a:context','restart from the Source');assert.equal(await signal('b'),'a->b:control');await settled();
  await continueFrom('a');assert.equal(await signal('a'),'source->a:context','restart from the first step');await settled();
  // Resume a run that failed in its first step.
  fs.writeFileSync(path.join(flags,'fail-a'),'');await continueFrom('a');await settled();assert.equal(await page.evaluate(()=>app.run.status),'failed');fs.rmSync(path.join(flags,'fail-a'));
  await resume();assert.equal(await signal('a'),'source->a:context','resume of the failed first step');await settled();
  // Resume and continue a run that failed in a middle step.
  fs.writeFileSync(path.join(flags,'fail-b'),'');await continueFrom('b');await settled();fs.rmSync(path.join(flags,'fail-b'));
  await resume();assert.equal(await signal('b'),'a->b:control','resume of a failed step');await settled();
  // A page reload while a continued run is running shows the signal again.
  await continueFrom('b');await signal('b');await page.reload();await page.locator('#app').waitFor({state:'visible'});if(!await page.evaluate(()=>!!app.run))await page.evaluate(id=>loadRun(id),run.id);
  assert.equal(await signal('c'),'b->c:control','after a reload');await settled();
  // Edit and continue with a new first step: the new run shows the Source edge entering it, as a first run does.
  await page.locator('#edit-continue').click();await page.waitForFunction(()=>!!app.editBase&&!app.run);
  await page.evaluate(cmd=>{app.wf.nodes.push({id:'e',kind:'Connection',title:'e',provider:'local',operation:'test',test_command:cmd,x:160,y:260});app.wf.edges=app.wf.edges.filter(e=>!(e.from==='source'&&e.to==='a'));app.wf.edges.push({from:'source',to:'e',kind:'context',outcome:''},{from:'e',to:'a',kind:'control',outcome:''});app.wf.entry='e';dirty();render()},command('e'));
  const preview=page.waitForResponse(r=>r.url().endsWith('/revision/preview')&&r.ok());await page.locator('#revision-preview').click();assert.equal((await (await preview).json()).restart,'e');
  await page.locator('#revision-confirm:enabled').click();await page.waitForFunction(id=>app.run&&app.run.id!==id,run.id);
  assert.equal(await signal('e'),'source->e:context','Edit and continue from a new first step');assert.equal(await signal('a'),'e->a:control');await settled();
  assert.deepEqual(errors,[]);
  console.log('PASS: animated edge signal on first run, continue from a middle step, restart from the Source and the first step, resume after a failure, page reload and Edit and continue from a new first step');
 }finally{await browser?.close();server.kill('SIGINT');await new Promise(r=>server.exitCode!==null?r():server.once('exit',r));fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
