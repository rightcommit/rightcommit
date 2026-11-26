// UI contract test for the local commit confirmation: a saved-format preview, no agents or Git.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const data=fs.mkdtempSync(path.join(os.tmpdir(),'commit-approval-')),base='http://127.0.0.1:18264',headers={'X-Nodo-Request':'1'};let browser;
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:data,env:{...process.env,NODO_DATA:data,NODO_LISTEN:'127.0.0.1:18264',NODO_SECURE_COOKIE:'0',RIGHTCOMMIT_UPDATE_CHECK:'0'},stdio:'ignore'});
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.stack));
  for(const f of ['controls.js','app.js','app.css','i18n.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  assert((await page.request.post(base+'/api/auth/register',{headers,data:{email:'commit@test.local',password:'temporary-commit-password'}})).ok());
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  const limits={file_lines:400,file_bytes:49152,total_bytes:131072,diff_files:200,new_file_bytes:1048576,test_chars:8000};
  const preview={files:[
   {path:'src/lib.rs',status:'modified',binary:false,added:2,removed:1,diff:'@@ -1,2 +1,3 @@\n fn a() {}\n-fn b() {}\n+fn b() -> u8 { 1 }\n+// <script>window.pwned=true</script>\n',cut:false,note:''},
   {path:'docs/new <img src=x onerror=window.pwned=1>.md',status:'added',binary:false,added:900,removed:0,diff:'@@ -0,0 +1,900 @@\n'+Array.from({length:399},(_,i)=>'+line '+i).join('\n')+'\n',cut:true,note:''},
   {path:'old.txt',status:'deleted',binary:false,added:0,removed:1,diff:'@@ -1 +0,0 @@\n-bye\n',cut:false,note:''},
   {path:'logo.png',status:'added',binary:true,added:null,removed:null,diff:'',cut:false,note:''},
   {path:'dump.sql',status:'added',binary:false,added:null,removed:0,diff:'',cut:false,note:'too_large'},
   {path:'zz.txt',status:'modified',binary:false,added:3,removed:3,diff:'',cut:false,note:'limit'}],
   more_files:0,kept:['notes/user notes.txt'],more_kept:0,changed:false,
   test:{state:'ran',node:'tests',command:'cargo test (rilevato da Cargo.toml)',output:'running 3 tests\ntest result: ok. 3 passed',output_cut:true,current:true},limits};
  const show=async(commit)=>page.evaluate(async commit=>{
   if(document.querySelector('#dialog').open)closeModal();
   app.reviewRun=null;app.run={id:'commit-ui-run',status:'waiting_review',current:'commit',created:1793706088,updated:1793706157,workflow:{id:'commit-workflow',name:'Commit demo',nodes:[{id:'commit',kind:'Connection',provider:'local',operation:'commit',confirm_commit:true}]},pending:{kind:'effect',node:'commit',questions:['Confirm the local commit (files: 6)? Check the files, the changes and the tests before approving.'],commit}};
   await pending();
  },commit);
  const language=async lang=>page.evaluate(lang=>{const el=document.querySelector('[data-language]');el.value=lang;el.dispatchEvent(new Event('change',{bubbles:true}))},lang);
  for(const lang of ['it','en'])for(const width of [1440,390]){
   await page.setViewportSize({width,height:1000});await language(lang);await show(preview);
   const section=page.locator('.commit-approval');await section.waitFor();
   assert.match(await section.locator('h3').textContent(),lang==='it'?/Conferma il commit locale/:/Confirm the local commit/);
   assert.equal(await page.locator('.commit-file').count(),6);
   const text=await section.innerText();
   assert(text.includes(lang==='it'?'File nel commit · 6':'Files in the commit · 6'),text);
   assert(text.includes(lang==='it'?'Differenze troncate: mostrate 399 delle 900 righe aggiunte o rimosse.':'Changes truncated: showing 399 of 900 added or removed lines.'),'truncation note');
   assert(text.includes(lang==='it'?'File binario: il contenuto non viene mostrato.':'Binary file: its content is not shown.'));
   assert(text.includes(lang==='it'?'File nuovo oltre 1 MiB':'New file over 1 MiB'));
   assert(text.includes(lang==='it'?'Differenze non mostrate: l’anteprima mostra al massimo 200 file o 128 KiB di differenze.':'Changes not shown: the preview shows at most 200 files or 128 KiB of changes.'));
   assert(text.includes(lang==='it'?'Comando di test: cargo test (rilevato da Cargo.toml)':'Test command: cargo test (detected from Cargo.toml)'),'test command');
   assert(text.includes(lang==='it'?'Mostrati gli ultimi 8000 caratteri dell’output.':'Showing the last 8000 characters of the output.'));
   assert(text.includes('notes/user notes.txt')&&text.includes('test result: ok. 3 passed'));
   assert(text.includes(lang==='it'?'Limiti dell’anteprima: 400 righe o 48 KiB per file, 200 file e 128 KiB di differenze in tutto.':'Preview limits: 400 lines or 48 KiB per file, 200 files and 128 KiB of changes in total.'));
   assert.equal(await page.locator('.commit-changed').count(),0);
   // Diff lines keep their text and colour, untranslated and escaped.
   const diff=page.locator('.commit-file').first().locator('.commit-diff');
   assert.deepEqual(await diff.locator('span').evaluateAll(s=>s.map(x=>x.className)),['diff-hunk','diff-line','diff-del','diff-add','diff-add']);
   assert.equal(await diff.locator('.diff-add').last().textContent(),'+// <script>window.pwned=true</script>');
   assert.equal(await page.evaluate(()=>window.pwned),undefined);assert.equal(await section.locator('script,img').count(),0);
   assert(await page.locator('#dialog-content').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'no horizontal page scroll');
   for(const decision of ['approve','reject'])assert(await page.locator(`[data-action=decide][data-value=${decision}]`).isVisible());
   if(width===1440)await page.screenshot({path:path.join(os.tmpdir(),'commit-approval-'+lang+'.png'),fullPage:true});
  }
  // Asked again after a change; no test step in the workflow.
  await show({...preview,changed:true,test:{state:'missing'}});
  assert.match(await page.locator('.commit-changed').textContent(),/The files changed after the approval: the commit was not created/);
  assert((await page.locator('.commit-approval').innerText()).includes('The workflow has no local test Connection node'));
  // The decision request is the existing one.
  let submitted;await page.route('**/api/runs/commit-ui-run/decide',r=>{submitted=r.request().postDataJSON();return r.fulfill({status:400,json:{error:'Test-only decision capture'}})});
  await page.locator('#decision-answer').fill('Diff reviewed');await page.locator('[data-action=decide][data-value=approve]').click();
  for(let i=0;i<30&&!submitted;i++)await page.waitForTimeout(50);assert.deepEqual(submitted,{action:'approve',answer:'Diff reviewed'});
  await page.evaluate(()=>closeModal());
  // The node option: on for a new commit node, saved when switched off.
  const workflow={id:'commit-node',name:'Commit node',description:'',entry:'old',nodes:[{id:'old',kind:'Connection',title:'Old commit',provider:'local',operation:'commit',x:100,y:100}],edges:[]};
  assert((await page.request.post(base+'/api/workflows',{headers,data:workflow})).ok());
  await page.evaluate(async()=>{app.run=null;app.workflows=await api('/workflows');selectWorkflow('commit-node');app.view='graph';app.selected='old';renderInspector()});
  const form=page.locator('#node-form');
  assert.equal(await form.locator('jui-switch[name=confirm_commit]').evaluate(s=>s.checked),false,'saved without the field: off');
  assert((await form.textContent()).includes('Confirm before commit'));
  await page.evaluate(()=>{newNode('Connection');const f=document.querySelector('#node-form').elements.operation;f.value='commit';f.dispatchEvent(new Event('change',{bubbles:true}))});
  assert.equal(await form.locator('jui-switch[name=confirm_commit]').evaluate(s=>s.checked),true,'new commit node: on');
  await form.locator('jui-switch[name=confirm_commit]').click();
  assert.equal(await page.evaluate(()=>currentWorkflow().nodes.at(-1).confirm_commit),false);
  assert.deepEqual(errors,[]);
  console.log('PASS: commit preview files, diff colours, truncation, binary and size notes, kept edits, test evidence, changed notice, IT/EN, mobile, escaping, unchanged decision request and the node switch');
 }finally{await browser?.close();if(server.exitCode===null){server.kill('SIGINT');await new Promise(r=>server.once('exit',r))}fs.rmSync(data,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exitCode=1});
