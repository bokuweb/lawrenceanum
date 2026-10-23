#!/usr/bin/env node
'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {parseArgs}=require('node:util');
const {projectLayout}=require('./placement.cjs');
const {writeAtomicJson}=require('./sidecar.cjs');
const sha256=b=>crypto.createHash('sha256').update(b).digest('hex');
const usage=`Usage: node tools/kanpo-ocr/project-cli.cjs --placement placement.json --output NEW.json observations.json

Project every OCR occurrence through explicit image/PDF affine coordinates.
No model download, inference, text correction or native/Article assignment.
The supplied placement/source relationship and final visibility remain unverified.
`;
function projectFile(options) {
  const raw=fs.readFileSync(options.observations),frame=fs.readFileSync(options.placement);
  const input=JSON.parse(raw.toString('utf8')),placement=JSON.parse(frame.toString('utf8'));
  if(Object.hasOwn(input,'projection_inputs'))throw new Error('Existing projection inputs must not be overwritten');
  const result=projectLayout(input,placement);
  result.projection_inputs={observations:{path:path.resolve(options.observations),sha256:sha256(raw),bytes:raw.length},placement:{path:path.resolve(options.placement),sha256:sha256(frame),bytes:frame.length},tool_sha256:sha256(fs.readFileSync(__filename)),geometry_module_sha256:sha256(fs.readFileSync(path.join(__dirname,'placement.cjs')))};
  writeAtomicJson(options.output,result);
  return result;
}
if(require.main===module){
  try {
    const {values,positionals}=parseArgs({allowPositionals:true,options:{placement:{type:'string'},output:{type:'string'},help:{type:'boolean'}}});
    if(values.help)process.stdout.write(usage);
    else {
      if(positionals.length!==1||!values.placement||!values.output)throw new Error(usage);
      projectFile({observations:positionals[0],placement:values.placement,output:values.output});process.stdout.write(values.output+'\n');
    }
  }catch(error){process.stderr.write(String(error)+(error?.partial_output?'\nPartial output: '+error.partial_output:'')+'\n');process.exitCode=1;}
}
module.exports={projectFile};
