'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {recognizeBuffers,run}=require('./sidecar.cjs');
test('single-column layout requests the documented PSM4 once without changing OCR text',async()=>{
  const parameters=[],events=[];let calls=0,terminated=0;
  const fake=async()=>({setParameters:async p=>parameters.push(p),recognize:async()=>{calls++;return {data:{text:'未知 & 償務者\n'}};},terminate:async()=>{terminated++;}});
  const output=await recognizeBuffers(fake,Buffer.from('immutable PNG'),Buffer.from('fixed model'),300,e=>events.push(e),'single-column');
  assert.deepEqual(parameters,[{tessedit_pageseg_mode:'4',user_defined_dpi:'300'}]);assert.equal(output.data.text,'未知 & 償務者\n');assert.equal(calls,1);assert.equal(terminated,1);
});
test('unknown or inherited layout keys fail before creating an engine',async()=>{
  for(const profile of ['per-case-result','constructor','toString','__proto__',null,4,true,{}]){
    let calls=0;const fake=async()=>{calls++;return {setParameters:async()=>{},recognize:async()=>({data:{text:''}}),terminate:async()=>{}};};
    await assert.rejects(recognizeBuffers(fake,Buffer.from('image'),Buffer.from('model'),300,()=>{},profile),/layout/);assert.equal(calls,0);
  }
});
test('invalid run layout fails before image/model files or output creation',async()=>{
  await assert.rejects(run({layout:'per-case-result',image:'must-not-be-read.png',model:'must-not-be-read.traineddata',output:'must-not-be-created'}),/layout/);
});
