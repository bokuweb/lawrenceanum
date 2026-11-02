// Test the production Vite bundle at a subpath with real OCR and inspect its DOCX.
import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, sep, extname } from 'node:path';
import { inflateRawSync } from 'node:zlib';
const root = fileURLToPath(new URL('../', import.meta.url));
const publicDir = resolve(root, '../public');
const modelDir = resolve(process.env.NDLOCR_MODEL_DIR || resolve(root, '../target/ndlocr-models'));
const prefix = '/ocr-test/';
const mime = {'.js':'text/javascript', '.mjs':'text/javascript', '.html':'text/html', '.wasm':'application/wasm', '.css':'text/css', '.json':'application/json'};
const server = createServer(async (req, res) => {
  try {
    const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    if (!path.startsWith(prefix)) {res.writeHead(404).end(); return;}
    const relative = path.slice(prefix.length);
    const models = relative.startsWith('ndlocr-models/');
    const base = models ? modelDir : publicDir;
    const file = resolve(base, models ? relative.slice('ndlocr-models/'.length) : relative || 'index.html');
    if (!file.startsWith(base + sep) || !(await stat(file)).isFile()) {res.writeHead(404).end(); return;}
    res.writeHead(200, {'Content-Type':mime[extname(file)] || 'application/octet-stream', 'Cross-Origin-Opener-Policy':'same-origin', 'Cross-Origin-Embedder-Policy':'credentialless'});
    res.end(await readFile(file));
  } catch {res.writeHead(404).end();}
});
function zipEntry(zip, name) {
  const end = zip.lastIndexOf(Buffer.from([0x50,0x4b,0x05,0x06]));
  let p = zip.readUInt32LE(end + 16);
  for (let n = zip.readUInt16LE(end + 10); n > 0; n--) {
    const method = zip.readUInt16LE(p + 10), size = zip.readUInt32LE(p + 20);
    const nameLen = zip.readUInt16LE(p + 28), extra = zip.readUInt16LE(p + 30), comment = zip.readUInt16LE(p + 32), local = zip.readUInt32LE(p + 42);
    if (zip.toString('utf8', p + 46, p + 46 + nameLen) === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return (method === 8 ? inflateRawSync(data) : data).toString('utf8');
    }
    p += 46 + nameLen + extra + comment;
  }
  throw new Error(`${name} not found`);
}
let browser;
try {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  browser = await chromium.launch({headless:true});
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.address().port}${prefix}#/kanpo-docx`);
  await page.getByRole('heading', {name:'官報 PDF → DOCX'}).waitFor();
  await page.waitForFunction(() => !document.querySelector('input[type=file]')?.disabled);
  assert.equal(await page.evaluate(() => crossOriginIsolated), true);
  assert.equal(await page.getByRole('checkbox').isEnabled(), true);
  assert.equal(await page.getByRole('checkbox').isChecked(), true);
  await page.locator('input[type=file]').setInputFiles(fileURLToPath(new URL('../tests/fixtures/kanpo-docx-image.pdf', import.meta.url)));
  const link = page.getByRole('link', {name:'ダウンロード'});
  await link.waitFor({timeout:180000});
  const [download] = await Promise.all([page.waitForEvent('download'), link.click()]);
  const xml = zipEntry(await readFile(await download.path()), 'word/document.xml');
  const text = xml.replace(/<[^>]+>/g, '');
  assert.ok(text.includes('株式会社楽一産業'));
  assert.ok(text.includes('破産管財人'));
  assert.ok(text.length > 2000, 'image page must yield a substantial OCR body');
  assert.match(await page.locator('main').innerText(), /画像 1 枚、OCR 1 枚/);
  assert.deepEqual(errors, []);
  console.log(`Production subpath PDF → OCR → DOCX passed (${text.length} text characters)`);
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
