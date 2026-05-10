import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  use: { headless: true, viewport: { width: 1280, height: 800 } },
  reporter: [["list"]],
});
