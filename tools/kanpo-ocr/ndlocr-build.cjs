#!/usr/bin/env node
'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {parseArgs}=require('node:util');
const PIN=Object.freeze({version:'0.0.15',commit:'f97e930b8c3e9512f7788f258ef685da26416483'});
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function checked(command,args,options={}){
  const result=spawnSync(command,args,{encoding:'utf8',...options});
  if(result.error)throw result.error;
  if(result.status!==0)throw new Error(`${command} failed (${result.status}): ${result.stderr??''}`);
  return result.stdout;
}
function build(source,receiptPath,execute=checked){
  source=fs.realpathSync(source);
  const binary=path.join(source,'target','release','ndlocr-lite-rs'+(process.platform==='win32'?'.exe':''));
  receiptPath=path.resolve(receiptPath??binary+'.kanpo-build.json');
  if(fs.existsSync(receiptPath))throw new Error('Build receipt already exists; choose a new --receipt file');
  const verify=()=>{
    const head=execute('git',['-C',source,'rev-parse','HEAD']).trim();
    const dirty=execute('git',['-C',source,'status','--porcelain','--untracked-files=all']);
    if(head!==PIN.commit||dirty.trim())throw new Error('NDLOCR build requires a clean checkout of pinned 0.0.15 commit '+PIN.commit);
  };
  verify();
  const args=['build','--manifest-path',path.join(source,'Cargo.toml'),'--target-dir',path.join(source,'target'),'--release','--features','onnx','--locked'];
  execute('cargo',args,{cwd:source,stdio:['ignore','inherit','inherit']});
  verify();
  const receipt={schema_version:1,package:'ndlocr-lite-rs',...PIN,features:['onnx'],profile:'release',binary_sha256:hash(fs.readFileSync(binary)),cargo_lock_sha256:hash(fs.readFileSync(path.join(source,'Cargo.lock'))),build_command:['cargo',...args],source_path:source,binary_path:binary};
  fs.writeFileSync(receiptPath,JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
  return {binary,receipt:receiptPath};
}
function verifyBinary(binary,receiptPath){
  receiptPath=receiptPath??process.env.NDLOCR_RECEIPT??binary+'.kanpo-build.json';
  let bytes,receipt;
  try{bytes=fs.readFileSync(receiptPath);receipt=JSON.parse(bytes);}catch(error){throw new Error('NDLOCR build receipt unavailable: '+error.message);}
  const binaryBytes=fs.readFileSync(binary);
  if(receipt.schema_version!==1||receipt.package!=='ndlocr-lite-rs'||receipt.version!==PIN.version||receipt.commit!==PIN.commit||receipt.profile!=='release'||!Array.isArray(receipt.features)||!receipt.features.includes('onnx')||receipt.binary_sha256!==hash(binaryBytes))throw new Error('NDLOCR build receipt does not match pinned 0.0.15 binary');
  return {binaryBytes,identity:{...PIN,build_receipt_sha256:hash(bytes),build_receipt_path:path.resolve(receiptPath),sha256:receipt.binary_sha256}};
}
if(require.main===module){try{
  const {values}=parseArgs({options:{source:{type:'string'},receipt:{type:'string'}}});
  if(!values.source)throw new Error('Usage: node tools/kanpo-ocr/ndlocr-build.cjs --source /clean/ndlocr-lite-rs [--receipt NEW_FILE]');
  process.stdout.write(JSON.stringify(build(values.source,values.receipt))+'\n');
}catch(error){process.stderr.write(String(error)+'\n');process.exitCode=1;}}
module.exports={PIN,build,verifyBinary};
