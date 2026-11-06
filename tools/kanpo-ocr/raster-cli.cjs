#!/usr/bin/env node
'use strict';
// Derived recognition images deliberately have a different hash from the
// decoder PNG. Keep that derivation explicit instead of aliasing either file.
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {parseArgs,isDeepStrictEqual}=require('node:util');
const {run}=require('./ndlocr.cjs');
const {projectLayout,validatePlacement}=require('./placement.cjs');
const {pngSize,writeAtomicJson,recordFailure,watchTerminal}=require('./sidecar.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const local=name=>{
  if(typeof name!=='string'||path.basename(name)!==name||['','.','..'].includes(name))throw new Error('Expected a local prepared raster member');
  return name;
};
const box=(v,w,h)=>Array.isArray(v)&&v.length===4&&v.every(Number.isSafeInteger)&&0<=v[0]&&v[0]<v[2]&&v[2]<=w&&0<=v[1]&&v[1]<v[3]&&v[3]<=h;
const contains=(a,b)=>a[0]<=b[0]&&a[1]<=b[1]&&b[2]<=a[2]&&b[3]<=a[3];
const overlap=(a,b)=>Math.max(a[0],b[0])<Math.min(a[2],b[2])&&Math.max(a[1],b[1])<Math.min(a[3],b[3]);
const area=b=>(b[2]-b[0])*(b[3]-b[1]);
function validateCover(boxes,parent){
  if(boxes.some(b=>!contains(parent,b))||boxes.reduce((n,b)=>n+area(b),0)!==area(parent))throw new Error('Incomplete prepared source pixel partition');
  for(let i=0;i<boxes.length;i++)for(let j=0;j<i;j++)if(overlap(boxes[i],boxes[j]))throw new Error('Overlapping prepared source pixel partition');
}
function copyInkSupport(sourceBox,core,rows){
  let pixels=0,owned=0,minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  for(let y=sourceBox[1];y<sourceBox[3];y++)for(const [a,b]of rows.get(y)??[]){
    const x0=Math.max(a,sourceBox[0]),x1=Math.min(b,sourceBox[2]);
    if(x0>=x1)continue;
    pixels+=x1-x0;minX=Math.min(minX,x0);minY=Math.min(minY,y);maxX=Math.max(maxX,x1);maxY=Math.max(maxY,y+1);
    if(core[1]<=y&&y<core[3])owned+=Math.max(0,Math.min(x1,core[2])-Math.max(x0,core[0]));
  }
  const bounds=pixels?[minX,minY,maxX,maxY]:null,cx=(minX+maxX)/2,cy=(minY+maxY)/2;
  return {policy:'bbox_of_actual_copy_ink_inside_recognizer_region',pixels,owned_core_pixels:owned,
    bounds_source_pixels:bounds,bbox_center_core:pixels?(core[0]<=cx&&cx<core[2]&&core[1]<=cy&&cy<core[3]?'center_inside_core':'halo_observation'):'no_copy_ink_unverified'};
}
function cardinalCrop(roi,turns){
  const [x,y,right,bottom]=roi,w=right-x,h=bottom-y;
  const matrix=[[1,0,0,1,0,0],[0,1,-1,0,w,0],[-1,0,0,-1,w,h],[0,-1,1,0,0,h]][turns].slice();
  matrix[4]+=x;matrix[5]+=y;
  return {matrix,width:turns%2?h:w,height:turns%2?w:h};
}
function sourceBox(tile,pixels){
  const [a,b,c,d,e,f]=tile.recognition_to_source??[1,0,0,1,tile.recognition_roi[0],tile.recognition_roi[1]];
  const points=[[pixels.x0,pixels.y0],[pixels.x1,pixels.y0],[pixels.x1,pixels.y1],[pixels.x0,pixels.y1]].map(([x,y])=>[a*x+c*y+e,b*x+d*y+f]);
  return [Math.min(...points.map(p=>p[0])),Math.min(...points.map(p=>p[1])),Math.max(...points.map(p=>p[0])),Math.max(...points.map(p=>p[1]))];
}
function loadPreparation(directory){
  const files=new Map();
  const read=name=>{local(name);if(!files.has(name))files.set(name,fs.readFileSync(path.join(directory,name)));return files.get(name);};
  const raw=read('preparation.json'),prep=JSON.parse(raw);
  if(prep.schema!==1||prep.kind!=='source_derived_ocr_preparation'||prep.coverage_verified!==false||prep.pixel_partition_verified!==true||prep.article_assignment!==null||prep.native_glyph_binding!==null)throw new Error('Expected unclassified source-derived preparation');
  const binding=JSON.parse(read('source-binding.json')),receipt=JSON.parse(read('source-receipt.json')),frame=JSON.parse(read('source-placement.json'));
  const source=prep.source;
  if(binding.kind!=='unclassified_pdf_raster_ruling_faces'||binding.coverage_verified!==false||binding.article_assignment!==null||binding.native_glyph_binding!==null)throw new Error('Expected unclassified original source binding');
  for(const [key,value]of Object.entries(binding.source))if(!isDeepStrictEqual(value,source[key]))throw new Error('Preparation changed the original source identity');
  const checks=[['source.pdf',source.document_sha256],['source.png',source.source_PNG_sha256],['source-binding.json',source.binding_sha256],['source-observation.json',source.observation_sha256],['source-receipt.json',source.source_receipt_sha256],['source-placement.json',source.source_placement_sha256],['recognition-parent.png',prep.recognition_parent_sha256],['erasure-mask.png',prep.erasure_mask_sha256]];
  for(const [name,digest]of checks)if(hash(read(name))!==digest)throw new Error('Prepared raster input hash mismatch: '+name);
  const code=prep.producer?.code_sha256;
  const names=code&&Object.keys(code).sort(),legacyNames=['prepare_raster.py','raster_grid.py','source_gutters.py'],patternNames=['prepare_raster.py','raster_grid.py','raster_ruling_patterns.py','source_gutters.py'];
  const partitionMode=prep.settings?.partition_geometry??'solid_ruling_faces';
  if(!['solid_ruling_faces','periodic_ruling_candidates'].includes(partitionMode))throw new Error('Unsupported source partition geometry');
  const completeBands=prep.settings?.complete_ruling_bands??false;
  if(typeof completeBands!=='boolean'||(completeBands&&partitionMode!=='periodic_ruling_candidates'))throw new Error('Invalid source ruling band completion declaration');
  const expectedNames=[...patternNames,...(partitionMode==='periodic_ruling_candidates'?['raster_periodic_grid.py']:[]),...(completeBands?['raster_band_completion.py']:[]),...(prep.orientation?['raster_orientation.py']:[])].sort();
  if(!code||(!isDeepStrictEqual(names,expectedNames)&&!(!prep.orientation&&partitionMode==='solid_ruling_faces'&&prep.profile!=='median3-ruling-patterns'&&isDeepStrictEqual(names,legacyNames))))throw new Error('Missing prepared implementation snapshot');
  for(const name of Object.keys(code)){
    const member='implementation/'+local(name),bytes=fs.readFileSync(path.join(directory,member));
    if(hash(bytes)!==code[name])throw new Error('Prepared implementation hash mismatch');
    files.set(member,bytes);
  }
  if(binding.observation_sha256!==source.observation_sha256||receipt.document_sha256!==source.document_sha256||receipt.PNG_sha256!==source.source_PNG_sha256||receipt.RGBA_pixels_sha256!==source.source_RGBA_sha256||receipt.placement_sha256!==source.source_placement_sha256||receipt.source_draw?.encoded_sha256!==source.encoded_sha256||receipt.page!==source.page||receipt.draw!==source.draw)throw new Error('Prepared source receipt mismatch');
  validatePlacement(frame);
  const size=pngSize(read('source.png')),parentSize=pngSize(read('recognition-parent.png'));
  const w=size.width,h=size.height;
  if(prep.canvas.pixel_width!==w||prep.canvas.pixel_height!==h||frame.pixel_width!==w||frame.pixel_height!==h||parentSize.width!==w||parentSize.height!==h)throw new Error('Prepared source canvas mismatch');
  const turns=prep.settings?.recognition_quarter_turns??0;
  if(!Number.isSafeInteger(turns)||turns<0||turns>3||(!prep.orientation&&turns!==0))throw new Error('Invalid recognition quarter-turn declaration');
  let orientation;
  if(prep.orientation){
    const bytes=read(prep.orientation.file);
    if(hash(bytes)!==prep.orientation.sha256)throw new Error('Source orientation hash mismatch');
    orientation=JSON.parse(bytes);
    if(orientation.schema!==1||orientation.kind!=='source_derived_raster_orientation'||!isDeepStrictEqual(orientation.source,source)||orientation.recognition_parent_sha256!==prep.recognition_parent_sha256||orientation.observation?.kind!=='unclassified_raster_glyph_orientation'||orientation.observation.coverage_verified!==false||!isDeepStrictEqual(orientation.observation.canvas,prep.canvas)||orientation.observation.quarter_turns_ccw!==turns||orientation.vision?.page?.revision!==3||orientation.vision.page.language_correction!==false)throw new Error('Orientation differs from its source or recognition copy');
    if(orientation.observation.status==='unanimous_observed_glyph_right_axes'){
      if(orientation.observation.glyph_right_axis!==turns)throw new Error('Orientation axes differ from the chosen quarter turn');
    }else if(turns!==0||orientation.observation.glyph_right_axis!==null)throw new Error('Unresolved orientation cannot choose a quarter turn');
  }
  const footprint=receipt.pixel_footprint_points;
  if(!Array.isArray(footprint)||footprint.length!==2||!footprint.every(v=>Number.isFinite(v)&&v>0)||!Number.isSafeInteger(receipt.model_dpi)||receipt.model_dpi<=0)throw new Error('Invalid source recognition sampling');
  if(!['identity','median3-ruling-bands','median3-ruling-patterns'].includes(prep.profile))throw new Error('Unsupported prepared recognition profile');
  const expectedOperations=prep.profile==='identity'?[]:[{kind:'pillow_channelwise_median',size:3},{kind:'erase_original_ruling_band_and_span',margin:1}];
  if(prep.profile==='median3-ruling-patterns'){
    expectedOperations.push({kind:'erase_isolated_one_pixel_components',connectivity:8},{kind:'erase_bounded_periodic_ruling_components',connectivity:8,source:'original_opaque_black_runs_between_perpendicular_rulings'});
    if(!prep.ruling_patterns)throw new Error('Missing original periodic ruling observations');
    const bytes=read(prep.ruling_patterns.file);
    if(hash(bytes)!==prep.ruling_patterns.sha256)throw new Error('Periodic ruling observations hash mismatch');
    const details=JSON.parse(bytes);
    if(details.schema!==1||details.kind!=='source_derived_ruling_pattern_erasure'||details.original?.source_RGBA_sha256!==source.source_RGBA_sha256)throw new Error('Periodic ruling observations differ from the original source');
  }
  if(!isDeepStrictEqual(prep.operations,expectedOperations))throw new Error('Unrecognized prepared image operations');
  if(!Array.isArray(prep.parts)||!Array.isArray(prep.tiles))throw new Error('Missing prepared pixel partition');
  const cropMode=prep.settings?.recognition_crop??'nonwhite_bounds';
  if(!['nonwhite_bounds','source_window'].includes(cropMode))throw new Error('Unsupported prepared crop policy');
  const sourceObservation=JSON.parse(read('source-observation.json'));
  let candidateObservation=sourceObservation;
  if(partitionMode==='periodic_ruling_candidates'){
    const declaration=prep.partition_observation;
    if(!declaration)throw new Error('Missing periodic partition observation');
    const bytes=read(declaration.file);
    if(hash(bytes)!==declaration.sha256)throw new Error('Periodic partition observation hash mismatch');
    candidateObservation=JSON.parse(bytes);
    if(candidateObservation.schema!==1||candidateObservation.kind!=='unclassified_periodic_ruling_faces'||candidateObservation.coverage_verified!==false||candidateObservation.article_assignment!==null||candidateObservation.native_glyph_binding!==null||candidateObservation.source_observation_sha256!==source.observation_sha256||candidateObservation.source_RGBA_sha256!==source.source_RGBA_sha256||candidateObservation.periodic_observations?.source_RGBA_sha256!==source.source_RGBA_sha256||candidateObservation.canvas?.pixel_width!==w||candidateObservation.canvas?.pixel_height!==h)throw new Error('Periodic partition differs from the original source observations');
    if((candidateObservation.settings?.source_band_completion??false)!==completeBands)throw new Error('Source ruling band completion differs from its partition declaration');
    const boundaryKinds=['solid_source','includes_inferred_periodic_spans',...(completeBands?['includes_source_band_completion']:[])];
    if(!Array.isArray(candidateObservation.rectangular_cells)||candidateObservation.rectangular_cells.some(c=>!boundaryKinds.includes(c.boundary_kind)))throw new Error('Missing periodic cell boundary evidence');
  }else if(prep.partition_observation)throw new Error('Undeclared periodic partition observation');
  const candidates=candidateObservation.rectangular_cells;
  if(!Array.isArray(candidates)||new Set(candidates.map(c=>c.id)).size!==candidates.length||candidates.some(c=>!Number.isSafeInteger(c.id)||c.id<0||!box(c.interior_pixels,w,h)))throw new Error('Invalid original source candidate inventory');
  for(const [i,part]of prep.parts.entries()){
    if(part.id!==i||!box(part.box,w,h)||!['ruling_face_candidate','outside_ruling_faces','overlapping_ruling_faces'].includes(part.role))throw new Error('Invalid prepared source part');
    const covering=candidates.filter(c=>overlap(c.interior_pixels,part.box));
    if(covering.some(c=>!contains(c.interior_pixels,part.box)))throw new Error('Prepared part straddles an original candidate boundary');
    const owners=covering.filter(c=>!covering.some(other=>!isDeepStrictEqual(c.interior_pixels,other.interior_pixels)&&contains(c.interior_pixels,other.interior_pixels))).map(c=>c.id).sort((a,b)=>a-b);
    if(owners.length===0){if(part.role!=='outside_ruling_faces'||Object.hasOwn(part,'cell')||Object.hasOwn(part,'cells'))throw new Error('Prepared part changed original candidate ownership');}
    else if(owners.length===1){if(part.role!=='ruling_face_candidate'||part.cell!==owners[0]||Object.hasOwn(part,'cells'))throw new Error('Prepared part changed original candidate ownership');}
    else if(part.role!=='overlapping_ruling_faces'||Object.hasOwn(part,'cell')||!isDeepStrictEqual(part.cells,owners))throw new Error('Prepared part discarded overlapping original candidates');
  }
  validateCover(prep.parts.map(p=>p.box),[0,0,w,h]);
  const perPart=prep.parts.map(()=>[]);
  for(const [i,tile]of prep.tiles.entries()){
    if(tile.id!==i||!Number.isSafeInteger(tile.part)||!prep.parts[tile.part]||!box(tile.core,w,h)||!box(tile.window,w,h)||!contains(tile.window,tile.core)||!contains(prep.parts[tile.part].box,tile.window)||tile.role!==prep.parts[tile.part].role||tile.cell!==prep.parts[tile.part].cell||!isDeepStrictEqual(tile.cells,prep.parts[tile.part].cells))throw new Error('Invalid prepared source tile');
    perPart[tile.part].push(tile.core);
    if(tile.status==='recognition_copy_empty_unverified'){
      if(['PNG','recognition_roi','placement','text'].some(k=>Object.hasOwn(tile,k)))throw new Error('Transformed empty tile cannot claim recognized pixels or text');
      continue;
    }
    if(tile.status!=='prepared_unclassified'||!box(tile.recognition_roi,w,h)||!contains(tile.window,tile.recognition_roi))throw new Error('Invalid prepared recognition region');
    if(cropMode==='source_window'&&!isDeepStrictEqual(tile.recognition_roi,tile.window))throw new Error('Recognition dimensions changed the fixed original source window');
    const png=read(tile.PNG),placementRaw=read(tile.placement);
    if(hash(png)!==tile.PNG_sha256||hash(placementRaw)!==tile.placement_sha256)throw new Error('Prepared recognition image/placement hash mismatch');
    const actual=JSON.parse(placementRaw),canvas=pngSize(png);
    const transform=cardinalCrop(tile.recognition_roi,turns),[a,b,c,d,e,f]=frame.recognition_to_image,[aa,bb,cc,dd,ee,ff]=transform.matrix;
    if((prep.orientation||tile.recognition_to_source)&&!isDeepStrictEqual(tile.recognition_to_source,transform.matrix))throw new Error('Recognition normalization differs from its source crop');
    const expected={...frame,pixel_width:transform.width,pixel_height:transform.height,recognition_to_image:[a*aa+c*bb,b*aa+d*bb,a*cc+c*dd,b*cc+d*dd,a*ee+c*ff+e,b*ee+d*ff+f]};
    validatePlacement(actual);
    const comparable=value=>({...value,recognition_to_image:value.recognition_to_image.map(v=>v===0?0:v)});
    if(canvas.width!==transform.width||canvas.height!==transform.height||!isDeepStrictEqual(comparable(actual),comparable(expected)))throw new Error('Prepared crop placement differs from original source matrix');
  }
  for(const [i,part]of prep.parts.entries())validateCover(perPart[i],part.box);
  let inkRows;
  if(prep.recognition_ink){
    if(!code['raster_ruling_patterns.py'])throw new Error('Missing copy ink implementation snapshot');
    const bytes=read(prep.recognition_ink.file);
    if(hash(bytes)!==prep.recognition_ink.sha256)throw new Error('Recognition copy ink hash mismatch');
    const ink=JSON.parse(bytes);
    if(ink.schema!==1||ink.kind!=='unclassified_recognition_copy_ink_support'||ink.coverage_verified!==false||ink.connectivity!==8||ink.source_RGBA_sha256!==source.source_RGBA_sha256||ink.recognition_parent_sha256!==prep.recognition_parent_sha256||!isDeepStrictEqual(ink.canvas,prep.canvas)||!Array.isArray(ink.components))throw new Error('Recognition copy ink differs from its source');
    if(orientation&&orientation.observation.recognition_RGBA_sha256!==ink.recognition_RGBA_sha256)throw new Error('Orientation pixels differ from the recorded recognition copy ink');
    inkRows=new Map();
    for(const [i,component]of ink.components.entries()){
      if(component.id!==i||!box(component.bounds,w,h)||!Array.isArray(component.support)||!component.support.length)throw new Error('Invalid copy ink component');
      let area=0,x0=w,y0=h,x1=0,y1=0,previous;
      for(const span of component.support){
        if(!Array.isArray(span)||span.length!==3||!span.every(Number.isSafeInteger))throw new Error('Invalid copy ink support span');
        const [y,a,b]=span;
        if(!(0<=y&&y<h&&0<=a&&a<b&&b<=w)||previous&&(y<previous[0]||y===previous[0]&&a<=previous[2]))throw new Error('Unordered or overlapping copy ink support');
        previous=span;area+=b-a;x0=Math.min(x0,a);y0=Math.min(y0,y);x1=Math.max(x1,b);y1=Math.max(y1,y+1);
        if(!inkRows.has(y))inkRows.set(y,[]);inkRows.get(y).push([a,b]);
      }
      if(area!==component.area||!isDeepStrictEqual([x0,y0,x1,y1],component.bounds))throw new Error('Copy ink component bounds or area differs from its support');
    }
    for(const spans of inkRows.values()){
      spans.sort((a,b)=>a[0]-b[0]);
      for(let i=1;i<spans.length;i++)if(spans[i][0]<=spans[i-1][1])throw new Error('Copy ink components share or split the same row run');
    }
  }
  return {prep,raw,files,frame,receipt,inkRows};
}
function bindDerived(observations,tile,loaded){
  const bytes=loaded.files.get(tile.PNG);
  if(observations.source?.image?.sha256!==hash(bytes)||observations.source.image.bytes!==bytes.length)throw new Error('Actual OCR PNG differs from the declared derived source');
  if(Object.hasOwn(observations,'source_derivation'))throw new Error('Existing source derivation must not be overwritten');
  if(observations.layout_available!==true||!Array.isArray(observations.recognition_regions))throw new Error('Requested OCR recognition regions were not returned');
  const frame=JSON.parse(loaded.files.get(tile.placement));
  const out=projectLayout(observations,frame);
  out.source_derivation={status:'producer_hashes_checked_visibility_unverified',preparation_sha256:hash(loaded.raw),original_document_sha256:loaded.prep.source.document_sha256,original_PNG_sha256:loaded.prep.source.source_PNG_sha256,profile:loaded.prep.profile,tile:tile.id,part:tile.part,role:tile.role,...(tile.cell===undefined?{}:{cell:tile.cell}),...(tile.cells===undefined?{}:{cells:tile.cells}),core:tile.core,window:tile.window,recognition_roi:tile.recognition_roi,...(loaded.prep.partition_observation?{partition_geometry:'periodic_ruling_candidates',partition_observation_sha256:loaded.prep.partition_observation.sha256}:{}),...(loaded.prep.orientation?{orientation_sha256:loaded.prep.orientation.sha256,recognition_quarter_turns:loaded.prep.settings.recognition_quarter_turns,recognition_to_source:tile.recognition_to_source}:{})};
  const ownership=node=>{
    if(Object.hasOwn(node,'source_core'))throw new Error('Existing source core metadata must not be overwritten');
    const b=node.bbox?.pixels;
    if(!b||!box([b.x0,b.y0,b.x1,b.y1],frame.pixel_width,frame.pixel_height)){node.source_core={status:'invalid_recognition_box'};return;}
    const global=sourceBox(tile,b),cx=(global[0]+global[2])/2,cy=(global[1]+global[3])/2,c=tile.core;
    node.source_core={status:c[0]<=cx&&cx<c[2]&&c[1]<=cy&&cy<c[3]?'center_inside_core':'halo_observation',source_pixels:global,crosses_core_boundary:!contains(c,global),policy:'bbox_center_in_half_open_source_core'};
    if(loaded.inkRows){
      if(Object.hasOwn(node,'source_ink'))throw new Error('Existing recognition copy ink evidence must not be overwritten');
      node.source_ink=copyInkSupport(global,c,loaded.inkRows);
    }
  };
  for(const region of out.recognition_regions??[])ownership(region);
  return out;
}
async function runPrepared(options,runtime={recognize:run}){
  fs.mkdirSync(options.output);
  const stop=watchTerminal(options.output);
  try{
    const loaded=loadPreparation(options.prepared),snapshot=path.join(options.output,'prepared');
    fs.mkdirSync(snapshot);
    fs.mkdirSync(path.join(snapshot,'implementation'));
    for(const [name,bytes]of loaded.files)fs.writeFileSync(path.join(snapshot,name),bytes,{flag:'wx',mode:0o400});
    writeAtomicJson(path.join(options.output,'run-inputs.json'),{preparation_sha256:hash(loaded.raw),tool_sha256:hash(fs.readFileSync(__filename)),projector_sha256:hash(fs.readFileSync(path.join(__dirname,'placement.cjs'))),engine:'ndlocr',regions_requested:true,canonical_integration:false});
    const rows=[];
    let requests=0;
    for(const tile of loaded.prep.tiles){
      const row={tile:tile.id,part:tile.part,role:tile.role,...(tile.cell===undefined?{}:{cell:tile.cell}),...(tile.cells===undefined?{}:{cells:tile.cells}),status:tile.status};
      const prefix=`tile-${String(tile.id).padStart(5,'0')}`;
      if(tile.status==='recognition_copy_empty_unverified'){
        rows.push(row);writeAtomicJson(path.join(options.output,prefix+'-result.json'),row);continue;
      }
      try{
        requests++;
        const [x,y,right,bottom]=tile.recognition_roi;
        const physical=[(right-x)*loaded.receipt.pixel_footprint_points[0]/loaded.frame.pixel_width,(bottom-y)*loaded.receipt.pixel_footprint_points[1]/loaded.frame.pixel_height];
        if((loaded.prep.settings?.recognition_quarter_turns??0)%2)physical.reverse();
        const observations=await runtime.recognize({...options,'ndlocr-regions':true,image:path.join(snapshot,tile.PNG),output:path.join(options.output,prefix+'-ocr'),width_points:physical[0],height_points:physical[1],dpi:loaded.receipt.model_dpi,layout:'page'});
        const projected=bindDerived(observations,tile,loaded),name=prefix+'-projected.json';
        writeAtomicJson(path.join(options.output,name),projected);
        row.status='recognized_unclassified';row.projection=name;row.observations=prefix+'-ocr/observations.json';
      }catch(error){row.status='failed';row.error=String(error);row.recognition_retry=false;}
      rows.push(row);
      writeAtomicJson(path.join(options.output,prefix+'-result.json'),row);
    }
    const partial=rows.some(r=>r.status==='failed');
    const index={schema:1,kind:'unclassified_source_derived_image_ocr',source:loaded.prep.source,preparation_sha256:hash(loaded.raw),rows,coverage_verified:false,visibility:'unverified',article_assignment:null,native_glyph_binding:null};
    writeAtomicJson(path.join(options.output,'index.json'),index);
    writeAtomicJson(path.join(options.output,'terminal.json'),{status:partial?'partial':'complete',recognition_requests:requests,recognition_retry:false,canonical_integration:false});
    return index;
  }catch(error){throw recordFailure(options.output,error);}
  finally{stop();}
}
if(require.main===module){
  (async()=>{
    const {values}=parseArgs({options:{prepared:{type:'string'},output:{type:'string'},model:{type:'string'},'ndlocr-bin':{type:'string'},'ndlocr-receipt':{type:'string'}}});
    if(!values.prepared||!values.output||!values.model)throw new Error('Usage: node tools/kanpo-ocr/raster-cli.cjs --prepared DIR --model MODEL_DIR --ndlocr-bin BINARY --ndlocr-receipt RECEIPT --output NEW_DIR');
    const result=await runPrepared(values);
    process.stdout.write(values.output+'\n',()=>process.exit(result.rows.some(r=>r.status==='failed')?2:0));
  })().catch(error=>process.stderr.write(String(error)+'\n',()=>process.exit(1)));
}
module.exports={loadPreparation,bindDerived,runPrepared,copyInkSupport,cardinalCrop,sourceBox};
