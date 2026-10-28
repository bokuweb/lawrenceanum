'use strict';
// NDLOCR regions describe recognizer crops. They are not glyph or word boxes.
function recognitionRegions(trace,text,canvas,{detection,confidence}) {
  const requireTrace=(condition,reason)=>{if(!condition)throw new Error(`NDLOCR trace ${reason}`);};
  requireTrace(trace?.schema_version===1,'schema mismatch');
  requireTrace(trace.width===canvas.pixel_width&&trace.height===canvas.pixel_height,'canvas mismatch');
  const settings=trace.settings;
  requireTrace(settings?.structure_rules===false&&settings.quality_boost===false&&settings.use_deim_detection===true,'raw recognition settings mismatch');
  for(const [key,expected]of [['det_conf_threshold',detection],['min_line_confidence',confidence]]){
    const actual=settings[key];
    requireTrace(Number.isFinite(actual)&&actual>=0&&actual<=1&&Math.fround(actual)===Math.fround(expected),'threshold mismatch');
  }
  const regions=trace.stages?.reading_order;
  requireTrace(Array.isArray(regions),'missing reading_order array');
  requireTrace(typeof text==='string'&&trace.body===text,'body differs from raw TXT');
  const out=regions.map((r,id)=>{
    requireTrace(r!==null&&typeof r==='object'&&typeof r.text==='string','invalid region text');
    requireTrace(Number.isFinite(r.confidence)&&r.confidence>=0&&r.confidence<=1&&typeof r.is_vertical==='boolean','invalid region score/orientation');
    const b=r.bbox;
    requireTrace(Array.isArray(b)&&b.length===4&&b.every(Number.isSafeInteger),'invalid region box');
    const [x0,y0,x1,y1]=b;
    requireTrace(x0>=0&&y0>=0&&x1>x0&&y1>y0&&x1<=trace.width&&y1<=trace.height,'out-of-canvas or degenerate region box');
    return {id,text:r.text,confidence:r.confidence,is_vertical:r.is_vertical,bbox:{pixels:{x0,y0,x1,y1}}};
  });
  requireTrace(out.map(r=>r.text).join('\n')===text,'reading_order differs from raw TXT');
  return out;
}
module.exports={recognitionRegions};
