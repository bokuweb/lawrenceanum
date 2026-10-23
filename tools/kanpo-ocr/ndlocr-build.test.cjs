'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const {PIN,build,verifyBinary}=require('./ndlocr-build.cjs');
function harness(callback){
 const source=fs.mkdtempSync(path.join(os.tmpdir(),'ndlocr-build-'));
 try{
  fs.mkdirSync(path.join(source,'target','release'),{recursive:true});fs.writeFileSync(path.join(source,'Cargo.lock'),'locked');
  let head=PIN.commit,dirty='',calls=0;
  const binary=path.join(source,'target','release','ndlocr-lite-rs'+(process.platform==='win32'?'.exe':''));
  const execute=(command,args)=>{
   if(command==='git')return args.includes('rev-parse')?head:dirty;
   assert.equal(command,'cargo');assert.ok(args.includes('--locked'));assert.ok(args.includes('--release'));assert.ok(args.includes('onnx'));calls++;
   fs.writeFileSync(binary,'compiled bytes');return '';
  };
  callback({source,binary,execute,calls:()=>calls,setHead:v=>head=v,setDirty:v=>dirty=v});
 }finally{fs.rmSync(source,{recursive:true,force:true});}
}
test('clean pinned source must finish a locked ONNX release build before receipt exists',()=>harness(h=>{
 const result=build(h.source,undefined,h.execute);assert.equal(h.calls(),1);
 assert.equal(verifyBinary(h.binary).identity.commit,PIN.commit);
 assert.equal(result.receipt,fs.realpathSync(h.binary)+'.kanpo-build.json');
 assert.throws(()=>build(h.source,undefined,h.execute),/already exists/);assert.equal(h.calls(),1);
}));
test('wrong commit, dirty source and failed build cannot receive a pin receipt',()=>harness(h=>{
 h.setHead('0'.repeat(40));assert.throws(()=>build(h.source,undefined,h.execute),/clean checkout/);assert.equal(h.calls(),0);
 h.setHead(PIN.commit);h.setDirty(' M src/main.rs');assert.throws(()=>build(h.source,undefined,h.execute),/clean checkout/);assert.equal(h.calls(),0);
 h.setDirty('');assert.throws(()=>build(h.source,undefined,(cmd,args)=>{if(cmd==='cargo')throw new Error('build failure');return h.execute(cmd,args);}),/build failure/);
 assert.equal(fs.existsSync(h.binary+'.kanpo-build.json'),false);
}));
test('source changes during compilation invalidate the resulting build',()=>harness(h=>{
 assert.throws(()=>build(h.source,undefined,(cmd,args)=>{const result=h.execute(cmd,args);if(cmd==='cargo')h.setDirty(' M src/main.rs');return result;}),/clean checkout/);
 assert.equal(fs.existsSync(h.binary+'.kanpo-build.json'),false);
}));
