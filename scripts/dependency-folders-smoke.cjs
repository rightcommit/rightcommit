// Real browser check: dependency, cache and generated folders (.venv, venv, node_modules,
// .dart_tool…) in a Source folder and in the run workspace. A copied Source with a virtual
// environment starts (its links are not copied), a permanent one too; the folders do not count
// toward the copy size; the agent's own .venv, node_modules and caches are not captured as
// artifacts and do not show in the Artifacts view (nor older captures of them), while a file named
// venv does; Storage hides them in the workspace and the result ZIP leaves them out.
// A disposable server, temporary data and NODO_SOURCES, and a fake offline Claude Code CLI.
const fs=require('fs'),os=require('os'),path=require('path'),assert=require('node:assert/strict'),{spawn,execFileSync}=require('child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const port=process.env.SMOKE_PORT||'18363',dir=fs.mkdtempSync(path.join(os.tmpdir(),'dependency-folders-')),base='http://127.0.0.1:'+port;
 const data=path.join(dir,'data'),sources=path.join(dir,'sources'),bin=path.join(dir,'bin');
 for(const d of [data,sources,bin])fs.mkdirSync(d);
 // A virtual environment as python3 -m venv makes it: a link to the interpreter and lib64 -> lib.
 const venv=root=>{fs.mkdirSync(path.join(root,'bin'),{recursive:true});fs.mkdirSync(path.join(root,'lib/python3/site-packages'),{recursive:true});
  fs.writeFileSync(path.join(root,'lib/python3/site-packages/big.so'),Buffer.alloc(3*1024*1024));fs.writeFileSync(path.join(root,'pyvenv.cfg'),'home = /usr/bin\n');
  fs.symlinkSync('/usr/bin/python3',path.join(root,'bin/python3'));fs.symlinkSync('lib',path.join(root,'lib64'))};
 for(const name of ['copied','permanent']){
  const root=path.join(sources,name);fs.mkdirSync(path.join(root,'services'),{recursive:true});
  fs.writeFileSync(path.join(root,'app.py'),'print("app")\n');fs.writeFileSync(path.join(root,'services/venv'),'VENV_NAME=api\n');
  venv(path.join(root,'.venv'));venv(path.join(root,'services/api/venv'));
  fs.mkdirSync(path.join(root,'web/node_modules/.bin'),{recursive:true});fs.symlinkSync('/etc',path.join(root,'web/node_modules/.bin/tool'));
 }
 fs.writeFileSync(path.join(bin,'claude'),`#!/usr/bin/env python3
import os,sys,json,pathlib
sys.stdin.read();args=sys.argv
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
cwd=pathlib.Path.cwd()
(cwd/'result.md').write_text('# Result\\n')
(cwd/'services/venv').write_text('VENV_NAME=worker\\n')
for venv in [cwd/'.venv',cwd/'tools/venv']:
 (venv/'lib/python3/site-packages').mkdir(parents=True,exist_ok=True)
 (venv/'lib/python3/site-packages/installed.py').write_text('x = 1\\n')
 (venv/'bin').mkdir(exist_ok=True)
 if not (venv/'bin/python3').is_symlink():(venv/'bin/python3').symlink_to('/usr/bin/python3')
for cache in ['node_modules/pkg','src/__pycache__','mobile/.dart_tool','web/.next','.pytest_cache']:
 (cwd/cache).mkdir(parents=True,exist_ok=True);(cwd/cache/'generated.js').write_text('generated')
def emit(x):print(json.dumps(x),flush=True)
emit(dict(type='system',subtype='init',session_id=sid))
emit(dict(type='result',result='<nodo_result>'+json.dumps(dict(status='completed',summary='Fixture output',artifacts=[],questions=[]))+'</nodo_result>',is_error=False))
`,{mode:0o755});
 const server=spawn(process.env.NODO_TEST_BINARY||path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:data,NODO_LISTEN:'127.0.0.1:'+port,NODO_SOURCES:sources,PATH:bin+':'+process.env.PATH,ANTHROPIC_AUTH_TOKEN:'offline-dependency-fixture'},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>localStorage.setItem('rightcommit.hideAgentSetupNotice','true'));
  for(const file of ['app.js','app.css','i18n.js'])await page.route('**/'+file,r=>r.fulfill({path:path.resolve('web/'+file)}));
  const call=async(url,body,status=200)=>{const r=await page.request.fetch(base+'/api'+url,{method:body?'POST':'GET',headers:{'X-Nodo-Request':'1'},data:body});assert.equal(r.status(),status,url+': '+await r.text());return r.json()};
  await call('/auth/register',{email:'dependency-folders@test.local',password:'temporary-dependency-password'});
  const checks=[];
  // The copy size counts app.py and the file named venv, not the 2 x 3 MiB inside the virtual environments.
  const size=await call('/storage/source-size?path=copied');
  assert.equal(size.bytes,'print("app")\n'.length+'VENV_NAME=api\n'.length);assert.equal(size.copy_error,null);
  checks.push('copy size leaves out .venv, venv and node_modules and their links');
  await page.goto(base);await page.locator('#app').waitFor({state:'visible'});
  for(const [mode,location] of [['copy','copied'],['direct','permanent']]){
   const wf={id:'deps-'+mode,name:'Dependencies '+mode,description:'',entry:'work',nodes:[
    {id:'source',kind:'Source',title:'Project',source_type:'folder',source_mode:mode,location,x:60,y:80},
    {id:'agent',kind:'Agent',title:'Fixture',runtime:'claude',x:300,y:300},
    {id:'work',kind:'Task',title:'Work',agent:'agent',prompt:'Install dependencies and write the result',x:300,y:80}],
    edges:[{from:'source',to:'agent',kind:'context'},{from:'agent',to:'work',kind:'context'}]};
   await call('/workflows',wf);const run=await call('/runs',{workflow_id:wf.id});let d;
   for(let i=0;i<300;i++){d=await call('/runs/'+run.id);assert.notEqual(d.run.status,'failed',d.run.error);if(d.run.status==='completed')break;await new Promise(r=>setTimeout(r,50))}
   assert.equal(d.run.status,'completed');const ws=d.run.workspace;
   if(mode==='copy'){
    assert(!fs.existsSync(path.join(ws,'web/node_modules'))&&!fs.existsSync(path.join(ws,'services/api/venv')),'dependency folders are not copied');
    assert.equal(fs.readFileSync(path.join(ws,'services/venv'),'utf8'),'VENV_NAME=worker\n');
   }else{
    assert.equal(ws,fs.realpathSync(path.join(sources,'permanent')));assert(fs.lstatSync(path.join(ws,'web/node_modules/.bin/tool')).isSymbolicLink());
   }
   const files=d.artifacts.filter(a=>a.file.change!=='report').map(a=>a.path.replace(/^permanent\//,'')).sort();
   assert.deepEqual(files,['result.md','services/venv']);
   checks.push(mode+': run starts with .venv in the Source; artifacts are only result.md and the file services/venv');
   // The Artifacts view, with captures an older version made inside the folders.
   await page.evaluate(id=>loadRun(id),run.id);await page.locator('.node-main[data-id=work]').click();await page.locator('[data-artifact-query]').waitFor();
   await page.evaluate(()=>{for(const p of ['.venv/lib/python3/site-packages/old.py','tools/venv/bin/activate','node_modules/pkg/index.js','mobile/.dart_tool/state','infra/.terraform/plugin','lib/_build/dev/app.beam'])app.artifacts.push({id:'old-'+p,node:'work',path:p,version:1,created:1,file:{root:'/data',location:'/old/'+p,change:'created'}});app.artifacts.push({id:'named-venv',node:'work',path:'docs/venv',version:1,created:1,file:{root:'/data',location:'/old/docs/venv',change:'created'}});renderInspector()});
   const shown=await page.locator('[data-inspector-artifacts]>.artifact-item').evaluateAll(rows=>rows.map(r=>app.artifacts.find(a=>a.id===r.dataset.id).path.replace(/^permanent\//,'')).sort());
   assert.deepEqual(shown,['docs/venv','result.md','services/venv']);
   await page.locator('[data-artifact-filter]').click();const folders=await page.locator('[data-artifact-folder]').evaluateAll(b=>b.map(x=>x.dataset.artifactFolder));
   assert(!folders.some(f=>/(\.venv|\/venv|node_modules|\.dart_tool|\.terraform|_build)/.test(f)),folders.join());await page.keyboard.press('Escape');
   checks.push(mode+': Artifacts view and its folder filter show neither the folders nor older captures in them; a file named venv stays');
   if(mode==='copy'){
    const names=async p=>(await call('/storage?root=workspaces&path='+encodeURIComponent(p))).entries.map(e=>e.name);
    const top=await names(run.id);assert(!top.includes('.venv')&&!top.includes('node_modules')&&!top.includes('.pytest_cache')&&top.includes('result.md'),top.join());
    assert.deepEqual(await names(run.id+'/tools'),[]);assert((await names(run.id+'/services')).includes('venv'));
    const zip=await page.request.get(base+'/api/storage/download?root=workspaces&path='+encodeURIComponent(run.id));assert.equal(zip.status(),200,await zip.text());
    const file=path.join(dir,'result.zip');fs.writeFileSync(file,await zip.body());
    const listed=execFileSync('python3',['-c','import sys,zipfile;print("\\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))',file]).toString();
    assert(/services\/venv$/m.test(listed)&&/result\.md$/m.test(listed),listed);assert(!/\.venv|tools\/venv|node_modules|\.dart_tool|\.next|\.pytest_cache/.test(listed),listed);
    checks.push('Storage hides the folders in the workspace and the result ZIP leaves them out instead of failing on their links');
    // /sources keeps showing real folders.
    assert((await call('/storage?root=sources&path=copied')).entries.some(e=>e.name==='.venv'));
   }
  }
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({pass:true,checks}));
 }finally{await browser?.close();server.kill('SIGINT');await new Promise(r=>setTimeout(r,300));fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
