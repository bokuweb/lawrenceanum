import { test, expect } from "@playwright/test";

// 履歴束 (history.ndjson.zst) を使った「版閲覧＋任意2版 diff」の e2e。
// 既定はローカル静的サーバ (fixture public)。CI も同じ BASE を webServer で立てる。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
// 複数版を持つ法令 (民法)。fixture/本番いずれにも履歴束がある想定。
const LAW = process.env.HISTORY_LAW ?? "129AC0000000089";

test("compare view loads the zstd history bundle and diffs two revisions", async ({ page }) => {
  // 履歴束が実際に fetch されることをネットワークで確認する。
  const bundle = page.waitForResponse(
    (r) => r.url().includes(`/laws/${LAW}/history.ndjson.zst`) && r.status() === 200,
    { timeout: 30_000 },
  );

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());

  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  await bundle; // 束 (zstd) を取得できた = fzstd 復号経路が走る

  // モックフォールバックではない (= live 履歴を束から復号できている)。
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);

  // 実データの条数で比較している (民法は 1000 条超)。
  // 初期表示は mock の数条 → 束 (zstd) 展開後に実データへ更新される。
  // 更新前に assert すると落ちるので、3 桁以上の条数になるまでリトライ待ちする。
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });

  // diff サマリ (追加/変更/削除) のバッジが出ている。
  await expect(page.locator("text=/追加 \\d+/")).toBeVisible();
  await expect(page.locator("text=/変更 \\d+/")).toBeVisible();
  await expect(page.locator("text=/削除 \\d+/")).toBeVisible();
});
