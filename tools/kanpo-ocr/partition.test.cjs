'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {geometryManifest,partitionSources}=require('./partition.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function fixture(){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-source-geometry-')),directory=path.join(root,'decoded');fs.mkdirSync(directory);
  const pdf=Buffer.from('original PDF snapshot'),png=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(png);png.writeUInt32BE(13,8);png.write('IHDR',12);png.writeUInt32BE(4,16);png.writeUInt32BE(6,20);
  const frame={pixel_width:4,pixel_height:6,image_width:4,image_height:6,recognition_to_image:[1,0,0,1,0,0],image_to_user:[20,0,0,30,10,40],crop_box:[0,0,100,200],rotate:0};
  const draw={draw:0,pixel_width:4,pixel_height:6,image_to_user:frame.image_to_user,object_ref:[9,0],source_geometry_complete:true};
  const packet={schema:1,source:'native_xobject_snapshot',pages:[{page:0,document_sha256:hash(pdf),crop_box:frame.crop_box,rotate:0,native_paths:[],images:[draw]}]};
  const receipt={document_sha256:hash(pdf),page:0,draw:0,source_draw:structuredClone(draw)};
  const entry={page:0,draw:0,status:'decoded',PNG:'pixels.png',placement:'placement.json',receipt:'receipt.json'};
  function write(){
    fs.writeFileSync(path.join(directory,'source.pdf'),pdf);fs.writeFileSync(path.join(directory,entry.PNG),png);fs.writeFileSync(path.join(directory,entry.placement),JSON.stringify(frame));fs.writeFileSync(path.join(directory,'sources.json'),JSON.stringify(packet));
    Object.assign(receipt,{native_packet_sha256:hash(fs.readFileSync(path.join(directory,'sources.json'))),placement_sha256:hash(fs.readFileSync(path.join(directory,entry.placement))),PNG_sha256:hash(png)});
    fs.writeFileSync(path.join(directory,entry.receipt),JSON.stringify(receipt));entry.receipt_sha256=hash(fs.readFileSync(path.join(directory,entry.receipt)));
    const index={kind:'intrinsic_pdf_image_sources',document_sha256:hash(pdf),native_packet_sha256:receipt.native_packet_sha256,rows:[entry]};fs.writeFileSync(path.join(directory,'index.json'),JSON.stringify(index));
  }
  write();return {root,directory,pdf,png,frame,packet,receipt,entry,write,clean(){fs.rmSync(root,{recursive:true,force:true});}};
}
test('full canvas geometry uses the existing affine projector and the same source packet',()=>{
  const f=fixture();try{const m=geometryManifest(f.directory);assert.equal(m.document_sha256,hash(f.pdf));assert.deepEqual(m.rows[0].page_quad,[[10,130],[30,130],[30,160],[10,160]]);assert.equal(m.rows[0].PNG_sha256,hash(f.png));assert.equal(m.kind,'source_canvas_geometry');}finally{f.clean();}
});
test('missing native path observations cannot be silently treated as an empty path list',()=>{
  const f=fixture();try{delete f.packet.pages[0].native_paths;f.write();assert.throws(()=>geometryManifest(f.directory),/native_paths/);}finally{f.clean();}
});
test('changed source PNG PDF or receipt is rejected before partitioning',()=>{
  for(const name of ['pixels.png','source.pdf','receipt.json']){const f=fixture();try{fs.appendFileSync(path.join(f.directory,name),'changed');assert.throws(()=>geometryManifest(f.directory));}finally{f.clean();}}
});
test('a self-consistent frame hash cannot replace the actual native placement',()=>{
  const f=fixture();try{f.frame.image_to_user=[20,0,0,30,70,80];f.write();assert.throws(()=>geometryManifest(f.directory),/native image draw/);}finally{f.clean();}
});
test('a receipt with the wrong draw is rejected even after its file hash is updated',()=>{
  const f=fixture();try{f.receipt.source_draw.object_ref=[10,0];f.write();assert.throws(()=>geometryManifest(f.directory),/native image draw/);}finally{f.clean();}
});
test('object property order does not change native observation identity',()=>{
  const f=fixture();try{f.receipt.source_draw=Object.fromEntries(Object.entries(f.receipt.source_draw).reverse());f.write();assert.doesNotThrow(()=>geometryManifest(f.directory));}finally{f.clean();}
});
test('source asset paths remain inside their producing directory',()=>{
  const f=fixture();try{const index=JSON.parse(fs.readFileSync(path.join(f.directory,'index.json')));index.rows[0].placement='../outside.json';fs.writeFileSync(path.join(f.directory,'index.json'),JSON.stringify(index));assert.throws(()=>geometryManifest(f.directory),/local source asset/);}finally{f.clean();}
});
test('missing Python is a retained partition process failure with no shell execution',async()=>{
  const f=fixture();try{await assert.rejects(partitionSources({output:f.root,python:path.join(f.root,'missing python;literal')}),/ENOENT/);const process=JSON.parse(fs.readFileSync(path.join(f.root,'partition-process.json')));assert.match(process.error,/ENOENT/);assert.equal(fs.existsSync(path.join(f.root,'partition-geometry.json')),true);}finally{f.clean();}
});
