'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {run,MODEL_FILES}=require('./ndlocr.cjs');
const crypto=require('node:crypto');
const PIN={version:'0.0.15',commit:'f97e930b8c3e9512f7788f258ef685da26416483'};
function receipt(binary,changes={}){fs.writeFileSync(binary+'.kanpo-build.json',JSON.stringify({schema_version:1,package:'ndlocr-lite-rs',...PIN,features:['onnx'],profile:'release',binary_sha256:crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),...changes}));}
async function harness(callback){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'ndlocr-adapter-'));
  try{
    const model=path.join(root,'models');fs.mkdirSync(model);
    for(const file of Object.values(MODEL_FILES))fs.writeFileSync(path.join(model,file),'model snapshot');
    const image=path.join(root,'input.png'),png=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(png);png.writeUInt32BE(13,8);png.write('IHDR',12);png.writeUInt32BE(2,16);png.writeUInt32BE(3,20);fs.writeFileSync(image,png);
    const binary=path.join(root,'fake ndlocr');
    fs.writeFileSync(binary,`#!/usr/bin/env node
const fs=require('node:fs');const args=process.argv.slice(2);
if(args.includes('--quality-boost=false'))process.exit(2);
if(!args.includes('--structure-rules=false'))process.exit(3);
fs.writeFileSync(args[args.indexOf('--output-txt')+1],'未補正の原文\\n');
console.log(JSON.stringify(args));
`,{mode:0o700});
    receipt(binary);
    await callback({image,model,'ndlocr-bin':binary,output:path.join(root,'out'),width_points:20,height_points:30,dpi:300});
  }finally{fs.rmSync(root,{recursive:true,force:true});}
}
test('NDLOCR snapshots every asset, preserves text, and never invents coordinates',()=>harness(async options=>{
  const result=await run(options);
  assert.equal(result.text,'未補正の原文\n');assert.equal(result.layout_available,false);assert.deepEqual(result.blocks,[]);
  assert.equal(result.source.engine.version,PIN.version);assert.equal(result.source.engine.commit,PIN.commit);
  assert.equal(result.source.models.length,5);assert.equal(result.source.engine.package,'ndlocr-lite-rs');
  const inputs=JSON.parse(fs.readFileSync(path.join(options.output,'inputs.json')));
  const snapshot=inputs.args[inputs.args.indexOf('--image')+1];
  assert.notEqual(snapshot,options.image);assert.equal(fs.existsSync(snapshot),false);
  assert.equal(JSON.parse(fs.readFileSync(path.join(options.output,'terminal.json'))).status,'complete');
  await assert.rejects(run(options),/EEXIST/);
}));
test('failed NDLOCR process retains diagnostics and a failed terminal',()=>harness(async options=>{
  fs.writeFileSync(options['ndlocr-bin'],'#!/bin/sh\necho "model error" >&2\nexit 7\n',{mode:0o700});
  receipt(options['ndlocr-bin']);
  await assert.rejects(run(options),/NDLOCR failed \(7\)/);
  assert.match(fs.readFileSync(path.join(options.output,'stderr.txt'),'utf8'),/model error/);
  assert.equal(JSON.parse(fs.readFileSync(path.join(options.output,'terminal.json'))).status,'failed');
  assert.equal(fs.existsSync(path.join(options.output,'observations.json')),false);
}));
test('explicit detection and line thresholds reach NDLOCR and the input receipt',()=>harness(async options=>{
  await run({...options,'det-conf-threshold':0.9,'min-line-confidence':0.7});
  const inputs=JSON.parse(fs.readFileSync(path.join(options.output,'inputs.json')));
  assert.equal(inputs.args[inputs.args.indexOf('--det-conf-threshold')+1],'0.9');
  assert.equal(inputs.args[inputs.args.indexOf('--min-line-confidence')+1],'0.7');
  assert.equal(inputs.confidence_filter,0.7);
}));
test('invalid thresholds fail before loading any model or image',async()=>{
  for(const value of [-1,1.1,NaN,Infinity,'oops'])await assert.rejects(run({'det-conf-threshold':value}),/threshold/);
});

test('unverified or replaced binaries fail before recognition and output creation',()=>harness(async options=>{
  const binary=options['ndlocr-bin'];
  for(const changes of [{version:'0.0.14'},{commit:'0'.repeat(40)},{features:[]},{profile:'debug'},{binary_sha256:'0'.repeat(64)}]){
    receipt(binary,changes);await assert.rejects(run(options),/NDLOCR.*(pin|receipt|build)/);assert.equal(fs.existsSync(options.output),false);
  }
  fs.unlinkSync(binary+'.kanpo-build.json');await assert.rejects(run(options),/receipt/);assert.equal(fs.existsSync(options.output),false);
}));

function tracingEngine(options,invalid=false){
 fs.writeFileSync(options['ndlocr-bin'],`#!/usr/bin/env node
const fs=require('node:fs'),args=process.argv.slice(2);
const text='生の\\n認識領域';
fs.writeFileSync(args[args.indexOf('--output-txt')+1],text);
const index=args.indexOf('--trace-json');
if(index<0)process.exit(9);
fs.writeFileSync(args[index+1],JSON.stringify({schema_version:1,width:2,height:3,settings:{structure_rules:false,quality_boost:false,use_deim_detection:true,det_conf_threshold:0.25,min_line_confidence:Math.fround(0.3)},body:text,stages:{reading_order:[{bbox:[0,0,2,${invalid?4:3}],text,confidence:0.8,is_vertical:true}]}}));
console.log(JSON.stringify(args));
`,{mode:0o700});receipt(options['ndlocr-bin']);
}
test('explicit NDLOCR regions use the same recognition pass and save raw trace',()=>harness(async options=>{
 tracingEngine(options);
 const result=await run({...options,'ndlocr-regions':true});
 assert.equal(result.text,'生の\n認識領域');assert.equal(result.layout_available,true);assert.deepEqual(result.blocks,[]);
 assert.equal(result.recognition_regions.length,1);assert.equal(result.recognition_regions[0].text,result.text);
 assert.deepEqual(result.recognition_regions[0].bbox.pixels,{x0:0,y0:0,x1:2,y1:3});
 assert.equal(result.article_assignment,null);assert.equal(result.native_glyph_binding,null);
 assert.equal(result.engine_result.trace_sha256,crypto.createHash('sha256').update(fs.readFileSync(path.join(options.output,'trace.json'))).digest('hex'));
 assert.equal(JSON.parse(fs.readFileSync(path.join(options.output,'terminal.json'))).recognition_runs,1);
}));
test('malformed trace retains raw diagnostics but does not emit observations or retry',()=>harness(async options=>{
 tracingEngine(options,true);
 await assert.rejects(run({...options,'ndlocr-regions':true}),/NDLOCR trace/);
 assert.equal(fs.existsSync(path.join(options.output,'trace.json')),true);
 assert.equal(fs.existsSync(path.join(options.output,'recognized.txt')),true);
 assert.equal(fs.existsSync(path.join(options.output,'observations.json')),false);
 assert.equal(JSON.parse(fs.readFileSync(path.join(options.output,'terminal.json'))).status,'failed');
}));
