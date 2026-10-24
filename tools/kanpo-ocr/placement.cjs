'use strict';
// Geometry only. Supplied PDF/image provenance and final visibility are unverified.
const matrixPoint=(m,[x,y])=>[m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]];
function matrix(m) {
  if(!Array.isArray(m)||m.length!==6||!Array.from(m).every(Number.isFinite))throw new Error('Expected a finite six-element affine matrix');
  const det=m[0]*m[3]-m[1]*m[2];
  if(!Number.isFinite(det)||det===0)throw new Error('Expected a representable nonsingular affine matrix');
}
function validatePlacement(f) {
  for(const k of ['pixel_width','pixel_height','image_width','image_height'])if(!Number.isSafeInteger(f?.[k])||f[k]<=0)throw new Error(`Expected positive integer ${k}`);
  matrix(f.recognition_to_image);matrix(f.image_to_user);
  const b=f.crop_box;
  if(!Array.isArray(b)||b.length!==4||!b.every(Number.isFinite)||!(b[0]<b[2]&&b[1]<b[3]))throw new Error('Expected an ordered finite CropBox');
  const w=b[2]-b[0],h=b[3]-b[1];
  if(!Number.isFinite(w)||!Number.isFinite(h))throw new Error('Unrepresentable CropBox dimensions');
  if(![0,90,180,270].includes(f.rotate))throw new Error('Expected canonical PDF Rotate0/90/180/270');
  return f.rotate%180===90?[h,w]:[w,h];
}
function device([x,y],f) {
  const [x0,y0,x1,y1]=f.crop_box;
  switch(f.rotate){
    case 90:return [y-y0,x-x0];
    case 180:return [x1-x,y-y0];
    case 270:return [y1-y,x1-x];
    default:return [x-x0,y1-y];
  }
}
function area(poly) {
  if(poly.length<3)return 0;
  const [x,y]=poly[0];let sum=0;
  for(let i=1;i+1<poly.length;i++)sum+=(poly[i][0]-x)*(poly[i+1][1]-y)-(poly[i][1]-y)*(poly[i+1][0]-x);
  return Math.abs(sum)/2;
}
function relation(poly,[x0,y0,x1,y1]) {
  if(poly.every(([x,y])=>x>=x0&&x<=x1&&y>=y0&&y<=y1))return 'inside';
  let clipped=poly;
  for(const [axis,bound,lower]of [[0,x0,true],[0,x1,false],[1,y0,true],[1,y1,false]]){
    const input=clipped;clipped=[];
    if(!input.length)break;
    const inside=p=>lower?p[axis]>=bound:p[axis]<=bound;
    let previous=input.at(-1);
    for(const current of input){
      const a=inside(previous),b=inside(current);
      if(a!==b){
        const t=(bound-previous[axis])/(current[axis]-previous[axis]);
        const p=[previous[0]+t*(current[0]-previous[0]),previous[1]+t*(current[1]-previous[1])];p[axis]=bound;clipped.push(p);
      }
      if(b)clipped.push(current);
      previous=current;
    }
  }
  if(!clipped.length)return 'outside';
  return area(clipped)>0?'partial':'touching';
}
function projector(frame) {
  const f=structuredClone(frame),[width,height]=validatePlacement(f);
  return box=>{
    const pixels=structuredClone(box??null);
    const invalid=reason=>({pixels,status:'invalid',reason,quad_points:null,bounds_points:null,visibility:'unverified',source_binding:'declared_unverified'});
    if(!box||!['x0','y0','x1','y1'].every(k=>Number.isSafeInteger(box[k]))||box.x0<0||box.y0<0||box.x1<=box.x0||box.y1<=box.y0||box.x1>f.pixel_width||box.y1>f.pixel_height)return invalid('invalid_recognition_box');
    const corners=[[box.x0,box.y0],[box.x1,box.y0],[box.x1,box.y1],[box.x0,box.y1]];
    const image=corners.map(p=>matrixPoint(f.recognition_to_image,p));
    const user=image.map(([x,y])=>matrixPoint(f.image_to_user,[x/f.image_width,(f.image_height-y)/f.image_height]));
    const quad=user.map(p=>device(p,f));
    if([...image,...user,...quad].some(p=>!p.every(Number.isFinite)))return invalid('numeric_overflow');
    const imageArea=area(image),pageArea=area(quad);
    if(!Number.isFinite(imageArea)||!Number.isFinite(pageArea))return invalid('numeric_overflow');
    if(imageArea===0||pageArea===0)return invalid('point_precision_loss');
    const xs=quad.map(p=>p[0]),ys=quad.map(p=>p[1]);
    return {pixels,status:'projected',quad_points:quad,bounds_points:[Math.min(...xs),Math.min(...ys),Math.max(...xs),Math.max(...ys)],source_image_relation:relation(image,[0,0,f.image_width,f.image_height]),page_relation:relation(quad,[0,0,width,height]),visibility:'unverified',source_binding:'declared_unverified'};
  };
}
function projectBox(box,frame){return projector(frame)(box);}
function projectLayout(observations,frame) {
  const project=projector(frame);
  if(observations.kind!=='unclassified_ocr'||observations.article_assignment!==null||observations.native_glyph_binding!==null)throw new Error('Expected unclassified OCR without Article/native Glyph assignments');
  if(observations.canvas?.pixel_width!==frame.pixel_width||observations.canvas?.pixel_height!==frame.pixel_height)throw new Error('Recognition canvas does not match placement');
  for(const key of ['declared_placement','visibility','source_binding','projection'])if(Object.hasOwn(observations,key))throw new Error('Existing root projection metadata must not be overwritten');
  const out=structuredClone(observations);
  function visit(node,key,next){
    if(Object.hasOwn(node,'projection'))throw new Error('Existing projection must not be overwritten');
    node.projection=project(node.bbox?.pixels);
    if(key){if(!Array.isArray(node[key]))throw new Error(`Missing OCR ${key}`);for(const child of node[key])visit(child,next,key==='paragraphs'?'words':null);}
  }
  if(!Array.isArray(out.blocks))throw new Error('Expected OCR block array');
  for(const b of out.blocks)visit(b,'paragraphs','lines');
  if(Object.hasOwn(out,'recognition_regions')){
    if(!Array.isArray(out.recognition_regions))throw new Error('Expected recognition region array');
    for(const region of out.recognition_regions)visit(region,null,null);
  }
  out.kind='unclassified_ocr_projection';out.declared_placement=structuredClone(frame);out.visibility='unverified';out.source_binding='declared_unverified';
  return out;
}
module.exports={validatePlacement,projectBox,projectLayout};
