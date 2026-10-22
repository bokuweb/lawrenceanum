'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {sidecar}=require('./sidecar.cjs');
const {runPdf,decode}=require('./pdf-cli.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function harness(){
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-pdf-cli-'));
  const options={pdf:path.join(directory,'input.pdf'),extractor:path.join(directory,'native'),python:'unused-python',model:path.join(directory,'jpn.traineddata'),output:path.join(directory,'out')};
  const png=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(png);png.writeUInt32BE(13,8);png.write('IHDR',12);png.writeUInt32BE(2,16);png.writeUInt32BE(3,20);
  let calls=0;
  const runtime={
    decode:async o=>{
      const dir=path.join(o.output,'decoded');fs.mkdirSync(dir);fs.writeFileSync(path.join(dir,'source.pdf'),'source snapshot');fs.writeFileSync(path.join(dir,'pixels.png'),png);
      const frame={pixel_width:2,pixel_height:3,image_width:2,image_height:3,recognition_to_image:[1,0,0,1,0,0],image_to_user:[12,0,0,18,5,7],crop_box:[0,0,100,200],rotate:0};
      fs.writeFileSync(path.join(dir,'placement.json'),JSON.stringify(frame));
      const receipt={document_sha256:hash(Buffer.from('source snapshot')),PNG_sha256:hash(png),placement_sha256:hash(fs.readFileSync(path.join(dir,'placement.json'))),visibility:'unverified'};
      fs.writeFileSync(path.join(dir,'receipt.json'),JSON.stringify(receipt));
      const entry={page:0,draw:0,status:'decoded',PNG:'pixels.png',placement:'placement.json',receipt:'receipt.json',receipt_sha256:hash(fs.readFileSync(path.join(dir,'receipt.json'))),decoded_opaque_white:false,width_points:12,height_points:18,dpi:12};
      const index={kind:'intrinsic_pdf_image_sources',document_sha256:receipt.document_sha256,body_coverage_verified:false,pages:[{page:0,capture_gaps:[{kind:'InlineImage'}],visibility:'unverified'}],rows:[entry]};
      fs.writeFileSync(path.join(dir,'index.json'),JSON.stringify(index));return index;
    },
    recognize:async o=>{
      calls++;fs.mkdirSync(o.output);
      const obs=sidecar({data:{text:'未知 & 償務者\n',blocks:[]}},{image:{sha256:hash(fs.readFileSync(o.image)),bytes:png.length}},{pixel_width:2,pixel_height:3,width_points:o.width_points,height_points:o.height_points,dpi:o.dpi});
      fs.writeFileSync(path.join(o.output,'observations.json'),JSON.stringify(obs));return obs;
    }
  };
  return {directory,options,runtime,calls:()=>calls,clean:()=>fs.rmSync(directory,{recursive:true,force:true})};
}
test('PDF pipeline preserves uncorrected observations and source gaps through a single recognition request',async()=>{
  const h=harness();try{
    const out=await runPdf(h.options,h.runtime);assert.equal(h.calls(),1);assert.equal(out.kind,'unclassified_pdf_image_ocr');assert.equal(out.body_coverage_verified,false);assert.equal(out.visibility,'unverified');assert.equal(out.article_assignment,null);assert.equal(out.pages[0].capture_gaps[0].kind,'InlineImage');
    const row=out.rows[0];assert.equal(row.status,'recognized_unclassified');const projection=JSON.parse(fs.readFileSync(path.join(h.options.output,row.projection)));assert.equal(projection.text,'未知 & 償務者\n');assert.equal(projection.source_binding,'declared_unverified');assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'complete');
  }finally{h.clean();}
});
test('exact opaque-white and failed decode occurrences remain in the index without recognition',async()=>{
  const h=harness();try{
    const old=h.runtime.decode;h.runtime.decode=async o=>{const index=await old(o);index.rows[0].decoded_opaque_white=true;index.rows.push({page:0,draw:1,status:'failed',error:'unsupported filter',source_draw:{object_ref:[99,0]}});return index;};
    const out=await runPdf(h.options,h.runtime);assert.equal(h.calls(),0);assert.equal(out.rows.length,2);assert.equal(out.rows[0].status,'not_recognized_exact_opaque_white');assert.equal(out.rows[1].error,'unsupported filter');assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'partial');
  }finally{h.clean();}
});
test('recognition failure is retained and never retried or converted to successful empty text',async()=>{
  const h=harness();try{
    let calls=0;h.runtime.recognize=async()=>{calls++;throw new Error('one fixed engine failure');};
    const out=await runPdf(h.options,h.runtime);assert.equal(calls,1);assert.equal(out.rows[0].status,'failed');assert.match(out.rows[0].error,/fixed engine failure/);assert.equal(out.rows[0].recognition_retry,false);assert.equal(out.rows[0].projection,undefined);
  }finally{h.clean();}
});
test('changed actual recognition input fails binding while raw observations survive',async()=>{
  const h=harness();try{
    const old=h.runtime.recognize;h.runtime.recognize=async o=>{const obs=await old(o);obs.source.image.sha256='0'.repeat(64);return obs;};
    const out=await runPdf(h.options,h.runtime);assert.equal(out.rows[0].status,'failed');assert.match(out.rows[0].error,/recognition PNG/);assert.equal(out.rows[0].projection,undefined);assert.equal(fs.existsSync(path.join(h.options.output,'page0-draw0-ocr','observations.json')),true);
  }finally{h.clean();}
});
test('new run refuses to replace an earlier run',async()=>{
  const h=harness();try{await runPdf(h.options,h.runtime);const first=fs.readFileSync(path.join(h.options.output,'index.json'));await assert.rejects(runPdf(h.options,h.runtime),/EEXIST/);assert.deepEqual(fs.readFileSync(path.join(h.options.output,'index.json')),first);assert.equal(h.calls(),1);}finally{h.clean();}
});
test('source decoder failure is recorded before any recognition',async()=>{
  const h=harness();try{h.runtime.decode=async()=>{throw new Error('source snapshot mismatch');};await assert.rejects(runPdf(h.options,h.runtime),/snapshot mismatch/);assert.equal(h.calls(),0);assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'failed');}finally{h.clean();}
});
test('missing Python executable is recorded as process failure without shell interpretation',async()=>{
  const h=harness();try{fs.mkdirSync(h.options.output);await assert.rejects(decode({...h.options,python:path.join(h.directory,'missing python;no shell')}),/ENOENT/);const saved=JSON.parse(fs.readFileSync(path.join(h.options.output,'decoder-process.json')));assert.match(saved.error,/ENOENT/);}finally{h.clean();}
});
