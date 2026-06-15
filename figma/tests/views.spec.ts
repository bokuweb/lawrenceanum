import { test, expect } from "@playwright/test";

// 検索 / 更新履歴ビューの UI 回帰テスト。fixture 静的サーバを使う。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

// 検索ページの検索ボックスはヘッダー(topbar)に集約され、body には無い。
// 以前は header と body で input が重複していた。
test("search page keeps a single search input (header only, none in body)", async ({ page }) => {
  await page.goto(new URL("#/search", BASE).toString());
  await expect(page.getByRole("heading", { name: "検索" })).toBeVisible({ timeout: 15_000 });

  // ヘッダーの検索 input は 1 つだけ存在する。
  await expect(page.getByPlaceholder(/検索/)).toHaveCount(1);
  // <main> (body) 側には検索 input が無い (Radix Checkbox は <button> なので input ではない)。
  await expect(page.locator("main input")).toHaveCount(0);
});

// サイドバー左下の「最新同期」は health.json の generated_at から動的表示する
// (以前はハードコードで固定だった)。fixture health = 2026-06-14T03:58:50Z → JST 12:58。
test("sidebar last-sync is derived from health.json (not hardcoded)", async ({ page }) => {
  await page.goto(new URL("#/", BASE).toString());
  const aside = page.locator("aside");
  await expect(aside.getByText("最新同期")).toBeVisible({ timeout: 15_000 });
  await expect(aside).toContainText("2026-06-14 12:58 JST", { timeout: 15_000 });
  // 旧ハードコード値が残っていないこと。
  await expect(aside).not.toContainText("2026-05-09 06:30");
});

// 更新履歴は、ロード中に mock 一覧ではなく skeleton を表示する。
test("updates view shows skeleton (not mock) while loading", async ({ page }) => {
  // updates/latest.json を保留 → useUpdatesIndex が await で止まりロード中が続く。
  let release: () => void = () => {};
  const gate = new Promise<void>((res) => (release = res));
  await page.route("**/updates/latest.json", async (route) => {
    await gate;
    await route.continue();
  });

  await page.goto(new URL("#/updates", BASE).toString());
  await expect(page.getByRole("heading", { name: "更新履歴" })).toBeVisible({ timeout: 15_000 });

  // ロード中: skeleton が出ていて「読み込み中…」表示。
  await expect(page.locator('[data-slot="skeleton"]').first()).toBeVisible({ timeout: 10_000 });
  await expect(page.locator("text=読み込み中")).toBeVisible();

  release();
});
