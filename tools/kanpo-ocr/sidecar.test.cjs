'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pngSize, validateFrame, mappedBox, sidecar, recognizeBuffers, run, encodeJson, writeAtomicJson, recordFailure, watchTerminal } = require('./sidecar.cjs');
function png(width=3368,height=2380) {
  const b=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(b);b.writeUInt32BE(13,8);b.write('IHDR',12);b.writeUInt32BE(width,16);b.writeUInt32BE(height,20);return b;
}
const frame={pixel_width:3368,pixel_height:2380,width_points:842,height_points:595,dpi:288};
function node(text,bbox,rest={}) { return {text,bbox,confidence:0,...rest}; }
const box={x0:240,y0:344,x1:956,y1:392};
function result() { return {data:{text:'償務者 <&>\n\n',version:'test-engine',psm:'AUTO',oem:'LSTM_ONLY',blocks:[node('償務者 <&>\n',box,{paragraphs:[node('償務者 <&>\n',box,{lines:[node('償務者 <&>\n',box,{words:[node('償務者',box),node('<&>',{x0:0,y0:0,x1:0,y1:0})]})]})]})]}}; }
test('PNG canvas dimensions come from IHDR',()=>assert.deepEqual(pngSize(png()),{width:3368,height:2380}));
test('truncated or foreign image is rejected before recognition',()=>{
  for(const b of [Buffer.alloc(0),png().subarray(0,24),Buffer.alloc(33)]) assert.throws(()=>pngSize(b));
});
test('invalid PNG dimensions and IHDR are rejected',()=>{
  for(const b of [png(0,1),png(1,0),png(0x80000000,1)])assert.throws(()=>pngSize(b));
  const b=png();b.writeUInt32BE(12,8);assert.throws(()=>pngSize(b));
});
test('page frame and DPI must be explicit positive finite values',()=>{
  assert.doesNotThrow(()=>validateFrame(frame));
  for(const key of ['width_points','height_points','dpi'])for(const value of [0,-1,NaN,Infinity,-Infinity,undefined])assert.throws(()=>validateFrame({...frame,[key]:value}));
  assert.throws(()=>validateFrame({...frame,dpi:1.5}));
});
test('pixel edges map separately to the explicit two-axis point frame',()=>{
  assert.deepEqual(mappedBox(box,frame),{pixels:box,points:[60,86,239,98],geometry:'valid'});
  const f={...frame,pixel_width:100,pixel_height:200,width_points:50,height_points:300};
  assert.deepEqual(mappedBox({x0:10,y0:20,x1:20,y1:40},f).points,[5,30,10,60]);
});
test('canvas edges are retained exactly',()=>assert.deepEqual(mappedBox({x0:0,y0:0,x1:3368,y1:2380},frame).points,[0,0,842,595]));
test('invalid or degenerate OCR boxes are retained and marked, never filtered',()=>{
  for(const b of [null,{}, {x0:2,y0:2,x1:1,y1:3}, {x0:0,y0:0,x1:0,y1:1}, {x0:-1,y0:0,x1:1,y1:1}, {x0:0,y0:0,x1:3369,y1:1}, {x0:0,y0:0,x1:1,y1:Infinity}]){
    const m=mappedBox(b,frame);assert.equal(m.geometry,'invalid');assert.equal(m.points,null);assert.deepEqual(m.pixels,b);
  }
});
test('OCR text and low confidence remain verbatim without expected-word corrections',()=>{
  const raw=result();const original=structuredClone(raw);const s=sidecar(raw,{image_sha256:'test'},frame);
  assert.equal(s.text,raw.data.text);assert.equal(s.blocks[0].paragraphs[0].lines[0].words[0].text,'償務者');
  assert.equal(s.blocks[0].confidence,0);assert.deepEqual(raw,original);assert.equal(s.article_assignment,null);
});
test('all block paragraph line and word occurrences retain their engine paths',()=>{
  const raw=result();raw.data.blocks.push(structuredClone(raw.data.blocks[0]));const s=sidecar(raw,{},frame);
  assert.deepEqual(s.blocks[1].source_path,[1]);assert.deepEqual(s.blocks[1].paragraphs[0].source_path,[1,0]);
  assert.deepEqual(s.blocks[1].paragraphs[0].lines[0].source_path,[1,0,0]);assert.deepEqual(s.blocks[1].paragraphs[0].lines[0].words[1].source_path,[1,0,0,1]);
  assert.equal(s.blocks[1].paragraphs[0].lines[0].words.length,2);
  assert.equal(s.blocks[1].paragraphs[0].lines[0].words[1].bbox.geometry,'invalid');
});
test('missing layout does not manufacture text positions',()=>{
  const s=sidecar({data:{text:'text with no boxes',blocks:null}}, {},frame);
  assert.equal(s.text,'text with no boxes');assert.deepEqual(s.blocks,[]);assert.equal(s.layout_available,false);
});
test('recognizer receives immutable image bytes and a private local model snapshot',async()=>{
  const image=png(),model=Buffer.from('model fixture');const imageBefore=Buffer.from(image),modelBefore=Buffer.from(model);let terminated=0,calls=[],modelDir;
  const fake=async(...args)=>{modelDir=args[2].langPath;assert.equal(args[0],'jpn');assert.equal(path.isAbsolute(modelDir),true);assert.deepEqual(fs.readFileSync(path.join(modelDir,'jpn.traineddata')),modelBefore);image.fill(0);model.fill(0);calls.push(['create',args]);return {
    setParameters:async p=>calls.push(['parameters',p]),
    recognize:async(...a)=>{calls.push(['recognize',a]);return result();},terminate:async()=>{terminated++;}
  };};
  const r=await recognizeBuffers(fake,image,model,288,()=>{});assert.equal(r.data.text,result().data.text);assert.equal(terminated,1);assert.equal(fs.existsSync(modelDir),false);
  const args=calls[0][1];assert.equal(args[1],1);assert.equal(args[2].cacheMethod,'none');assert.equal(args[2].gzip,false);
  assert.deepEqual(calls[1],['parameters',{tessedit_pageseg_mode:'3',user_defined_dpi:'288'}]);
  assert.deepEqual(Buffer.from(calls[2][1][0]),imageBefore);assert.deepEqual(calls[2][1][1],{rotateAuto:false,rotateRadians:0});assert.equal(calls[2][1][2].blocks,true);
});
test('worker is terminated when recognition fails and is not retried',async()=>{
  let calls=0,terminated=0;const error=new Error('fixed recognition failure');
  const fake=async()=>({setParameters:async()=>{},recognize:async()=>{calls++;throw error;},terminate:async()=>{terminated++;}});
  await assert.rejects(recognizeBuffers(fake,png(),Buffer.from('x'),288,()=>{}),e=>e===error);assert.equal(calls,1);assert.equal(terminated,1);
});
test('recognition error is retained when cleanup also fails',async()=>{
  const events=[];const error=new Error('recognition failure');const fake=async()=>({setParameters:async()=>{},recognize:async()=>{throw error;},terminate:async()=>{throw new Error('cleanup failure');}});
  await assert.rejects(recognizeBuffers(fake,png(),Buffer.from('x'),288,e=>events.push(e)),e=>e===error);assert.equal(events.at(-1).status,'terminate_error');
});
test('invalid model fails before output creation or engine loading',async()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-test-'));
  try {const image=path.join(d,'in.png'),model=path.join(d,'jpn.traineddata'),output=path.join(d,'out');fs.writeFileSync(image,png());fs.writeFileSync(model,'wrong model');
    await assert.rejects(run({image,model,output,width_points:842,height_points:595,dpi:288}),/model SHA256/);assert.equal(fs.existsSync(output),false);
  } finally {fs.rmSync(d,{recursive:true,force:true});}
});
test('frame validates the PNG pixel axes used by coordinate mapping',()=>{
  for(const key of ['pixel_width','pixel_height'])for(const v of [0,-1,1.5,NaN,undefined])assert.throws(()=>validateFrame({...frame,[key]:v}));
});
test('coordinate mapping retains finite canvas endpoints across numeric extremes',()=>{
  for(const value of [Number.MIN_VALUE,Number.MAX_VALUE]) {
    const f={...frame,width_points:value,height_points:value};
    assert.deepEqual(mappedBox({x0:0,y0:0,x1:3368,y1:2380},f).points,[0,0,value,value]);
  }
});
test('successful recognition is retained when cleanup reports an error',async()=>{
  const events=[];const raw=result();const fake=async()=>({setParameters:async()=>{},recognize:async()=>raw,terminate:async()=>{throw new Error('cleanup failure');}});
  assert.equal(await recognizeBuffers(fake,png(),Buffer.from('x'),288,e=>events.push(e)),raw);assert.equal(events.at(-1).status,'terminate_error');
});
test('source and frame metadata are independent immutable copies',()=>{
  const source={image:{sha256:'frozen'}};const f={...frame};const s=sidecar(result(),source,f);source.image.sha256='changed';f.width_points=1;
  assert.equal(s.source.image.sha256,'frozen');assert.equal(s.canvas.width_points,842);
});
test('CLI help does not require a model or an engine installation',()=>{
  const {spawnSync}=require('node:child_process');const r=spawnSync(process.execPath,[path.join(__dirname,'cli.cjs'),'--help'],{encoding:'utf8'});
  assert.equal(r.status,0);assert.match(r.stdout,/No model download/);
});
test('CLI rejects unknown flags before creating any output',()=>{
  const {spawnSync}=require('node:child_process');const r=spawnSync(process.execPath,[path.join(__dirname,'cli.cjs'),'--unexpected'],{encoding:'utf8'});
  assert.notEqual(r.status,0);assert.match(r.stderr,/unexpected/);
});
test('positive pixel boxes that collapse in point precision are marked invalid',()=>{
  const f={...frame,pixel_width:100,pixel_height:100,width_points:Number.MIN_VALUE,height_points:Number.MIN_VALUE};
  const b=mappedBox({x0:1,y0:1,x1:2,y1:2},f);assert.equal(b.geometry,'invalid');assert.equal(b.points,null);
});
test('persisted malformed source numbers remain distinguishable',()=>{
  const make=v=>JSON.stringify(mappedBox({x0:0,y0:0,x1:2,y1:v},frame));assert.notEqual(make(Infinity),make(NaN));assert.notEqual(make(Infinity),make(-Infinity));
});
test('raw JSON explicitly encodes nonfinite numbers and negative zero',()=>{
  const d=JSON.parse(encodeJson({nan:NaN,positive:Infinity,negative:-Infinity,minusZero:-0,ordinary:0}));
  assert.deepEqual(d,{nan:{$js_number:'NaN'},positive:{$js_number:'Infinity'},negative:{$js_number:'-Infinity'},minusZero:{$js_number:'-0'},ordinary:0});
});
test('partial atomic JSON write preserves the primary error without publishing partial terminal',()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-write-'));const target=path.join(d,'terminal.json');const primary=Object.assign(new Error('injected disk full'),{code:'ENOSPC'});
  const io={...fs,writeFileSync:(fd)=>{fs.writeFileSync(fd,'{"status":');throw primary;}};
  try {assert.throws(()=>writeAtomicJson(target,{status:'complete'},io),e=>e===primary);assert.equal(fs.existsSync(target),false);assert.equal(fs.readFileSync(primary.partial_output,'utf8'),'{"status":');}
  finally {fs.rmSync(d,{recursive:true,force:true});}
});
test('secondary journal failure cannot hide the primary error',()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-journal-'));const primary=Object.assign(new Error('primary failure'),{code:'ENOSPC'});
  try {const result=recordFailure(d,primary,()=>{throw Object.assign(new Error('secondary failure'),{code:'EEXIST'});});assert.equal(result,primary);assert.equal(primary.code,'ENOSPC');assert.match(primary.recording_error,/secondary failure/);}
  finally {fs.rmSync(d,{recursive:true,force:true});}
});
test('atomic publication cannot overwrite an existing observation file',()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-exclusive-'));const target=path.join(d,'terminal.json');fs.writeFileSync(target,'old evidence');
  try {assert.throws(()=>writeAtomicJson(target,{status:'complete'}),e=>e.code==='EEXIST');assert.equal(fs.readFileSync(target,'utf8'),'old evidence');}
  finally {fs.rmSync(d,{recursive:true,force:true});}
});
test('uncaught worker failures leave a failed terminal journal',()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-fatal-'));const {spawnSync}=require('node:child_process');
  const code=`const {watchTerminal}=require(${JSON.stringify(path.join(__dirname,'sidecar.cjs'))});watchTerminal(${JSON.stringify(d)});setImmediate(()=>{throw new Error('fixed uncaught worker failure');});`;
  try {const r=spawnSync(process.execPath,['-e',code],{encoding:'utf8'});assert.notEqual(r.status,0);const t=JSON.parse(fs.readFileSync(path.join(d,'terminal.json'),'utf8'));assert.equal(t.status,'failed');assert.match(t.error,/fixed uncaught worker failure/);}
  finally {fs.rmSync(d,{recursive:true,force:true});}
});
test('non-Error engine rejections survive a second journal failure',()=>{
  const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-rejection-'));
  try {for(const error of ['engine rejected',null])assert.equal(recordFailure(d,error,()=>{throw new Error('journal failed');}),error);}
  finally {fs.rmSync(d,{recursive:true,force:true});}
});

for (const primary of [new Error('primary recognition failure'),null]) {
  test(`recognizer rejection ${primary===null?'null':'Error'} survives cleanup observer failure and model snapshot is removed`,async()=>{
    let directory;
    const diagnostic=new Error('diagnostic append failed');
    const create=async(_language,_oem,options)=>{
      directory=options.langPath;
      return {setParameters:async()=>{},recognize:async()=>{throw primary;},terminate:async()=>{throw new Error('terminate failed');}};
    };
    let caught=false,observed;
    try {await recognizeBuffers(create,Buffer.from('image'),Buffer.from('model'),144,()=>{throw diagnostic;});}
    catch(error) {caught=true;observed=error;}
    // Cleanup assertions run even on the old error-identity failure.
    const cleaned=!fs.existsSync(directory);
    if (!cleaned) fs.rmSync(directory,{recursive:true,force:true});
    assert.equal(caught,true);assert.equal(observed,primary);assert.equal(cleaned,true);
  });
}
test('a sole cleanup diagnostic failure is reported after removing the model snapshot',async()=>{
  let directory;
  const diagnostic=new Error('diagnostic append failed');
  const create=async(_language,_oem,options)=>{
    directory=options.langPath;
    return {setParameters:async()=>{},recognize:async()=>({data:{text:'unchanged'}}),terminate:async()=>{throw new Error('terminate failed');}};
  };
  let observed;
  try {await recognizeBuffers(create,Buffer.from('image'),Buffer.from('model'),144,()=>{throw diagnostic;});}
  catch(error) {observed=error;}
  const cleaned=!fs.existsSync(directory);
  if (!cleaned) fs.rmSync(directory,{recursive:true,force:true});
  assert.equal(observed,diagnostic);assert.equal(cleaned,true);
});
