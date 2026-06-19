import { defineConfig } from "@playwright/test";

// 履歴ビューア (compare view) の self-contained e2e。
// 本番 Pages を叩く smoke.spec とは分離し、history.spec だけを
// ローカルでビルドした fixture (履歴束つき) に対して実行する。
const PORT = Number(process.env.PORT ?? 8799);
const HOST = process.env.HOST ?? "127.0.0.1";
const BASE = `http://${HOST}:${PORT}/`;

export default defineConfig({
  testDir: "./tests",
  testMatch: /(history|browse|views|pubcomment|kanpo|feed|enforcement|gian|tsutatsu)\.spec\.ts/,
  use: {
    headless: true,
    viewport: { width: 1280, height: 800 },
    baseURL: BASE,
  },
  reporter: [["list"], ["html", { open: "never" }]],
  // SPA をビルドして fixture を重ね、依存ゼロの静的サーバで配信する。
  // vite build を含むので timeout は長めに取る。
  webServer: {
    command: "node tests/serve.mjs",
    url: BASE,
    timeout: 180_000,
    reuseExistingServer: !process.env.CI,
    stdout: "pipe",
    stderr: "pipe",
    env: {
      PORT: String(PORT),
      HOST,
    },
  },
});
