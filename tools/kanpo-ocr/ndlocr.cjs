'use strict';
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {pngSize,validateFrame,writeAtomicJson,recordFailure,watchTerminal}=require('./sidecar.cjs');
const {verifyBinary}=require('./ndlocr-build.cjs');
const MODEL_FILES={
  '--det-model':'deim-s-1024x1024.onnx',
  '--model':'parseq-ndl-24x768-100-tiny-153epoch-tegaki3-r8data-202604.onnx',
  '--model30':'parseq-ndl-24x256-30-tiny-189epoch-tegaki3-r8data-202604.onnx',
  '--model50':'parseq-ndl-24x384-50-tiny-300epoch-tegaki3-r8data-202604.onnx',
  '--charset':'NDLmoji.yaml',
};
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function executable(name) {
  const candidates=name.includes(path.sep)?[path.resolve(name)]:(process.env.PATH??'').split(path.delimiter).map(p=>path.resolve(p,name));
  for(const candidate of candidates){try{fs.accessSync(candidate,fs.constants.X_OK);if(fs.statSync(candidate).isFile())return fs.realpathSync(candidate);}catch{}}
  throw new Error('NDLOCR executable not found; set --ndlocr-bin or NDLOCR_BIN to a release + onnx binary');
}
async function run(options) {
  const threshold=(name,fallback)=>{const value=Number(options[name]??fallback);if(!Number.isFinite(value)||value<0||value>1)throw new Error(`Invalid ${name} threshold; expected 0..1`);return value;};
  const detection=threshold('det-conf-threshold',0.25),confidence=threshold('min-line-confidence',0.3);
  const binary=executable(options['ndlocr-bin']??process.env.NDLOCR_BIN??'ndlocr-lite-rs');
  const verified=verifyBinary(binary,options['ndlocr-receipt']);
  const image=fs.readFileSync(options.image),size=pngSize(image);
  const frame={pixel_width:size.width,pixel_height:size.height,width_points:options.width_points,height_points:options.height_points,dpi:options.dpi};
  validateFrame(frame);
  if(!fs.statSync(options.model).isDirectory())throw new Error('NDLOCR --model must be a models directory; use --engine tesseract for traineddata');
  fs.mkdirSync(options.output);
  const stop=watchTerminal(options.output),snapshot=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-ndlocr-'));
  const json=(name,value)=>writeAtomicJson(path.join(options.output,name),value);
  try {
    const runtimeBinary=path.join(snapshot,path.basename(binary));
    fs.writeFileSync(runtimeBinary,verified.binaryBytes,{flag:'wx',mode:0o500});
    const input=path.join(snapshot,'input.png');fs.writeFileSync(input,image,{flag:'wx',mode:0o400});
    const models=[],args=['recognize-page','--image',input,'--structure-rules=false','--det-conf-threshold',String(detection),'--min-line-confidence',String(confidence)];
    for(const [flag,file]of Object.entries(MODEL_FILES)){
      const original=path.resolve(options.model,file),bytes=fs.readFileSync(original),copy=path.join(snapshot,file);
      fs.writeFileSync(copy,bytes,{flag:'wx',mode:0o400});args.push(flag,copy);models.push({path:original,sha256:hash(bytes),bytes:bytes.length});
    }
    const source={image:{path:path.resolve(options.image),sha256:hash(image),bytes:image.length},models,engine:{package:'ndlocr-lite-rs',binary,...verified.identity},tool_sha256:hash(fs.readFileSync(__filename))};
    json('inputs.json',{source,canvas:frame,args,normalization:null,confidence_filter:confidence,detection_confidence_threshold:detection,model_input:'private_readonly_local_snapshot',layout_requested:options.layout??'page'});
    const stdout=fs.openSync(path.join(options.output,'stdout.txt'),'wx'),stderr=fs.openSync(path.join(options.output,'stderr.txt'),'wx');
    let result;
    const start=performance.now();
    try {result=spawnSync(runtimeBinary,[...args,'--output-txt',path.resolve(options.output,'recognized.txt')],{stdio:['ignore',stdout,stderr]});}
    finally {fs.closeSync(stdout);fs.closeSync(stderr);}
    json('raw.json',{status:result.status,signal:result.signal,error:result.error&&String(result.error),process_seconds:(performance.now()-start)/1000});
    if(result.error)throw result.error;
    if(result.status!==0)throw new Error(`NDLOCR failed (${result.status??result.signal}); see stderr.txt`);
    const text=fs.readFileSync(path.join(options.output,'recognized.txt'),'utf8');
    const observations={schema_version:1,kind:'unclassified_ocr',source,canvas:frame,text,engine_result:{output:'recognize-page TXT'},layout_available:false,blocks:[],article_assignment:null,native_glyph_binding:null};
    json('observations.json',observations);
    json('terminal.json',{status:'complete',recognition_runs:1,output_kind:'unclassified_ocr',layout_available:false,canonical_integration:false});
    return observations;
  }catch(error){throw recordFailure(options.output,error);}
  finally{try{fs.rmSync(snapshot,{recursive:true,force:true});}finally{stop();}}
}
module.exports={run,MODEL_FILES};
