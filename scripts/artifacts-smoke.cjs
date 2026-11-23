const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
// A one-page PDF with correct cross-reference offsets, so the browser's reader opens it.
const pdfFixture=(()=>{
 const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>','','<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
 const stream='BT /F1 24 Tf 40 60 Td (PDF preview) Tj ET';objects[3]='<< /Length '+stream.length+' >>\nstream\n'+stream+'\nendstream';
 let body='%PDF-1.4\n%\u0000\u00e2\u00e3\u00cf\u00d3\n';const offsets=objects.map((object,i)=>{const at=body.length;body+=(i+1)+' 0 obj\n'+object+'\nendobj\n';return at});
 const xref=body.length;body+='xref\n0 6\n0000000000 65535 f \n'+offsets.map(at=>String(at).padStart(10,'0')+' 00000 n \n').join('')+'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n'+xref+'\n%%EOF\n';
 return Buffer.from(body,'latin1');
})();
const yamlFixture='name: demo\nservices:\n  web:\n    image: nginx\n    ports:\n    - "80:80"\n    - "443:443"\n  db:\n    image: postgres\n# end\nversion: 2\n';
const xmlFixture='<?xml version="1.0"?>\n<!-- note\n  <fake> -->\n<root a="x>y">\n  <item id="1">\n    <name>A</name>\n  </item>\n  <empty/>\n  <b><c>\n    text</c></b>\n</root>\n';
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'run-artifacts-')),base='http://127.0.0.1:18162',bin=path.join(dir,'bin'),sources=path.join(dir,'sources');fs.mkdirSync(bin);
 for(const folder of ['main','second','copied']){fs.mkdirSync(path.join(sources,folder),{recursive:true});fs.writeFileSync(path.join(sources,folder,'original.txt'),'original');fs.writeFileSync(path.join(sources,folder,'unchanged.txt'),'untouched');}
 const second=path.join(sources,'second');
 fs.writeFileSync(path.join(bin,'claude'),`#!/usr/bin/env python3
import os,sys,json,pathlib,re
prompt=sys.stdin.read();args=sys.argv
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
second_turn='SECOND TURN' in prompt
cwd=pathlib.Path.cwd()
(cwd/'original.txt').write_text('second version' if second_turn else 'first version')
(cwd/'.hidden').mkdir(exist_ok=True)
(cwd/'.hidden/result.md').write_text('# Hidden output')
(cwd/'binary.dat').write_bytes(b'\\0\\xffsecond' if second_turn else b'\\0\\xfffirst')
(cwd/'large.bin').write_bytes(b'x'*(1024*1024+1))
import base64
(cwd/'chart.png').write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='))
(cwd/'data.json').write_text('{"id":12345678901234567890,"items":[1,{"deep":true}],"empty":{}}')
(cwd/'tool.py').write_text('import sys\\n\\ndef main():\\n    return 0\\n')
(cwd/'LEGACY.cbl').write_bytes(b'      * Attivit\\xe0 gi\\xe0 chiusa\\n       MOVE A TO B.\\n')
(cwd/'compose.yaml').write_bytes(base64.b64decode('${Buffer.from(yamlFixture).toString('base64')}'))
(cwd/'layout.xml').write_bytes(base64.b64decode('${Buffer.from(xmlFixture).toString('base64')}'))
(cwd/'manual.pdf').write_bytes(base64.b64decode('${pdfFixture.toString('base64')}'))
(cwd/'report.html').write_text('<h1 id="t">Report</h1><script>document.getElementById("t").textContent="Script ran";try{parent.document.title="REACHED"}catch(e){document.body.append(" isolated")}fetch("/api/auth/me").then(function(){document.body.append(" NETWORK")}).catch(function(){document.body.append(" no-network")})</script>')
pathlib.Path(${JSON.stringify(second)}).joinpath('result.txt').write_text('secondary output')
paths=re.findall(r'"workspace_path"\\s*:\\s*"([^"\\n]+)"',prompt)
for raw in paths:
 p=pathlib.Path(raw)
 if 'copied' in p.parts:
  if not p.is_absolute():p=cwd/p
  if p.is_dir():(p/'copied-output.txt').write_text('copied output')
def emit(x):print(json.dumps(x),flush=True)
emit(dict(type='system',subtype='init',session_id=sid))
emit(dict(type='result',result='<nodo_result>'+json.dumps(dict(status='completed',summary='Fixture output',artifacts=[],questions=[]))+'</nodo_result>',is_error=False))
`,{mode:0o755});
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18162',NODO_SOURCES:sources,PATH:bin+':'+process.env.PATH,ANTHROPIC_AUTH_TOKEN:'offline-artifact-fixture'},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  // The assertions read the Italian source labels; a first launch is in English.
  await page.addInitScript(()=>{try{if(localStorage.getItem('nodo.language')===null)localStorage.setItem('nodo.language','it')}catch{}});
  for(const file of ['app.js','app.css','i18n.js'])await page.route('**/'+file,r=>r.fulfill({path:path.resolve('web/'+file)}));
  const api=async(p,b)=>{const r=await page.request.fetch(base+'/api'+p,{method:b?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:b});assert(r.ok(),await r.text());return r.json()};
  await api('/auth/register',{email:'artifacts@test.local',password:'temporary-artifacts-password'});
  for(const mode of ['direct','copy']){
   for(const name of ['binary.dat','large.bin','.hidden','chart.png','report.html','data.json','tool.py','LEGACY.cbl','manual.pdf','compose.yaml','layout.xml'])fs.rmSync(path.join(sources,'main',name),{recursive:true,force:true});
   fs.writeFileSync(path.join(sources,'main','original.txt'),'original');fs.rmSync(path.join(second,'result.txt'),{force:true});
   const wf={id:'artifacts-'+mode,name:'Artifacts '+mode,description:'Offline artifact checks',entry:'first',nodes:[
    {id:'source',kind:'Source',title:'Primary',source_type:'folder',source_mode:mode,location:'main'},
    {id:'second',kind:'Source',title:'Second',source_type:'folder',source_mode:'direct',location:'second'},
    {id:'copied',kind:'Source',title:'Copied',source_type:'folder',source_mode:'copy',location:'copied'},
    {id:'agent',kind:'Agent',title:'Claude fixture',runtime:'claude'},
    {id:'first',kind:'Task',title:'First task',agent:'agent',prompt:'FIRST TURN'},
    {id:'last',kind:'Task',title:'Second task',agent:'agent',prompt:'SECOND TURN'}],edges:[
    {from:'source',to:'agent',kind:'context'},{from:'second',to:'agent',kind:'context'},{from:'copied',to:'agent',kind:'context'},
    {from:'agent',to:'first',kind:'context'},{from:'agent',to:'last',kind:'context'},{from:'first',to:'last',kind:'control'}]};
   wf.nodes.forEach((n,i)=>{n.x=80+i*145;n.y=120});
   await api('/workflows',wf);const run=await api('/runs',{workflow_id:wf.id});let data;
   for(let i=0;i<300;i++){data=await api('/runs/'+run.id);assert.notEqual(data.run.status,'failed',data.run.error);if(data.run.status==='completed')break;await new Promise(r=>setTimeout(r,50))}
   assert.equal(data.run.status,'completed');
   const files=data.artifacts.filter(a=>a.file.change!=='report');assert(!files.some(a=>a.path.endsWith('unchanged.txt')));
   assert(files.some(a=>a.path.endsWith('copied-output.txt')),'Additional copied source output');assert(files.some(a=>a.path.endsWith('result.txt')&&a.file.root==='/sources'));
   const versions=files.filter(a=>a.path==='binary.dat'||a.path==='main/binary.dat');assert.equal(versions.length,2);
   const first=versions.find(a=>a.version===1),last=versions.find(a=>a.version===2);assert(first&&last);
   const large=files.find(a=>a.path.endsWith('large.bin'));assert(large);const largePreview=await api('/artifacts/'+large.id);assert(largePreview.artifact.file.preview);assert.equal(largePreview.artifact.content.length,1024*1024+1);assert.equal((await (await page.request.get(base+'/api/artifacts/'+large.id+'/download')).body()).length,1024*1024+1);
   const old=await page.request.get(base+'/api/artifacts/'+first.id+'/download');assert.deepEqual(await old.body(),Buffer.from([0,255,...Buffer.from('first')]));
   const full=await page.request.get(base+'/api/artifacts/'+last.id+'/download');assert.deepEqual(await full.body(),Buffer.from([0,255,...Buffer.from('second')]));
   assert.equal(last.file.root,mode==='direct'?'/sources':'/data');
   await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.evaluate(id=>loadRun(id),run.id);await page.locator('.node-main[data-id=first]').click();
   await page.locator('[data-artifact-query]').waitFor();
   const list=page.locator('[data-inspector-artifacts]>.artifact-item');assert.equal(await list.count(),new Set(files.map(a=>a.file.location)).size);assert.equal(await list.locator('time.artifact-date').count(),await list.count());assert((await list.locator('time.artifact-date').first().innerText()).match(/\d{2}:\d{2}/));
   assert((await page.locator('[data-inspector-artifacts] .artifact-origin').allTextContents()).includes('sources'));
   assert((await page.locator('[data-inspector-artifacts] .artifact-origin').allTextContents()).includes('data'));
   const times=await list.evaluateAll((rows,arts)=>rows.map(row=>{const a=arts.find(a=>a.id===row.dataset.id);return a.file.modified||a.created*1000}),data.artifacts);assert.deepEqual(times,[...times].sort((a,b)=>b-a));
   await page.locator('[data-artifact-query]').fill('binary.dat');
   assert.equal(await list.evaluateAll(rows=>rows.filter(row=>!row.hidden).length),1);
   await page.locator('[data-inspector-artifacts]>.artifact-item:visible').click();await page.locator('#artifact-location').waitFor({state:'visible'});assert((await page.locator('#dialog .modal-body').innerText()).includes('Anteprima non disponibile per questo formato'));
   const download=page.waitForEvent('download');assert.equal(await page.locator('#artifact-download').innerText(),'Download');assert(!(await page.locator('#artifact-download').getAttribute('class')).includes('primary'));const close=page.locator('#dialog .modal-actions [data-action=close-dialog]');assert((await close.getAttribute('class')).includes('primary'));assert((await page.locator('#artifact-download').boundingBox()).x<(await close.boundingBox()).x);await page.locator('#artifact-download').click();assert.equal((await download).suggestedFilename(),'binary.dat');await page.keyboard.press('Escape');await page.locator('#dialog').waitFor({state:'hidden'});
   await page.locator('[data-artifact-query]').fill('.hidden');assert.equal(await list.evaluateAll(rows=>rows.filter(row=>!row.hidden).length),1);
   await page.locator('[data-artifact-query]').fill('');
   // The hidden-files switch is a browser preference that survives a reload.
   const hiddenSwitch=page.locator('[data-artifact-show-hidden]'),hiddenRows=()=>list.evaluateAll(rows=>rows.filter(row=>!row.hidden&&row.dataset.artifactHidden==='true').length);
   assert.equal(await hiddenSwitch.getAttribute('aria-checked'),'true');assert(await hiddenRows()>0);
   await hiddenSwitch.click();assert.equal(await page.evaluate(()=>localStorage.getItem('rightcommit.artifactsShowHidden')),'false');assert.equal(await hiddenRows(),0);
   await page.reload();await page.locator('#app').waitFor({state:'visible'});await page.evaluate(id=>loadRun(id),run.id);await page.locator('.node-main[data-id=first]').click();await page.locator('[data-artifact-query]').waitFor();
   assert.equal(await hiddenSwitch.getAttribute('aria-checked'),'false');assert.equal(await hiddenRows(),0);
   await hiddenSwitch.click();assert.equal(await page.evaluate(()=>localStorage.getItem('rightcommit.artifactsShowHidden')),'true');assert(await hiddenRows()>0);
   // Images and HTML open in the viewer; the HTML runs isolated from the app and without network access.
   const openArtifact=async name=>{await page.locator('[data-artifact-query]').fill(name);await page.locator('[data-inspector-artifacts]>.artifact-item:visible').click();await page.locator('#artifact-location').waitFor({state:'visible'})};
   const closeArtifact=async()=>{await page.keyboard.press('Escape');await page.locator('#dialog').waitFor({state:'hidden'})};
   const pageTitle=await page.title();
   await openArtifact('chart.png');await page.waitForFunction(()=>document.querySelector('#dialog img.file-view-image')?.naturalWidth===1);assert.equal(await page.locator('#dialog .tabs').count(),0);await closeArtifact();
   // A PDF opens in the browser's reader: its own type, no sandbox, original bytes, no Source or Compare tabs.
   // HTML and PDF frames fill the dialog: nothing scrolls around them and they end at the content's edge.
   const frameFills=()=>page.evaluate(()=>{const frame=document.querySelector('#dialog iframe.file-view-frame'),view=frame.parentElement,body=document.querySelector('#dialog .modal-body');return frame.clientHeight>=400&&view.scrollHeight<=view.clientHeight&&body.scrollHeight<=body.clientHeight&&Math.abs(frame.getBoundingClientRect().bottom-view.getBoundingClientRect().bottom)<=1});
   await openArtifact('manual.pdf');const pdfFrame=page.locator('#dialog iframe.file-view-frame');assert.equal(await pdfFrame.getAttribute('sandbox'),null);assert.equal(await page.locator('#dialog .tabs').count(),0);
   const pdfResponse=await page.request.get(base+await pdfFrame.getAttribute('src'));assert.equal(pdfResponse.headers()['content-type'],'application/pdf');assert(!pdfResponse.headers()['content-security-policy'].includes('sandbox'));assert.deepEqual(await pdfResponse.body(),pdfFixture);
   assert.equal(await page.locator('#dialog a.file-view-open').getAttribute('href'),await pdfFrame.getAttribute('src'));assert(await frameFills());if(process.env.VIEW_SHOTS){await page.waitForTimeout(1500);await page.screenshot({path:path.join(process.env.VIEW_SHOTS,'pdf-'+mode+'.png')})}await closeArtifact();
   if(mode==='direct'){await page.evaluate(()=>openStorage('main/manual.pdf','sources'));assert.equal(await page.locator('#dialog iframe.file-view-frame').getAttribute('sandbox'),null);assert.equal(await page.locator('#dialog .file-view-source').count(),0);assert(await frameFills());if(process.env.VIEW_SHOTS){await page.waitForTimeout(1500);await page.screenshot({path:path.join(process.env.VIEW_SHOTS,'pdf-storage.png')})}await page.keyboard.press('Escape');await page.locator('#dialog').waitFor({state:'hidden'})}
   await openArtifact('report.html');const frame=page.locator('#dialog iframe.file-view-frame');assert.equal(await frame.getAttribute('sandbox'),'allow-scripts');
   const report=frame.contentFrame().locator('body');await report.getByText('no-network').waitFor();const reportText=await report.innerText();
   assert(reportText.includes('Script ran')&&reportText.includes('isolated')&&!reportText.includes('NETWORK'),reportText);assert.equal(await page.title(),pageTitle);
   // The same file opens in its own tab, still isolated: opaque origin, no network.
   const openLink=page.locator('#dialog a.file-view-open');assert.equal(await openLink.getAttribute('target'),'_blank');assert.equal(await openLink.getAttribute('href'),await frame.getAttribute('src'));
   const [tab]=await Promise.all([page.context().waitForEvent('page'),openLink.click()]);await tab.getByText('no-network').waitFor();
   assert.equal(await tab.evaluate(()=>origin),'null');const tabText=await tab.locator('body').innerText();assert(tabText.includes('Script ran')&&!tabText.includes('NETWORK'),tabText);await tab.close();
   await page.locator('#dialog [data-action=artifact-mode][data-mode=source]').click();await page.locator('#dialog pre.source-view').waitFor();assert((await page.locator('#dialog pre.source-view').innerText()).includes('<h1 id="t">Report</h1>'));assert.equal(await page.locator('#dialog iframe').count(),0);await closeArtifact();
   await page.locator('[data-artifact-query]').fill('');
   // Text files show line numbers; JSON is indented, keeps its values as written and folds.
   const numbers=()=>page.locator('#dialog .code-line:visible').evaluateAll(rows=>rows.map(row=>row.dataset.n).join(','));
   await openArtifact('tool.py');assert.equal(await numbers(),'1,2,3,4');assert.equal(await page.locator('#dialog .code-fold').count(),0);
   // Long content scrolls inside its own area: the version, mode and path controls stay in place.
   assert.deepEqual(await page.evaluate(()=>{const box=document.querySelector('#dialog .artifact-view'),bar=document.querySelector('#dialog .artifact-toolbar'),body=document.querySelector('#dialog .modal-body'),filler=document.createElement('div');filler.style.height='3000px';box.append(filler);const top=bar.getBoundingClientRect().top;box.scrollTop=1500;body.scrollTop=1500;const result=[box.scrollTop,body.scrollTop,bar.getBoundingClientRect().top===top,document.querySelector('#artifact-location').getBoundingClientRect().bottom<=box.getBoundingClientRect().top];filler.remove();return result}),[1500,0,true,true]);
   assert.equal(await page.locator('#dialog .code-line').first().evaluate(el=>getComputedStyle(el,'::before').content),'"1"');
   assert.equal(await page.locator('#dialog pre.code-view').innerText(),'import sys\n\ndef main():\n    return 0\n');await closeArtifact();
   // YAML folds by indentation and XML by element, both shown exactly as written.
   await openArtifact('compose.yaml');assert.equal(await page.locator('#dialog pre.code-view').innerText(),yamlFixture);const yamlFolds=page.locator('#dialog .code-fold');assert.equal(await yamlFolds.count(),4);
   await yamlFolds.nth(2).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10,11');await yamlFolds.nth(0).click();assert.equal(await numbers(),'1,2,10,11');
   await yamlFolds.nth(0).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10,11');await closeArtifact();
   await openArtifact('layout.xml');assert.equal(await page.locator('#dialog pre.code-view').innerText(),xmlFixture);const xmlFolds=page.locator('#dialog .code-fold');assert.equal(await xmlFolds.count(),2);
   await xmlFolds.nth(1).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10,11');assert.equal(await page.locator('#dialog .code-line[data-n="5"] .code-rest').evaluate(el=>getComputedStyle(el,'::after').content),'" … </item>"');
   await xmlFolds.nth(0).click();assert.equal(await numbers(),'1,2,3,4');await closeArtifact();
   // A Windows-1252 source is shown as text, with a note; UTF-8 files have none; the download keeps the original bytes.
   assert.equal(await page.locator('#dialog .file-encoding-note').count(),0);
   await openArtifact('LEGACY.cbl');assert.equal(await page.locator('#dialog pre.code-view').innerText(),'      * Attività già chiusa\n       MOVE A TO B.\n');
   assert.equal(await page.locator('#dialog .file-encoding-note').innerText(),'File non UTF-8, mostrato come Windows-1252. Alcuni caratteri potrebbero non essere corretti.');
   const legacy=await page.request.get(base+await page.locator('#artifact-download').getAttribute('href'));assert((await legacy.body()).includes(Buffer.from([0x74,0xe0,0x20])));await closeArtifact();
   if(mode==='direct'){await page.evaluate(()=>openStorage('main/LEGACY.cbl','sources'));await page.locator('#dialog .file-encoding-note').waitFor();assert((await page.locator('#dialog pre.storage-preview').innerText()).includes('Attività già chiusa'));await page.keyboard.press('Escape');await page.locator('#dialog').waitFor({state:'hidden'})}
   await openArtifact('data.json');assert.equal(await numbers(),'1,2,3,4,5,6,7,8,9,10');
   assert.equal(await page.locator('#dialog .code-line[data-n="2"]').textContent(),'  "id": 12345678901234567890,\n');
   const folds=page.locator('#dialog .code-fold');assert.equal(await folds.count(),3);
   await folds.nth(2).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10');
   await folds.nth(1).click();assert.equal(await numbers(),'1,2,3,9,10');assert.equal(await page.locator('#dialog .code-line[data-n="3"]').locator('.code-rest').evaluate(el=>getComputedStyle(el,'::after').content),'" … ],"');
   await folds.nth(1).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10');assert.equal(await folds.nth(1).getAttribute('aria-expanded'),'true');
   await folds.nth(0).click();assert.equal(await numbers(),'1');await folds.nth(0).click();assert.equal(await numbers(),'1,2,3,4,5,8,9,10');
   await page.locator('#dialog [data-action=artifact-mode][data-mode=source]').click();await page.waitForFunction(()=>document.querySelector('#dialog .code-line')&&!document.querySelector('#dialog .code-fold'));assert.equal(await numbers(),'1');assert.equal(await page.locator('#dialog .code-fold').count(),0);
   assert.equal(await page.locator('#dialog pre.code-view').innerText(),'{"id":12345678901234567890,"items":[1,{"deep":true}],"empty":{}}\n');await closeArtifact();await page.locator('[data-artifact-query]').fill('');
   if(mode==='direct'){
    await page.evaluate(()=>openStorage('main/data.json','sources'));await page.locator('#dialog pre.storage-preview .code-fold').first().waitFor();assert.equal(await numbers(),'1,2,3,4,5,6,7,8,9,10');
    // Storage downloads are named after the file or folder, not after the root.
    for(const [query,name] of [['path=main%2Ftool.py&root=sources','tool.py.zip'],['path=main&root=sources','main.zip'],['path=&root=sources','sources.zip']])assert.equal((await page.request.get(base+'/api/storage/download?'+query)).headers()['content-disposition'],`attachment; filename="${name}"; filename*=UTF-8''${name}`);
    assert.match((await page.request.get(base+'/api/storage/download?root=workspaces&path='+run.id)).headers()['content-disposition'],/filename="Artifacts direct [0-9a-f]{8}\.zip"/);
    assert.equal(await page.locator('#dialog pre.code-view').first().evaluate(el=>getComputedStyle(el).backgroundImage.startsWith('linear-gradient')),true);
    // Storage uses the same viewer for the files the direct run left in the source folder.
    await page.evaluate(()=>openStorage('main/chart.png','sources'));await page.waitForFunction(()=>document.querySelector('#dialog img.file-view-image')?.naturalWidth===1);
    await page.evaluate(()=>openStorage('main/report.html','sources'));const storageFrame=page.locator('#dialog iframe.file-view-frame');assert.equal(await storageFrame.getAttribute('sandbox'),'allow-scripts');
    const storageText=storageFrame.contentFrame().locator('body');await storageText.getByText('no-network').waitFor();assert((await storageText.innerText()).includes('isolated'));
    assert((await page.locator('#dialog .file-view-source pre').textContent()).includes('<h1 id="t">Report</h1>'));
    await page.evaluate(()=>openStorage('main/original.txt','sources'));await page.locator('#dialog pre.storage-preview').waitFor();assert.equal(await page.locator('#dialog iframe,#dialog img.file-view-image').count(),0);
    await page.keyboard.press('Escape');await page.locator('#dialog').waitFor({state:'hidden'});
   }
   await page.setViewportSize({width:390,height:844});assert(await page.locator('#inspector').evaluate(el=>el.scrollWidth<=el.clientWidth));await page.setViewportSize({width:1440,height:1050});
   await page.screenshot({path:path.resolve('test-results/artifacts-'+mode+'.png')});
  }
  // Historical snapshots also sort newest first using their acquisition dates.
  const recovered={id:'recovered',path:'recovered.txt',version:2,created:500,node:'artifact-recovery',file:{location:'/data/recovered.txt',root:'/data',modified:200000,change:'recovered',preview:true,blob:false},hash:'test-hash',content:'Recovery preview',session:'',turn:0};
  await page.route('**/api/artifacts/recovered',r=>r.fulfill({json:{artifact:recovered,html:'<p>Recovery preview</p>'}}));
  await page.evaluate(recovered=>{const meta=[{id:'old',path:'old.txt',version:1,created:1,node:'first'},{id:'recent',path:'recent.txt',version:1,created:99,node:'last'},recovered];app.artifacts=meta;document.querySelector('#artifacts-panel').innerHTML=inspectorArtifacts(meta);bindArtifactSearch()},recovered);
  assert.equal(await page.locator('[data-inspector-artifacts]>.artifact-item').first().getAttribute('data-id'),'recovered');
  assert.equal(await page.locator('[data-id=recovered] time').getAttribute('datetime'),'1970-01-01T00:03:20.000Z');
  assert(!(await page.locator('[data-id=recovered]').innerText()).includes('artifact-recovery'));
  await page.locator('[data-inspector-artifacts] [data-id=recovered]').click();await page.locator('#artifact-location').waitFor({state:'visible'});
  const detail=await page.locator('#dialog').innerText();assert(!/artifact-recovery|Recuperato|Recovered|Historical|Storico/.test(detail));assert.match(await page.locator('#artifact-version .caption').innerText(),/Versione 2|Version 2/);assert(!detail.includes('SHA256'));
  assert(!/Recuperato|Recovered|Historical|Storico/.test(await page.locator('[data-inspector-artifacts]').innerText()));
  assert.deepEqual(errors,[]);console.log('PASS: real direct/copy runs, initial-file filtering, additional sources, binary and large outputs, immutable downloads, all-run inspector, hidden files, search, labels, legacy encodings, numbered lines, folding JSON, YAML and XML, image, PDF and sandboxed HTML views in artifacts and Storage, recency order and mobile');
 }finally{await browser?.close();server.kill('SIGINT');fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
