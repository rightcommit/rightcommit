// FinOps with a Codex ChatGPT sign-in: a stand-in codex writes the exec events and the native log
// as Codex does with a ChatGPT plan (rate limits with plan_type) and with an API key (no plan).
// The run works directly in the whole sources folder, which the app saves with a trailing slash:
// both runs must be recovered from their logs and get the same estimated cost at API prices.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright'),{DatabaseSync}=require('node:sqlite');
(async()=>{
 const dir=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'finops-codex-login-'))),data=path.join(dir,'data'),sources=path.join(dir,'sources'),bin=path.join(data,'system/tools/bin'),base='http://127.0.0.1:18177';
 fs.mkdirSync(bin,{recursive:true});fs.mkdirSync(sources);fs.writeFileSync(path.join(sources,'README.md'),'FinOps fixture\n');
 fs.writeFileSync(path.join(bin,'codex'),`#!/usr/bin/env python3
import datetime,json,os,pathlib,sys,uuid
args=sys.argv[1:];sys.stdin.read()
sid=args[args.index('resume')+1] if 'resume' in args else str(uuid.uuid4())
model=args[args.index('--model')+1] if '--model' in args else 'gpt-6.2-sol'
home=pathlib.Path(os.environ['CODEX_HOME']);plan=None
if not os.environ.get('CODEX_API_KEY'):
 try:plan='prolite' if json.loads((home/'auth.json').read_text())['tokens']['access_token'] else None
 except Exception:plan=None
ts=lambda:datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')
usage={'input_tokens':17378,'cached_input_tokens':13696,'cache_write_input_tokens':0,'output_tokens':119,'reasoning_output_tokens':30,'total_tokens':17497}
rate=None if plan is None else {'limit_id':'codex','limit_name':None,'primary':{'used_percent':1.0,'window_minutes':300,'resets_at':0},'secondary':None,'credits':{'has_credits':False,'unlimited':False,'balance':None},'plan_type':plan,'individual_limit':None,'rate_limit_reached_type':None,'spend_control_reached':False}
now=datetime.datetime.now(datetime.timezone.utc);folder=home/'sessions'/now.strftime('%Y/%m/%d');folder.mkdir(parents=True,exist_ok=True)
turn=str(uuid.uuid4())
with open(folder/('rollout-'+now.strftime('%Y-%m-%dT%H-%M-%S')+'-'+sid+'.jsonl'),'a') as log:
 for line in [{'type':'session_meta','payload':{'id':sid,'session_id':sid,'cwd':os.getcwd(),'originator':'codex_exec','source':'exec','model_provider':'openai'}},
  {'type':'turn_context','payload':{'turn_id':turn,'cwd':os.getcwd(),'model':model}},
  {'type':'event_msg','payload':{'type':'task_started','turn_id':turn}},
  {'type':'token_usage_record','payload':{'response_id':'r','turn_id':turn,'root_turn_id':turn,'session_id':sid,'thread_id':sid,'usage':usage,'turn_token_usage':usage,'thread_token_usage':usage}},
  {'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':usage,'last_usage_includes_reasoning':True,'model_context_window':475000},'rate_limits':rate}},
  {'type':'event_msg','payload':{'type':'task_complete','turn_id':turn}}]:
  line['timestamp']=ts();log.write(json.dumps(line)+'\\n')
result='<nodo_result>'+json.dumps({'status':'completed','summary':'Done','artifacts':[],'questions':[]})+'</nodo_result>'
for event in [{'type':'thread.started','thread_id':sid},{'type':'turn.started'},{'type':'item.completed','item':{'id':'item_0','type':'agent_message','text':result}},
 {'type':'turn.completed','usage':{k:v for k,v in usage.items() if k!='total_tokens'}}]:print(json.dumps(event),flush=True)
`,{mode:0o755});
 const env={...process.env,PATH:bin+path.delimiter+process.env.PATH,NODO_DATA:data,NODO_LISTEN:'127.0.0.1:18177',NODO_SOURCES:sources,NODO_SECURE_COOKIE:'0'};
 for(const key of ['CODEX_API_KEY','OPENAI_API_KEY','CODEX_ACCESS_TOKEN','OPENAI_BASE_URL'])delete env[key];
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:dir,env,stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{try{localStorage.setItem('rightcommit.hideAgentSetupNotice','true');if(localStorage.getItem('nodo.language')===null)localStorage.setItem('nodo.language','it')}catch{}});
  const api=async(p,b,method)=>{const r=await page.request.fetch(base+'/api'+p,{method:method||(b?'POST':'GET'),headers:{'X-Nodo-Request':'1'},data:b});assert(r.ok(),await r.text());return r.json()};
  await api('/auth/register',{email:'finops-login@test.local',password:'temporary-finops-password'});
  // The ChatGPT sign-in, as codex login writes it in the account's own Codex folder.
  fs.mkdirSync(path.join(data,'users/1/.codex'),{recursive:true});
  fs.writeFileSync(path.join(data,'users/1/.codex/auth.json'),JSON.stringify({tokens:{access_token:'stand-in',id_token:'stand-in'}}));
  const wf={id:'finops-plan',name:'Plan workflow',description:'Offline fixture',entry:'task',nodes:[{id:'source',kind:'Source',title:'Sources',source_type:'folder',source_mode:'direct',location:''},{id:'agent',kind:'Agent',title:'Codex',runtime:'codex'},{id:'task',kind:'Task',title:'Work',agent:'agent',prompt:'Offline FinOps test'}],edges:[{from:'source',to:'agent',kind:'context'},{from:'agent',to:'task',kind:'context'}]};
  await api('/workflows',wf);
  const run=async()=>{const r=await api('/runs',{workflow_id:wf.id});for(let i=0;i<300;i++){const d=await api('/runs/'+r.id);assert.notEqual(d.run.status,'failed',d.run.error);if(d.run.status==='completed')return d.run;await new Promise(r=>setTimeout(r,100))}throw new Error('run did not complete')};
  const plan=await run();
  assert(plan.workspace.endsWith('/'),plan.workspace); // the case the native log must still match
  // Then the same workflow with a personal API key, which takes precedence over the sign-in.
  let settings=await api('/settings');
  settings=await api('/settings',{revision:settings._revision,changes:{agents:{codex:{api_key:'sk-finops-stand-in'}}}},'PATCH');
  const keyed=await run();
  // Reference prices for the stand-in model, so the estimate does not depend on the network.
  const now=Math.floor(Date.now()/1000),pricing={prompt:'0.00000125',completion:'0.00001',input_cache_read:'0.000000125'};
  const db=new DatabaseSync(path.join(data,'system/nodo.sqlite'));db.exec('PRAGMA busy_timeout=5000');
  db.prepare('INSERT INTO finops_catalog(id,json) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET json=excluded.json').run(JSON.stringify({version:1,fetched_at:now,attempted_at:now,stale:false,data:[{id:'openai/gpt-6.2-sol',pricing}]}));db.close();
  const report=await api('/finops');
  assert(report.pricing.available&&report.pricing.prices['openai/gpt-6.2-sol']);
  const metrics=id=>report.runs.find(r=>r.id===id).metrics;
  const p=metrics(plan.id),k=metrics(keyed.id);
  // (17378 - 13696) input + 13696 cache read + 119 output at the prices above.
  const cost=3682*1.25e-6+13696*1.25e-7+119*1e-5,near=(a,b)=>assert(Math.abs(a-b)<1e-12,`${a} != ${b}`);
  for(const m of [p,k]){
   assert.equal(m.records,1);assert.equal(m.input,17378);assert.equal(m.output,119);assert.equal(m.cache_read,13696);assert.equal(m.reasoning,30);
   assert.equal(m.historical_codex_records,1);assert.equal(m.estimated_records,1);assert.equal(m.usd,null);near(m.estimated_usd,cost);
   assert(!('subscription_records' in m));
  }
  near(p.estimated_usd,k.estimated_usd);
  const s=report.summary.metrics;
  assert.equal(s.total,2*(17378+119));assert.equal(s.estimated_records,2);near(s.estimated_usd,2*cost);
  assert.equal(report.summary.historical_codex_sessions_unavailable,0);
  const model=report.models.find(m=>m.name==='codex · gpt-6.2-sol');assert.equal(model.metrics.records,2);near(model.metrics.estimated_usd,2*cost);
  near(report.agents.reduce((n,a)=>n+(a.metrics.estimated_usd||0),0),2*cost);
  near(report.workflows.find(w=>w.id===wf.id).metrics.estimated_usd,2*cost);
  // The view, in Italian then in English: the ChatGPT run shows its model, tokens and estimate.
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.locator('#finops').click();await page.locator('[data-finops-refresh]').waitFor();
  const text=()=>page.locator('#dialog').innerText(),usd=v=>page.evaluate(v=>fusd(v),v);
  const chart=title=>page.locator('.finops-chart').filter({has:page.locator('h3',{hasText:title})});
  const noSubscription=async()=>{const t=(await text()).toLowerCase();assert(!t.includes('abbonamento')&&!t.includes('subscription'),'no subscription label')};
  await page.locator('[data-finops-coverage] summary').click();
  assert((await text()).includes('2 record con costo stimato'));
  assert((await text()).includes('Costo LLM stimato'));assert((await text()).includes(await usd(2*cost)));
  for(const title of ['Ripartizione costi per modello','Confronto costi per workflow']){
   const c=chart(title);assert.equal(await c.count(),1,title);assert(!(await c.innerText()).includes('Nessun dato disponibile.'),title);
  }
  assert((await chart('Ripartizione costi per modello').innerText()).includes('gpt-6.2-sol'));
  await noSubscription();
  await page.screenshot({path:path.join(dir,'finops-codex-login-it.png'),fullPage:true});
  await page.locator('[data-finops-view=runs]').click();
  assert.equal(await page.locator('#dialog tbody tr').filter({hasText:await usd(cost)}).count(),2);
  await page.locator(`[data-finops-run="${plan.id}"]`).click();
  const detail=await text();
  assert(detail.includes('17.497'));assert(detail.includes(await usd(cost)));assert(detail.includes('Costo LLM stimato'));
  await noSubscription();
  await page.screenshot({path:path.join(dir,'finops-codex-login-run-it.png'),fullPage:true});
  await page.evaluate(()=>{const s=document.querySelector('[data-language]');s.value='en';s.dispatchEvent(new Event('change',{bubbles:true}))});
  assert((await text()).includes('Estimated LLM cost'));assert((await text()).includes(await usd(cost)));
  await noSubscription();
  await page.screenshot({path:path.join(dir,'finops-codex-login-run-en.png'),fullPage:true});
  await page.locator('[data-finops-view=models]').click();
  assert((await text()).includes('codex · gpt-6.2-sol'));assert((await text()).includes(await usd(2*cost)));
  await page.locator('[data-finops-view=overview]').click();
  assert((await text()).includes('2 records with estimated cost'));assert((await text()).includes(await usd(2*cost)));
  assert(!(await chart('Cost breakdown by model').innerText()).includes('No data available.'));
  await noSubscription();
  await page.screenshot({path:path.join(dir,'finops-codex-login-en.png'),fullPage:true});
  assert.deepEqual(errors,[]);
  console.log('PASS: ChatGPT sign-in and API key runs with a trailing-slash workspace recovered from their logs with the same estimated cost, in totals, groupings and charts, IT/EN view');
 }finally{await browser?.close();server.kill('SIGINT');if(!process.env.KEEP_FINOPS_SMOKE)fs.rmSync(dir,{recursive:true,force:true});else console.log(dir)}
})().catch(e=>{console.error(e);process.exit(1)});
