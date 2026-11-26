// Disposable native server and CLI: no real credentials or provider calls.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'manager-errors-')),base='http://127.0.0.1:18157';let browser,server;
 try{
  fs.writeFileSync(path.join(dir,'codex'),`#!/usr/bin/env python3
import json,sys,pathlib,time
prompt=sys.stdin.read()
sid='errors-smoke-session'
print(json.dumps({'type':'thread.started','thread_id':sid}),flush=True)
mode=pathlib.Path(${JSON.stringify(path.join(dir,'mode'))}).read_text()
time.sleep(0.4)
follow_up='ended without the required <nodo_result> block' in prompt
# The follow-up takes long enough for the chat to poll its notice.
if follow_up and mode=='plain':time.sleep(2.5)
with open(${JSON.stringify(path.join(dir,'calls'))},'a') as calls:calls.write(json.dumps({'mode':mode,'follow_up':follow_up,'resume':'resume' in sys.argv[1:]})+chr(10))
# An answer without the structured result; the follow-up gives it only in mode plain.
if mode in ('plain','plain-twice') and not (follow_up and mode=='plain'):
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'OUTPUT-LINE-1'+chr(10)+chr(10)+'OUTPUT-LINE-2'}}),flush=True)
 print(json.dumps({'type':'turn.completed','usage':{'input_tokens':1,'output_tokens':1}}),flush=True)
 sys.exit(0)
if mode=='fail':
 print(json.dumps({'type':'turn.failed','error':{'message':'unexpected status 401 Unauthorized: Incorrect API key provided: fake-partial******** url: https://provider.invalid request id: secret-request'}}),flush=True)
 sys.exit(1)
result={'status':'completed','summary':'Output reported above.' if mode=='plain' else 'Recovered successfully','details':'','questions':[],'artifacts':[],'decision':'explain'}
print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'<nodo_result>'+json.dumps(result)+'</nodo_result>'}}),flush=True)
print(json.dumps({'type':'turn.completed','usage':{'input_tokens':1,'output_tokens':1}}),flush=True)
`);fs.chmodSync(path.join(dir,'codex'),0o755);fs.writeFileSync(path.join(dir,'mode'),'fail');
  server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:dir,env:{...process.env,PATH:dir+path.delimiter+process.env.PATH,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18157'},stdio:'ignore'});
  for(let i=0;i<150;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:950}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  for(const f of ['controls.js','app.js','app.css','i18n.js','a2a.js','mcp.js','rag.js','manager.js','mcp-oauth.js','icons.svg','schedules.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  await page.route(base+'/',r=>r.fulfill({path:path.resolve('web/index.html'),contentType:'text/html'}));
  const call=async(p,body)=>{const r=await page.request.fetch(base+'/api'+p,{method:body?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:body});assert(r.ok(),await r.text());return r.json()};
  await call('/auth/register',{email:'manager-errors@test.local',password:'temporary-manager-password'});
  const w={id:'manager-errors-test',name:'Error recovery',description:'Disposable test',entry:'',nodes:[],edges:[]};await call('/workflows',w);
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});await page.evaluate(id=>selectWorkflow(id),w.id);await page.locator('#manager-toggle').click();
  const input=page.locator('#manager-input'),submit=page.locator('#manager-form [type=submit]');
  const send=async(text,following)=>{await input.fill(text);const response=page.waitForResponse(r=>r.url()===base+'/api/manager/chat'&&r.request().method()==='POST');await submit.click();if(following)await input.fill(following);const r=await response;await page.waitForFunction(()=>!document.querySelector('#manager-form [type=submit]').disabled);return r};
  for(const lang of ['it','en']){
   await page.evaluate(value=>{const el=document.querySelector('[data-language]');el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}))},lang);
   const response=await send('  retry me  ');assert.equal(response.status(),400);const payload=await response.json();assert(!JSON.stringify(payload).match(/fake-partial|secret-request|turn.failed|provider.invalid/));
   await page.locator('.manager-error').waitFor();await page.setViewportSize({width:390,height:844});await input.scrollIntoViewIfNeeded();assert(await input.evaluate(el=>{const r=el.getBoundingClientRect();return r.height>0&&r.bottom<=innerHeight+1}),JSON.stringify(await input.evaluate(el=>({input:el.getBoundingClientRect().toJSON(),form:el.closest('form').getBoundingClientRect().toJSON(),panel:el.closest('#manager-panel').getBoundingClientRect().toJSON(),height:innerHeight}))));await page.setViewportSize({width:1440,height:950});assert.match(await page.locator('.manager-error p').textContent(),lang==='it'?/credenziali/:/credentials/);
   await page.screenshot({path:path.join(os.tmpdir(),'rightcommit-manager-error-'+lang+'.png')});assert.equal(await input.inputValue(),'  retry me  ');assert(await input.isEditable());assert(await input.evaluate(el=>el===document.activeElement));assert.equal((await call('/auth/me')).email,'manager-errors@test.local');
   await input.fill('continue typing');await input.evaluate(el=>el.setSelectionRange(3,7));await page.locator('#manager-dismiss-error').click();assert.equal(await page.locator('.manager-error').count(),0);assert.equal(await input.inputValue(),'continue typing');assert.deepEqual(await input.evaluate(el=>[el.selectionStart,el.selectionEnd]),[3,7]);await input.press('End');await input.type(' more');assert.equal(await input.inputValue(),'continue typing more');
  }
  await send('retry me','new draft while waiting');assert.equal(await input.inputValue(),'new draft while waiting');
  await page.locator('#manager-error-settings').click();await page.locator('#settings-panel-agents').waitFor({state:'visible'});await page.keyboard.press('Escape');
  const history=await call('/manager/chat?workflow_id='+w.id);assert.equal(history.messages.length,1,'Retry must not duplicate the unanswered user message');assert.equal(history.messages[0].role,'user');assert(!JSON.stringify(history).includes('fake-partial'));
  fs.writeFileSync(path.join(dir,'mode'),'success');const response=await send('retry me');assert.equal(response.status(),200,await response.text());assert.equal(await page.locator('.manager-error').count(),0);assert.match(await page.locator('#manager-messages').textContent(),/Recovered successfully/);assert.equal(await input.inputValue(),'');
  // An answer without the structured result gets one follow-up in the same session; the text already written is the reply.
  const calls=()=>fs.readFileSync(path.join(dir,'calls'),'utf8').trim().split('\n').map(line=>JSON.parse(line));let seen=calls().length;
  // While the follow-up runs, the chat's activity line shows the same notice as the Activity log of a run.
  fs.writeFileSync(path.join(dir,'mode'),'plain');
  for(const lang of ['it','en']){
   await page.evaluate(value=>{const el=document.querySelector('[data-language]');el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}))},lang);
   const sent=send('CASE report the output verbatim, nothing else');
   await page.locator('#manager-activity').getByText(lang==='it'?'Risultato strutturato mancante: richiesto all’agente':'Structured result missing: requested from the agent',{exact:true}).waitFor({timeout:5000});
   const plain=await sent;assert.equal(plain.status(),200,await plain.text());assert.equal(await page.locator('#manager-activity').count(),0,'the notice goes away with the turn');
   const reply=(await plain.json()).messages.at(-1);assert.equal(reply.role,'assistant');assert.equal(reply.text,'OUTPUT-LINE-1\n\nOUTPUT-LINE-2');assert(!reply.text.includes('Output reported above'));
   await page.locator('.manager-message').last().getByText('OUTPUT-LINE-2').waitFor();assert.equal(await page.locator('.manager-error').count(),0);
   assert.deepEqual(calls().slice(seen),[{mode:'plain',follow_up:false,resume:true},{mode:'plain',follow_up:true,resume:true}]);seen=calls().length;
  }
  // If the follow-up has no structured result either, the error stays, with no third turn.
  fs.writeFileSync(path.join(dir,'mode'),'plain-twice');const twice=await send('CASE report the output again');assert.equal(twice.status(),400);
  assert.equal((await twice.json()).error,'Risposta agente priva del risultato strutturato rightcommit. Riprendi nella stessa sessione con Retry.');
  await page.locator('.manager-error').waitFor();assert.match(await page.locator('.manager-error p').textContent(),/no rightcommit structured result/);
  assert.deepEqual(calls().slice(seen),[{mode:'plain-twice',follow_up:false,resume:true},{mode:'plain-twice',follow_up:true,resume:true}]);
  assert.deepEqual(errors,[]);
  console.log('PASS: actual native 401, safe HTTP/history, IT/EN, dismiss and focus/caret, draft recovery, typing during failure, settings action, deduplicated retry and successful recovery, one structured-result follow-up in the same session keeping the written reply with its IT/EN notice in the chat, and the unchanged error with no third turn');
 }finally{await browser?.close();if(server&&server.exitCode===null){server.kill('SIGINT');await new Promise(resolve=>server.once('exit',resolve))}fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exitCode=1});
