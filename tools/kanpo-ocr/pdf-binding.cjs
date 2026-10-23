'use strict';
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {projectLayout}=require('./placement.cjs');
const {pngSize}=require('./sidecar.cjs');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function bindProjection(observations,entry,directory){
  if(Object.hasOwn(observations,'pdf_source_evidence'))throw new Error('Existing source evidence must not be overwritten');
  const receiptBytes=fs.readFileSync(path.join(directory,entry.receipt));
  if(hash(receiptBytes)!==entry.receipt_sha256)throw new Error('Decoded source receipt hash mismatch');
  const receipt=JSON.parse(receiptBytes);
  const pdf=fs.readFileSync(path.join(directory,'source.pdf'));
  if(hash(pdf)!==receipt.document_sha256)throw new Error('Original PDF snapshot hash mismatch');
  const placementBytes=fs.readFileSync(path.join(directory,entry.placement));
  if(hash(placementBytes)!==receipt.placement_sha256)throw new Error('Decoded source placement hash mismatch');
  const frame=JSON.parse(placementBytes);
  const png=fs.readFileSync(path.join(directory,entry.PNG)),size=pngSize(png),digest=hash(png);
  if(digest!==receipt.PNG_sha256)throw new Error('Decoded source PNG hash mismatch');
  if(observations.source?.image?.sha256!==digest||observations.source.image.bytes!==png.length)throw new Error('Actual recognition PNG does not match decoded source');
  if(size.width!==frame.pixel_width||size.height!==frame.pixel_height)throw new Error('Decoded PNG canvas does not match source placement');
  const out=projectLayout(observations,frame);
  // These are checks of the producer's saved artifacts, not a rendering attestation.
  // Preserve declared_unverified/visibility and all original OCR fields.
  out.pdf_source_evidence={status:'producer_hashes_checked',receipt,receipt_sha256:entry.receipt_sha256,placement_sha256:receipt.placement_sha256,tool_sha256:hash(fs.readFileSync(__filename)),geometry_module_sha256:hash(fs.readFileSync(path.join(__dirname,'placement.cjs')))};
  return out;
}
module.exports={bindProjection};
