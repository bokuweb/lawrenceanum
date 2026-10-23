'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {projectBox,projectLayout}=require('./placement.cjs');
function frame(rest={}) {return {pixel_width:100,pixel_height:200,image_width:100,image_height:200,recognition_to_image:[1,0,0,1,0,0],image_to_user:[50,0,0,100,10,20],crop_box:[0,0,300,400],rotate:0,...rest};}
const box={x0:10,y0:20,x1:30,y1:60};
function approx(actual,expected){assert.equal(actual.length,expected.length);for(let i=0;i<actual.length;i++)assert.ok(Math.abs(actual[i]-expected[i])<=Number.EPSILON*Math.max(1,Math.abs(expected[i]))*8,`${actual[i]} vs ${expected[i]}`);}
function observation(){const bbox={pixels:structuredClone(box),points:[999,999,1000,1000],geometry:'valid'};return {kind:'unclassified_ocr',source:{image:{sha256:'retained'}},canvas:{pixel_width:100,pixel_height:200},text:'任意本文 苦\n',article_assignment:null,native_glyph_binding:null,blocks:[{source_path:[0],text:'任意本文 苦\n',confidence:0,bbox,paragraphs:[{source_path:[0,0],text:'任意本文 苦\n',confidence:0,bbox,lines:[{source_path:[0,0,0],text:'任意本文 苦\n',confidence:0,bbox,words:[{source_path:[0,0,0,0],text:'苦',confidence:95,bbox},{source_path:[0,0,0,1],text:'',confidence:0,bbox:{pixels:{x0:0,y0:0,x1:0,y1:1},points:null,geometry:'invalid'}}]}]}]}]};}
test('partial image placement uses its original matrix rather than whole-page scaling',()=>{
 const p=projectBox(box,frame());assert.equal(p.status,'projected');assert.deepEqual(p.quad_points,[[15,290],[25,290],[25,310],[15,310]]);assert.deepEqual(p.bounds_points,[15,290,25,310]);assert.equal(p.source_image_relation,'inside');assert.equal(p.page_relation,'inside');
});
test('CropBox offsets and all PDF quarter-turns have explicit expected corners',()=>{
 const cases=[[0,[[5,270],[15,270],[15,290],[5,290]]],[90,[[90,5],[90,15],[70,15],[70,5]]],[180,[[195,90],[185,90],[185,70],[195,70]]],[270,[[270,195],[270,185],[290,185],[290,195]]]];
 for(const [rotate,want]of cases)assert.deepEqual(projectBox(box,frame({crop_box:[10,20,210,380],rotate})).quad_points,want);
});
test('recognition rotation, raw image top-left axes and PDF Rotate270 cancel correctly',()=>{
 const f=frame({pixel_width:3509,pixel_height:2480,image_width:2480,image_height:3509,recognition_to_image:[0,1,-1,0,2480,0],image_to_user:[595,0,0,842,0,0],crop_box:[0,0,595,842],rotate:270});const b={x0:100,y0:200,x1:300,y1:400};const p=projectBox(b,f);approx(p.bounds_points,[100/3509*842,200/2480*595,300/3509*842,400/2480*595]);
});
test('nonzero crop origin works with a flipped image matrix',()=>{
 const p=projectBox(box,frame({image_to_user:[-50,0,0,100,110,120],crop_box:[10,20,310,420]}));assert.deepEqual(p.quad_points,[[95,210],[85,210],[85,230],[95,230]]);
});
test('sheared placement retains its quadrilateral instead of claiming its whole bounding rectangle',()=>{
 const p=projectBox(box,frame({image_to_user:[100,25,40,200,0,0]}));assert.deepEqual(p.quad_points,[[46,217.5],[66,212.5],[58,252.5],[38,257.5]]);assert.deepEqual(p.bounds_points,[38,212.5,66,257.5]);
});
test('scaling translation and arbitrary recognition canvas do not introduce point thresholds',()=>{
 const base=projectBox(box,frame()).quad_points;
 for(const s of [0.125,1,8]){
  const p=projectBox(box,frame({image_to_user:[50*s,0,0,100*s,10*s-100,20*s-200],crop_box:[-100,-200,300*s-100,400*s-200]}));for(let i=0;i<4;i++)approx(p.quad_points[i],base[i].map(x=>x*s));
 }
});
test('recognition images may be cropped or scaled explicitly without guessed orientation',()=>{
 const p=projectBox({x0:0,y0:0,x1:20,y1:40},frame({pixel_width:20,pixel_height:40,recognition_to_image:[2,0,0,2,10,20]}));assert.deepEqual(p.bounds_points,[15,290,35,330]);
});
test('outside source-image observations stay present and are classified geometrically',()=>{
 const p=projectBox(box,frame({recognition_to_image:[1,0,0,1,200,0]}));assert.equal(p.status,'projected');assert.equal(p.source_image_relation,'outside');assert.deepEqual(p.pixels,box);assert.ok(p.quad_points);
});
test('partly outside source-image and page bounds are not silently clipped',()=>{
 const p=projectBox(box,frame({recognition_to_image:[1,0,0,1,-20,0],image_to_user:[50,0,0,100,0,350]}));assert.equal(p.source_image_relation,'partial');assert.equal(p.page_relation,'outside');assert.deepEqual(p.bounds_points,[-5,-40,5,-20]);
});
test('degenerate, out-of-canvas and exceptional OCR boxes are retained as invalid',()=>{
 for(const b of [null,{}, {x0:1,y0:2,x1:1,y1:3},{x0:-1,y0:0,x1:2,y1:3},{x0:0,y0:0,x1:101,y1:1},{x0:0,y0:0,x1:1,y1:NaN},{x0:0.5,y0:0,x1:1,y1:1}]){const p=projectBox(b,frame());assert.equal(p.status,'invalid');assert.deepEqual(p.pixels,b);assert.equal(p.quad_points,null);}
});
test('invalid source metadata and singular transforms reject the entire placement',()=>{
 for(const f of [frame({rotate:45}),frame({crop_box:[0,0,0,1]}),frame({crop_box:[0,0,Infinity,1]}),frame({image_width:0}),frame({pixel_width:1.5}),frame({recognition_to_image:[1,0,2,0,0,0]}),frame({image_to_user:[1,0,0,NaN,0,0]})])assert.throws(()=>projectBox(box,f));
});
test('finite transforms with overflowing output do not become false positions',()=>{
 const p=projectBox(box,frame({recognition_to_image:[Number.MAX_VALUE,0,0,1,0,0]}));assert.equal(p.status,'invalid');assert.equal(p.quad_points,null);assert.match(p.reason,/numeric/);
});
test('point precision collapse is explicit rather than a manufactured nonzero box',()=>{
 const p=projectBox(box,frame({image_to_user:[1,0,0,1,1e30,1e30]}));assert.equal(p.status,'invalid');assert.equal(p.quad_points,null);assert.match(p.reason,/precision/);
});
test('all layout nodes, original geometry, text, confidence and engine paths are preserved',()=>{
 const raw=observation(),before=structuredClone(raw),f=frame();const p=projectLayout(raw,f);assert.deepEqual(raw,before);assert.equal(p.text,raw.text);const w=p.blocks[0].paragraphs[0].lines[0].words;assert.equal(w.length,2);assert.equal(w[0].text,'苦');assert.equal(w[0].confidence,95);assert.deepEqual(w[0].source_path,[0,0,0,0]);assert.deepEqual(w[0].bbox,raw.blocks[0].paragraphs[0].lines[0].words[0].bbox);assert.equal(w[1].projection.status,'invalid');assert.equal(p.article_assignment,null);assert.equal(p.native_glyph_binding,null);assert.equal(p.visibility,'unverified');assert.equal(p.source_binding,'declared_unverified');
 raw.source.image.sha256='changed';f.image_to_user[0]=999;assert.equal(p.source.image.sha256,'retained');assert.equal(p.declared_placement.image_to_user[0],50);
});
test('a missing layout is not reconstructed or treated as a missing recognition result',()=>{
 const o=observation();o.blocks=[];o.layout_available=false;const p=projectLayout(o,frame());assert.deepEqual(p.blocks,[]);assert.equal(p.text,o.text);
});
test('mismatched recognition canvas and already assigned native context are rejected',()=>{
 const o=observation();assert.throws(()=>projectLayout(o,frame({pixel_width:200})));assert.throws(()=>projectLayout({...o,kind:'native'},frame()));assert.throws(()=>projectLayout({...o,article_assignment:[]},frame()));assert.throws(()=>projectLayout({...o,native_glyph_binding:{}},frame()));
});
test('projection never silently overwrites an existing node projection',()=>{
 const o=observation();o.blocks[0].projection={unexpected:true};assert.throws(()=>projectLayout(o,frame()));
});
test('a previous point-frame precision failure does not erase recoverable original pixels',()=>{
 const o=observation();o.blocks[0].bbox.points=null;o.blocks[0].bbox.geometry='invalid';o.blocks[0].bbox.reason='point_precision_loss';const p=projectLayout(o,frame());assert.equal(p.blocks[0].bbox.geometry,'invalid');assert.equal(p.blocks[0].projection.status,'projected');
});
test('reserved root projection metadata cannot be silently replaced',()=>{
 for(const key of ['declared_placement','visibility','source_binding','projection'])assert.throws(()=>projectLayout({...observation(),[key]:{}},frame()));
});
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawnSync}=require('node:child_process');
const {projectFile}=require('./project-cli.cjs');
function files(fn){const d=fs.mkdtempSync(path.join(os.tmpdir(),'kanpo-placement-'));try{const observations=path.join(d,'raw observations.json'),placement=path.join(d,'frame.json'),output=path.join(d,'new projection.json');fs.writeFileSync(observations,JSON.stringify(observation()));fs.writeFileSync(placement,JSON.stringify(frame()));return fn({observations,placement,output});}finally{fs.rmSync(d,{recursive:true,force:true});}}
test('file projection records the exact input bytes and preserves all original occurrences',()=>files(f=>{
 const crypto=require('node:crypto'),H=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');const before=fs.readFileSync(f.observations);const r=projectFile(f);assert.equal(r.projection_inputs.observations.sha256,H(f.observations));assert.equal(r.projection_inputs.placement.sha256,H(f.placement));assert.deepEqual(fs.readFileSync(f.observations),before);assert.deepEqual(JSON.parse(fs.readFileSync(f.output)),r);
}));
test('projection file output is exclusive and cannot overwrite either an old result or input',()=>files(f=>{
 fs.writeFileSync(f.output,'previous');assert.throws(()=>projectFile(f),/EEXIST/);assert.equal(fs.readFileSync(f.output,'utf8'),'previous');const old=fs.readFileSync(f.observations);assert.throws(()=>projectFile({...f,output:f.observations}),/EEXIST/);assert.deepEqual(fs.readFileSync(f.observations),old);
}));
test('malformed geometry or input JSON cannot publish a completed projection',()=>files(f=>{
 fs.writeFileSync(f.placement,'{');assert.throws(()=>projectFile(f));assert.equal(fs.existsSync(f.output),false);fs.writeFileSync(f.placement,JSON.stringify(frame({rotate:1})));assert.throws(()=>projectFile(f));assert.equal(fs.existsSync(f.output),false);
}));
test('CLI uses paths with spaces without a shell and requires no model',()=>files(f=>{
 const r=spawnSync(process.execPath,[path.join(__dirname,'project-cli.cjs'),'--placement',f.placement,'--output',f.output,f.observations],{encoding:'utf8'});assert.equal(r.status,0,r.stderr);assert.equal(JSON.parse(fs.readFileSync(f.output)).kind,'unclassified_ocr_projection');
}));
test('CLI help and failures do not load an OCR engine or create results',()=>files(f=>{
 const cli=path.join(__dirname,'project-cli.cjs');const h=spawnSync(process.execPath,[cli,'--help'],{encoding:'utf8'});assert.equal(h.status,0);assert.match(h.stdout,/No model download/);const r=spawnSync(process.execPath,[cli,'--unknown'],{encoding:'utf8'});assert.notEqual(r.status,0);assert.equal(fs.existsSync(f.output),false);
}));
test('exceptional source coordinates remain explicit in serialized invalid observations',()=>files(f=>{
 const {encodeJson}=require('./sidecar.cjs');const o=observation();o.blocks[0].bbox.pixels.y1=Infinity;fs.writeFileSync(f.observations,encodeJson(o));const r=projectFile(f);assert.equal(r.blocks[0].projection.status,'invalid');assert.deepEqual(r.blocks[0].projection.pixels.y1,{$js_number:'Infinity'});
}));
test('geometric page containment does not certify clipping transparency or visibility',()=>{
 const f=frame({declared_clip_in_user_coordinates:[0,0,1,1],declared_soft_mask:'unknown',declared_pdf_sha256:'unverified'});const p=projectLayout(observation(),f);assert.equal(p.blocks[0].projection.page_relation,'inside');assert.equal(p.visibility,'unverified');assert.equal(p.source_binding,'declared_unverified');assert.deepEqual(p.declared_placement,f);
});
test('a sheared quad whose bounding rectangle overlaps the page can still be wholly outside',()=>{
 const f=frame({image_to_user:[1,-1,1,1,-1.75,1.75],crop_box:[0,0,1,1]});const p=projectBox({x0:0,y0:0,x1:100,y1:200},f);assert.deepEqual(p.bounds_points,[-1.75,-1.75,0.25,0.25]);assert.equal(p.page_relation,'outside');
});
test('touching a page edge and having positive-area coverage remain distinct',()=>{
 const box={x0:0,y0:0,x1:100,y1:200};assert.equal(projectBox(box,frame({image_to_user:[1,-1,1,1,-1.5,1.5],crop_box:[0,0,1,1]})).page_relation,'touching');assert.equal(projectBox(box,frame({image_to_user:[1,-1,1,1,-1,1],crop_box:[0,0,1,1]})).page_relation,'partial');
});
test('sparse matrix translations cannot bypass complete finite-metadata validation',()=>{
 const m=[1,0,0,1];m.length=6;assert.throws(()=>projectBox(box,frame({image_to_user:m})));assert.throws(()=>projectBox(box,frame({recognition_to_image:m})));
});
test('recognizer regions project directly without fabricated block/paragraph/word nodes',()=>{
 const raw=observation();raw.blocks=[];raw.layout_available=true;raw.recognition_regions=[{id:0,text:'元の\n領域',confidence:0.8,is_vertical:true,bbox:{pixels:box}}];const before=structuredClone(raw);
 const out=projectLayout(raw,frame());assert.deepEqual(raw,before);assert.deepEqual(out.blocks,[]);
 const region=out.recognition_regions[0];assert.deepEqual(region.projection.quad_points,[[15,290],[25,290],[25,310],[15,310]]);
 const {projection,...unchanged}=region;assert.deepEqual(unchanged,raw.recognition_regions[0]);assert.equal(Object.hasOwn(region,'words'),false);
 assert.equal(out.article_assignment,null);assert.equal(out.native_glyph_binding,null);
});
test('recognition region projection cannot overwrite source evidence or silently drop an invalid container',()=>{
 for(const regions of [null,{},[{projection:{},bbox:{pixels:box}}]]){const raw=observation();raw.blocks=[];raw.recognition_regions=regions;assert.throws(()=>projectLayout(raw,frame()));}
});
