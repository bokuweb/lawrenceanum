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

// 検索クエリに法律 term が含まれると、シソーラスの別表記を「同義語も検索」として表示する
// (検索は自動でそれらも OR 検索する)。ヒントはクライアント側計算で search.db 不要。
test("search shows thesaurus synonyms for a legal term query", async ({ page }) => {
  await page.goto(new URL("#/search?q=バーゼル規制", BASE).toString());
  await expect(page.getByRole("heading", { name: "検索" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("同義語も検索:")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("BIS規制", { exact: true })).toBeVisible();
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

test("dashboard shows corpus counts, freshness, and missing indexes", async ({ page }) => {
  await page.goto(new URL("#/", BASE).toString());
  await expect(page.getByRole("heading", { name: "コーパス収録状況" })).toBeVisible({ timeout: 15_000 });

  const proceedings = page.locator('[data-corpus="proceedings"]');
  await expect(proceedings).toContainText("国会会議録");
  await expect(proceedings).toContainText("903 会議");
  await expect(proceedings).toContainText("データ 2026-07-24");
  await expect(proceedings).toContainText("収集成功 2026-08-11");

  const pubcomment = page.locator('[data-corpus="pubcomment"]');
  await expect(pubcomment).toContainText("44 件");
  await expect(pubcomment).toContainText("データ 2026-08-09");
  await expect(pubcomment).toContainText("収集失敗 2026-08-11");

  const procurement = page.locator('[data-corpus="procurement"]');
  await expect(procurement).toContainText("政府調達");
  await expect(procurement).toContainText("未配信");
  await expect(procurement).toContainText("収集失敗 2026-08-11");
});

test("dashboard corpus tiles link to each corpus list (unavailable ones stay inert)", async ({ page }) => {
  await page.goto(new URL("#/", BASE).toString());
  await expect(page.getByRole("heading", { name: "コーパス収録状況" })).toBeVisible({ timeout: 15_000 });

  await expect(page.locator('a[data-corpus="proceedings"]')).toHaveAttribute("href", "#/proceedings");
  await expect(page.locator('a[data-corpus="shingikai"]')).toHaveAttribute("href", "#/shingikai");
  await expect(page.locator('a[data-corpus="gian"]')).toHaveAttribute("href", "#/gian");
  await expect(page.locator('a[data-corpus="reiki"]')).toHaveAttribute("href", "#/reiki");
  // 未配信 (index なし) のコーパスはリンクにしない。
  await expect(page.locator('a[data-corpus="procurement"]')).toHaveCount(0);
  await expect(page.locator('div[data-corpus="procurement"]')).toBeVisible();

  await page.locator('a[data-corpus="proceedings"]').click();
  await expect(page).toHaveURL(/#\/proceedings$/);
});

// 更新トレンドは日ごとの件数を法令種別 (law_id の種別コード) で積み上げ、
// hover でその日の種別・変更種別の内訳と法令名を出す。
test("dashboard update trend breaks each day down by law kind", async ({ page }) => {
  const today = new Date().toISOString().slice(0, 10);
  const law = (law_id: string, title: string, change_type: string) =>
    ({ law_id, title, change_type, current: `laws/${law_id}/current.json` });
  await page.route("**/updates/*.json", async (route) => {
    if (!route.request().url().endsWith(`/updates/${today}.json`)) {
      return route.fulfill({ status: 404, body: "" });
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({
        date: today,
        updated_laws: [
          law("211AC0000000070", "健康保険法", "modified"),
          law("322CO0000000016", "地方自治法施行令", "modified"),
          law("323CO0000000201", "テスト政令", "added"),
          law("427M60000002001", "テスト府省令", "modified"),
          law("119IO0000000000", "テスト勅令", "removed"),
        ],
      }),
    });
  });

  await page.goto(new URL("#/", BASE).toString());
  const card = page.getByTestId("update-breakdown");
  await expect(card).toBeVisible({ timeout: 15_000 });

  const legend = page.getByTestId("update-breakdown-legend");
  await expect(legend).toContainText("法律1");
  await expect(legend).toContainText("政令2");
  await expect(legend).toContainText("府省令1");
  await expect(legend).toContainText("その他1");

  // 当日 (右端) の棒は 4 段積み。hover すると内訳 tooltip が出る。
  const segments = card.locator(".recharts-bar-rectangle path");
  await expect(segments).toHaveCount(4);
  await segments.last().hover();
  const tooltip = card.locator(".recharts-tooltip-wrapper");
  await expect(tooltip).toContainText(today.slice(5));
  await expect(tooltip).toContainText("5 件");
  await expect(tooltip).toContainText("改正 3");
  await expect(tooltip).toContainText("追加 1");
  await expect(tooltip).toContainText("廃止 1");
  await expect(tooltip).toContainText("健康保険法");
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
