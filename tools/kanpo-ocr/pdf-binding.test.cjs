'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {sidecar}=require('./sidecar.cjs');
const {bindProjection}=require('./pdf-binding.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function fixture() {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-pdf-binding-'));
  const png=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(png);png.writeUInt32BE(13,8);png.write('IHDR',12);png.writeUInt32BE(4,16);png.writeUInt32BE(6,20);
  const frame={pixel_width:4,pixel_height:6,image_width:4,image_height:6,recognition_to_image:[1,0,0,1,0,0],image_to_user:[20,0,0,30,10,40],crop_box:[0,0,100,200],rotate:0};
  fs.writeFileSync(path.join(dir,'source.pdf'),'immutable PDF fixture');fs.writeFileSync(path.join(dir,'draw.png'),png);fs.writeFileSync(path.join(dir,'placement.json'),JSON.stringify(frame));
  const receipt={document_sha256:hash(fs.readFileSync(path.join(dir,'source.pdf'))),page:0,draw:2,object_ref:[9,0],PNG_sha256:hash(png),placement_sha256:hash(fs.readFileSync(path.join(dir,'placement.json'))),source_geometry_complete:false,capture_gaps:[{kind:'InlineImage'}],clips:[{geometry_complete:false}],visibility:'unverified'};
  fs.writeFileSync(path.join(dir,'receipt.json'),JSON.stringify(receipt));
  const entry={PNG:'draw.png',placement:'placement.json',receipt:'receipt.json',receipt_sha256:hash(fs.readFileSync(path.join(dir,'receipt.json')))};
  const data={text:'償務者\n',blocks:[{text:'償務者\n',confidence:0,bbox:{x0:0,y0:1,x1:4,y1:5},paragraphs:[{text:'償務者\n',confidence:0,bbox:{x0:0,y0:1,x1:4,y1:5},lines:[{text:'償務者\n',confidence:0,bbox:{x0:0,y0:1,x1:4,y1:5},words:[{text:'償務者',confidence:0,bbox:{x0:0,y0:1,x1:4,y1:5}}]}]}]}]};
  const obs=sidecar({data},{image:{sha256:hash(png),bytes:png.length}},{pixel_width:4,pixel_height:6,width_points:20,height_points:30,dpi:72});
  return {dir,frame,receipt,entry,obs,clean(){fs.rmSync(dir,{recursive:true,force:true});}};
}
function check(f){return bindProjection(f.obs,f.entry,f.dir);}
test('source projection preserves raw OCR occurrences and reports evidence without visibility or assignment',()=>{
  const f=fixture();try {const before=structuredClone(f.obs),out=check(f);assert.deepEqual(f.obs,before);assert.equal(out.text,before.text);assert.equal(out.blocks[0].confidence,0);assert.equal(out.source_binding,'declared_unverified');assert.equal(out.visibility,'unverified');assert.equal(out.native_glyph_binding,null);assert.equal(out.article_assignment,null);assert.deepEqual(out.pdf_source_evidence.receipt,f.receipt);assert.equal(out.pdf_source_evidence.status,'producer_hashes_checked');assert.deepEqual(out.blocks[0].projection.bounds_points,[10,135,30,155]);}finally{f.clean();}
});
test('a different recognition PNG hash is rejected even with matching dimensions',()=>{
  const f=fixture();try{f.obs.source.image.sha256='0'.repeat(64);assert.throws(()=>check(f),/recognition PNG/);}finally{f.clean();}
});
test('changed decoded pixels are rejected before mapping',()=>{
  const f=fixture();try{fs.appendFileSync(path.join(f.dir,'draw.png'),'changed');assert.throws(()=>check(f),/PNG/);}finally{f.clean();}
});
test('changed source PDF snapshot is rejected',()=>{
  const f=fixture();try{fs.writeFileSync(path.join(f.dir,'source.pdf'),'different PDF');assert.throws(()=>check(f),/PDF snapshot/);}finally{f.clean();}
});
test('changed placement or receipt is rejected',()=>{
  for(const file of ['placement.json','receipt.json']){const f=fixture();try{fs.appendFileSync(path.join(f.dir,file),' ');assert.throws(()=>check(f),/placement|receipt/);}finally{f.clean();}}
});
test('recognizer byte length and canvas must match the decoded input',()=>{
  for(const change of [o=>o.source.image.bytes++,o=>o.canvas.pixel_height++]){const f=fixture();try{change(f.obs);assert.throws(()=>check(f));}finally{f.clean();}}
});
test('pre-existing source evidence cannot be silently overwritten',()=>{
  const f=fixture();try{f.obs.pdf_source_evidence={old:true};assert.throws(()=>check(f),/evidence/);}finally{f.clean();}
});
