// Local CI test command: panel field, persistence, translation and import warning; a Python project
// tested with its own .venv, whose missing pytest is explained in Italian and English.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('assert'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'test-command-')),base='http://127.0.0.1:18137',headers={'X-Nodo-Request':'1'};
 const sources=path.join(dir,'sources'),project=path.join(sources,'python-project');
 // A Python project whose virtual environment has no pytest: its python says so as Python would.
 fs.mkdirSync(path.join(project,'.venv/bin'),{recursive:true});fs.writeFileSync(path.join(project,'pyproject.toml'),'[project]\nname = "demo"\n');
 fs.writeFileSync(path.join(project,'.venv/bin/python'),'#!/bin/sh\necho "$0: No module named $2" >&2\nexit 1\n',{mode:0o755});
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:path.join(dir,'data'),NODO_LISTEN:'127.0.0.1:18137',NODO_SOURCES:sources},stdio:'ignore'});
 let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  for(const f of ['controls.js','app.js','app.css','i18n.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  await page.request.post(base+'/api/auth/register',{headers,data:{email:'tests@test.local',password:'temporary-test-password'}});
  const workflow={id:'local-ci',name:'Local CI',description:'',entry:'ci',nodes:[{id:'ci',kind:'Connection',title:'Local CI · tests',provider:'local',operation:'test',x:100,y:100}],edges:[]};
  assert((await page.request.post(base+'/api/workflows',{headers,data:workflow})).ok());
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  const open=()=>page.evaluate(()=>{selectWorkflow('local-ci');app.view='graph';app.selected='ci';renderInspector()});
  await open();
  const form=page.locator('#node-form');
  assert.equal(await form.locator('[name=test_command]').count(),1,'test command field');
  assert.equal(await form.locator('[name=location]').count(),0,'ticket field hidden for test');
  assert((await form.textContent()).includes('Cargo.toml → cargo test'));
  await form.locator('[name=test_command]').fill('node --test');
  await page.evaluate(()=>{captureInspectorFields();return saveWorkflow()});
  const saved=await (await page.request.get(base+'/api/workflows')).json();
  assert.equal(saved.find(w=>w.id==='local-ci').nodes[0].test_command,'node --test');
  // Another local operation shows the ticket field again and keeps the saved command.
  await page.evaluate(()=>{const field=document.querySelector('#node-form').elements.operation;field.value='commit';field.dispatchEvent(new Event('change',{bubbles:true}))});
  assert.equal(await form.locator('[name=test_command]').count(),0);
  assert.equal(await form.locator('[name=location]').count(),1);
  assert.equal(await page.evaluate(()=>currentWorkflow().nodes[0].test_command),'node --test');
  await page.evaluate(()=>{const field=document.querySelector('#node-form').elements.operation;field.value='test';field.dispatchEvent(new Event('change',{bubbles:true}))});
  assert.equal(await form.locator('[name=test_command]').inputValue(),'node --test');
  await page.evaluate(()=>{const s=document.querySelector('[data-language]');s.value='en';s.dispatchEvent(new Event('change',{bubbles:true}))});
  const text=await form.textContent();
  assert(text.includes('Test command (empty = auto-detect)'),'English label');
  assert(text.includes('When the field is empty, the command is chosen from the files in the workspace root'),'English hint');
  assert(text.includes('without a shell'),'English shell note');
  // Import warns about commands that would run on the server.
  await page.evaluate(()=>importWorkflowDialog());
  const imported={...workflow,id:'x',name:'Imported',nodes:[{...workflow.nodes[0],test_command:"pytest -q -k 'not slow'"}]};
  await page.locator('#workflow-import-file').setInputFiles({name:'imported.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(imported))});
  await page.locator('#workflow-import-commands').waitFor({state:'visible'});
  const warning=await page.locator('#workflow-import-commands').textContent();
  assert(warning.includes('This workflow runs these commands on the server')&&warning.includes("Local CI · tests: pytest -q -k 'not slow'"),warning);
  await page.locator('#workflow-import-file').setInputFiles({name:'plain.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({...workflow,id:'y',name:'Plain'}))});
  await page.waitForFunction(()=>!document.querySelector('#workflow-import-confirm').disabled);
  assert(await page.locator('#workflow-import-commands').isHidden());
  // A run of the bundled Python test. The step report keeps the unittest output, which later steps
  // receive; the viewers show it as written, so its line of dashes is not a Markdown rule.
  await page.evaluate(()=>closeModal());
  const example=JSON.parse(fs.readFileSync(path.resolve('examples/local-ci/workflow.json'),'utf8'));
  assert((await page.request.post(base+'/api/workflows',{headers,data:example})).ok());
  const started=await (await page.request.post(base+'/api/runs',{headers,data:{workflow_id:example.id}})).json();
  let detail;for(let i=0;i<300;i++){detail=await (await page.request.get(base+'/api/runs/'+started.id)).json();if(['completed','failed'].includes(detail.run.status))break;await new Promise(r=>setTimeout(r,100))}
  assert.equal(detail.run.status,'completed',detail.run.error);
  const report=detail.artifacts.find(a=>a.path==='.rightcommit/reports/tests.md');
  const stored=(await (await page.request.get(base+'/api/artifacts/'+report.id)).json()).artifact.content;
  assert(/\n-{20,}\n/.test(stored)&&stored.includes('Ran 1 test'),stored);
  assert.equal(detail.run.outputs.tests.summary,stored,'later steps receive the report as stored');
  await page.evaluate(id=>loadRun(id),started.id);
  const shown=async()=>{const view=page.locator('#dialog .artifact-view');await view.waitFor();assert.equal(await view.locator('pre.step-output').count(),1);assert.equal(await view.locator('hr,h1,h2,ul,.markdown').count(),0);assert.equal(await view.locator('pre.step-output').textContent(),stored);assert.match(await view.locator('pre.step-output').evaluate(e=>getComputedStyle(e).fontFamily),/monospace/);await page.evaluate(()=>closeModal())};
  await page.evaluate(id=>artifact(id),report.id);await shown();
  await page.evaluate(p=>openStorage(p,'workspaces'),detail.run.workspace.split('/workspaces/')[1]+'/.rightcommit/reports/tests.md');await shown();
  // The Activity log keeps its translated header lines above the output.
  await page.evaluate(()=>logs(true));await page.waitForFunction(id=>activityLog.runs.get(id)?.loaded,started.id);
  const log=await page.locator('#activity-panel').innerText();
  assert(log.includes('Test command: python3 -m unittest -v test_calculator (set in the node)')&&log.includes("Tests passed. Last lines of the output (all of it is in the step's report):"),log);
  // The detected Python command runs with the project's .venv; without pytest there, the error says how to fix it.
  const python={id:'python-ci',name:'Python CI',description:'',entry:'tests',nodes:[{id:'source',kind:'Source',title:'Python project',source_type:'folder',source_mode:'direct',location:'python-project',x:100,y:100},{id:'tests',kind:'Connection',title:'Local CI tests',provider:'local',operation:'test',x:400,y:100}],edges:[{from:'source',to:'tests',kind:'context',outcome:''}]};
  assert((await page.request.post(base+'/api/workflows',{headers,data:python})).ok());
  const failing=await (await page.request.post(base+'/api/runs',{headers,data:{workflow_id:python.id}})).json();
  for(let i=0;i<300;i++){detail=await (await page.request.get(base+'/api/runs/'+failing.id)).json();if(['completed','failed'].includes(detail.run.status))break;await new Promise(r=>setTimeout(r,100))}
  const hint={en:"pytest is not installed for .venv/bin/python. Install it with the project's dependencies in a virtual environment in the project folder (python3 -m venv .venv, then .venv/bin/pip install pytest), which the detected command uses, or set a test command in the node.",it:'pytest non è installato per .venv/bin/python. Installalo con le dipendenze del progetto in un ambiente virtuale nella cartella del progetto (python3 -m venv .venv, poi .venv/bin/pip install pytest), che il comando rilevato usa, oppure imposta un comando di test nel nodo.'};
  assert.equal(detail.run.status,'failed');assert(detail.run.error.startsWith(hint.en+'\n.venv/bin/python: ')&&detail.run.error.includes('No module named pytest'),detail.run.error);
  await page.evaluate(id=>loadRun(id),failing.id);
  for(const lang of ['it','en']){
   await page.evaluate(lang=>{const s=document.querySelector('[data-language]');s.value=lang;s.dispatchEvent(new Event('change',{bubbles:true}))},lang);
   await page.evaluate(()=>logs(true));await page.waitForFunction(id=>activityLog.runs.get(id)?.loaded,failing.id);
   await page.locator('#activity-panel [data-log-read]').last().click();const message=await page.locator('#dialog pre').first().textContent();await page.evaluate(()=>closeModal());
   assert(message.startsWith(hint[lang]+'\n.venv/bin/python: '),lang+': '+message);
   const command=await page.locator('#activity-panel').innerText();assert(command.includes(lang==='en'?'Test command: .venv/bin/python -m pytest (detected from pyproject.toml)':'Comando di test: .venv/bin/python -m pytest (rilevato da pyproject.toml)'),command);
   const shown=await page.locator('#execution-list .error').first().textContent();assert(shown.startsWith(hint[lang]+'\n'),lang+': '+shown);
  }
  assert.deepEqual(errors,[]);
  console.log('PASS: test command field, persistence, operation switch, English text, import warning, test report shown as written, .venv Python detected and its missing pytest explained in Italian and English');
 }finally{await browser?.close();server.kill();fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
