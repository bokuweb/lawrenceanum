'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {recognitionRegions}=require('./ndlocr-regions.cjs');
const frame={pixel_width:100,pixel_height:200};
const thresholds={detection:0.25,confidence:0.3};
const make=()=>({schema_version:1,width:100,height:200,settings:{structure_rules:false,quality_boost:false,use_deim_detection:true,det_conf_threshold:0.25,min_line_confidence:Math.fround(0.3)},body:'任意の原文\n改行を含む\n領域',stages:{reading_order:[{bbox:[10,20,30,80],text:'任意の原文',confidence:0.8,is_vertical:true},{bbox:[40,100,90,120],text:'改行を含む\n領域',confidence:0.9,is_vertical:false}]}});
test('recognition regions retain raw reading order, internal newlines, orientation and engine scores',()=>{
 const trace=make(),before=structuredClone(trace);
 const regions=recognitionRegions(trace,trace.body,frame,thresholds);
 assert.deepEqual(trace,before);
 assert.deepEqual(regions.map(r=>r.text),trace.stages.reading_order.map(r=>r.text));
 assert.deepEqual(regions.map(r=>r.id),[0,1]);
 assert.deepEqual(regions[0].bbox.pixels,{x0:10,y0:20,x1:30,y1:80});
 assert.equal(regions[0].confidence,0.8);assert.equal(regions[0].is_vertical,true);
 assert.equal(regions[1].text,'改行を含む\n領域');
 assert.equal(Object.hasOwn(regions[0],'words'),false);
});
test('schema, canvas, body and one-pass reading-order text must all match',()=>{
 const changes=[t=>t.schema_version=2,t=>t.width=99,t=>t.height=201,t=>t.body+=' ',t=>t.stages.reading_order.reverse(),t=>delete t.stages,t=>t.stages.reading_order=null,t=>t.stages.reading_order[0].text=7];
 for(const change of changes){const t=make(),txt=t.body;change(t);assert.throws(()=>recognitionRegions(t,txt,frame,thresholds),/NDLOCR trace/);}
});
test('malformed, out-of-canvas, degenerate and fractional region boxes are rejected',()=>{
 for(const bbox of [[-1,20,30,80],[10,20,101,80],[10,20,30,201],[10,20,10,80],[10,80,30,20],[10.5,20,30,80],[10,20,30],[10,20,NaN,80]]){
  const t=make();t.stages.reading_order[0].bbox=bbox;assert.throws(()=>recognitionRegions(t,t.body,frame,thresholds),/NDLOCR trace/);
 }
});
test('missing or invalid recognition scores and orientations fail without clamping',()=>{
 for(const [key,values]of Object.entries({confidence:[undefined,-0.1,1.1,NaN,'0.9'],is_vertical:[undefined,0,'true',null]}))for(const value of values){const t=make();t.stages.reading_order[0][key]=value;assert.throws(()=>recognitionRegions(t,t.body,frame,thresholds),/NDLOCR trace/);}
});
test('trace settings prove raw selection and the requested f32 thresholds',()=>{
 for(const [key,value]of [['structure_rules',true],['quality_boost',true],['use_deim_detection',false],['det_conf_threshold',0.9],['min_line_confidence',0.7],['min_line_confidence',undefined]]){
  const t=make();t.settings[key]=value;assert.throws(()=>recognitionRegions(t,t.body,frame,thresholds),/NDLOCR trace/);
 }
});
test('empty recognition is a valid empty trace rather than invented geometry',()=>{
 const t=make();t.body='';t.stages.reading_order=[];assert.deepEqual(recognitionRegions(t,'',frame,thresholds),[]);
});
