// The update notice: bell badge on the avatar, "vX.Y.Z available" in the account menu, dismiss per version.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'update-notice-')),base='http://127.0.0.1:18147';
 // The real check is off: the server must answer without calling GitHub.
 const server=spawn(path.resolve('target/debug/rightcommit'),[],{env:{...process.env,NODO_DATA:dir,NODO_LISTEN:'127.0.0.1:18147',NODO_SECURE_COOKIE:'0',RIGHTCOMMIT_UPDATE_CHECK:'0'},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(base+'/healthz')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
  const installed=(await (await fetch(base+'/healthz')).json()).version;
  browser=await chromium.launch({channel:'chrome',headless:true});const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{try{localStorage.setItem('rightcommit.hideAgentSetupNotice','true');if(localStorage.getItem('nodo.language')===null)localStorage.setItem('nodo.language','it')}catch{}});
  assert.equal((await page.request.get(base+'/api/update')).status(),401);
  assert((await page.request.post(base+'/api/auth/register',{headers:{'X-Nodo-Request':'1'},data:{email:'update@test.local',password:'temporary-update-password'}})).ok());
  // Disabled check: no notice and the installed version only.
  assert.deepEqual(await (await page.request.get(base+'/api/update')).json(),{current:installed,latest:null,available:false,url:null});
  const open=async()=>{await page.goto(base);await page.locator('#app').waitFor({state:'visible'});await page.waitForFunction(()=>app.update!==undefined)};
  await open();
  assert.equal(await page.locator('#account.has-update').count(),0);await page.locator('#account').click();
  assert.equal(await page.locator('#account-menu [data-update-notice]').count(),0);await page.locator('#account').click();

  let update={current:installed,latest:'99.2.3',available:true,url:'https://github.com/rightcommit/rightcommit/releases/tag/v99.2.3'};
  await page.route('**/api/update',r=>update?r.fulfill({json:update}):r.fulfill({status:500,body:'{}'}));
  await open();
  const account=page.locator('#account');
  await page.locator('#account.has-update').waitFor();
  assert.equal(await page.locator('#update-status').textContent(),'Aggiornamento disponibile: v99.2.3');assert.equal(await account.getAttribute('aria-describedby'),'update-status');
  const badge=()=>account.evaluate(el=>{const s=getComputedStyle(el,'::after');return {content:s.content,animation:s.animationName,image:s.backgroundImage.includes('svg'),color:s.backgroundColor}});
  assert.deepEqual({...(await badge()),color:undefined},{content:'""',animation:'update-blink',image:true,color:undefined});
  await account.click();
  const notice=page.locator('#account-menu [data-update-notice]'),link=notice.locator('a');
  assert.equal(await link.textContent(),'v99.2.3 disponibile');
  assert.equal(await link.getAttribute('href'),'https://github.com/rightcommit/rightcommit/releases/tag/v99.2.3');assert.equal(await link.getAttribute('target'),'_blank');assert.equal(await link.getAttribute('rel'),'noopener noreferrer');
  assert.equal(await link.evaluate(el=>getComputedStyle(el).animationName),'update-blink');
  // Directly under the first row: name, installed version and site link.
  assert(await notice.evaluate(el=>el.previousElementSibling.classList.contains('account-version')));
  assert.equal((await page.locator('#account-menu .account-version-number').textContent()).trim(),'rightcommit v'+installed);
  await page.screenshot({path:'/tmp/update-notice-menu.png'});
  // Keyboard: the link and the dismiss control are part of the menu.
  assert.equal(await page.locator('#account-menu [role=menuitem]').count()>=4,true);
  // Reduced motion stops the blinking.
  await page.emulateMedia({reducedMotion:'reduce'});
  assert.equal((await badge()).animation,'none');assert.equal(await link.evaluate(el=>getComputedStyle(el).animationName),'none');
  await page.emulateMedia({reducedMotion:'no-preference'});
  // English.
  await account.click();
  await page.evaluate(()=>{localStorage.setItem('nodo.language','en')});await open();await page.locator('#account.has-update').waitFor();await account.click();
  assert.equal(await link.textContent(),'v99.2.3 available');assert.equal(await page.locator('#update-status').textContent(),'Update available: v99.2.3');
  // Dismiss hides it for this version, also after a reload; a later version brings it back.
  await notice.locator('[data-action=dismiss-update]').click();
  assert.equal(await notice.count(),0);assert.equal(await page.locator('#account.has-update').count(),0);assert.equal(await page.locator('#update-status').count(),0);assert.equal(await account.getAttribute('aria-describedby'),null);
  await open();assert.equal(await page.locator('#account.has-update').count(),0);
  update={...update,latest:'99.3.0'};await open();await page.locator('#account.has-update').waitFor();await account.click();assert.equal(await link.textContent(),'v99.3.0 available');await account.click();
  // A malformed version or a failed request shows nothing and raises no error.
  update={...update,latest:'<img src=x onerror=alert(1)>'};await open();assert.equal(await page.locator('#account.has-update').count(),0);
  update=null;await open();assert.equal(await page.locator('#account.has-update').count(),0);
  await account.click();assert.equal(await notice.count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS: update check disabled and authenticated, bell badge, menu line under the version row, release link, IT/EN, reduced motion, dismiss per version, malformed and failed responses');
 }finally{await browser?.close();server.kill('SIGINT');fs.rmSync(dir,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exit(1)});
