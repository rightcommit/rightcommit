// Source preparation fields, generic skill field, Agent binding by edge and Start errors in the dialog.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('assert'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'source-setup-')),base='http://127.0.0.1:18138',headers={'X-Nodo-Request':'1'};
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18138',NODO_SOURCES:path.resolve('sources')},stdio:'ignore'});
 let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  for(const f of ['controls.js','app.js','app.css','i18n.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  await page.request.post(base+'/api/auth/register',{headers,data:{email:'tests@test.local',password:'temporary-test-password'}});
  const workflow={id:'own',name:'Own project',description:'',entry:'task',nodes:[
   {id:'src',kind:'Source',title:'Own project',source_type:'example',x:0,y:0},
   {id:'author',kind:'Agent',title:'Author',x:200,y:0},
   {id:'reviewer',kind:'Agent',title:'Reviewer',x:200,y:200},
   {id:'task',kind:'Task',title:'Propose',agent:'author',prompt:'Propose the change',x:400,y:0},
   {id:'review',kind:'Review',title:'Check',mode:'agent',prompt:'Check it',x:600,y:0}],
   edges:[{from:'src',to:'author',kind:'context',outcome:''},{from:'author',to:'task',kind:'context',outcome:''},{from:'task',to:'review',kind:'control',outcome:''}]};
  assert((await page.request.post(base+'/api/workflows',{headers,data:workflow})).ok());
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  const select=id=>page.evaluate(id=>{selectWorkflow('own');app.view='graph';app.selected=id;renderInspector()},id);
  const form=page.locator('#node-form');

  // Source: Spec Kit switch and setup command, saved with the workflow.
  await select('src');
  assert.equal(await form.locator('jui-switch[name=speckit]').count(),1,'Spec Kit switch');
  await form.locator('jui-switch[name=speckit]').click();
  await form.locator('[name=setup_command]').fill('openspec init --tools claude,codex,pi');
  await page.evaluate(()=>{captureInspectorFields();return saveWorkflow()});
  let saved=(await (await page.request.get(base+'/api/workflows')).json()).find(w=>w.id==='own');
  const source=saved.nodes.find(n=>n.id==='src');
  assert.equal(source.speckit,true);assert.equal(source.setup_command,'openspec init --tools claude,codex,pi');

  // Task: the skill field takes any skill name.
  await select('task');
  assert((await form.textContent()).includes('openspec-propose'));

  // Dragging an Agent onto a Review assigns it; another Agent replaces it and its edge.
  await page.evaluate(()=>connect('reviewer','review'));
  let state=await page.evaluate(()=>({agent:app.wf.nodes.find(n=>n.id==='review').agent,edges:app.wf.edges.filter(e=>e.to==='review'&&e.kind==='context').map(e=>e.from)}));
  assert.deepEqual(state,{agent:'reviewer',edges:['reviewer']});
  await page.evaluate(()=>connect('author','review'));
  state=await page.evaluate(()=>({agent:app.wf.nodes.find(n=>n.id==='review').agent,edges:app.wf.edges.filter(e=>e.to==='review'&&e.kind==='context').map(e=>e.from)}));
  assert.deepEqual(state,{agent:'author',edges:['author']});
  // Choosing another Agent in the field drops the edge that no longer matches.
  await select('review');
  await page.evaluate(()=>{const f=document.querySelector('#node-form').elements.agent;f.value='reviewer';f.dispatchEvent(new Event('change',{bubbles:true}))});
  state=await page.evaluate(()=>({agent:app.wf.nodes.find(n=>n.id==='review').agent,edges:app.wf.edges.filter(e=>e.to==='review'&&e.kind==='context').map(e=>e.from)}));
  assert.deepEqual(state,{agent:'reviewer',edges:[]});
  await page.evaluate(()=>saveWorkflow());
  saved=(await (await page.request.get(base+'/api/workflows')).json()).find(w=>w.id==='own');
  assert.equal(saved.nodes.find(n=>n.id==='review').agent,'reviewer');

  // English UI: labels, and a server error shown inside the Start dialog.
  await page.evaluate(()=>{const s=document.querySelector('[data-language]');s.value='en';s.dispatchEvent(new Event('change',{bubbles:true}))});
  await select('src');
  const text=await form.textContent();
  assert(text.includes('Add Spec Kit if missing')&&text.includes('Setup command (optional)'),text);
  await page.evaluate(()=>{app.wf.nodes.push({id:'lonely',kind:'Agent',title:'Lonely agent',x:0,y:300});return saveWorkflow()});
  await page.locator('#execute').click();await page.locator('[data-run-now]').click();
  await page.locator('[data-action=start-run]').click();
  await page.locator('#run-error').waitFor({state:'visible'});
  assert.equal((await page.locator('#run-error').textContent()).trim(),'Node has no connections: Lonely agent');

  // Import lists setup commands with test commands.
  await page.evaluate(()=>importWorkflowDialog());
  await page.locator('#workflow-import-file').setInputFiles({name:'own.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({...saved,id:'copy'}))});
  await page.locator('#workflow-import-commands').waitFor({state:'visible'});
  const warning=await page.locator('#workflow-import-commands').textContent();
  assert(warning.includes('This workflow runs these commands on the server')&&warning.includes('Own project: openspec init --tools claude,codex,pi'),warning);
  assert.deepEqual(errors,[]);
  console.log('PASS: Source preparation fields, skill field, Agent binding, Start error and import warning');
 }finally{await browser?.close();server.kill();fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
