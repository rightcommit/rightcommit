// Real browser check: a Source of type "Folder in /sources" whose folder does not exist. A manual
// start asks whether to create it, in Italian and in English: No leaves nothing on disk and starts
// no run, Create makes the folder and the run starts. API starts without the confirmation and a
// file in place of the folder are refused with errors that name the Source and the folder.
// A disposable server with a temporary NODO_SOURCES; no agent or external service is used: the
// workflows run a skipped Task and stop at a human Review.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('node:assert/strict'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const port=process.env.SMOKE_PORT||'18361',dir=fs.mkdtempSync(path.join(os.tmpdir(),'source-folder-missing-')),base='http://127.0.0.1:'+port,data=path.join(dir,'data'),sources=path.join(dir,'sources');
 for(const d of [data,sources])fs.mkdirSync(d);
 const server=spawn(process.env.NODO_TEST_BINARY||path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:data,NODO_LISTEN:'127.0.0.1:'+port,NODO_SOURCES:sources},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>localStorage.setItem('rightcommit.hideAgentSetupNotice','true'));
  const call=async(url,body,status=200)=>{const r=await page.request.fetch(base+'/api'+url,{method:body?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:body});assert.equal(r.status(),status,url+': '+await r.text());return r.json()};
  await call('/auth/register',{email:'source-folder-missing@test.local',password:'temporary-source-folder-password'});
  // Folder Sources, an Agent with no credentials, a skipped Task as entry and a human Review.
  const workflow=(id,list)=>({id,name:'Missing folder '+id,description:'',entry:'read',nodes:[...list.map(([sid,title,mode,location],i)=>({id:sid,kind:'Source',title,source_type:'folder',source_mode:mode,location,x:60,y:80+160*i})),
   {id:'reader',kind:'Agent',title:'Reader',x:300,y:300},{id:'read',kind:'Task',title:'Read',agent:'reader',skip:true,x:300,y:80},{id:'review',kind:'Review',title:'Check',mode:'human',prompt:'Check',x:540,y:80}],
   edges:[...list.map(([sid])=>({from:sid,to:'reader',kind:'context'})),{from:'reader',to:'read',kind:'context'},{from:'read',to:'review',kind:'control'}]});
  await call('/workflows',workflow('one',[['project','Progetto QA','direct','qa-new/project']]));
  await call('/workflows',workflow('two',[['alpha','Alpha','direct','qa-two/a'],['beta','','copy','qa-two/b']]));
  await call('/workflows',workflow('file',[['gamma','Gamma','copy','qa-file']]));
  fs.writeFileSync(path.join(sources,'qa-file'),'not a folder');
  const workspaces=()=>{try{return fs.readdirSync(path.join(data,'users/1/workspaces'))}catch{return []}};
  const runs=id=>call('/runs?workflow_id='+id);
  const language=async value=>{await page.evaluate(value=>{const select=document.querySelector('[data-language]');select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}))},value);await page.waitForFunction(value=>document.documentElement.lang===value,value)};
  const openWorkflow=async id=>{await page.locator('#workflow-select').click();await page.locator(`#workflow-topic-menu [data-workflow-choice="${id}"]`).click();await page.waitForFunction(id=>app.wf?.id===id,id)};
  const openStart=async()=>{await page.locator('#execute').click();await page.locator('[data-run-now]').click();await page.locator('[data-action=start-run]').waitFor({state:'visible'})};
  const startDialog=async()=>{await page.locator('[data-action=start-run]').click();const dialog=page.locator('.app-confirm');await dialog.waitFor({state:'visible'});
   return {title:await dialog.locator('h2').textContent(),message:await dialog.locator('#confirmation-message').textContent(),buttons:await dialog.locator('.modal-actions button').allTextContents()}};
  const answer=async create=>{await page.locator(create?'.app-confirm [data-confirm-accept]':'.app-confirm .modal-actions [data-confirm-cancel]').click();await page.locator('.app-confirm').waitFor({state:'detached'})};
  const started=async id=>{await page.locator('#close-run').waitFor({state:'visible'});await page.waitForFunction(()=>app.run?.status==='waiting_review',{},{timeout:30000});const list=await runs(id);assert.equal(list.length,1);return list[0]};
  const checks=[];
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  // Italian, one missing folder: No starts nothing and creates nothing.
  await language('it');await openWorkflow('one');await openStart();
  assert.deepEqual(await startDialog(),{title:'Cartella Source mancante',message:'Il Source Progetto QA usa la cartella qa-new/project, che non esiste in /sources. Vuoi crearla e avviare l’esecuzione?',buttons:['No','Crea']});
  await answer(false);await page.waitForFunction(()=>!document.querySelector('[data-action=start-run]').disabled);
  assert(await page.locator('#run-error').isHidden());assert.deepEqual(await runs('one'),[]);assert(!fs.existsSync(path.join(sources,'qa-new')));assert.deepEqual(workspaces(),[]);
  checks.push('IT single dialog, No leaves no folder, run or workspace');
  // English, same start: Create makes the nested folder and the run starts on it.
  await language('en');
  assert.deepEqual(await startDialog(),{title:'Missing Source folder',message:'The Source Progetto QA uses the folder qa-new/project, which does not exist in /sources. Create it and start the run?',buttons:['No','Create']});
  await answer(true);const one=await started('one');
  assert(fs.statSync(path.join(sources,'qa-new/project')).isDirectory());assert.deepEqual(fs.readdirSync(path.join(sources,'qa-new/project')),[]);
  assert.equal((await call('/runs/'+one.id)).run.workspace,fs.realpathSync(path.join(sources,'qa-new/project')));
  checks.push('EN single dialog, Create makes qa-new/project and the run starts on it');
  await page.locator('#close-run').click();
  // Two Sources missing at once, a direct one and a copied one: one dialog lists both.
  await openWorkflow('two');await openStart();const before=workspaces();
  assert.deepEqual(await startDialog(),{title:'Missing Source folders',message:'These Sources use folders that do not exist in /sources: Alpha (qa-two/a), beta (qa-two/b). Create them and start the run?',buttons:['No','Create']});
  await answer(false);await page.waitForFunction(()=>!document.querySelector('[data-action=start-run]').disabled);
  assert.deepEqual(await runs('two'),[]);assert(!fs.existsSync(path.join(sources,'qa-two')));assert.deepEqual(workspaces(),before);
  checks.push('EN plural dialog, No leaves nothing');
  await language('it');
  assert.deepEqual(await startDialog(),{title:'Cartelle Source mancanti',message:'Questi Source usano cartelle che non esistono in /sources: Alpha (qa-two/a), beta (qa-two/b). Vuoi crearle e avviare l’esecuzione?',buttons:['No','Crea']});
  await answer(true);await started('two');
  for(const f of ['qa-two/a','qa-two/b'])assert(fs.statSync(path.join(sources,f)).isDirectory(),f);
  checks.push('IT plural dialog, Create makes both folders and the run starts');
  await page.locator('#close-run').click();
  // A file in place of the folder: no dialog, the reason next to Start in both languages.
  await openWorkflow('file');await openStart();await page.locator('[data-action=start-run]').click();
  await page.locator('#run-error').waitFor({state:'visible'});
  assert.equal(await page.locator('#run-error').textContent(),'Source Gamma: qa-file in /sources è un file, non una cartella. Scegli una cartella');
  assert.equal(await page.locator('.app-confirm').count(),0);
  await language('en');assert.equal(await page.locator('#run-error').textContent(),'Source Gamma: qa-file in /sources is a file, not a folder. Choose a folder');
  assert(fs.statSync(path.join(sources,'qa-file')).isFile());assert.deepEqual(await runs('file'),[]);
  checks.push('file in place of the folder refused, IT and EN');
  await page.keyboard.press('Escape');
  // The API: no creation without the flag, the structured refusal names each Source and folder.
  fs.rmSync(path.join(sources,'qa-two'),{recursive:true});
  const refused=await call('/runs',{workflow_id:'two'},409);
  assert.equal(refused.code,'source_folders_missing');
  assert.deepEqual(refused.missing,[{node:'alpha',title:'Alpha',folder:'qa-two/a'},{node:'beta',title:'beta',folder:'qa-two/b'}]);
  assert.equal(refused.error,'Le cartelle di questi Source non esistono in /sources: Alpha (qa-two/a), beta (qa-two/b). Creale o scegli altre cartelle');
  assert(!fs.existsSync(path.join(sources,'qa-two')));
  const size=await call('/storage/source-size?path=qa-two/b');assert.equal(size.missing,true);assert.equal(size.bytes,0);assert(!fs.existsSync(path.join(sources,'qa-two')));
  assert.equal((await call('/storage/source-size?path=qa-file')).not_folder,'qa-file');
  checks.push('API refusal 409 source_folders_missing without the flag, size preview of a missing folder and of a file');
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({pass:true,checks}));
 }finally{await browser?.close();server.kill('SIGINT');await new Promise(r=>setTimeout(r,300));fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
