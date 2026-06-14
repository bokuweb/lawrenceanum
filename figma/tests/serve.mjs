// 履歴 e2e 用の self-contained 静的サーバ。
//
//   1. SPA を `tests/.serve` にビルド (未ビルド or REBUILD=1 のとき)
//   2. `tests/fixtures/public` の小さな fixture データを上にコピー
//      (index.json / health.json / laws/index.json / 民法の versions.json と
//       history.ndjson.zst)
//   3. その合成ディレクトリを依存ゼロの http サーバで配信
//
// これ 1 つで「ビルド済み SPA + 履歴束」を CI でもローカルでも同条件で配信できる。
// Playwright の webServer から起動する想定 (playwright.history.config.ts)。
import { createServer } from 'node:http'
import { spawnSync } from 'node:child_process'
import { createReadStream, existsSync, statSync, cpSync } from 'node:fs'
import { extname, join, resolve, normalize } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = fileURLToPath(new URL('.', import.meta.url))
const figmaRoot = resolve(here, '..')
const serveDir = resolve(figmaRoot, 'tests/.serve')
const fixtureDir = resolve(figmaRoot, 'tests/fixtures/public')
const port = Number(process.env.PORT ?? 8799)
const host = process.env.HOST ?? '127.0.0.1'

function buildIfNeeded() {
  const built = existsSync(join(serveDir, 'index.html'))
  if (built && process.env.REBUILD !== '1') {
    console.log(`[serve] reuse existing build at ${serveDir} (REBUILD=1 to force)`)
    return
  }
  console.log('[serve] building SPA into tests/.serve ...')
  // vite.config の outDir は ../public 固定なので CLI で上書きする。
  // .serve はソース外なので --emptyOutDir でクリーンビルドして問題ない。
  const npx = process.platform === 'win32' ? 'npx.cmd' : 'npx'
  const r = spawnSync(npx, ['vite', 'build', '--outDir', serveDir, '--emptyOutDir'], {
    cwd: figmaRoot,
    stdio: 'inherit',
  })
  if (r.status !== 0) {
    console.error('[serve] vite build failed')
    process.exit(r.status ?? 1)
  }
}

function overlayFixtures() {
  console.log('[serve] overlaying fixture data from tests/fixtures/public')
  cpSync(fixtureDir, serveDir, { recursive: true })
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.ico': 'image/x-icon',
  '.wasm': 'application/wasm',
  '.zst': 'application/zstd',
  '.map': 'application/json',
  '.txt': 'text/plain; charset=utf-8',
  '.xml': 'application/xml; charset=utf-8',
}

function serve() {
  const server = createServer((req, res) => {
    let urlPath = decodeURIComponent((req.url ?? '/').split('?')[0])
    if (urlPath === '/') urlPath = '/index.html'
    // パストラバーサル対策。
    const filePath = normalize(join(serveDir, urlPath))
    if (!filePath.startsWith(serveDir)) {
      res.statusCode = 403
      res.end('Forbidden')
      return
    }
    const exists = existsSync(filePath) && statSync(filePath).isFile()
    // HashRouter なので実体のないパスは index.html を返す (SPA fallback)。
    const target = exists ? filePath : join(serveDir, 'index.html')
    res.setHeader('Content-Type', MIME[extname(target)] ?? 'application/octet-stream')
    res.setHeader('Cache-Control', 'no-store')
    createReadStream(target).pipe(res)
  })
  server.listen(port, host, () => {
    console.log(`[serve] listening on http://${host}:${port}/ (root: ${serveDir})`)
  })
}

buildIfNeeded()
overlayFixtures()
serve()
