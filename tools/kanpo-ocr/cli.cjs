#!/usr/bin/env node
'use strict';
const { parseArgs } = require('node:util');
const { run } = require('./sidecar.cjs');
const usage = `Usage: node tools/kanpo-ocr/cli.cjs --model /path/jpn.traineddata --output NEW_DIR --width-points WIDTH --height-points HEIGHT --dpi DPI page.png

Opt-in Japanese full-page PNG recognition. The PNG must already have display orientation.
Saves raw recognition, unchanged text and block/paragraph/line/word observations.
PDF/native output and Article assignment are handled separately. No model download is performed.
`;
async function main() {
  const { values,positionals }=parseArgs({allowPositionals:true,options:{model:{type:'string'},output:{type:'string'},'width-points':{type:'string'},'height-points':{type:'string'},dpi:{type:'string'},help:{type:'boolean'}}});
  if(values.help){process.stdout.write(usage,()=>process.exit(0));return;}
  if(positionals.length!==1 || ['model','output','width-points','height-points','dpi'].some(k=>values[k]===undefined)) throw new Error(usage);
  await run({image:positionals[0],model:values.model,output:values.output,width_points:Number(values['width-points']),height_points:Number(values['height-points']),dpi:Number(values.dpi)});
  process.stdout.write(values.output+'\n',()=>process.exit(0));
}
main().catch(error=>process.stderr.write(String(error)+(error?.recording_error?'\nTerminal journal: '+error.recording_error:'')+'\n',()=>process.exit(1)));
