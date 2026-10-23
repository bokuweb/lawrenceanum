'use strict';
function engineName(options) {
  const name=options.engine??'ndlocr';
  if(!['ndlocr','tesseract'].includes(name))throw new Error('Unsupported OCR engine');
  if(name==='tesseract'&&['det-conf-threshold','min-line-confidence'].some(key=>options[key]!==undefined))throw new Error('NDLOCR thresholds require --engine ndlocr');
  return name;
}
async function run(options,runtime) {
  const name=engineName(options);
  if(runtime)return runtime[name](options);
  return require(name==='ndlocr'?'./ndlocr.cjs':'./sidecar.cjs').run(options);
}
module.exports={engineName,run};
