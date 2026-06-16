import { defineConfig } from 'vite'
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
    /^\/(laws|updates|kanpo|schema|proceedings|links|pubcomment|procurement|shingikai|budget|reiki)\//.test(url)

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

export default defineConfig({
  plugins: [
    figmaAssetResolver(),
    lawpubJsonDevServer(),
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

  // File types to support raw imports. Never add .css, .tsx, or .ts files to this.
  assetsInclude: ['**/*.svg', '**/*.csv'],
})
