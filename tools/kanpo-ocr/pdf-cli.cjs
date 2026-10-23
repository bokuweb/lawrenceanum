#!/usr/bin/env node
'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {parseArgs}=require('node:util');
const {run,writeAtomicJson,recordFailure,watchTerminal}=require('./sidecar.cjs');
const {bindProjection}=require('./pdf-binding.cjs');
const {partitionSources}=require('./partition.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const usage=`Usage: node tools/kanpo-ocr/pdf-cli.cjs --extractor /path/image_sources --python /path/python3 --model /path/jpn.traineddata --output NEW_DIR [--partition native-gutters-ltr] original.pdf

Opt-in source image decoding, one-pass OCR and PDF placement. Requires the pinned
Python decoder dependencies. No downloads, text correction or canonical merging.
Intrinsic image pixels do not certify final PDF visibility or body coverage.
Optional native-gutters-ltr partitions preserve every source pixel and explicitly
assume horizontal text read left-to-right in source-ruled columns.
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
async function runPdf(options,runtime={decode,recognize:run,partition:partitionSources}){
  fs.mkdirSync(options.output);
  const stopWatching=watchTerminal(options.output);
  try{
    writeAtomicJson(path.join(options.output,'run-inputs.json'),{pdf_path:path.resolve(options.pdf),extractor_path:path.resolve(options.extractor),python:options.python,model_path:path.resolve(options.model),tool_sha256:hash(fs.readFileSync(__filename)),binding_sha256:hash(fs.readFileSync(path.join(__dirname,'pdf-binding.cjs'))),recognition_partition:options.partition??'none',normalization:null,confidence_filter:null,canonical_integration:false});
    if(options.partition!==undefined&&!['none','native-gutters-ltr'].includes(options.partition))throw new Error('Unsupported recognition partition mode');
    let sources=await runtime.decode(options),directory=path.join(options.output,'decoded');
    if(sources.kind!=='intrinsic_pdf_image_sources'||sources.body_coverage_verified!==false||!Array.isArray(sources.rows))throw new Error('Unsupported decoded source index');
    if(options.partition==='native-gutters-ltr'){
      sources=await runtime.partition(options);directory=path.join(options.output,'partitioned');
      if(sources.kind!=='partitioned_pdf_image_sources'||sources.body_coverage_verified!==false||!Array.isArray(sources.rows))throw new Error('Unsupported partition source index');
    }
    const rows=[];
    let requests=0;
    for(const entry of sources.rows){
      if(![entry.page,entry.draw].every(v=>Number.isSafeInteger(v)&&v>=0))throw new Error('Invalid source draw occurrence');
      if(entry.status==='failed'){rows.push({...entry});continue;}
      const partitioned=entry.status==='partitioned';
      if(entry.status!=='decoded'&&!partitioned)throw new Error('Unsupported source draw status');
      if(partitioned&&(!Array.isArray(entry.partitions)||!entry.partitions.length))throw new Error('Missing source partitions');
      const processed=[],texts=[];
      for(const region of partitioned?entry.partitions:[entry]){
        const row={...region};
        if(region.page!==entry.page||region.draw!==entry.draw||region.status!=='decoded')throw new Error('Partition/source occurrence mismatch');
        if(partitioned&&(!Number.isSafeInteger(region.partition)||region.partition!==processed.length))throw new Error('Invalid source partition sequence');
        if(region.decoded_opaque_white){row.status='not_recognized_exact_opaque_white';processed.push(row);texts.push('');continue;}
        const prefix=`page${entry.page}-draw${entry.draw}`+(partitioned?`-part${region.partition}`:''),output=path.join(options.output,prefix+'-ocr');
        try{
          requests++;
          const observations=await runtime.recognize({image:path.join(directory,region.PNG),model:options.model,output,width_points:region.width_points,height_points:region.height_points,dpi:region.dpi,layout:partitioned?'single-column':'page'});
          const projected=bindProjection(observations,region,directory),projection=prefix+'-projected.json';
          writeAtomicJson(path.join(options.output,projection),projected);
          row.status='recognized_unclassified';row.observations=path.relative(options.output,path.join(output,'observations.json'));row.projection=projection;texts.push(observations.text);
        }catch(error){row.status='failed';row.error=String(error);row.recognition_retry=false;}
        processed.push(row);
      }
      if(partitioned){
        const failed=processed.some(r=>r.status==='failed'),row={...entry,status:failed?'failed':'partitioned_unclassified',partitions:processed};
        if(!failed){const filename=`page${entry.page}-draw${entry.draw}-partitioned-text.txt`;fs.writeFileSync(path.join(options.output,filename),texts.join('\n'),{flag:'wx'});row.partitioned_text=filename;row.text_derivation='raw partition text in explicit horizontal_ltr column order, separated by newline';}
        rows.push(row);
      }else rows.push(processed[0]);
    }
    const partial=rows.some(r=>r.status==='failed');
    const index={schema:1,kind:'unclassified_pdf_image_ocr',document_sha256:sources.document_sha256,decoded_index_sha256:hash(fs.readFileSync(path.join(options.output,'decoded','index.json'))),pages:sources.pages,rows,visibility:'unverified',source_binding:'producer_hashes_checked_visibility_unverified',body_coverage_verified:false,article_assignment:null,native_glyph_binding:null};
    if(options.partition==='native-gutters-ltr')index.recognition_partition={mode:options.partition,column_layout:'single-column',column_psm:4,index_sha256:hash(fs.readFileSync(path.join(directory,'index.json')))};
    writeAtomicJson(path.join(options.output,'index.json'),index);
    writeAtomicJson(path.join(options.output,'terminal.json'),{status:partial?'partial':'complete',recognition_requests:requests,recognition_retry:false,canonical_integration:false});
    return index;
  }catch(error){throw recordFailure(options.output,error);}
  finally{stopWatching();}
}
if(require.main===module){
  (async()=>{
    const {values,positionals}=parseArgs({allowPositionals:true,options:{extractor:{type:'string'},python:{type:'string',default:'python3'},model:{type:'string'},output:{type:'string'},partition:{type:'string',default:'none'},help:{type:'boolean'}}});
    if(values.help){process.stdout.write(usage);return;}
    if(positionals.length!==1||!values.extractor||!values.model||!values.output)throw new Error(usage);
    const result=await runPdf({...values,pdf:positionals[0]});
    process.stdout.write(values.output+'\n',()=>process.exit(result.rows.some(r=>r.status==='failed')?2:0));
  })().catch(error=>process.stderr.write(String(error)+(error?.recording_error?'\nTerminal journal: '+error.recording_error:'')+'\n',()=>process.exit(1)));
}
module.exports={runPdf,decode};
