'use strict';
window.KANPO_DATA = Object.create(null);
const manifest = window.KANPO_MANIFEST;
const select = document.querySelector('#case-select');
const states = Object.fromEntries(['original','converted'].map(side => {
  const pane=document.querySelector('#'+side+'-pane');
  return [side,{side,pane,view:pane.querySelector('.viewport'),canvas:pane.querySelector('canvas'),status:pane.querySelector('.status'),bar:pane.querySelector('.toolbar'),pdf:null,page:1,zoom:null,version:0,task:null,queue:Promise.resolve()}];
}));
let current=null, mode='off', generation=0, downloadUrls=[];
function bytes(s){return Uint8Array.from(atob(s),c=>c.charCodeAt(0));}
class OfflineBinaryDataFactory {
  async fetch({kind,filename}) {
    const data=window.PDF_RESOURCES[kind]?.[filename];
    if(!data) throw new Error('同梱リソースが見つかりません: '+filename);
    return bytes(data);
  }
}
function message(s,text,error=false){s.canvas.hidden=true;s.status.hidden=false;s.status.textContent=text;s.status.classList.toggle('error',error);}
function updateToolbar(s){
  const n=s.pdf?.numPages||1;
  s.bar.querySelector('input').value=s.page;s.bar.querySelector('input').max=n;
  s.bar.querySelector('.page-total').textContent='/ '+(s.pdf?n:'—');
  s.bar.querySelector('[data-action=prev]').disabled=!s.pdf||s.page<=1;
  s.bar.querySelector('[data-action=next]').disabled=!s.pdf||s.page>=n;
  s.bar.querySelector('.zoom-label').textContent=s.zoom===null?'幅に合わせる':Math.round(s.zoom*100)+'%';
}
function render(s){
  const version=++s.version;
  s.task?.cancel();
  s.queue=s.queue.catch(()=>{}).then(async()=>{
    if(version!==s.version||!s.pdf)return;
    message(s,'ページを描画中…');
    updateToolbar(s);
    const page=await s.pdf.getPage(s.page);
    if(version!==s.version)return;
    const base=page.getViewport({scale:1});
    s.fitScale=Math.max(.15,(s.view.clientWidth-38)/base.width);
    const scale=s.zoom??s.fitScale;
    const viewport=page.getViewport({scale});
    const dpr=Math.min(window.devicePixelRatio||1,2);
    s.canvas.width=Math.ceil(viewport.width*dpr);s.canvas.height=Math.ceil(viewport.height*dpr);
    s.canvas.style.width=viewport.width+'px';s.canvas.style.height=viewport.height+'px';
    s.task=page.render({canvasContext:s.canvas.getContext('2d'),viewport,transform:dpr===1?null:[dpr,0,0,dpr,0,0]});
    await s.task.promise;s.task=null;
    if(version!==s.version)return;
    s.status.hidden=true;s.canvas.hidden=false;s.view.scrollTo(0,0);
    s.pane.dataset.ready='true';
  }).catch(e=>{if(e.name!=='RenderingCancelledException'&&version===s.version){message(s,'表示できませんでした。'+e.message+'。filesフォルダーのPDFを直接開くこともできます。',true);s.pane.dataset.ready='error';console.error(e);}});
  return s.queue;
}
async function openPdf(s,data,token){
  s.task?.cancel();++s.version;s.pane.dataset.ready='loading';message(s,'PDFを読み込み中…');
  await s.queue.catch(()=>{});
  const previous=s.pdf;s.pdf=null;
  if(previous)await previous.destroy();
  const pdf=await pdfjsLib.getDocument({data:bytes(data),BinaryDataFactory:OfflineBinaryDataFactory,useWorkerFetch:false,
    cMapUrl:'embedded/',standardFontDataUrl:'embedded/',wasmUrl:'embedded/',useWasm:true,
    isEvalSupported:false,isOffscreenCanvasSupported:false,isImageDecoderSupported:false,verbosity:0}).promise;
  if(token!==generation){await pdf.destroy();return;}
  s.pdf=pdf;s.page=1;s.zoom=null;
  await render(s);
}
function loadData(id){
  if(window.KANPO_DATA[id])return Promise.resolve(window.KANPO_DATA[id]);
  return new Promise((resolve,reject)=>{
    const script=document.createElement('script');script.src='data/'+id+'.js';
    script.onload=()=>{script.remove();window.KANPO_DATA[id]?resolve(window.KANPO_DATA[id]):reject(new Error('データが空です'));};
    script.onerror=()=>{script.remove();reject(new Error('同梱のdataフォルダーを読み込めません。ZIPをすべて展開してからindex.htmlを開いてください。'));};
    document.body.append(script);
  });
}
function updateInfo(){
  document.querySelector('#document-title').textContent=current.title;
  document.querySelector('#document-meta').textContent=[current.issue,current.notice,'原本 '+current.source_pages+'ページ','変換後 '+current.variants[mode].pages+'ページ'].filter(Boolean).join('　｜　');
  const note=document.querySelector('#document-note');note.textContent=current.note;note.hidden=!current.note;
  document.querySelector('#ocr-badge').textContent='OCR '+mode.toUpperCase();
  document.querySelector('#ocr-description').textContent=mode==='off'?'OFF：PDFの文字情報と元画像を保持した変換結果を表示します。':'ON：元画像を保持し、帳票などの画像内文字を認識した本文も表示します。';
  const v=current.variants[mode];
  document.querySelector('#conversion-detail').textContent='選択中：OCR '+mode.toUpperCase()+'／OCR結果を使用したページ '+v.ocr_pages+'／DOCX内の文字 '+v.text_characters.toLocaleString('ja-JP')+'文字（空白・ルビ・画像の注記を含む）。';
  for(const a of document.querySelectorAll('a.download')){a.removeAttribute('href');a.setAttribute('aria-disabled','true');}
}
function setDownloads(entry,data,selectedMode){
  for(const url of downloadUrls)URL.revokeObjectURL(url);downloadUrls=[];
  for(const [id,content,mime,filename] of [
    ['original',data.original,'application/pdf',entry.id+'-original.pdf'],
    ['docx',data[selectedMode].docx,'application/vnd.openxmlformats-officedocument.wordprocessingml.document',entry.id+'-ocr-'+selectedMode+'.docx'],
    ['pdf',data[selectedMode].pdf,'application/pdf',entry.id+'-ocr-'+selectedMode+'.pdf']]){
    const a=document.querySelector('#download-'+id),url=URL.createObjectURL(new Blob([bytes(content)],{type:mime}));
    downloadUrls.push(url);a.href=url;a.download=filename;a.removeAttribute('aria-disabled');
  }
}
let selectionQueue=Promise.resolve();
function show(changeOriginal){
  const token=++generation, entry=current, selectedMode=mode;
  selectionQueue=selectionQueue.catch(()=>{}).then(async()=>{
    if(token!==generation)return;
    updateInfo();
    const data=await loadData(entry.id);
    if(token!==generation)return;
    setDownloads(entry,data,selectedMode);
    const tasks=[openPdf(states.converted,data[selectedMode].pdf,token)];
    if(changeOriginal||!states.original.pdf||states.original.sourceId!==entry.id){states.original.sourceId=entry.id;tasks.push(openPdf(states.original,data.original,token));}
    await Promise.all(tasks);
  }).catch(e=>{if(token===generation){for(const s of Object.values(states))message(s,e.message,true);console.error(e);}});
  return selectionQueue;
}
function navigate(s,n){
  if(!s.pdf)return;
  s.page=Math.max(1,Math.min(s.pdf.numPages,Math.round(n)||1));render(s);
  if(document.querySelector('#sync').checked){const other=states[s.side==='original'?'converted':'original'];if(other.pdf){other.page=Math.min(s.page,other.pdf.numPages);render(other);}}
}
for(const s of Object.values(states)){
  s.bar.querySelector('input').addEventListener('change',e=>navigate(s,Number(e.target.value)));
  s.bar.addEventListener('click',e=>{
    const a=e.target.closest('button')?.dataset.action;if(!a||!s.pdf)return;
    if(a==='prev'||a==='next')return navigate(s,s.page+(a==='prev'?-1:1));
    if(a==='fit')s.zoom=null;
    else{s.zoom=Math.max(.25,Math.min(3,(s.zoom??s.fitScale??1)+(a==='in'?.2:-.2)));}
    render(s);
  });
}
for(const entry of manifest.entries){const o=document.createElement('option');o.value=entry.id;o.textContent=entry.label;select.append(o);}
select.options[0].remove();select.disabled=false;
select.addEventListener('change',()=>{current=manifest.entries.find(x=>x.id===select.value);show(true);});
document.querySelectorAll('[name=ocr]').forEach(input=>input.addEventListener('change',()=>{mode=input.value;show(false);}));
let resizeTimer;window.addEventListener('resize',()=>{clearTimeout(resizeTimer);resizeTimer=setTimeout(()=>{for(const s of Object.values(states))if(s.pdf&&s.zoom===null)render(s);},200);});
current=manifest.entries[0];select.value=current.id;
window.KANPO_REPORT={states,get current(){return current;},get mode(){return mode;},get settled(){return selectionQueue;}};
show(true);
