#!/usr/bin/env node
'use strict';
const { parseArgs } = require('node:util');
const { run } = require('./engine.cjs');
const usage = `Usage: node tools/kanpo-ocr/cli.cjs [--engine ndlocr|tesseract] [--ndlocr-bin /path/ndlocr-lite-rs] [--ndlocr-receipt /path/build.json] [--ndlocr-regions] --model MODEL --output NEW_DIR --width-points WIDTH --height-points HEIGHT --dpi DPI page.png

Opt-in Japanese full-page PNG recognition. The PNG must already have display orientation.
Default: NDLOCR 0.0.15, pinned build receipt; MODEL is a models directory.
Explicit --engine tesseract uses pinned jpn.traineddata and saves layout observations.
NDLOCR: MODEL is a models directory; TXT by default.
Optional --ndlocr-regions exports raw recognizer rectangles without word/glyph boxes.
NDLOCR-only thresholds: --det-conf-threshold 0.25 --min-line-confidence 0.3
PDF/native output and Article assignment are handled separately. No model download is performed.
`;
async function main() {
  const { values,positionals }=parseArgs({allowPositionals:true,options:{engine:{type:'string'},'ndlocr-bin':{type:'string'},'ndlocr-receipt':{type:'string'},'ndlocr-regions':{type:'boolean'},'det-conf-threshold':{type:'string'},'min-line-confidence':{type:'string'},model:{type:'string'},output:{type:'string'},'width-points':{type:'string'},'height-points':{type:'string'},dpi:{type:'string'},help:{type:'boolean'}}});
  if(values.help){process.stdout.write(usage,()=>process.exit(0));return;}
  if(positionals.length!==1 || ['model','output','width-points','height-points','dpi'].some(k=>values[k]===undefined)) throw new Error(usage);
  await run({...values,image:positionals[0],width_points:Number(values['width-points']),height_points:Number(values['height-points']),dpi:Number(values.dpi)});
  process.stdout.write(values.output+'\n',()=>process.exit(0));
}
main().catch(error=>process.stderr.write(String(error)+(error?.cleanup_recording_error?'\nCleanup journal: '+error.cleanup_recording_error:'')+(error?.recording_error?'\nTerminal journal: '+error.recording_error:'')+'\n',()=>process.exit(1)));
