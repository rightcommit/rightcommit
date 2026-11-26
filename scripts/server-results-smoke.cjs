// Results the server writes into a run are stored in English, as the next agents receive them, and the
// interface shows them in its language: a new run, a run stored with the Italian text of 1.10.3 and
// earlier, and every view that shows such a result. Disposable local instance, no agents or model calls.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const data=fs.mkdtempSync(path.join(os.tmpdir(),'server-results-')),base='http://127.0.0.1:18183',headers={'X-Nodo-Request':'1'};let browser;
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:data,env:{...process.env,NODO_DATA:data,NODO_LISTEN:'127.0.0.1:18183',NODO_ASSETS:path.resolve('.'),NODO_SECURE_COOKIE:'0',RIGHTCOMMIT_UPDATE_CHECK:'0'},stdio:'ignore'});
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.stack));
  for(const f of ['controls.js','app.js','app.css','i18n.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  assert((await page.request.post(base+'/api/auth/register',{headers,data:{email:'results@test.local',password:'temporary-results-password'}})).ok());
  // A run that only commits locally, Unattended: the report gains the line about the missing confirmation.
  const workflow={id:'server-results',name:'Server results',description:'',entry:'commit',execution:{mode:'unattended'},nodes:[{id:'source',kind:'Source',title:'Source',source_type:'example',x:40,y:40},{id:'commit',kind:'Connection',title:'Commit',provider:'local',operation:'commit',confirm_commit:true,x:240,y:40}],edges:[{from:'source',to:'commit',kind:'context',outcome:''}]};
  assert((await page.request.post(base+'/api/workflows',{headers,data:workflow})).ok());
  const started=await page.request.post(base+'/api/runs',{headers,data:{workflow_id:workflow.id,input:'Commit'}});assert(started.ok(),await started.text());const rid=(await started.json()).id;
  let detail;for(let i=0;i<300;i++){detail=await (await page.request.get(base+'/api/runs/'+rid)).json();if(['completed','failed'].includes(detail.run.status))break;await new Promise(r=>setTimeout(r,100))}
  assert.equal(detail.run.status,'completed',detail.run.error);
  const stored=detail.run.outputs.commit.summary,sha=stored.match(/^Local commit verified: ([0-9a-f]{40})\.$/m)?.[1];
  assert(sha,stored);
  assert.equal(stored,`Local commit verified: ${sha}.\nFiles from the run in the commit: none.\nNothing was pushed, merged or deployed.\nCommitted without confirmation: the run is in Unattended mode, so nobody could approve it.`);
  const report=detail.artifacts.find(a=>a.path==='.rightcommit/reports/commit.md');assert(report);
  const english=[`Local commit verified: ${sha}.`,'Files from the run in the commit: none.','Nothing was pushed, merged or deployed.','Committed without confirmation: the run is in Unattended mode, so nobody could approve it.'];
  const italian=[`Commit locale verificato: ${sha}.`,'File della run nel commit: nessuno.','Nessun push, merge o deploy eseguito.','Commit eseguito senza conferma: la run è in modalità non presidiata, quindi nessuno poteva approvarlo.'];
  const language=async lang=>page.evaluate(lang=>{const el=document.querySelector('[data-language]');el.value=lang;el.dispatchEvent(new Event('change',{bubbles:true}))},lang);
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  const shown=async(lang,label)=>{
   await language(lang);await page.evaluate(id=>{if(document.querySelector('#dialog').open)closeModal();return loadRun(id)},rid);
   await page.evaluate(id=>artifact(id),report.id);const lines=page.locator('#dialog .artifact-view .commit-report p');await lines.first().waitFor();
   assert.deepEqual(await lines.allTextContents(),lang==='it'?italian:english,label+' '+lang);
   await page.evaluate(()=>closeModal());
  };
  for(const lang of ['it','en'])await shown(lang,'new run');
  // The same run as 1.10.3 stored it: the Italian report, in the output the next steps receive and in the artifact.
  const old=italian.join('\n');
  const rewrite=spawnSync('python3',['-c',`import sqlite3,json,sys
db=sqlite3.connect(sys.argv[1]);rid,text=sys.argv[2],sys.argv[3]
run=json.loads(db.execute('SELECT json FROM runs WHERE id=?',(rid,)).fetchone()[0]);run['outputs']['commit']['summary']=text
db.execute('UPDATE runs SET json=? WHERE id=?',(json.dumps(run),rid))
for aid,raw in db.execute('SELECT id,json FROM artifacts WHERE run=?',(rid,)).fetchall():
 a=json.loads(raw)
 if a['path']=='.rightcommit/reports/commit.md':a['content']=text;db.execute('UPDATE artifacts SET json=? WHERE id=?',(json.dumps(a),aid))
db.commit()`,path.join(data,'system/nodo.sqlite'),rid,old],{encoding:'utf8'});
  assert.equal(rewrite.status,0,rewrite.stderr);
  assert.equal((await (await page.request.get(base+'/api/runs/'+rid)).json()).run.outputs.commit.summary,old);
  for(const lang of ['it','en'])await shown(lang,'stored Italian run');
  // Every other result the server writes, new (English) and stored by earlier versions (Italian), in each view.
  const receipt='{"actor":"human","decision":"approve","answer":"Looks right · ok","user":"reviewer@test.local"}';
  const cases=[
   ['Review approved\nApproval recorded by the server: '+receipt,'Review approvata\nApprovazione registrata dal server: '+receipt,'Review approved\nApproval recorded by the server: '+receipt],
   ['Convergence not reached: tasks still open: 3; previous report rework=false. Deterministic check, no approval simulated.','Convergenza non raggiunta: 3 task aperti; rapporto precedente rework=false. Check deterministico, nessuna approvazione simulata.'],
   ["The server's tests are missing or no longer valid for the current files. Keep the fixes and go back through implementation, CI and review before delivery.",'I test del server sono mancanti o non più validi per i file correnti. Conserva le correzioni e ripassa da implementazione, CI e review prima della consegna.'],
   ['The gate changed the workspace after the tests: CI and review must run again.\n\nThe agent kept *its* text.','Il gate ha modificato il workspace dopo i test. Nuova CI e review richieste.\n\nThe agent kept *its* text.'],
   ['Agent summary\nApproval recorded by the server: '+receipt,'Agent summary\nApprovazione registrata dal server: '+receipt]
  ];
  for(const lang of ['it','en']){
   await language(lang);
   for(const [en,it] of cases){
    const expected=lang==='it'?it:en;
    for(const stored of [en,it]){
     // Edit and continue lists the kept results.
     const listed=await page.evaluate(summary=>{const box=document.createElement('div');box.innerHTML=revisionSteps([{title:'Step',kind:'Gate',reason:'kept',summary}]);return [...box.querySelectorAll('.revision-result')].map(p=>p.querySelectorAll('p').length?[...p.querySelectorAll('p')].map(x=>x.textContent).join('\n'):p.textContent).join('\n')},stored);
     assert.equal(listed,expected,`revision ${lang}: ${stored}`);
    }
   }
   // A Gate report that opens with the server's line keeps the agent's Markdown below it.
   for(const stored of ['<p>The gate changed the workspace after the tests: CI and review must run again.</p><p>The agent kept <em>its</em> text.</p>','<p>Il gate ha modificato il workspace dopo i test. Nuova CI e review richieste.</p><p>The agent kept <em>its</em> text.</p>']){
    const html=await page.evaluate(html=>serverMarkdown(html),stored);
    assert.equal(html,lang==='it'?'<p>Il gate ha modificato il workspace dopo i test. Nuova CI e review richieste.</p><p>The agent kept <em>its</em> text.</p>':'<p>The gate changed the workspace after the tests: CI and review must run again.</p><p>The agent kept <em>its</em> text.</p>');
   }
   // Requests waiting for a decision, and the supervisor's name in the decision log.
   for(const [en,it,provider] of [['Authorize create_pr on github? Check the target and the artifacts in the settings and in the run.','Autorizzare create_pr su github?  Controlla target e artefatti nelle impostazioni e nel run.','github'],['Authorize twg on atlassian? twg jira issue view KEY-1 Check the target and the artifacts in the settings and in the run.','Autorizzare twg su atlassian? twg jira issue view KEY-1 Controlla target e artefatti nelle impostazioni e nel run.','atlassian'],['The A2A service asks for a clarification','Il servizio A2A richiede un chiarimento','']]){
    for(const question of [en,it]){
     await page.evaluate(({question,provider})=>{if(document.querySelector('#dialog').open)closeModal();app.reviewRun=null;app.run={id:'results-ui-run',status:'waiting_review',current:'step',created:1793706088,updated:1793706157,workflow:{id:'w',name:'W',nodes:[{id:'step',kind:provider?'Connection':'A2A',provider}]},pending:{kind:provider?'effect':'question',node:'step',questions:[question]}};return pending()},{question,provider});
     assert((await page.locator('#dialog-content').innerText()).includes(lang==='it'?it.replace('?  ','? '):en),`${lang}: ${question}`);
    }
   }
   for(const user of ['AI supervisor','Supervisore AI']){
    await page.evaluate(user=>{if(document.querySelector('#dialog').open)closeModal();app.decisions=[{id:'d',actor:'ai',run:'results-ui-run',node:'review',decision:'approve',answer:'',user,created:1793706088,artifacts:[],supervisor:{rationale:'Checked',session:'s',turn:1}}];reviewHistory()},user);
    assert((await page.locator('.review-record').innerText()).includes('review · '+(lang==='it'?'Supervisore AI':'AI supervisor')));
   }
   await page.evaluate(()=>closeModal());
  }
  // Run errors stay Italian in the source and follow the interface, with the agent's words as written.
  await language('en');assert.deepEqual(await page.evaluate(()=>['Agente bloccato: Missing data','Review rifiutata. Correggi il workflow o avvia una nuova esecuzione.'].map(uiText)),['Agent blocked: Missing data','Review rejected. Fix the workflow or start a new run.']);
  // An agent's own words are never translated, even when they look like an interface label.
  await language('it');assert.equal(await page.evaluate(()=>serverLines('New feature\nSave\nReview approved')),'New feature\nSave\nReview approvata');
  assert.deepEqual(errors,[]);
  console.log('PASS: server-written results stored in English, shown in Italian and English for a new run and a run stored in Italian: commit report, Review approval, Gate checks, approval requests, supervisor name');
 }finally{await browser?.close();server.kill();fs.rmSync(data,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
