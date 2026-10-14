// Build and serve the SPA with VITE_SEARCH_DB_URL / VITE_REIKI_SEARCH_DB_URL first.
// Usage: node scripts/benchmark-search.mjs BASE_URL OUTPUT_JSON [RUNS=3] [READ_DELAY_MS=200] [MAIN_DB_FILE]
// Each run uses a fresh browser context. The optional delay applies to every DB read.
import { chromium } from '@playwright/test';
import { open, stat, writeFile } from 'node:fs/promises';
const [base, output, runs = '3', delay = '200', mainDbFile] = process.argv.slice(2);
if (!base || !output || !Number.isInteger(Number(runs)) || Number(runs) < 1 || !Number.isFinite(Number(delay)) || Number(delay) < 0) {
  throw new Error('Expected BASE_URL OUTPUT_JSON [RUNS=3] [READ_DELAY_MS=200] [MAIN_DB_FILE]');
}
// A local copy keeps the dataset fixed and removes R2 latency from comparisons.
const db = mainDbFile ? await open(mainDbFile, 'r') : undefined;
const dbSize = mainDbFile ? (await stat(mainDbFile)).size : undefined;
const browser = await chromium.launch();
const results = [];
try {
  for (let run = 0; run < Number(runs); run++) {
    const context = await browser.newContext({viewport:{width:1280,height:800}});
    const page = await context.newPage();
    const totals = {requests:0, completed:0, bytes:0};
    await page.route('**/*search.db', async route => {
      totals.requests++;
      try {
        let response;
        let body;
        let headers;
        if (db && new URL(route.request().url()).pathname.endsWith('/search.db')) {
          const head = route.request().method() === 'HEAD';
          const range = /^bytes=(\d+)-(\d+)$/.exec(route.request().headers().range ?? '');
          if (!head && !range) throw new Error('Expected a bounded Range request');
          const start = head ? 0 : Number(range[1]);
          const end = head ? -1 : Number(range[2]);
          if (start < 0 || end >= dbSize) throw new Error('Range exceeds DB size');
          body = Buffer.alloc(head ? 0 : end - start + 1);
          if (!head) {
            const {bytesRead} = await db.read(body, 0, body.length, start);
            if (bytesRead !== body.length) throw new Error('Incomplete DB read');
          }
          headers = {'content-length':String(head ? dbSize : body.length), 'accept-ranges':'bytes',
            'content-type':'application/x-sqlite3', 'access-control-expose-headers':'content-length,content-range,accept-ranges',
            ...(!head ? {'content-range':`bytes ${start}-${end}/${dbSize}`} : {})};
        } else {
          response = await route.fetch({timeout:60000});
          body = await response.body();
          headers = response.headers();
        }
        if (Number(delay)) await new Promise(resolve => setTimeout(resolve, Number(delay)));
        totals.completed++;
        totals.bytes += body.length;
        await route.fulfill({...(response ? {response} : {status:route.request().method() === 'HEAD' ? 200 : 206}), body,
          headers:{...headers,'access-control-allow-origin':'*'}});
      } catch { await route.abort().catch(()=>{}); }
    });
    const start = performance.now();
    await page.goto(`${base}#/search?q=${encodeURIComponent('労働')}`);
    await page.getByRole('tab', {name:/^法令・条文 \d+件$/}).waitFor({timeout:70000});
    const first = {ms: Math.round(performance.now()-start), ...totals, lawTab:await page.getByRole('tab',{name:/^法令・条文/}).innerText()};
    await page.getByRole('status').waitFor({state:'hidden',timeout:90000});
    const all = {ms:Math.round(performance.now()-start),...totals,tabs:await page.getByRole('tablist').innerText()};
    const errors = await page.getByRole('tab', {name:/エラー$/}).allTextContents();
    if (errors.length) throw new Error(`Search failed: ${errors.join(', ')}`);
    results.push({run:run+1,first,all});
    console.log(JSON.stringify(results.at(-1)));
    await context.close();
  }
  await writeFile(output, JSON.stringify({base,delay:Number(delay),mainDbFile,results},null,2));
} finally { await browser.close(); await db?.close(); }
