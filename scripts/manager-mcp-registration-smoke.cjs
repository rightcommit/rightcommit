// The Project Manager proposes an MCP server registration and the user confirms it in the chat.
// A disposable server runs with scripts/fixtures/manager on PATH (test only): no provider calls.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('assert/strict'),{spawn}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const SECRET='smoke-server-variable-value',LITERAL='smoke-literal-secret-value';
(async()=>{const dir=fs.mkdtempSync(path.join(os.tmpdir(),'manager-mcp-')),base='http://127.0.0.1:18171';
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,PATH:path.resolve('scripts/fixtures/manager')+path.delimiter+process.env.PATH,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18171',NODO_SOURCES:path.resolve('sources'),RIGHTCOMMIT_SMOKE_MCP_TOKEN:SECRET},stdio:'ignore'});let browser;try{
 for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
 browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 for(const f of ['controls.js','app.js','app.css','i18n.js','a2a.js','mcp.js','rag.js','manager.js','mcp-oauth.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
 await page.route(base+'/',r=>r.fulfill({path:path.resolve('web/index.html'),contentType:'text/html'}));
 await page.addInitScript(()=>{localStorage.setItem('rightcommit.hideAgentSetupNotice','true');localStorage.setItem('nodo.language','it')});
 const call=async(route,body,method)=>{const response=await page.request.fetch(base+'/api'+route,{method:method||(body?'POST':'GET'),headers:{'X-Nodo-Request':'1'},data:body});const raw=await response.text();return {status:response.status(),raw,json:JSON.parse(raw)}};
 // The first account is the first administrator (ID 1).
 assert.equal((await call('/auth/register',{email:'mcp-registration@test.local',password:'temporary-registration-password'})).status,200);
 const w={id:'mcp-registration-'+Date.now(),name:'MCP registration smoke',description:'',entry:'read',nodes:[{id:'source',kind:'Source',title:'Source',source_type:'example',x:80,y:100},{id:'read',kind:'Connection',title:'Read',provider:'local',operation:'read_ticket',location:'README.md',x:320,y:100}],edges:[{from:'source',to:'read',kind:'context'}]};
 assert.equal((await call('/workflows',w)).status,200);
 await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
 await page.locator('#workflow-select').click();await page.locator(`#workflow-topic-menu [data-workflow-choice="${w.id}"]`).click();await page.waitForFunction(id=>app.wf?.id===id,w.id);
 const history=page.waitForResponse(r=>r.url().includes('/api/manager/chat?'));await page.locator('#manager-toggle').click();await history;
 const send=async text=>{await page.locator('#manager-input').fill(text);const response=page.waitForResponse(r=>r.url()===base+'/api/manager/chat'&&r.request().method()==='POST');await page.locator('#manager-form [type=submit]').click();const r=await response;const raw=await r.text();assert.equal(r.status(),200,raw);await page.locator('#manager-form [type=submit]').waitFor();return {raw,json:JSON.parse(raw)}};
 const servers=async()=>(await call('/settings')).json.mcp_servers||{};

 // 1. The proposal is shown and nothing is saved before the confirmation.
 let reply=await send('registra il server mcp');
 assert.equal(reply.json.mcp_registration.server_id,'smoke-mcp');assert.deepEqual(reply.json.mcp_registration.env_from_server,['RIGHTCOMMIT_SMOKE_MCP_TOKEN']);
 assert(!reply.raw.includes(SECRET));
 await page.locator('.manager-mcp-proposal').waitFor();assert.equal((await page.locator('.manager-mcp-proposal b').textContent()),'Smoke MCP');
 assert.deepEqual(await servers(),{});
 // 2. The confirmation shows exactly what will be registered.
 await page.locator('#manager-mcp-review').click();await page.locator('#manager-mcp-confirm').waitFor();
 const dialog=await page.locator('#dialog').textContent();
 for(const text of ['Registrare il server MCP?','smoke-mcp','Smoke MCP',reply.json.mcp_registration.command,'server.py','SMOKE_MODE=test','SMOKE_API_KEY','da compilare in Configurazioni personali → MCP','RIGHTCOMMIT_SMOKE_MCP_TOKEN','Registra server'])assert(dialog.includes(text),text);
 assert(!dialog.includes(SECRET)&&!dialog.includes('Esiste già'));
 // The same dialog in English.
 await page.evaluate(()=>{const select=document.querySelector('[data-language]');select.value='en';select.dispatchEvent(new Event('change',{bubbles:true}))});await page.waitForFunction(()=>document.documentElement.lang==='en');
 const english=await page.locator('#dialog').textContent();
 for(const text of ['Register the MCP server?','Check what will be registered in Personal settings → MCP. Nothing is saved until you confirm.','Variables from the server environment','to fill in under Personal settings → MCP','Register server'])assert(english.includes(text),text);
 // Server messages about server variables have exact English entries; names are shown as written.
 const messages={
  'La variabile MY_TOKEN non è impostata nell’ambiente del server o è vuota: aggiungila e riavvia rightcommit':'Variable MY_TOKEN is not set in the server environment or is empty: add it and restart rightcommit',
  'La variabile HOME è impostata da rightcommit e non può arrivare dall’ambiente del server':'Variable HOME is set by rightcommit and cannot come from the server environment',
  'Solo il primo amministratore (ID 1) può passare variabili dall’ambiente del server a un server MCP':'Only the first administrator (ID 1) can pass variables from the server environment to an MCP server',
  'La variabile MY_TOKEN è indicata due volte':'Variable MY_TOKEN is listed twice',
  'La variabile MY_TOKEN è sia nelle variabili d’ambiente sia in quelle dall’ambiente del server: tienila in una sola':'Variable MY_TOKEN is both in the environment variables and in the variables from the server environment: keep it in only one',
  'Esiste già un server MCP con ID smoke-mcp: conferma la sostituzione':'An MCP server with ID smoke-mcp already exists: confirm the replacement',
  'La proposta di registrazione MCP contiene un valore per MY_API_KEY, che sembra un segreto: indica solo il nome della variabile':'The MCP registration proposal contains a value for MY_API_KEY, which looks like a secret: give only the variable name',
  'La proposta di registrazione MCP non è più disponibile':'The MCP registration proposal is no longer available',
  'Al massimo 20 variabili dall’ambiente del server per un server MCP':'At most 20 variables from the server environment for one MCP server',
 };
 assert.deepEqual(await page.evaluate(messages=>Object.keys(messages).map(m=>uiText(m)),messages),Object.values(messages));
 await page.evaluate(()=>{const select=document.querySelector('[data-language]');select.value='it';select.dispatchEvent(new Event('change',{bubbles:true}))});await page.waitForFunction(()=>document.documentElement.lang==='it');
 // 3. Confirming saves it through the settings path and resolves the proposal.
 const saved=page.waitForResponse(r=>r.url().endsWith('/api/manager/chat/mcp/register'));await page.locator('#manager-mcp-confirm').click();const savedResponse=await saved;assert.equal(savedResponse.status(),200);
 const savedBody=await savedResponse.text();assert(!savedBody.includes(SECRET));
 await page.locator('#dialog').waitFor({state:'hidden'});assert.equal(await page.locator('.manager-mcp-proposal').count(),0);
 assert((await page.locator('#manager-feedback').textContent()).includes('Server MCP registrato in Configurazioni personali.'));
 let stored=(await servers())['smoke-mcp'];assert.equal(stored.transport,'stdio');assert.deepEqual(stored.env_from_server,['RIGHTCOMMIT_SMOKE_MCP_TOKEN']);assert.deepEqual(Object.keys(stored.env).sort(),['SMOKE_API_KEY','SMOKE_MODE']);
 assert.equal((await call('/manager/chat?workflow_id='+w.id)).json.mcp_registration,null);
 // Test connection starts it with the server variable, and never returns the value.
 const probe=await call('/mcp/smoke-mcp/probe',{});assert.equal(probe.status,200,probe.raw);assert.equal(probe.json.tools[0].name,'token_present');assert(!probe.raw.includes(SECRET));
 // The MCP form shows the names to the first administrator.
 await page.evaluate(()=>mcpEdit('smoke-mcp'));assert.equal(await page.locator('#mcp-form [name=env_from_server]').inputValue(),'RIGHTCOMMIT_SMOKE_MCP_TOKEN');await page.keyboard.press('Escape');
 // A missing variable stops the start with its name.
 const revision=(await call('/settings')).json._revision;
 assert.equal((await call('/settings',{revision,changes:{mcp_servers:{'smoke-missing':{name:'Missing',transport:'stdio',command:stored.command,args:stored.args,env_from_server:['RIGHTCOMMIT_SMOKE_MISSING'],auth:{mode:'none'}}}}},'PATCH')).status,200);
 const missing=await call('/mcp/smoke-missing/probe',{});assert.equal(missing.status,400);assert(missing.json.error.includes('RIGHTCOMMIT_SMOKE_MISSING'),missing.raw);
 // 4. The same ID again: the confirmation says it is a replacement; dismissing saves nothing.
 reply=await send('registra il server mcp di nuovo');await page.locator('#manager-mcp-review').click();await page.locator('#manager-mcp-confirm').waitFor();
 assert((await page.locator('#dialog').textContent()).includes('Esiste già un server MCP con questo ID: la conferma lo sostituisce.'));assert.equal((await page.locator('#manager-mcp-confirm').textContent()).trim(),'Sostituisci server');
 await page.locator('#dialog [data-action=close-dialog]').last().click();
 const dismissed=page.waitForResponse(r=>r.url().endsWith('/api/manager/chat/proposal/resolve'));await page.locator('#manager-mcp-dismiss').click();assert((await dismissed).ok());
 assert.equal(await page.locator('.manager-mcp-proposal').count(),0);assert.equal((await call('/manager/chat?workflow_id='+w.id)).json.mcp_registration,null);
 // A stale confirmation of a resolved proposal is refused.
 assert.equal((await call('/manager/chat/mcp/register',{conversation_id:reply.json.conversation_id,proposal_id:reply.json.mcp_registration.id,revision:(await call('/settings')).json._revision,replace:true})).status,400);
 // 5. A proposal with a secret literal is refused and sent back once; the corrected one has no value.
 reply=await send('registra il server mcp con segreto');
 assert(!reply.raw.includes(LITERAL));assert.equal(reply.json.mcp_registration.env.SMOKE_API_KEY,'');
 await page.locator('#manager-mcp-review').click();await page.locator('#manager-mcp-confirm').waitFor();assert(!(await page.locator('#dialog').textContent()).includes(LITERAL));
 await page.keyboard.press('Escape');
 assert(!fs.readFileSync(path.join(dir,'system/nodo.sqlite')).includes(SECRET));
 assert.deepEqual(errors,[]);console.log('PASS: manager MCP registration proposal, review, confirm, replacement, dismiss, secret refusal and server variables');
 }finally{await browser?.close();server.kill('SIGINT');fs.rmSync(dir,{recursive:true,force:true})}})().catch(e=>{console.error(e);process.exit(1)});
