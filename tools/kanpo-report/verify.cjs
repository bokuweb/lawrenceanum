const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const {chromium,webkit,firefox}=require('/Users/bokuweb/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const root=process.argv[2];
const engine=process.argv[3]||'chrome';
const audit=process.argv[4]||path.join(path.dirname(root),'report-build-audit');
const full=process.argv.includes('--full');
(async()=>{
 const browser=await (engine==='webkit'?webkit.launch({headless:true,executablePath:'/Users/bokuweb/Library/Caches/ms-playwright/webkit-2370/pw_run.sh'}):engine==='firefox'?firefox.launch({headless:true,executablePath:'/Users/bokuweb/Library/Caches/ms-playwright/firefox-1555/firefox/Nightly.app/Contents/MacOS/firefox'}):chromium.launch({headless:true,channel:'chrome'}));
 try{
 const context=await browser.newContext({viewport:{width:1500,height:1050},acceptDownloads:true,offline:true});
 const page=await context.newPage();
 const errors=[],requests=[];
 page.on('pageerror',e=>errors.push(e.message));
 page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
 page.on('request',r=>{if(/^https?:/.test(r.url()))requests.push(r.url());});
 await page.goto(pathToFileURL(path.join(root,'index.html')).href);
 await page.waitForFunction(()=>!!window.KANPO_REPORT);
 const settle=()=>page.evaluate(async()=>{await KANPO_REPORT.settled;for(const s of Object.values(KANPO_REPORT.states))await s.queue;});
 await settle();
 const entries=JSON.parse(fs.readFileSync(path.join(root,'manifest.json'))).entries;
 assert.equal(entries.length,21);assert.equal(entries.reduce((n,e)=>n+e.source_pages,0),185);
 const results=[];
 for(const e of entries){
  await page.selectOption('#case-select',e.id);await settle();
  for(const mode of ['off','on']){
   await page.locator(`input[name=ocr][value=${mode}]`).check();await settle();
   const result=await page.evaluate(async({id,mode,full})=>{
    const r=KANPO_REPORT,states=r.states;
    if(r.current.id!==id||r.mode!==mode)throw new Error('wrong selected case/variant');
    const row={id,mode,renderedPages:0};
    for(const s of Object.values(states)){
      if(s.pane.dataset.ready!=='true'||s.canvas.hidden||!s.canvas.width)throw new Error('pane is not rendered '+s.side);
      row[s.side]=s.pdf.numPages;
      if(full&&(mode==='off'||s.side==='converted'))for(let n=1;n<=s.pdf.numPages;n++){
        const p=await s.pdf.getPage(n),v=p.getViewport({scale:.35}),c=document.createElement('canvas');c.width=Math.ceil(v.width);c.height=Math.ceil(v.height);
        await p.render({canvasContext:c.getContext('2d'),viewport:v}).promise;
        row.renderedPages++;c.width=c.height=0;
      }
    }
    return row;
   },{id:e.id,mode,full});
   assert.equal(result.original,e.source_pages);assert.equal(result.converted,e.variants[mode].pages);
   assert.match(await page.locator('#download-docx').getAttribute('href'),/^blob:/);
   assert.equal(await page.locator('#download-docx').getAttribute('download'),`${e.id}-ocr-${mode}.docx`);
   await page.locator('#original-pane [data-action=next]').click({force:true});
   await page.evaluate(async()=>{for(const s of Object.values(KANPO_REPORT.states))await s.queue;});
   const downloadPromise=page.waitForEvent('download');await page.locator('#download-docx').click();const download=await downloadPromise;
   assert.equal(await download.failure(),null);
   assert.equal(fs.readFileSync(await download.path()).equals(fs.readFileSync(path.join(root,'files',e.id,`ocr-${mode}.docx`))),true);
   if(['case-01','case-10','case-16'].includes(e.id))await page.screenshot({path:path.join(audit,`${engine}-${e.id}-${mode}.png`),fullPage:true});
   results.push(result);console.log(JSON.stringify(result));
  }
 }
 await page.selectOption('#case-select','case-13');await settle();
 await page.locator('#sync').check();
 await page.locator('#original-pane input[type=number]').fill('37');await page.locator('#original-pane input[type=number]').press('Tab');
 await page.evaluate(async()=>{for(const s of Object.values(KANPO_REPORT.states))await s.queue;});
 assert.deepEqual(await page.evaluate(()=>Object.values(KANPO_REPORT.states).map(s=>s.page)),[37,37]);
 await page.locator('#converted-pane [data-action=in]').click();await page.evaluate(()=>KANPO_REPORT.states.converted.queue);
 assert.equal(await page.evaluate(()=>KANPO_REPORT.states.converted.zoom>0),true);
 await page.locator('#converted-pane [data-action=fit]').click();await page.evaluate(()=>KANPO_REPORT.states.converted.queue);
 assert.equal(await page.evaluate(()=>KANPO_REPORT.states.converted.zoom),null);
 // Rapid selection and OCR changes must not display stale results.
 await page.evaluate(()=>{const s=document.querySelector('#case-select');for(const id of ['case-01','case-16','case-10']){s.value=id;s.dispatchEvent(new Event('change'));}const x=document.querySelector('[name=ocr][value=off]');x.checked=true;x.dispatchEvent(new Event('change'));});
 await settle();assert.equal(await page.evaluate(()=>KANPO_REPORT.current.id),'case-10');assert.equal(await page.evaluate(()=>KANPO_REPORT.mode),'off');
 assert.equal(requests.length,0);assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(audit,`verification-${engine}.json`),JSON.stringify({engine,offline:true,protocol:'file:',externalRequests:requests,errors,results,allChecksPassed:true},null,2));
 console.log('PASS '+engine+' '+results.length+' case/variant combinations; '+results.reduce((n,x)=>n+x.renderedPages,0)+' PDF pages rendered');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
