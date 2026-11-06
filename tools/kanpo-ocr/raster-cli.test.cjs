'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const crypto=require('node:crypto');
const {loadPreparation,runPrepared,copyInkSupport}=require('./raster-cli.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFUlEQVR4nGNkYGD4z8DAwMAEIkAYAA4oAQOSwUJsAAAAAElFTkSuQmCC','base64');
const pixelPng=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==','base64');
function harness(){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-derived-raster-')),prepared=path.join(root,'prepared');
  fs.mkdirSync(prepared);fs.mkdirSync(path.join(prepared,'implementation'));
  const write=(name,value)=>{const bytes=Buffer.isBuffer(value)?value:Buffer.from(JSON.stringify(value)+'\n');fs.writeFileSync(path.join(prepared,name),bytes);return hash(bytes);};
  const frame={pixel_width:2,pixel_height:2,image_width:2,image_height:2,recognition_to_image:[0,1,-1,0,2,0],image_to_user:[10,0,0,20,5,7],crop_box:[0,0,100,200],rotate:0};
  const source={asset:7,page:2,draw:3,document_sha256:write('source.pdf',Buffer.from('original PDF fixture')),
    encoded_sha256:hash(Buffer.from('original encoded draw')),source_PNG_sha256:write('source.png',png),source_RGBA_sha256:hash(Buffer.from([0,0,0,255,0,0,0,255,0,0,0,255,0,0,0,255])),
    source_placement:'source-placement.json',source_placement_sha256:write('source-placement.json',frame),source_receipt:'source-receipt.json'};
  const receipt={document_sha256:source.document_sha256,PNG_sha256:source.source_PNG_sha256,RGBA_pixels_sha256:source.source_RGBA_sha256,placement_sha256:source.source_placement_sha256,page:2,draw:3,source_draw:{encoded_sha256:source.encoded_sha256},pixel_footprint_points:[10,20],model_dpi:72};
  source.source_receipt_sha256=write('source-receipt.json',receipt);
  const observation=write('source-observation.json',{kind:'unit_observation_fixture',rectangular_cells:[]});
  const binding={schema:1,kind:'unclassified_pdf_raster_ruling_faces',source:{...source},observation_sha256:observation,coverage_verified:false,article_assignment:null,native_glyph_binding:null};
  source.binding_sha256=write('source-binding.json',binding);source.observation_sha256=observation;
  const tiles=[[0,0,1,1],[1,0,2,1],[0,1,1,2],[1,1,2,2]].map((core,i)=>{
    const name=`tile-${String(i).padStart(5,'0')}`;
    return {id:i,part:0,role:'outside_ruling_faces',core,window:[0,0,2,2],status:'prepared_unclassified',recognition_roi:[0,0,2,2],PNG:name+'.png',PNG_sha256:write(name+'.png',png),placement:name+'.placement.json',placement_sha256:write(name+'.placement.json',frame)};
  });
  const code={};for(const name of ['prepare_raster.py','raster_grid.py','source_gutters.py'])code[name]=write('implementation/'+name,fs.readFileSync(path.join(__dirname,name)));
  const prep={schema:1,kind:'source_derived_ocr_preparation',source,canvas:{pixel_width:2,pixel_height:2},profile:'identity',operations:[],recognition_parent_sha256:write('recognition-parent.png',png),erasure_mask_sha256:write('erasure-mask.png',png),producer:{code_sha256:code},parts:[{id:0,role:'outside_ruling_faces',box:[0,0,2,2]}],tiles,pixel_partition_verified:true,coverage_verified:false,visibility:'unverified',article_assignment:null,native_glyph_binding:null};
  const save=()=>write('preparation.json',prep);save();
  let calls=0;
  const options={prepared,output:path.join(root,'out'),model:path.join(root,'models'),'ndlocr-bin':'test-binary','ndlocr-receipt':'test-receipt'};
  const runtime={recognize:async o=>{
    calls++;assert.equal(o['ndlocr-regions'],true);assert.equal(o.layout,'page');
    fs.mkdirSync(o.output);
    const bytes=fs.readFileSync(o.image),text='未校正 ロ & <字>\n';
    const result={schema_version:1,kind:'unclassified_ocr',source:{image:{sha256:hash(bytes),bytes:bytes.length}},canvas:{pixel_width:2,pixel_height:2},text,layout_available:true,blocks:[],recognition_regions:[{id:0,text,confidence:0,is_vertical:false,bbox:{pixels:{x0:0,y0:0,x1:1,y1:1}}}],article_assignment:null,native_glyph_binding:null};
    fs.writeFileSync(path.join(o.output,'observations.json'),JSON.stringify(result));return result;
  }};
  const candidates=value=>{
    const digest=write('source-observation.json',{kind:'unit_observation_fixture',rectangular_cells:value});
    const bound=JSON.parse(fs.readFileSync(path.join(prepared,'source-binding.json')));bound.observation_sha256=digest;
    prep.source.observation_sha256=digest;prep.source.binding_sha256=write('source-binding.json',bound);
  };
  return {root,prepared,prep,frame,options,runtime,save,write,candidates,calls:()=>calls,cleanup:()=>fs.rmSync(root,{recursive:true,force:true})};
}
function subdivide(h,owners){
  h.prep.parts=h.prep.tiles.map((tile,i)=>({id:i,box:tile.core.slice(),...owners[i]}));
  for(const [i,tile]of h.prep.tiles.entries()){
    Object.assign(tile,owners[i],{part:i,window:tile.core.slice(),recognition_roi:tile.core.slice()});
    const [x,y]=tile.core;
    tile.PNG_sha256=h.write(tile.PNG,pixelPng);
    tile.placement_sha256=h.write(tile.placement,{...h.frame,pixel_width:1,pixel_height:1,recognition_to_image:[0,1,-1,0,2-y,x]});
  }
  h.save();
}
test('all source cores run once; original coordinates and uncorrected text are preserved',async()=>{
  const h=harness();try{
    const input=fs.readFileSync(path.join(h.prepared,'preparation.json'));
    const out=await runPrepared(h.options,h.runtime);assert.equal(h.calls(),4);assert.equal(out.coverage_verified,false);assert.equal(out.article_assignment,null);assert.equal(out.native_glyph_binding,null);
    const observations=out.rows.map(row=>JSON.parse(fs.readFileSync(path.join(h.options.output,row.projection))));
    for(const x of observations){assert.equal(x.text,'未校正 ロ & <字>\n');assert.equal(x.recognition_regions[0].confidence,0);assert.deepEqual(x.recognition_regions[0].projection.bounds_points,[10,173,15,183]);assert.deepEqual(x.recognition_regions[0].source_core.source_pixels,[0,0,1,1]);assert.equal(x.source_derivation.original_document_sha256,h.prep.source.document_sha256);}
    assert.deepEqual(observations.map(x=>x.recognition_regions[0].source_core.status),['center_inside_core','halo_observation','halo_observation','halo_observation']);
    assert.equal(fs.readFileSync(path.join(h.options.output,'prepared/preparation.json')).compare(input),0);
    assert.equal(fs.readFileSync(path.join(h.prepared,'preparation.json')).compare(input),0);
    assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).recognition_requests,4);
    assert.equal(fs.existsSync(path.join(h.options.output,'tile-00003-result.json')),true);
  }finally{h.cleanup();}
});
test('transformed-empty cores retain their source occurrence without successful empty text',async()=>{
  const h=harness();try{
    const tile=h.prep.tiles[0];for(const key of ['PNG','PNG_sha256','recognition_roi','placement','placement_sha256'])delete tile[key];tile.status='recognition_copy_empty_unverified';h.save();
    const out=await runPrepared(h.options,h.runtime);assert.equal(h.calls(),3);assert.equal(out.rows[0].status,'recognition_copy_empty_unverified');assert.equal(out.rows[0].observations,undefined);assert.equal(out.rows[0].text,undefined);assert.equal(out.coverage_verified,false);
  }finally{h.cleanup();}
});
test('overlapping cores and missing source areas are rejected before OCR',async()=>{
  for(const mutation of ['overlap','hole']){const h=harness();try{
    if(mutation==='overlap')h.prep.tiles[1].core=[0,0,1,1];else h.prep.tiles.pop();h.save();
    await assert.rejects(runPrepared(h.options,h.runtime),/partition/);assert.equal(h.calls(),0);
  }finally{h.cleanup();}}
});
test('nested candidate ownership survives disjoint source subdivision',()=>{
  const h=harness();try{
    h.candidates([{id:0,interior_pixels:[0,0,2,2]},{id:1,interior_pixels:[0,0,1,1]}]);
    subdivide(h,[{role:'ruling_face_candidate',cell:1},{role:'ruling_face_candidate',cell:0},{role:'ruling_face_candidate',cell:0},{role:'ruling_face_candidate',cell:0}]);
    const loaded=loadPreparation(h.prepared);assert.deepEqual(loaded.prep.parts.map(p=>p.cell),[1,0,0,0]);
    h.prep.parts[0].cell=0;h.prep.tiles[0].cell=0;h.save();assert.throws(()=>loadPreparation(h.prepared),/original candidate ownership/);
  }finally{h.cleanup();}
});
test('partial overlaps retain both candidate identities rather than selecting one',()=>{
  const h=harness();try{
    h.candidates([{id:0,interior_pixels:[0,0,1,2]},{id:1,interior_pixels:[0,0,2,1]}]);
    subdivide(h,[{role:'overlapping_ruling_faces',cells:[0,1]},{role:'ruling_face_candidate',cell:1},{role:'ruling_face_candidate',cell:0},{role:'outside_ruling_faces'}]);
    const loaded=loadPreparation(h.prepared);assert.deepEqual(loaded.prep.parts[0].cells,[0,1]);assert.equal(loaded.prep.parts[0].cell,undefined);
    h.prep.parts[0].cells=[0];h.prep.tiles[0].cells=[0];h.save();assert.throws(()=>loadPreparation(h.prepared),/discarded overlapping/);
  }finally{h.cleanup();}
});
test('a changed derived PNG or implementation snapshot cannot be recognized',async()=>{
  for(const member of ['tile-00000.png','implementation/prepare_raster.py']){const h=harness();try{
    fs.writeFileSync(path.join(h.prepared,member),'changed bytes');
    assert.throws(()=>loadPreparation(h.prepared),/hash mismatch/);
    await assert.rejects(runPrepared(h.options,h.runtime),/hash mismatch/);assert.equal(h.calls(),0);
  }finally{h.cleanup();}}
});
test('a crop with a self-consistent file hash but foreign translation is rejected',async()=>{
  const h=harness();try{
    const tile=h.prep.tiles[0],frame=JSON.parse(fs.readFileSync(path.join(h.prepared,tile.placement)));frame.recognition_to_image[4]+=1;tile.placement_sha256=h.write(tile.placement,frame);h.save();
    assert.throws(()=>loadPreparation(h.prepared),/original source matrix/);assert.equal(h.calls(),0);
  }finally{h.cleanup();}
});
test('one OCR failure keeps all other source observations and is never retried',async()=>{
  const h=harness();try{
    const old=h.runtime.recognize;let attempts=0;h.runtime.recognize=async o=>{if(++attempts===2)throw new Error('one fixed failure');return old(o);};
    const out=await runPrepared(h.options,h.runtime);assert.equal(attempts,4);assert.equal(out.rows[1].status,'failed');assert.equal(out.rows[1].recognition_retry,false);assert.equal(out.rows[2].status,'recognized_unclassified');assert.equal(JSON.parse(fs.readFileSync(path.join(h.options.output,'terminal.json'))).status,'partial');
  }finally{h.cleanup();}
});
test('OCR for another PNG fails binding while retaining raw recognition evidence',async()=>{
  const h=harness();try{
    const old=h.runtime.recognize;h.runtime.recognize=async o=>{const r=await old(o);r.source.image.sha256='0'.repeat(64);return r;};
    const out=await runPrepared(h.options,h.runtime);assert.equal(h.calls(),4);assert.ok(out.rows.every(r=>r.status==='failed'));assert.equal(fs.existsSync(path.join(h.options.output,'tile-00000-ocr/observations.json')),true);assert.equal(fs.existsSync(path.join(h.options.output,'tile-00000-projected.json')),false);
  }finally{h.cleanup();}
});
test('missing requested OCR region data cannot silently become a successful projection',async()=>{
  const h=harness();try{
    const old=h.runtime.recognize;h.runtime.recognize=async o=>{const r=await old(o);r.layout_available=false;delete r.recognition_regions;return r;};
    const out=await runPrepared(h.options,h.runtime);assert.ok(out.rows.every(r=>r.status==='failed'));assert.match(out.rows[0].error,/regions were not returned/);
  }finally{h.cleanup();}
});
test('an existing OCR output cannot be overwritten',async()=>{
  const h=harness();try{await runPrepared(h.options,h.runtime);const before=fs.readFileSync(path.join(h.options.output,'index.json'));await assert.rejects(runPrepared(h.options,h.runtime),/EEXIST/);assert.equal(fs.readFileSync(path.join(h.options.output,'index.json')).compare(before),0);assert.equal(h.calls(),4);}finally{h.cleanup();}
});
test('periodic preprocessing requires its captured helper, declared operations and original source evidence',()=>{
  const h=harness();try{
    h.prep.profile='median3-ruling-patterns';
    h.prep.operations=[{kind:'pillow_channelwise_median',size:3},{kind:'erase_original_ruling_band_and_span',margin:1},{kind:'erase_isolated_one_pixel_components',connectivity:8},{kind:'erase_bounded_periodic_ruling_components',connectivity:8,source:'original_opaque_black_runs_between_perpendicular_rulings'}];
    h.prep.producer.code_sha256['raster_ruling_patterns.py']=h.write('implementation/raster_ruling_patterns.py',fs.readFileSync(path.join(__dirname,'raster_ruling_patterns.py')));
    const details={schema:1,kind:'source_derived_ruling_pattern_erasure',original:{source_RGBA_sha256:h.prep.source.source_RGBA_sha256,candidates:[]},erasure:{selected_components:[]}};
    h.prep.ruling_patterns={file:'ruling-patterns.json',sha256:h.write('ruling-patterns.json',details)};h.save();
    assert.equal(loadPreparation(h.prepared).prep.profile,'median3-ruling-patterns');
    details.original.source_RGBA_sha256='foreign source';h.prep.ruling_patterns.sha256=h.write('ruling-patterns.json',details);h.save();
    assert.throws(()=>loadPreparation(h.prepared),/differ from the original source/);
    details.original.source_RGBA_sha256=h.prep.source.source_RGBA_sha256;h.prep.ruling_patterns.sha256=h.write('ruling-patterns.json',details);
    delete h.prep.producer.code_sha256['raster_ruling_patterns.py'];h.save();
    assert.throws(()=>loadPreparation(h.prepared),/implementation snapshot/);
  }finally{h.cleanup();}
});
test('a fixed source-window policy rejects an opportunistically trimmed crop',()=>{
  const h=harness();try{
    h.prep.settings={recognition_crop:'source_window'};h.save();assert.equal(loadPreparation(h.prepared).prep.settings.recognition_crop,'source_window');
    const tile=h.prep.tiles[0];tile.recognition_roi=[0,0,1,1];tile.PNG_sha256=h.write(tile.PNG,pixelPng);
    tile.placement_sha256=h.write(tile.placement,{...h.frame,pixel_width:1,pixel_height:1});h.save();
    assert.throws(()=>loadPreparation(h.prepared),/fixed original source window/);
  }finally{h.cleanup();}
});

test('periodic partition ownership uses its separately bound inventory and keeps inferred boundaries explicit',()=>{
  const h=harness();try{
    h.prep.settings={partition_geometry:'periodic_ruling_candidates'};
    for(const name of ['raster_ruling_patterns.py','raster_periodic_grid.py'])h.prep.producer.code_sha256[name]=h.write('implementation/'+name,fs.readFileSync(path.join(__dirname,name)));
    const observation={schema:1,kind:'unclassified_periodic_ruling_faces',coverage_verified:false,article_assignment:null,native_glyph_binding:null,source_observation_sha256:h.prep.source.observation_sha256,source_RGBA_sha256:h.prep.source.source_RGBA_sha256,periodic_observations:{source_RGBA_sha256:h.prep.source.source_RGBA_sha256},canvas:{pixel_width:2,pixel_height:2},rectangular_cells:[{id:7,interior_pixels:[0,0,1,1],boundary_kind:'includes_inferred_periodic_spans'}]};
    const saveObservation=()=>{h.prep.partition_observation={file:'partition-observation.json',sha256:h.write('partition-observation.json',observation)};h.save();};
    saveObservation();
    subdivide(h,[{role:'ruling_face_candidate',cell:7},{role:'outside_ruling_faces'},{role:'outside_ruling_faces'},{role:'outside_ruling_faces'}]);
    assert.equal(loadPreparation(h.prepared).prep.parts[0].cell,7);
    observation.source_observation_sha256='other original';saveObservation();
    assert.throws(()=>loadPreparation(h.prepared),/differs from the original/);
    observation.source_observation_sha256=h.prep.source.observation_sha256;saveObservation();
    observation.rectangular_cells[0].boundary_kind='verified_table';saveObservation();
    assert.throws(()=>loadPreparation(h.prepared),/boundary evidence/);
    observation.rectangular_cells[0].boundary_kind='includes_inferred_periodic_spans';saveObservation();
    delete h.prep.producer.code_sha256['raster_periodic_grid.py'];h.save();
    assert.throws(()=>loadPreparation(h.prepared),/implementation snapshot/);
  }finally{h.cleanup();}
});

test('source band completion needs its helper and matching geometry declaration',()=>{
  const h=harness();try{
    h.prep.settings={partition_geometry:'periodic_ruling_candidates',complete_ruling_bands:true};
    for(const name of ['raster_ruling_patterns.py','raster_periodic_grid.py','raster_band_completion.py'])h.prep.producer.code_sha256[name]=h.write('implementation/'+name,fs.readFileSync(path.join(__dirname,name)));
    const observation={schema:1,kind:'unclassified_periodic_ruling_faces',coverage_verified:false,article_assignment:null,native_glyph_binding:null,source_observation_sha256:h.prep.source.observation_sha256,source_RGBA_sha256:h.prep.source.source_RGBA_sha256,periodic_observations:{source_RGBA_sha256:h.prep.source.source_RGBA_sha256},canvas:{pixel_width:2,pixel_height:2},settings:{source_band_completion:true},rectangular_cells:[]};
    const save=()=>{h.prep.partition_observation={file:'partition-observation.json',sha256:h.write('partition-observation.json',observation)};h.save();};save();
    assert.equal(loadPreparation(h.prepared).prep.settings.complete_ruling_bands,true);
    observation.settings.source_band_completion=false;save();
    assert.throws(()=>loadPreparation(h.prepared),/completion differs/);
    observation.settings.source_band_completion=true;save();
    delete h.prep.producer.code_sha256['raster_band_completion.py'];h.save();
    assert.throws(()=>loadPreparation(h.prepared),/implementation snapshot/);
  }finally{h.cleanup();}
});

test('copy ink distinguishes halo-only context without rejecting a glyph whose center is white',()=>{
  const onlyHalo=new Map([[4,[[1,3]]],[5,[[1,3]]]]);
  assert.deepEqual(copyInkSupport([0,0,10,10],[5,0,10,10],onlyHalo),{policy:'bbox_of_actual_copy_ink_inside_recognizer_region',pixels:4,owned_core_pixels:0,bounds_source_pixels:[1,4,3,6],bbox_center_core:'halo_observation'});
  const ring=new Map([[0,[[0,10]]],[9,[[0,10]]]]);
  for(let y=1;y<9;y++)ring.set(y,[[0,1],[9,10]]);
  const hollow=copyInkSupport([0,0,10,10],[4,2,6,8],ring);
  assert.equal(hollow.owned_core_pixels,0);assert.equal(hollow.bbox_center_core,'center_inside_core');assert.deepEqual(hollow.bounds_source_pixels,[0,0,10,10]);
  const empty=copyInkSupport([4,2,6,8],[4,2,6,8],ring);
  assert.equal(empty.bbox_center_core,'no_copy_ink_unverified');assert.equal(empty.bounds_source_pixels,null);
});

test('copy ink evidence is bound to the parent image and preserves every raw recognition field',async()=>{
  const h=harness();try{
    h.prep.producer.code_sha256['raster_ruling_patterns.py']=h.write('implementation/raster_ruling_patterns.py',fs.readFileSync(path.join(__dirname,'raster_ruling_patterns.py')));
    const evidence={schema:1,kind:'unclassified_recognition_copy_ink_support',source_RGBA_sha256:h.prep.source.source_RGBA_sha256,recognition_parent_sha256:h.prep.recognition_parent_sha256,canvas:h.prep.canvas,connectivity:8,coverage_verified:false,components:[{id:0,bounds:[0,0,2,2],area:4,support:[[0,0,2],[1,0,2]]}]};
    const save=()=>{h.prep.recognition_ink={file:'recognition-ink.json',sha256:h.write('recognition-ink.json',evidence)};h.save();};save();
    const result=await runPrepared(h.options,h.runtime),projection=JSON.parse(fs.readFileSync(path.join(h.options.output,result.rows[0].projection)));
    assert.equal(projection.recognition_regions[0].text,'未校正 ロ & <字>\n');assert.equal(projection.recognition_regions[0].confidence,0);
    assert.deepEqual(projection.recognition_regions[0].source_ink.bounds_source_pixels,[0,0,1,1]);assert.equal(projection.recognition_regions[0].source_ink.pixels,1);
    evidence.recognition_parent_sha256='foreign image';save();assert.throws(()=>loadPreparation(h.prepared),/ink differs/);
    evidence.recognition_parent_sha256=h.prep.recognition_parent_sha256;evidence.components[0].support.push([1,1,2]);save();assert.throws(()=>loadPreparation(h.prepared),/overlapping/);
  }finally{h.cleanup();}
});

test('glyph quarter turns compose the existing image basis, sampling dimensions and core ownership',async()=>{
  for(const turn of [0,1,2,3]){const h=harness();try{
    for(const name of ['raster_ruling_patterns.py','raster_orientation.py'])h.prep.producer.code_sha256[name]=h.write('implementation/'+name,fs.readFileSync(path.join(__dirname,name)));
    h.prep.settings={recognition_quarter_turns:turn};
    const orientation={schema:1,kind:'source_derived_raster_orientation',source:h.prep.source,recognition_parent_sha256:h.prep.recognition_parent_sha256,
      vision:{page:{revision:3,language_correction:false}},observation:{kind:'unclassified_raster_glyph_orientation',coverage_verified:false,canvas:h.prep.canvas,status:'unanimous_observed_glyph_right_axes',glyph_right_axis:turn,quarter_turns_ccw:turn}};
    h.prep.orientation={file:'source-orientation.json',sha256:h.write('source-orientation.json',orientation)};
    const sourceMatrices=[[1,0,0,1,0,0],[0,1,-1,0,2,0],[-1,0,0,-1,2,2],[0,-1,1,0,0,2]],imageMatrices=[[0,1,-1,0,2,0],[-1,0,0,-1,2,2],[0,-1,1,0,0,2],[1,0,0,1,0,0]];
    for(const t of h.prep.tiles){t.recognition_to_source=sourceMatrices[turn].slice();t.placement_sha256=h.write(t.placement,{...h.frame,recognition_to_image:imageMatrices[turn]});}h.save();
    const runtime={recognize:async o=>{assert.equal(o.width_points,turn%2?20:10);assert.equal(o.height_points,turn%2?10:20);return h.runtime.recognize(o);}};
    const result=await runPrepared(h.options,runtime),regions=result.rows.map(r=>JSON.parse(fs.readFileSync(path.join(h.options.output,r.projection))).recognition_regions[0]);
    const sourceBoxes=[[0,0,1,1],[1,0,2,1],[1,1,2,2],[0,1,1,2]],pageBoxes=[[10,173,15,183],[10,183,15,193],[5,183,10,193],[5,173,10,183]];
    for(const r of regions){assert.deepEqual(r.source_core.source_pixels,sourceBoxes[turn]);assert.deepEqual(r.projection.bounds_points,pageBoxes[turn]);assert.equal(r.text,'未校正 ロ & <字>\n');}
    assert.deepEqual(regions.map(r=>r.source_core.status),[0,1,2,3].map(i=>i===[0,1,3,2][turn]?'center_inside_core':'halo_observation'));
    h.prep.tiles[0].recognition_to_source[4]=3;h.save();assert.throws(()=>loadPreparation(h.prepared),/normalization differs/);
    h.prep.tiles[0].recognition_to_source[4]=sourceMatrices[turn][4];h.prep.settings.recognition_quarter_turns=(turn+1)%4;h.save();assert.throws(()=>loadPreparation(h.prepared),/Orientation differs/);
  }finally{h.cleanup();}}
});
