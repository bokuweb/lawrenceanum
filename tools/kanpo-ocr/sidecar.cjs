'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const MODEL_SHA256 = '1f5de9236d2e85f5fdf4b3c500f2d4926f8d9449f28f5394472d9e8d83b91b4d';
const ENGINE_VERSION = '6.0.1';
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const jsonNumber = value => typeof value==='number' && (!Number.isFinite(value)||Object.is(value,-0)) ? (Object.is(value,-0)?'-0':String(value)) : undefined;
function encodeJson(value,space=2) {
  return JSON.stringify(value,(_key,v)=>jsonNumber(v)===undefined?v:{$js_number:jsonNumber(v)},space)+'\n';
}
function writeAtomicJson(filename,value,io=fs) {
  const temporary=filename+'.'+crypto.randomUUID()+'.tmp';let fd;
  try {
    fd=io.openSync(temporary,'wx');io.writeFileSync(fd,encodeJson(value));io.fsyncSync(fd);io.closeSync(fd);fd=undefined;
    // Publish a complete file exclusively, on the same filesystem.
    io.linkSync(temporary,filename);io.unlinkSync(temporary);
  } catch(error) {
    if (fd!==undefined) {try {io.closeSync(fd);} catch {} }
    error.partial_output=temporary;throw error;
  }
}
function recordFailure(output,error,writer=writeAtomicJson) {
  const filename=path.join(output,'terminal.json');
  if (!fs.existsSync(filename)) {
    try {writer(filename,{status:'failed',error:String(error),stack:error?.stack,code:error?.code,partial_output:error?.partial_output,recognition_retry:false,canonical_integration:false});}
    catch (recordingError) {if (error && typeof error==='object') {try {error.recording_error=String(recordingError);} catch {}}}
  }
  return error;
}
function watchTerminal(output) {
  const fatal=error=>recordFailure(output,error);
  const exit=code=>recordFailure(output,Object.assign(new Error('Process exited before a terminal result'),{code}));
  process.once('uncaughtExceptionMonitor',fatal);process.once('exit',exit);
  return ()=>{process.removeListener('uncaughtExceptionMonitor',fatal);process.removeListener('exit',exit);};
}

function pngSize(bytes) {
  const signature = Buffer.from([137,80,78,71,13,10,26,10]);
  if (bytes.length < 33 || !bytes.subarray(0,8).equals(signature) || bytes.readUInt32BE(8) !== 13 || bytes.toString('ascii',12,16) !== 'IHDR') throw new Error('Expected a PNG with an IHDR canvas');
  const width=bytes.readUInt32BE(16),height=bytes.readUInt32BE(20);
  if (!width || !height || width>0x7fffffff || height>0x7fffffff) throw new Error('Invalid PNG canvas dimensions');
  return {width,height};
}
function validateFrame(frame) {
  for (const k of ['width_points','height_points','dpi']) if (!Number.isFinite(frame[k]) || frame[k]<=0) throw new Error(`Expected positive finite ${k}`);
  for (const k of ['pixel_width','pixel_height']) if (!Number.isSafeInteger(frame[k]) || frame[k]<=0) throw new Error(`Expected positive integer ${k}`);
  if (!Number.isSafeInteger(frame.dpi)) throw new Error('Expected integer dpi');
}
function mappedBox(box,frame) {
  const pixels=box == null ? null : structuredClone(box);
  const exceptionalNumbers={};
  for (const k of ['x0','y0','x1','y1']) if (box && jsonNumber(box[k])!==undefined) exceptionalNumbers[k]=jsonNumber(box[k]);
  const exceptional=Object.keys(exceptionalNumbers).length?{exceptional_numbers:exceptionalNumbers}:{};
  const valid=box && ['x0','y0','x1','y1'].every(k=>Number.isSafeInteger(box[k])) && box.x0>=0 && box.y0>=0 && box.x1>box.x0 && box.y1>box.y0 && box.x1<=frame.pixel_width && box.y1<=frame.pixel_height;
  if (!valid) return {pixels,points:null,geometry:'invalid',...exceptional};
  const project=(p,pixels,points)=>{
    if (p===0) return 0;
    if (p===pixels) return points;
    const pixelsPerPoint=pixels/points;
    return Number.isFinite(pixelsPerPoint) ? p/pixelsPerPoint : p/pixels*points;
  };
  const points=[project(box.x0,frame.pixel_width,frame.width_points),project(box.y0,frame.pixel_height,frame.height_points),project(box.x1,frame.pixel_width,frame.width_points),project(box.y1,frame.pixel_height,frame.height_points)];
  if (!points.every(Number.isFinite) || points[0]>=points[2] || points[1]>=points[3]) return {pixels,points:null,geometry:'invalid',reason:'point_precision_loss',...exceptional};
  return {pixels,points,geometry:'valid',...exceptional};
}
function sidecar(result,source,frame) {
  const data=result.data;
  if (!data || typeof data.text!=='string') throw new Error('Recognizer did not return text');
  if (data.blocks != null && !Array.isArray(data.blocks)) throw new Error('Recognizer returned unsupported block layout');
  const convert=(value,source_path,childKey,nextKey)=>{
    if (typeof value.text!=='string') throw new Error('Recognizer returned a layout node without text');
    const out={source_path,text:value.text,confidence:value.confidence,bbox:mappedBox(value.bbox,frame)};
    if (childKey) {
      if (!Array.isArray(value[childKey])) throw new Error(`Recognizer returned a node without ${childKey}`);
      out[childKey]=value[childKey].map((v,i)=>convert(v,[...source_path,i],nextKey,childKey==='paragraphs'?'words':null));
    }
    return out;
  };
  return {
    schema_version:1,kind:'unclassified_ocr',source:structuredClone(source),canvas:{...frame},
    text:data.text,engine_result:{version:data.version,psm:data.psm,oem:data.oem},
    layout_available:Array.isArray(data.blocks),
    blocks:(data.blocks||[]).map((b,i)=>convert(b,[i],'paragraphs','lines')),
    article_assignment:null,native_glyph_binding:null,
  };
}
async function recognizeBuffers(createWorker,image,model,dpi,onEvent) {
  let worker;
  const imageSnapshot=Buffer.from(image);
  const modelDirectory=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ocr-model-'));
  const cleanupModel=()=>{
    try { fs.rmSync(modelDirectory,{recursive:true,force:true}); }
    catch (error) { onEvent({status:'model_cleanup_error',path:modelDirectory,error:String(error)}); }
  };
  process.once('exit',cleanupModel);
  try {
    fs.writeFileSync(path.join(modelDirectory,'jpn.traineddata'),Buffer.from(model),{flag:'wx',mode:0o400});
    worker=await createWorker('jpn',1,{
      langPath:modelDirectory,
      gzip:false,cacheMethod:'none',legacyCore:false,legacyLang:false,
      logger:onEvent,errorHandler:error=>onEvent({status:'engine_error',error:String(error)}),
    });
    await worker.setParameters({tessedit_pageseg_mode:'3',user_defined_dpi:String(dpi)});
    return await worker.recognize(imageSnapshot,{rotateAuto:false,rotateRadians:0},{text:true,blocks:true});
  } finally {
    if (worker) {
      try { await worker.terminate(); }
      catch (error) { onEvent({status:'terminate_error',error:String(error)}); }
    }
    process.removeListener('exit',cleanupModel);
    cleanupModel();
  }
}
async function run(options) {
  const {image:modelImage,model:languageFile,output}=options;
  const image=fs.readFileSync(modelImage),model=fs.readFileSync(languageFile);
  const size=pngSize(image);
  const frame={pixel_width:size.width,pixel_height:size.height,width_points:options.width_points,height_points:options.height_points,dpi:options.dpi};
  validateFrame(frame);
  if (sha256(model)!==MODEL_SHA256) throw new Error('Unexpected jpn model SHA256; use the pinned tessdata_fast model');
  const pkg=require('tesseract.js/package.json');
  if (pkg.version!==ENGINE_VERSION) throw new Error(`Expected tesseract.js ${ENGINE_VERSION}, received ${pkg.version}`);
  const {createWorker}=require('tesseract.js');
  // A fresh output directory keeps previous observations and failures intact.
  fs.mkdirSync(output);
  const write=(name,data)=>fs.writeFileSync(path.join(output,name),data,{flag:'wx'});
  const json=(name,obj)=>writeAtomicJson(path.join(output,name),obj);
  const stopWatching=watchTerminal(output);
  try {
    const source={image:{path:path.resolve(modelImage),sha256:sha256(image),bytes:image.length},model:{language:'jpn',sha256:sha256(model),bytes:model.length},engine:{package:'tesseract.js',version:pkg.version},node:process.version,tool_sha256:sha256(fs.readFileSync(__filename)),package_lock_sha256:sha256(fs.readFileSync(path.join(__dirname,'package-lock.json')))};
    json('inputs.json',{source,canvas:frame,parameters:{psm:3,oem:1,rotateAuto:false,rotateRadians:0,cacheMethod:'none',model_input:'private_readonly_local_snapshot'},normalization:null,confidence_filter:null,exceptional_number_encoding:{$js_number:'NaN | Infinity | -Infinity | -0'}});
    let workerCleanupFailed=false;const cleanupErrors=[];
    const result=await recognizeBuffers(createWorker,image,model,frame.dpi,event=>{
      if (event.status==='terminate_error') workerCleanupFailed=true;
      if (event.status==='terminate_error'||event.status==='model_cleanup_error') cleanupErrors.push(event);
      fs.appendFileSync(path.join(output,'events.ndjson'),encodeJson(event,0));
    });
    json('raw.json',result);
    const observations=sidecar(result,source,frame);
    json('observations.json',observations);
    write('recognized.txt',result.data.text);
    json('terminal.json',{status:'complete',worker_cleanup_failed:workerCleanupFailed,cleanup_errors:cleanupErrors,recognition_runs:1,output_kind:'unclassified_ocr',canonical_integration:false});
    return observations;
  } catch (error) {
    throw recordFailure(output,error);
  } finally {stopWatching();}
}
module.exports={pngSize,validateFrame,mappedBox,sidecar,recognizeBuffers,run,encodeJson,writeAtomicJson,recordFailure,watchTerminal,MODEL_SHA256,ENGINE_VERSION};
