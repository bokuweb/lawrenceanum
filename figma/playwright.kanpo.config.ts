import { defineConfig } from "@playwright/test";

// 実際の corpus PDF と WASM で、PDF と解析結果の重なりを確認する。
const PORT = 8800;
const BASE = `http://127.0.0.1:${PORT}/`;

export default defineConfig({
  testDir: "./tests",
  testMatch: "kanpo-playground.spec.ts",
  use: { headless: true, viewport: { width: 1280, height: 900 }, baseURL: BASE },
  reporter: [["list"]],
  webServer: {
    command: "pnpm test:amend-document && pnpm build:wasm && node tests/serve.mjs",
    url: BASE,
    timeout: 180_000,
    reuseExistingServer: !process.env.CI,
    env: { PORT: String(PORT), REBUILD: "1" },
  },
});
