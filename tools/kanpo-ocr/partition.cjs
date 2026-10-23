'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {isDeepStrictEqual}=require('node:util');
const {projectBox}=require('./placement.cjs');
const {writeAtomicJson,pngSize}=require('./sidecar.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function asset(directory,name){
  if(typeof name!=='string'||path.basename(name)!==name||name==='.'||name==='..')throw new Error('Expected a local source asset filename');
  return fs.readFileSync(path.join(directory,name));
}
function geometryManifest(directory){
  const indexBytes=asset(directory,'index.json'),packetBytes=asset(directory,'sources.json'),pdf=asset(directory,'source.pdf');
  const index=JSON.parse(indexBytes),packet=JSON.parse(packetBytes),document=hash(pdf),rows=[];
  if(index.kind!=='intrinsic_pdf_image_sources'||index.document_sha256!==document||index.native_packet_sha256!==hash(packetBytes)||packet.schema!==1||packet.source!=='native_xobject_snapshot')throw new Error('Decoded PDF/source packet mismatch');
  for(const entry of index.rows){
    if(entry.status==='failed')continue;
    if(entry.status!=='decoded'||![entry.page,entry.draw].every(n=>Number.isSafeInteger(n)&&n>=0))throw new Error('Invalid decoded draw occurrence');
    const page=packet.pages[entry.page];
    if(!page||page.page!==entry.page||page.document_sha256!==document)throw new Error('Native page does not match PDF snapshot');
    if(!Array.isArray(page.native_paths))throw new Error('Partitioning requires native_paths from the current image_sources example');
    const frameBytes=asset(directory,entry.placement),receiptBytes=asset(directory,entry.receipt),png=asset(directory,entry.PNG),receipt=JSON.parse(receiptBytes),frame=JSON.parse(frameBytes),size=pngSize(png);
    if(hash(receiptBytes)!==entry.receipt_sha256||receipt.document_sha256!==document||receipt.native_packet_sha256!==hash(packetBytes)||receipt.placement_sha256!==hash(frameBytes)||receipt.PNG_sha256!==hash(png))throw new Error('Decoded draw hashes do not match source artifacts');
    if(size.width!==frame.pixel_width||size.height!==frame.pixel_height)throw new Error('Source PNG canvas does not match placement');
    const draw=page.images?.[entry.draw];
    if(!draw||!isDeepStrictEqual(receipt.source_draw,draw))throw new Error('Receipt does not match native image draw');
    if(!isDeepStrictEqual(frame.image_to_user,draw.image_to_user)||frame.image_width!==draw.pixel_width||frame.image_height!==draw.pixel_height||!isDeepStrictEqual(frame.crop_box,page.crop_box)||frame.rotate!==page.rotate)throw new Error('Source placement differs from native image draw/page');
    const full=projectBox({x0:0,y0:0,x1:size.width,y1:size.height},frame);
    rows.push({page:entry.page,draw:entry.draw,PNG_sha256:hash(png),placement_sha256:hash(frameBytes),receipt_sha256:hash(receiptBytes),projection_status:full.status,page_quad:full.quad_points});
  }
  return {schema:1,kind:'source_canvas_geometry',document_sha256:document,decoded_index_sha256:hash(indexBytes),native_packet_sha256:hash(packetBytes),projector_sha256:hash(fs.readFileSync(path.join(__dirname,'placement.cjs'))),producer_sha256:hash(fs.readFileSync(__filename)),rows};
}
async function partitionSources(options){
  const directory=path.join(options.output,'decoded'),geometry=path.join(options.output,'partition-geometry.json');
  writeAtomicJson(geometry,geometryManifest(directory));
  const helper=path.join(__dirname,'partition_sources.py');
  let stdout,stderr,result;
  try{
    stdout=fs.openSync(path.join(options.output,'partition.stdout'),'wx');stderr=fs.openSync(path.join(options.output,'partition.stderr'),'wx');
    result=spawnSync(options.python,[helper,'--input',directory,'--geometry',geometry,'--output',path.join(options.output,'partitioned')],{stdio:['ignore',stdout,stderr]});
  }finally{if(stdout!==undefined)fs.closeSync(stdout);if(stderr!==undefined)fs.closeSync(stderr);}
  writeAtomicJson(path.join(options.output,'partition-process.json'),{status:result.status,signal:result.signal,error:result.error&&String(result.error),helper_sha256:hash(fs.readFileSync(helper))});
  if(result.error)throw result.error;
  if(![0,2].includes(result.status))throw new Error('Source partitioning failed; see partition.stderr and partitioned/terminal.json');
  const index=JSON.parse(asset(path.join(options.output,'partitioned'),'index.json'));
  if(index.kind!=='partitioned_pdf_image_sources'||index.document_sha256!==JSON.parse(asset(directory,'index.json')).document_sha256||index.body_coverage_verified!==false)throw new Error('Unexpected source partition index');
  return index;
}
module.exports={geometryManifest,partitionSources};
