'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {engineName,run}=require('./engine.cjs');
test('NDLOCR is the default; Tesseract requires explicit selection',()=>{
  assert.equal(engineName({}),'ndlocr');
  assert.equal(engineName({engine:'tesseract'}),'tesseract');
  assert.equal(engineName({engine:'ndlocr'}),'ndlocr');
  assert.throws(()=>engineName({engine:'other'}),/Unsupported/);
  assert.throws(()=>engineName({engine:'tesseract','det-conf-threshold':'0.9'}),/require --engine ndlocr/);
});
test('NDLOCR failure never falls back to Tesseract',async()=>{
  let legacy=0;
  await assert.rejects(run({engine:'ndlocr'}, {ndlocr:async()=>{throw new Error('NDLOCR failed');},tesseract:async()=>{legacy++;}}),/NDLOCR failed/);
  assert.equal(legacy,0);
});
