'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {sidecar}=require('./sidecar.cjs');
const {runPdf}=require('./pdf-cli.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function png(width,height){const b=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(b);b.writeUInt32BE(13,8);b.write('IHDR',12);b.writeUInt32BE(width,16);b.writeUInt32BE(height,20);return b;}
function harness(count=2){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-partition-driver-'));
  const options={pdf:path.join(root,'original.pdf'),extractor:path.join(root,'native'),python:'test-python',model:path.join(root,'jpn.traineddata'),output:path.join(root,'out'),partition:'native-gutters-ltr'};
  let recognitions=0,partitions=0;
  function source(directory,number=null){
    const width=number===null?2*count:2,prefix=number===null?'original':'part'+number,bytes=png(width,6);
    const frame={pixel_width:width,pixel_height:6,image_width:2*count,image_height:6,recognition_to_image:[1,0,0,1,number===null?0:2*number,0],image_to_user:[10*count,0,0,30,10,40],crop_box:[0,0,100,200],rotate:0};
    fs.writeFileSync(path.join(directory,prefix+'.png'),bytes);fs.writeFileSync(path.join(directory,prefix+'.placement.json'),JSON.stringify(frame));
    const receipt={document_sha256:hash(Buffer.from('original PDF')),PNG_sha256:hash(bytes),placement_sha256:hash(fs.readFileSync(path.join(directory,prefix+'.placement.json'))),source_crop_pixels:[number===null?0:2*number,0,number===null?2*count:2*number+2,6],visibility:'unverified'};
    fs.writeFileSync(path.join(directory,prefix+'.receipt.json'),JSON.stringify(receipt));
    return {page:0,draw:0,...(number===null?{}:{partition:number}),status:'decoded',PNG:prefix+'.png',placement:prefix+'.placement.json',receipt:prefix+'.receipt.json',receipt_sha256:hash(fs.readFileSync(path.join(directory,prefix+'.receipt.json'))),decoded_opaque_white:false,width_points:number===null?10*count:10,height_points:30,dpi:72};
  }
  function index(kind,rows){return {kind,document_sha256:hash(Buffer.from('original PDF')),body_coverage_verified:false,pages:[{page:0,capture_gaps:[],visibility:'unverified'}],rows};}
  const runtime={
    decode:async o=>{const dir=path.join(o.output,'decoded');fs.mkdirSync(dir);fs.writeFileSync(path.join(dir,'source.pdf'),'original PDF');const result=index('intrinsic_pdf_image_sources',[source(dir)]);fs.writeFileSync(path.join(dir,'index.json'),JSON.stringify(result));return result;},
    partition:async o=>{partitions++;const dir=path.join(o.output,'partitioned');fs.mkdirSync(dir);fs.writeFileSync(path.join(dir,'source.pdf'),'original PDF');const parts=Array.from({length:count},(_,n)=>source(dir,n)),result=index('partitioned_pdf_image_sources',[{page:0,draw:0,status:'partitioned',all_source_RGBA_pixels_reconstruct_exactly:true,partitions:parts}]);fs.writeFileSync(path.join(dir,'index.json'),JSON.stringify(result));return result;},
    recognize:async o=>{
      const n=recognitions++;fs.mkdirSync(o.output);const text=`未知<&> ${n}\n`,box={x0:0,y0:0,x1:2,y1:6};
      const raw={data:{text,blocks:[{text,confidence:0,bbox:box,paragraphs:[{text,confidence:0,bbox:box,lines:[{text,confidence:0,bbox:box,words:[{text,confidence:0,bbox:box}]}]}]}]}};
      const observations=sidecar(raw,{image:{sha256:hash(fs.readFileSync(o.image)),bytes:33}},{pixel_width:2,pixel_height:6,width_points:o.width_points,height_points:o.height_points,dpi:o.dpi});
      fs.writeFileSync(path.join(o.output,'observations.json'),JSON.stringify(observations));return observations;
    }
  };
  return {options,runtime,recognitions:()=>recognitions,partitions:()=>partitions,clean:()=>fs.rmSync(root,{recursive:true,force:true})};
}
test('explicit column mode recognizes each source crop once and retains raw text confidence and global coordinates',async()=>{
  const h=harness();try{const out=await runPdf(h.options,h.runtime);assert.equal(h.partitions(),1);assert.equal(h.recognitions(),2);assert.equal(out.body_coverage_verified,false);assert.equal(out.visibility,'unverified');assert.equal(out.article_assignment,null);const row=out.rows[0];assert.equal(row.status,'partitioned_unclassified');assert.equal(out.recognition_partition.mode,'native-gutters-ltr');const projected=row.partitions.map(p=>JSON.parse(fs.readFileSync(path.join(h.options.output,p.projection))));assert.deepEqual(projected.map(o=>o.blocks[0].projection.bounds_points),[[10,130,20,160],[20,130,30,160]]);assert.deepEqual(projected.map(o=>o.blocks[0].confidence),[0,0]);assert.equal(fs.readFileSync(path.join(h.options.output,row.partitioned_text),'utf8'),'未知<&> 0\n\n未知<&> 1\n');}finally{h.clean();}
});
test('partition count comes from the supplied source partition list',async()=>{
  for(const count of [3,5]){const h=harness(count);try{const out=await runPdf(h.options,h.runtime);assert.equal(h.recognitions(),count);assert.equal(out.rows[0].partitions.length,count);assert.deepEqual(out.rows[0].partitions.map(p=>p.partition),Array.from({length:count},(_,n)=>n));}finally{h.clean();}}
});
test('every source column uses one fixed single-column profile regardless of recognized words',async()=>{
  const h=harness();try{const old=h.runtime.recognize,layouts=[];h.runtime.recognize=async o=>{layouts.push(o.layout);return old(o);};await runPdf(h.options,h.runtime);assert.deepEqual(layouts,['single-column','single-column']);}finally{h.clean();}
});
test('one failed recognition preserves other raw partitions and does not publish truncated combined text',async()=>{
  const h=harness();try{const old=h.runtime.recognize;let calls=0;h.runtime.recognize=async o=>{if(++calls===2)throw new Error('second crop failure');return old(o);};const out=await runPdf(h.options,h.runtime);assert.equal(calls,2);assert.equal(out.rows[0].status,'failed');assert.equal(out.rows[0].partitions[0].status,'recognized_unclassified');assert.match(out.rows[0].partitions[1].error,/second crop failure/);assert.equal(out.rows[0].partitioned_text,undefined);assert.equal(fs.existsSync(path.join(h.options.output,'page0-draw0-partitioned-text.txt')),false);assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'partial');}finally{h.clean();}
});
test('source partition failures stop recognition and retain decoded inputs',async()=>{
  const h=harness();try{h.runtime.partition=async()=>{throw new Error('native_paths unavailable');};await assert.rejects(runPdf(h.options,h.runtime),/native_paths unavailable/);assert.equal(h.recognitions(),0);assert.equal(fs.existsSync(path.join(h.options.output,'decoded','index.json')),true);assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'failed');}finally{h.clean();}
});
test('out-of-sequence partition identifiers are rejected before recognition',async()=>{
  const h=harness();try{const old=h.runtime.partition;h.runtime.partition=async o=>{const index=await old(o);index.rows[0].partitions[0].partition=7;return index;};await assert.rejects(runPdf(h.options,h.runtime),/partition sequence/);assert.equal(h.recognitions(),0);}finally{h.clean();}
});
test('partition draw ownership cannot silently differ from the original source draw',async()=>{
  const h=harness();try{const old=h.runtime.partition;h.runtime.partition=async o=>{const index=await old(o);index.rows[0].partitions[0].draw=1;return index;};await assert.rejects(runPdf(h.options,h.runtime),/occurrence mismatch/);assert.equal(h.recognitions(),0);}finally{h.clean();}
});
test('unsupported partition mode is rejected before decoding or recognition',async()=>{
  const h=harness();try{let decoded=false;h.runtime.decode=async()=>{decoded=true;};await assert.rejects(runPdf({...h.options,partition:'guess-from-company-name'},h.runtime),/Unsupported recognition partition/);assert.equal(decoded,false);assert.equal(h.recognitions(),0);}finally{h.clean();}
});
test('without opt-in partitioning the partition producer is never called',async()=>{
  const h=harness();try{h.runtime.partition=async()=>{throw new Error('must not be called');};h.runtime.recognize=async o=>{fs.mkdirSync(o.output);const bytes=fs.readFileSync(o.image);const observations=sidecar({data:{text:'raw full image\n',blocks:[]}},{image:{sha256:hash(bytes),bytes:bytes.length}},{pixel_width:4,pixel_height:6,width_points:o.width_points,height_points:o.height_points,dpi:o.dpi});fs.writeFileSync(path.join(o.output,'observations.json'),JSON.stringify(observations));return observations;};const out=await runPdf({...h.options,partition:'none'},h.runtime);assert.equal(out.rows[0].status,'recognized_unclassified');assert.equal(out.recognition_partition,undefined);}finally{h.clean();}
});
