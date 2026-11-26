// The errors of the MCP and RAG tools rightcommit hands to agents are written in English by the server,
// and a person sees each one in the interface language: the English text of this version and the
// Italian one of 1.10.3 and earlier, in a toast and in the RAG search of a real keyword index.
// Disposable local instance, no agents or model calls.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
// [Italian text of earlier versions, English text the server writes now, file that writes it].
const errors=[
 ['Accesso MCP non valido o scaduto','MCP access not valid or expired','src/mcp_client.rs'],
 ['Richiesta MCP fallita. Verifica connessione, tool consentiti e timeout; nessun retry automatico.','MCP request failed. Check the connection, the allowed tools and the timeout; no automatic retry.','src/mcp_client.rs'],
 ['Credenziale MCP mancante','MCP credential missing','src/mcp_client.rs'],
 ['Credenziale MCP non valida','Invalid MCP credential','src/mcp_client.rs'],
 ['Credenziale MCP revocata','MCP credential revoked','src/mcp_client.rs'],
 ['Turno agente MCP non attivo','MCP agent turn not active','src/mcp_client.rs'],
 ['Il task MCP è cambiato','The MCP task has changed','src/mcp_client.rs'],
 ['Tool mancante','Tool name missing','src/mcp_client.rs'],
 ['Timeout metadati MCP','MCP metadata timeout','src/mcp_client.rs'],
 ['Timeout MCP: verifica gli effetti prima del retry','MCP timeout: check the effects before retrying','src/mcp_client.rs'],
 ["Tool MCP non associato all'agente",'MCP tool not assigned to the agent','src/mcp_client.rs'],
 ['Metodo MCP non supportato','Unsupported MCP method','src/mcp_client.rs'],
 ['Metodo metadati MCP non valido','Invalid MCP metadata method','src/mcp_client.rs'],
 ['Percorso non consentito','Path not allowed','src/mcp_workspace.rs'],
 ['Percorso fuori workspace','Path outside the workspace','src/mcp_workspace.rs'],
 ['File troppo grande','File too large','src/mcp_workspace.rs'],
 ['Tool sconosciuto','Unknown tool','src/mcp_workspace.rs'],
 ['Workspace MCP: file o percorso non disponibile','Workspace MCP: file or path not available','src/mcp_workspace.rs'],
 ['Indice RAG non trovato','RAG index not found','src/rag/mod.rs'],
 ['Indice non ancora disponibile','Index not available yet','src/rag/mod.rs'],
 ['Indice non disponibile','Index not available','src/rag/mod.rs'],
 ['Query: 1–4096 caratteri','Query: 1–4096 characters','src/rag/mod.rs'],
 ['Risultati: 1–30','Results: 1–30','src/rag/mod.rs'],
 ['Modalità non valida','Invalid search mode','src/rag/mod.rs'],
 ['Indice keyword: rigenera con embedding per la ricerca semantica','Keyword index: rebuild it with embeddings for semantic search','src/rag/mod.rs'],
 ['chunk_id richiesto','chunk_id required','src/rag/mod.rs'],
 ['La query supera il limite encoder di 512 token; usa una domanda più breve','The query exceeds the encoder limit of 512 tokens; use a shorter question','src/rag/embedding.rs'],
 ['Percorso non valido','Invalid path','src/rag/vector.rs'],
 ['Embedding mancante','Embedding missing','src/rag/vector.rs'],
 ['ID chunk non valido','Invalid chunk ID','src/rag/vector.rs'],
 ['Chunk non trovato nell’indice associato','Chunk not found in the linked index','src/rag/vector.rs'],
 ['Payload indice mancante','Index payload missing','src/rag/vector.rs'],
 ['Payload non valido','Invalid payload','src/rag/vector.rs'],
 ['Vettore non valido','Invalid vector','src/rag/vector.rs']
];
// Each English text is what the server writes now, and the Italian one is gone from its source.
for(const [it,en,file] of errors){
 const source=fs.readFileSync(path.resolve(file),'utf8'),literal=t=>'"'+t.replace('512','{}')+'"';
 assert(source.includes(literal(en)),`${file} does not write: ${en}`);
 assert(!source.includes(literal(it)),`${file} still writes: ${it}`);
}
(async()=>{
 const data=fs.mkdtempSync(path.join(os.tmpdir(),'agent-tool-errors-')),sources=path.join(data,'projects'),base='http://127.0.0.1:18187',headers={'X-Nodo-Request':'1'};let browser;
 fs.mkdirSync(path.join(sources,'docs'),{recursive:true});fs.writeFileSync(path.join(sources,'docs/billing.md'),'# Billing\n\nInvoice reconciliation runs every night.\n');
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{cwd:data,env:{...process.env,NODO_DATA:path.join(data,'data'),NODO_SOURCES:sources,NODO_LISTEN:'127.0.0.1:18187',NODO_ASSETS:path.resolve('.'),NODO_SECURE_COOKIE:'0',RIGHTCOMMIT_UPDATE_CHECK:'0'},stdio:'ignore'});
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),pageErrors=[];page.on('pageerror',e=>pageErrors.push(e.stack));
  for(const f of ['controls.js','app.js','app.css','i18n.js','rag.js'])await page.route('**/'+f,r=>r.fulfill({path:path.resolve('web/'+f)}));
  assert((await page.request.post(base+'/api/auth/register',{headers,data:{email:'tool-errors@test.local',password:'temporary-tool-errors-password'}})).ok());
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(()=>{if(document.querySelector('[data-agent-setup-notice]'))closeModal()});
  const language=async lang=>{await page.evaluate(lang=>{const el=document.querySelector('[data-language]');el.value=lang;el.dispatchEvent(new Event('change',{bubbles:true}))},lang);await page.waitForFunction(lang=>document.documentElement.lang===lang,lang)};
  // Every error, as this version and as earlier versions wrote it, follows the interface language.
  await page.evaluate(pairs=>{const box=document.createElement('div');box.id='tool-error-probes';for(const [it,en] of pairs)for(const t of [en,it]){const p=document.createElement('p');p.textContent=t;box.append(p)}document.body.append(box)},errors);
  for(const lang of ['it','en']){
   await language(lang);
   assert.deepEqual(await page.locator('#tool-error-probes p').allTextContents(),errors.flatMap(([it,en])=>lang==='it'?[it,it]:[en,en]),lang);
  }
  await page.evaluate(()=>document.querySelector('#tool-error-probes').remove());
  // A real API error in a toast: an index that does not exist.
  const missing=await page.request.get(base+'/api/rag/00000000-0000-4000-8000-000000000000');
  assert.equal(missing.status(),400);assert.deepEqual(await missing.json(),{error:'RAG index not found'});
  for(const [lang,expected] of [['it','Indice RAG non trovato'],['en','RAG index not found']]){
   await language(lang);await page.evaluate(()=>api('/rag/00000000-0000-4000-8000-000000000000').catch(e=>toast(e.message)));
   await page.waitForFunction(expected=>document.querySelector('#toast').textContent===expected,expected);
  }
  // A real keyword index searched semantically, as a RAG node set to Semantic would search it.
  const started=await page.request.post(base+'/api/rag',{headers,data:{root:'sources',path:'docs',config:{retrieval:'keyword'}}});assert(started.ok(),await started.text());const id=(await started.json()).id;
  let index;for(let i=0;i<300;i++){index=await (await page.request.get(base+'/api/rag/'+id)).json();if(!['queued','running'].includes(index.progress.state))break;await new Promise(r=>setTimeout(r,100))}
  assert.equal(index.progress.state,'completed',index.progress.message);
  const semantic=await page.request.post(base+'/api/rag/'+id+'/search',{headers,data:{query:'invoice',mode:'semantic',top_k:8}});
  assert.equal(semantic.status(),400);assert.deepEqual(await semantic.json(),{error:'Keyword index: rebuild it with embeddings for semantic search'});
  // The RAG dialog's test search shows the server's error in the interface language.
  await page.route('**/api/rag/'+id+'/search',r=>r.continue({postData:JSON.stringify({...JSON.parse(r.request().postData()),mode:'semantic'})}));
  for(const [lang,expected] of [['it','Indice keyword: rigenera con embedding per la ricerca semantica'],['en','Keyword index: rebuild it with embeddings for semantic search']]){
   await language(lang);await page.evaluate(()=>{if(document.querySelector('#dialog').open)closeModal();return openRag('sources','docs')});
   await page.locator('#rag-search [name=query]').fill('invoice');await page.locator('#rag-search button').click();
   await page.waitForFunction(expected=>document.querySelector('#rag-results')?.textContent===expected,expected);
  }
  await page.unroute('**/api/rag/'+id+'/search');
  await page.locator('#rag-search [name=query]').fill('invoice');await page.locator('#rag-search button').click();
  await page.waitForFunction(()=>document.querySelector('#rag-results details summary')?.textContent.startsWith('billing.md'));
  assert.deepEqual(pageErrors,[]);
  console.log(`PASS: ${errors.length} MCP and RAG tool errors written in English by the server, each shown in Italian and English also as stored by earlier versions; real errors in a toast and in the RAG test search`);
 }finally{await browser?.close();server.kill();fs.rmSync(data,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
