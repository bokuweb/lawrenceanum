#!/usr/bin/env node
'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {parseArgs}=require('node:util');
const {run,writeAtomicJson,recordFailure,watchTerminal}=require('./sidecar.cjs');
const {bindProjection}=require('./pdf-binding.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const usage=`Usage: node tools/kanpo-ocr/pdf-cli.cjs --extractor /path/image_sources --python /path/python3 --model /path/jpn.traineddata --output NEW_DIR original.pdf

Opt-in source image decoding, one-pass OCR and PDF placement. Requires the pinned
Python decoder dependencies. No downloads, text correction or canonical merging.
Intrinsic image pixels do not certify final PDF visibility or body coverage.
`;
async function decode(options){
  const decoder=path.join(__dirname,'decode_sources.py');
  const stdout=fs.openSync(path.join(options.output,'decoder.stdout'),'wx'),stderr=fs.openSync(path.join(options.output,'decoder.stderr'),'wx');
  let processResult;
  try{processResult=spawnSync(options.python,[decoder,'--extractor',options.extractor,'--output',path.join(options.output,'decoded'),options.pdf],{stdio:['ignore',stdout,stderr]});}
  finally{fs.closeSync(stdout);fs.closeSync(stderr);}
  writeAtomicJson(path.join(options.output,'decoder-process.json'),{status:processResult.status,signal:processResult.signal,error:processResult.error&&String(processResult.error),decoder_sha256:hash(fs.readFileSync(decoder))});
  if(processResult.error)throw processResult.error;
  if(![0,2].includes(processResult.status))throw new Error('PDF source decoder failed; see decoder.stderr and decoded/terminal.json');
  return JSON.parse(fs.readFileSync(path.join(options.output,'decoded','index.json'),'utf8'));
}
async function runPdf(options,runtime={decode,recognize:run}){
  fs.mkdirSync(options.output);
  const stopWatching=watchTerminal(options.output);
  try{
    writeAtomicJson(path.join(options.output,'run-inputs.json'),{pdf_path:path.resolve(options.pdf),extractor_path:path.resolve(options.extractor),python:options.python,model_path:path.resolve(options.model),tool_sha256:hash(fs.readFileSync(__filename)),binding_sha256:hash(fs.readFileSync(path.join(__dirname,'pdf-binding.cjs'))),normalization:null,confidence_filter:null,canonical_integration:false});
    const sources=await runtime.decode(options),directory=path.join(options.output,'decoded');
    if(sources.kind!=='intrinsic_pdf_image_sources'||sources.body_coverage_verified!==false||!Array.isArray(sources.rows))throw new Error('Unsupported decoded source index');
    const rows=[];
    let requests=0;
    for(const entry of sources.rows){
      if(![entry.page,entry.draw].every(v=>Number.isSafeInteger(v)&&v>=0))throw new Error('Invalid source draw occurrence');
      const row={...entry};
      if(entry.status==='failed'){rows.push(row);continue;}
      if(entry.status!=='decoded')throw new Error('Unsupported source draw status');
      if(entry.decoded_opaque_white){row.status='not_recognized_exact_opaque_white';rows.push(row);continue;}
      const prefix=`page${entry.page}-draw${entry.draw}`,output=path.join(options.output,prefix+'-ocr');
      try{
        requests++;
        const observations=await runtime.recognize({image:path.join(directory,entry.PNG),model:options.model,output,width_points:entry.width_points,height_points:entry.height_points,dpi:entry.dpi});
        const projected=bindProjection(observations,entry,directory),projection=prefix+'-projected.json';
        writeAtomicJson(path.join(options.output,projection),projected);
        row.status='recognized_unclassified';row.observations=path.relative(options.output,path.join(output,'observations.json'));row.projection=projection;
      }catch(error){row.status='failed';row.error=String(error);row.recognition_retry=false;}
      rows.push(row);
    }
    const partial=rows.some(r=>r.status==='failed');
    const index={schema:1,kind:'unclassified_pdf_image_ocr',document_sha256:sources.document_sha256,decoded_index_sha256:hash(fs.readFileSync(path.join(directory,'index.json'))),pages:sources.pages,rows,visibility:'unverified',source_binding:'producer_hashes_checked_visibility_unverified',body_coverage_verified:false,article_assignment:null,native_glyph_binding:null};
    writeAtomicJson(path.join(options.output,'index.json'),index);
    writeAtomicJson(path.join(options.output,'terminal.json'),{status:partial?'partial':'complete',recognition_requests:requests,recognition_retry:false,canonical_integration:false});
    return index;
  }catch(error){throw recordFailure(options.output,error);}
  finally{stopWatching();}
}
if(require.main===module){
  (async()=>{
    const {values,positionals}=parseArgs({allowPositionals:true,options:{extractor:{type:'string'},python:{type:'string',default:'python3'},model:{type:'string'},output:{type:'string'},help:{type:'boolean'}}});
    if(values.help){process.stdout.write(usage);return;}
    if(positionals.length!==1||!values.extractor||!values.model||!values.output)throw new Error(usage);
    const result=await runPdf({...values,pdf:positionals[0]});
    process.stdout.write(values.output+'\n',()=>process.exit(result.rows.some(r=>r.status==='failed')?2:0));
  })().catch(error=>process.stderr.write(String(error)+(error?.recording_error?'\nTerminal journal: '+error.recording_error:'')+'\n',()=>process.exit(1)));
}
module.exports={runPdf,decode};
