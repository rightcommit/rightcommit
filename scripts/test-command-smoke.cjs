// Local CI test command: panel field, persistence, translation and import warning.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('assert'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'test-command-')),base='http://127.0.0.1:18137',headers={'X-Nodo-Request':'1'};
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18137',NODO_SOURCES:path.resolve('sources')},stdio:'ignore'});
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
  assert(warning.includes('This workflow runs test commands on the server')&&warning.includes("Local CI · tests: pytest -q -k 'not slow'"),warning);
  await page.locator('#workflow-import-file').setInputFiles({name:'plain.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({...workflow,id:'y',name:'Plain'}))});
  await page.waitForFunction(()=>!document.querySelector('#workflow-import-confirm').disabled);
  assert(await page.locator('#workflow-import-commands').isHidden());
  assert.deepEqual(errors,[]);
  console.log('PASS: test command field, persistence, operation switch, English text and import warning');
 }finally{await browser?.close();server.kill();fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
