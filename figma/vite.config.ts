import { defineConfig, searchForWorkspaceRoot } from 'vite'
import path from 'path'
import fs from 'fs'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'


function figmaAssetResolver() {
  return {
    name: 'figma-asset-resolver',
    resolveId(id) {
      if (id.startsWith('figma:asset/')) {
        const filename = id.replace('figma:asset/', '')
        return path.resolve(__dirname, 'src/assets', filename)
      }
    },
  }
}

/**
 * Dev-only middleware that serves `lawpub` の生成済 JSON (`../public/`) を
 * `/index.json` `/laws/...` `/updates/...` `/kanpo/...` `/schema/...` `/health.json`
 * `/manifest.json` で配信する。本番ビルドはどのみち同じ `../public/` に書き出す
 * のでこの middleware は不要。
 */
function lawpubJsonDevServer() {
  const publicRoot = path.resolve(__dirname, '../public')
  const matches = (url: string) =>
    /^\/(index|manifest|health)\.json(\?|$)/.test(url) ||
    /^\/(laws|updates|kanpo|schema|proceedings|links|pubcomment|procurement|shingikai|budget|reiki|feeds|enforcement|gian|tsutatsu|hanrei|wiki)\//.test(url)

  return {
    name: 'lawpub-json-dev-server',
    configureServer(server: any) {
      server.middlewares.use((req: any, res: any, next: any) => {
        const url = (req.url || '').split('?')[0]
        if (!matches(url)) return next()
        const file = path.join(publicRoot, url)
        if (!file.startsWith(publicRoot)) return next() // path traversal guard
        if (!fs.existsSync(file) || !fs.statSync(file).isFile()) return next()
        res.setHeader('Content-Type', 'application/json; charset=utf-8')
        res.setHeader('Cache-Control', 'no-store')
        fs.createReadStream(file).pipe(res)
      })
    },
  }
}

/**
 * Dev-only middleware that serves `../public/search.db` with HTTP Range support.
 * sql.js-httpvfs (`serverMode: "full"`) は byte-range で SQLite ページを取得するため
 * Range 必須。本番は VITE_SEARCH_DB_URL (R2) を使うので dev のみ。
 */
function lawpubSearchDbDevServer() {
  const publicRoot = path.resolve(__dirname, '../public')
  return {
    name: 'lawpub-searchdb-dev-server',
    configureServer(server: any) {
      server.middlewares.use((req: any, res: any, next: any) => {
        const url = (req.url || '').split('?')[0]
        if (url !== '/search.db') return next()
        const file = path.join(publicRoot, 'search.db')
        if (!fs.existsSync(file) || !fs.statSync(file).isFile()) return next()
        const size = fs.statSync(file).size
        res.setHeader('Accept-Ranges', 'bytes')
        res.setHeader('Content-Type', 'application/octet-stream')
        res.setHeader('Cache-Control', 'no-store')
        const range = req.headers['range']
        const m = range && /^bytes=(\d*)-(\d*)$/.exec(range)
        if (m) {
          let start = m[1] === '' ? undefined : parseInt(m[1], 10)
          let end = m[2] === '' ? undefined : parseInt(m[2], 10)
          if (start === undefined) { start = size - (end as number); end = size - 1 }
          else if (end === undefined) { end = size - 1 }
          if (start < 0 || end >= size || start > end) {
            res.statusCode = 416
            res.setHeader('Content-Range', `bytes */${size}`)
            return res.end()
          }
          res.statusCode = 206
          res.setHeader('Content-Range', `bytes ${start}-${end}/${size}`)
          res.setHeader('Content-Length', String(end - start + 1))
          return fs.createReadStream(file, { start, end }).pipe(res)
        }
        res.setHeader('Content-Length', String(size))
        fs.createReadStream(file).pipe(res)
      })
    },
  }
}

// cross-origin isolated にする service worker（src/coi/coi-serviceworker.js）を、サイトの直下
// （/coi-serviceworker.js）に出し、開発サーバーでも同じ場所で配る。官報 PDF → DOCX のブラウザ OCR 用。
function coiServiceWorker() {
  const source = path.resolve(__dirname, 'src/coi/coi-serviceworker.js')
  return {
    name: 'coi-serviceworker',
    configureServer(server: { middlewares: { use: (fn: (req: { url?: string }, res: any, next: () => void) => void) => void } }) {
      server.middlewares.use((req, res, next) => {
        if (req.url?.split('?')[0] !== '/coi-serviceworker.js') return next()
        res.setHeader('Content-Type', 'text/javascript')
        fs.createReadStream(source).pipe(res)
      })
    },
    generateBundle(this: { emitFile: (f: { type: 'asset'; fileName: string; source: string }) => void }) {
      this.emitFile({ type: 'asset', fileName: 'coi-serviceworker.js', source: fs.readFileSync(source, 'utf8') })
    },
  }
}

export default defineConfig({
  plugins: [
    figmaAssetResolver(),
    coiServiceWorker(),
    lawpubJsonDevServer(),
    lawpubSearchDbDevServer(),
    // The React and Tailwind plugins are both required for Make, even if
    // Tailwind is not being actively used – do not remove them
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      // Alias @ to the src directory
      '@': path.resolve(__dirname, './src'),
    },
  },

  // 相対パスでアセット参照することで GitHub Pages の任意サブパス配信に対応する。
  base: './',

  build: {
    // ビルド出力は `lawpub` が生成する JSON と同じ `public/` に統合する。
    // emptyOutDir=false で JSON を保護。publicDir=false で再帰コピーを防止。
    outDir: path.resolve(__dirname, '../public'),
    emptyOutDir: false,
    sourcemap: false,
    // recharts は単体で大きいので警告を 700KB に緩和。
    chunkSizeWarningLimit: 700,
    // 重量級ライブラリは vendor チャンクへ分離する。
    rollupOptions: {
      output: {
        manualChunks: {
          radix: [
            '@radix-ui/react-tabs',
            '@radix-ui/react-select',
            '@radix-ui/react-tooltip',
            '@radix-ui/react-dialog',
            '@radix-ui/react-popover',
            '@radix-ui/react-dropdown-menu',
            '@radix-ui/react-checkbox',
            '@radix-ui/react-scroll-area',
            '@radix-ui/react-switch',
            '@radix-ui/react-slot',
          ],
          recharts: ['recharts'],
          icons: ['lucide-react'],
        },
      },
    },
  },
  publicDir: false,

  server: {
    fs: {
      // 官報パーサ playground はリポジトリ直下の回帰テスト用コーパス
      // (crates/kanpo-parser/tests/corpus) を import.meta.glob で取り込むので、dev でも配信を許す。
      allow: [searchForWorkspaceRoot(process.cwd()), path.resolve(__dirname, '../crates/kanpo-parser/tests/corpus')],
    },
  },

  // File types to support raw imports. Never add .css, .tsx, or .ts files to this.
  assetsInclude: ['**/*.svg', '**/*.csv'],
})
