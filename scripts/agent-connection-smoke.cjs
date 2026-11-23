// Test connection for Codex and Claude Code in Personal Settings: form values, saved-key fallback, errors, nothing saved.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'agent-check-')),base='http://127.0.0.1:18149',bin=path.join(dir,'bin');fs.mkdirSync(bin);
 // Offline fixtures: they answer only when the key and the model are the expected ones.
 fs.writeFileSync(path.join(bin,'claude'),`#!/bin/sh
case " $* " in *" --tools  --model good-model "*) ;; *) echo '{"type":"result","is_error":true,"result":"The model does not exist"}'; exit 1 ;; esac
[ "$ANTHROPIC_API_KEY" = "good-claude-key" ] || { echo '{"type":"result","is_error":true,"result":"Invalid API key secret-detail"}'; exit 1; }
echo '{"type":"result","is_error":false,"result":"OK"}'
`,{mode:0o755});
 fs.writeFileSync(path.join(bin,'codex'),`#!/bin/sh
[ "$CODEX_API_KEY" = "good-codex-key" ] || { echo '{"type":"error","message":"401 Unauthorized secret-detail"}'; exit 1; }
echo '{"type":"item.completed","item":{"type":"agent_message","text":"OK"}}'
`,{mode:0o755});
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18149',NODO_SECURE_COOKIE:'0',RIGHTCOMMIT_UPDATE_CHECK:'0',PATH:bin+':'+process.env.PATH},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1280,height:900}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{try{localStorage.setItem('rightcommit.hideAgentSetupNotice','true');if(localStorage.getItem('nodo.language')===null)localStorage.setItem('nodo.language','it')}catch{}});
  assert.equal((await page.request.post(base+'/api/agents/claude/status',{headers:{'X-Nodo-Request':'1'},data:{}})).status(),401);
  assert((await page.request.post(base+'/api/auth/register',{headers:{'X-Nodo-Request':'1'},data:{email:'agents@test.local',password:'temporary-agents-password'}})).ok());
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  const before=await (await page.request.get(base+'/api/settings')).json();
  await page.evaluate(()=>settingsDialog('agents'));
  const section=rt=>page.locator('#settings-form section[data-service="'+rt+'"]'),status=rt=>section(rt).locator('[data-agent-status]');
  const check=async(rt,expected)=>{await section(rt).locator('[data-action=agent-check]').click();await page.waitForFunction(([rt,text])=>document.querySelector('#settings-form section[data-service="'+rt+'"] [data-agent-status]').textContent===text,[rt,expected])};
  assert.equal(await section('pi').locator('[data-action=agent-check]').count(),0);assert.equal(await section('pi').locator('[data-action=pi-check]').count(),1);
  // Claude Code: wrong key, then the right key with a wrong model, then both right.
  await section('claude').locator('[name=key_claude]').fill('wrong-key');await section('claude').locator('[name=model_claude]').fill('good-model');
  await check('claude','Autenticazione fallita. Verifica la chiave API o l’accesso dell’agente.');assert(await status('claude').evaluate(el=>el.classList.contains('error')));
  await section('claude').locator('[name=key_claude]').fill('good-claude-key');await section('claude').locator('[name=model_claude]').fill('missing-model');
  await check('claude','Il modello non è disponibile. Verifica il nome del modello e i permessi del tuo account.');
  await section('claude').locator('[name=model_claude]').fill('good-model');
  await check('claude','Connessione riuscita. Il modello ha risposto correttamente.');assert.equal(await status('claude').evaluate(el=>el.classList.contains('error')),false);
  // Codex is checked on its own; the Claude result stays.
  await section('codex').locator('[name=key_codex]').fill('wrong-key');await check('codex','Autenticazione fallita. Verifica la chiave API o l’accesso dell’agente.');
  await section('codex').locator('[name=key_codex]').fill('good-codex-key');await check('codex','Connessione riuscita. Il modello ha risposto correttamente.');
  assert.equal(await status('claude').textContent(),'Connessione riuscita. Il modello ha risposto correttamente.');
  assert(!(await page.locator('#dialog').innerText()).includes('secret-detail'));
  // Nothing was saved by the checks.
  const after=await (await page.request.get(base+'/api/settings')).json();assert.deepEqual(after.agents,before.agents);
  // English labels.
  await page.evaluate(()=>{localStorage.setItem('nodo.language','en')});await page.reload();await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>settingsDialog('agents'));
  assert.equal((await section('codex').locator('[data-action=agent-check]').innerText()).trim(),'Test connection');
  await section('codex').locator('[name=key_codex]').fill('good-codex-key');await section('codex').locator('[data-action=agent-check]').click();
  await page.waitForFunction(()=>document.querySelector('#settings-form section[data-service="codex"] [data-agent-status]').textContent==='Connection successful. The model responded correctly.');
  assert.deepEqual(errors,[]);console.log('PASS: Test connection for Claude Code and Codex with form values, authentication and model errors, no saved changes, no native output, English labels');
 }finally{await browser?.close();server.kill('SIGINT');fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
