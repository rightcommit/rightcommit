const fs=require('fs'),os=require('os'),path=require('path'),assert=require('assert'),{spawn,spawnSync}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'rightcommit-schedules-')),base='http://127.0.0.1:18271';
 fs.mkdirSync(path.join(dir,'sources/material'),{recursive:true});fs.writeFileSync(path.join(dir,'sources/material/test.txt'),'schedule test');
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:dir,env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18271',NODO_SOURCES:path.join(dir,'sources'),NODO_ASSETS:path.resolve('.')},stdio:['ignore',fs.openSync(path.join(dir,'server.log'),'a'),fs.openSync(path.join(dir,'server.log'),'a')]});let browser;
 try{
  for(let i=0;i<200;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  // Serve local UI so the same test can exercise changes before rebuilding assets.
  for(const f of ['index.html','controls.js','app.js','schedules.js','app.css','i18n.js','a2a.js','mcp.js','rag.js','manager.js','templates.js','finops.js','icons.svg'])await page.route(f==='index.html'?base+'/':'**/'+f,r=>r.fulfill({path:path.resolve('web/'+f),contentType:f.endsWith('.js')?'text/javascript':f.endsWith('.css')?'text/css':f==='index.html'?'text/html':'image/svg+xml'}));
  const headers={'X-Nodo-Request':'1'};
  let r=await page.request.post(base+'/api/auth/register',{headers,data:{email:'schedule-admin@test.local',password:'temporary-schedule-test-password'}});assert(r.ok(),await r.text());
  console.log('registered');await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  console.log('booted');const workflow={id:'schedule-ui-test',name:'Scheduled review',description:'',entry:'review',nodes:[{id:'source',kind:'Source',source_type:'folder',location:'material',source_mode:'copy'},{id:'agent',kind:'Agent'},{id:'task',kind:'Task',agent:'agent',title:'Task',prompt:'Test task'},{id:'review',kind:'Review',mode:'human',title:'Review',prompt:'Check'}],edges:[{from:'source',to:'agent',kind:'context',outcome:''},{from:'agent',to:'task',kind:'context',outcome:''},{from:'review',to:'task',kind:'control',outcome:''}]};
  r=await page.request.post(base+'/api/workflows',{headers,data:workflow});assert(r.ok(),await r.text());
  // A run starts only when its agents have credentials (7cb128c): give the default agent a dummy key. The run stops at the human Review first.
  const settings=await(await page.request.get(base+'/api/settings')).json();r=await page.request.patch(base+'/api/settings',{headers,data:{revision:settings._revision,changes:{agents:{[settings.default_runtime]:{api_key:'dummy-schedule-key'}}}}});assert(r.ok(),await r.text());
  await page.evaluate(async()=>{app.workflows=await api('/workflows');selectWorkflow('schedule-ui-test')});
  console.log('workflow ready');await page.locator('#execute').click();assert.equal(await page.locator('#schedule-run-menu [role=menuitem]').count(),2);
  await page.keyboard.press('Escape');assert.equal(await page.locator('#execute').getAttribute('aria-expanded'),'false');
  await page.locator('#execute').click();await page.locator('[data-run-now]').click();await page.locator('#run-form').waitFor();assert.equal(await page.locator('#schedule-form').count(),0);await page.evaluate(()=>closeModal());
  await page.locator('#execute').click();await page.locator('[data-run-scheduled]').click();await page.locator('#schedule-form').waitFor();
  await page.locator('#schedule-save').waitFor({state:'visible'});await page.waitForFunction(()=>!document.querySelector('#schedule-save').disabled);
  assert.equal(await page.locator('#schedule-form select, #schedule-form input[type=date], #schedule-form input[type=time], #schedule-form input[type=datetime-local], #schedule-form input[type=checkbox], #schedule-form input[type=radio]').count(),0);
  assert.equal(await page.locator('#schedule-preview li').count(),5);
  assert.equal((await (await page.request.get(base+'/api/runs')).json()).length,0);
  console.log('saving schedule');await page.locator('#schedule-save').click();await page.locator('[data-schedule-detail]').waitFor();
  let schedules=await(await page.request.get(base+'/api/schedules')).json();assert.equal(schedules.length,1);const id=schedules[0].schedule.id;
  assert.equal((await(await page.request.get(base+'/api/runs')).json()).length,0);
  await page.locator('#schedule-active button').click();await page.waitForFunction(()=>document.querySelector('#schedule-active')&&!document.querySelector('#schedule-active').checked);
  await page.waitForTimeout(300);schedules=await(await page.request.get(base+'/api/schedules')).json();assert.equal(schedules[0].schedule.config.active,false);
  await page.locator('#schedule-active button').click();await page.waitForTimeout(300);schedules=await(await page.request.get(base+'/api/schedules')).json();assert.equal(schedules[0].schedule.config.active,true);
  await page.locator('#schedule-edit').click();await page.locator('#schedule-form').waitFor();
  await page.locator('#schedule-form [name=name]').fill('Weekly check');
  await page.evaluate(()=>{const kind=document.querySelector('#schedule-form [name=kind]');kind.value='minutes';kind.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.locator('#schedule-form [name=every]').fill('72');await page.waitForFunction(()=>!document.querySelector('#schedule-save').disabled);
  assert.equal(await page.locator('#schedule-preview li').count(),5);
  await page.locator('#schedule-form [name=every]').fill('invalid');

  await page.evaluate(()=>{const kind=document.querySelector('#schedule-form [name=kind]');kind.value='weekly';kind.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.waitForFunction(()=>!document.querySelector('#schedule-save').disabled);assert.equal(await page.locator('#schedule-weekdays').isVisible(),true);
  await page.locator('#schedule-save').click();await page.locator('[data-schedule-detail]').waitFor();
  // Advance only this isolated test occurrence to exercise dispatch without waiting a minute.
  const advance=spawnSync('python3',['-c',"import sqlite3,json,sys,time; db=sqlite3.connect(sys.argv[1]); raw=db.execute('SELECT json FROM schedules WHERE id=?',(sys.argv[2],)).fetchone()[0]; s=json.loads(raw); s['next']=int(time.time())+2; db.execute('UPDATE schedules SET json=? WHERE id=?',(json.dumps(s),sys.argv[2])); db.commit()",path.join(dir,'system/nodo.sqlite'),id],{encoding:'utf8'});assert.equal(advance.status,0,advance.stderr);
  let scheduledRuns=[];
  for(let i=0;i<40;i++){
   const response=await page.request.get(base+'/api/runs');assert(response.ok(),await response.text());scheduledRuns=await response.json();
   if(scheduledRuns.length===1&&scheduledRuns[0].status==='waiting_review')break;
   await page.waitForTimeout(250);
  }
  assert(scheduledRuns.length===1,JSON.stringify(scheduledRuns));assert.equal(scheduledRuns[0].status,'waiting_review');assert.equal(scheduledRuns[0].schedule_id,id);
  await page.evaluate(id=>scheduleDetails(id),id);await page.locator('[data-schedule-run]').first().click();await page.locator('#stop').click();
  await page.waitForFunction(()=>app.run?.status==='interrupted');assert.equal((await(await page.request.get(base+'/api/schedules')).json())[0].schedule.config.active,true);
  await page.evaluate(()=>closeModal());await page.locator('#history-tab').click();await page.locator('[data-schedule-open]').waitFor();assert.equal(await page.locator('#run-history [data-schedule-open]').count(),1);assert.equal(await page.locator('#run-history [data-action=load-run]').count(),0);
  await page.locator('#executions').click();await page.locator('#executions-schedules [data-schedule-open]').waitFor();assert.equal(await page.locator('#executions-schedules [data-schedule-open]').count(),1);
  await page.locator('#executions-schedules [data-schedule-open]').click();await page.locator('[data-schedule-detail]').waitFor();
  for(const width of [1440,390]){await page.setViewportSize({width,height:1050});await page.locator('#schedule-edit').click();await page.locator('#schedule-form').waitFor();assert(await page.locator('#dialog').evaluate(el=>el.scrollWidth<=el.clientWidth+1));await page.screenshot({path:path.join(dir,'schedule-'+width+'.png')});await page.evaluate(()=>closeModal());await page.evaluate(id=>scheduleDetails(id),id);}
  await page.evaluate(()=>{const el=document.querySelector('[data-language]');el.value='en';el.dispatchEvent(new Event('change',{bubbles:true}))});
  await page.locator('#schedule-edit').click();await page.locator('#schedule-form').waitFor();assert.equal(await page.locator('#schedule-save').textContent(),'Save schedule');
  await page.evaluate(()=>closeModal());await page.evaluate(id=>scheduleDetails(id),id);
  await page.locator('#schedule-delete').click();await page.locator('[data-confirm-accept]').click();await page.waitForFunction(()=>!!document.querySelector('#executions-rows'));
  assert.equal((await(await page.request.get(base+'/api/schedules')).json()).length,0);assert.equal((await(await page.request.get(base+'/api/runs')).json()).length,1);
  // A supervisor model chosen while the Pi provider was OpenRouter, after switching the provider to Groq:
  // the scheduled start stops before any run or agent call, a manual start is refused, the UI warns.
  console.log('supervisor model provider');
  const current=await(await page.request.get(base+'/api/settings')).json();r=await page.request.patch(base+'/api/settings',{headers,data:{revision:current._revision,changes:{agents:{pi:{provider:'groq',api_key:'dummy-groq-key'}}}}});assert(r.ok(),await r.text());
  await page.evaluate(async()=>{app.settings=await api('/settings')});
  const stale={mode:'unattended',runtime:'pi',model:'vendor/model',model_provider:'openrouter',instructions:'',max_decisions:50,max_per_node:5,allow_external_writes:false};
  r=await page.request.post(base+'/api/runs',{headers,data:{workflow_id:'schedule-ui-test',input:'manual',base_run:null,execution:stale}});assert.equal(r.status(),400);
  const manualError=(await r.json()).error;
  assert.equal(manualError,'Il modello supervisore vendor/model è stato scelto per Pi (OpenRouter), ma ora il supervisore usa Pi (Groq): correggi il campo Modello supervisore della modalità di esecuzione o lascialo vuoto per usare il modello delle Impostazioni personali');
  assert.equal(await page.evaluate(m=>uiText(m),manualError),'The supervisor model vendor/model was chosen for Pi (OpenRouter), but the supervisor now runs with Pi (Groq): correct the Supervisor model field of the execution mode, or leave it empty to use the model from Personal settings');
  const start=new Date(Date.now()+3600000),pad=n=>String(n).padStart(2,'0'),local=`${start.getFullYear()}-${pad(start.getMonth()+1)}-${pad(start.getDate())}T${pad(start.getHours())}:${pad(start.getMinutes())}`;
  r=await page.request.post(base+'/api/schedules',{headers,data:{workflow_id:'schedule-ui-test',name:'Stale supervisor',kind:'daily',start:local,end:null,timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,every:1,weekdays:[],input:'',execution:stale,base_run:null,active:true}});assert(r.ok(),await r.text());
  const staleId=(await r.json()).id;
  const advanceStale=spawnSync('python3',['-c',"import sqlite3,json,sys,time; db=sqlite3.connect(sys.argv[1]); raw=db.execute('SELECT json FROM schedules WHERE id=?',(sys.argv[2],)).fetchone()[0]; s=json.loads(raw); s['next']=int(time.time())+2; db.execute('UPDATE schedules SET json=? WHERE id=?',(json.dumps(s),sys.argv[2])); db.commit()",path.join(dir,'system/nodo.sqlite'),staleId],{encoding:'utf8'});assert.equal(advanceStale.status,0,advanceStale.stderr);
  let staleHistory=[];
  for(let i=0;i<40&&!staleHistory.length;i++){await page.waitForTimeout(250);staleHistory=await(await page.request.get(base+'/api/schedules/'+staleId+'/history')).json()}
  assert.equal(staleHistory.length,1,JSON.stringify(staleHistory));assert.equal(staleHistory[0].state,'error');assert.equal(staleHistory[0].run_id,null);
  assert.equal(staleHistory[0].message,'Pianificazione Stale supervisor: il modello supervisore vendor/model è stato scelto per Pi (OpenRouter), ma ora il supervisore usa Pi (Groq). Modifica la pianificazione e correggi il campo Modello supervisore in Opzioni avanzate, o lascialo vuoto per usare il modello delle Impostazioni personali');
  assert.equal((await(await page.request.get(base+'/api/runs')).json()).length,1);
  // The interface is in English here: the stored Italian error and the warnings show in English.
  await page.evaluate(id=>scheduleDetails(id),staleId);await page.locator('[data-schedule-model-warning]').waitFor();
  assert.equal(await page.locator('[data-schedule-model-warning]').textContent(),'The supervisor model vendor/model was chosen for Pi (OpenRouter), but the supervisor now runs with Pi (Groq). The next starts will not run: edit the schedule and choose the model again, or leave the field empty.');
  assert((await page.locator('#schedule-live').textContent()).includes('Schedule Stale supervisor: the supervisor model vendor/model was chosen for Pi (OpenRouter), but the supervisor now runs with Pi (Groq). Edit the schedule and correct the Supervisor model field under Advanced options'));
  await page.locator('#schedule-edit').click();await page.locator('#schedule-form').waitFor();
  assert(await page.locator('#schedule-form details').evaluate(el=>el.open));const formWarning=page.locator('#schedule-form [data-supervisor-model-warning]');assert(await formWarning.isVisible());
  assert.equal(await formWarning.textContent(),'This model was chosen for Pi (OpenRouter), but the supervisor now runs with Pi (Groq). Runs will not start until you choose the model again or leave the field empty.');
  // Choosing the model again records the current provider and clears the warning; saving starts working again.
  await page.locator('#schedule-form [name=supervisor_model]').fill('vendor/model');assert(await formWarning.isHidden());
  await page.waitForFunction(()=>!document.querySelector('#schedule-save').disabled);await page.locator('#schedule-save').click();await page.locator('[data-schedule-detail]').waitFor();
  const saved=(await(await page.request.get(base+'/api/schedules')).json()).find(row=>row.schedule.id===staleId).schedule.config.execution;
  assert.deepEqual([saved.model,saved.model_provider],['vendor/model','groq']);assert.equal(await page.locator('[data-schedule-model-warning]').count(),0);
  await page.evaluate(()=>closeModal());
  assert.deepEqual(errors,[]);console.log('Schedule UI smoke passed; screenshots: '+dir);
 }finally{if(browser)await browser.close();server.kill('SIGTERM');}
})().catch(e=>{console.error(e);process.exitCode=1});
