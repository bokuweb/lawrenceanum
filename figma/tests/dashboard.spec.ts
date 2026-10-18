import { test, expect } from "@playwright/test";

// ダッシュボードの横断パネル (wiki / 新着 / 施行予定) と官報タイルの e2e。
// wiki/index.json・feeds/recent.json・enforcement/upcoming.json を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const WIKI_INDEX = {
  schema_version: 1,
  generated_at: "2026-10-17T00:00:00Z",
  pages: [
    { path: "laws/L1", type: "law", title: "予防接種法", description: "副反応救済をめぐる議論", date: null, tags: [] },
    { path: "topics/fukuhannou", type: "topic", title: "副反応救済", description: "", date: "2026-10-02", tags: [] },
    { path: "committees/kousei", type: "committee", title: "厚生科学審議会", description: "", date: "2026-10-10", tags: [] },
    { path: "parties/P1", type: "party", title: "テスト会派", description: "", date: "2026-10-09", tags: [] },
    { path: "people/yamada", type: "person", title: "山田太郎", description: "", date: "2026-10-11", tags: [] },
    { path: "meetings/kokkai/M1", type: "meeting", title: "参議院 厚生労働委員会 第1号", description: "", date: "2026-10-01", tags: [] },
    { path: "index", type: "index", title: "lawrenceanum wiki", description: "", date: null, tags: [] },
    // 要約前のページは件数・一覧から外し、「要約前 n」としてだけ数える。
    { path: "bills/221/B9", type: "bill", title: "要約前の議案", description: "", date: "2026-10-12", tags: [], stub: true },
  ],
};

const FEED = {
  schema_version: 1,
  generated_at: "2026-10-17T00:00:00Z",
  count: 2,
  items: [
    { kind: "pubcomment", date: "2026-10-16", title: "ダッシュボード用パブコメ", href: "/pubcomment/dash-001", internal: true, ministry: "法務省" },
    { kind: "kanpo", date: "2026-10-15", title: "ダッシュボード用官報項目", href: "https://www.kanpo.go.jp/test.pdf", internal: false },
  ],
};

/** 「30 日以内」は実行日基準で数えるので、施行日は今日からの相対で作る。 */
function isoInDays(days: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

const SOON = isoInDays(3);
const LATER = isoInDays(60);

const UPCOMING = {
  schema_version: 1,
  generated_at: "2026-10-17T00:00:00Z",
  as_of: isoInDays(0),
  count: 3,
  items: [
    { date: SOON, law_id: "L1", title: "予防接種法", amending_law_title: "改正法A", date_kind: "scheduled" },
    { date: SOON, law_id: "L2", title: "健康保険法", amending_law_title: "改正法A", date_kind: "scheduled" },
    { date: LATER, law_id: "L3", title: "ずっと先に施行される法", amending_law_title: "改正法B", date_kind: "scheduled" },
  ],
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/wiki/index.json", (route) => route.fulfill({ json: WIKI_INDEX }));
  await page.route("**/feeds/recent.json", (route) => route.fulfill({ json: FEED }));
  await page.route("**/enforcement/upcoming.json", (route) => route.fulfill({ json: UPCOMING }));
}

test("dashboard shows the kanpo corpus tile", async ({ page }) => {
  await page.goto(new URL("#/", BASE).toString());
  const kanpo = page.locator('[data-corpus="kanpo"]');
  await expect(kanpo).toContainText("官報", { timeout: 15_000 });
  await expect(kanpo).toContainText("1 日分");
  await expect(kanpo).toContainText("データ 2026-04-02");
  await expect(page.locator('a[data-corpus="kanpo"]')).toHaveAttribute("href", "#/kanpo");
});

test("dashboard wiki card breaks pages down by OKF type and lists the newest ones", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/", BASE).toString());

  const card = page.getByTestId("dashboard-wiki");
  await expect(card).toBeVisible({ timeout: 15_000 });
  // 要約前 (stub) を除いた 6 ページ。index も OKF type 外なので数えない。
  await expect(card).toContainText("6");
  await expect(card).toContainText("要約前 1");

  // 人物・会議体・会派のページも種別チップとして出る。
  await expect(card.locator('[data-wiki-type="committee"]').first()).toContainText("会議体");
  await expect(card.locator('[data-wiki-type="party"]').first()).toContainText("会派");
  await expect(card.locator('[data-wiki-type="person"]').first()).toContainText("人物");

  // 直近に書かれたページは日付降順。要約前のページ (2026-10-12) は出さない。
  const links = card.locator('a[href^="#/wiki/"]');
  await expect(links.first()).toHaveAttribute("href", "#/wiki/people/yamada");
  await expect(card).not.toContainText("要約前の議案");

  await links.first().click();
  await expect(page).toHaveURL(/#\/wiki\/people\/yamada$/);
});

test("dashboard feed card links internal items in-app and kanpo PDFs out", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/", BASE).toString());

  const card = page.getByTestId("dashboard-feed");
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(card).toContainText("ダッシュボード用パブコメ");
  await expect(card.locator('a[href="#/pubcomment/dash-001"]')).toHaveCount(1);
  const external = card.locator('a[href="https://www.kanpo.go.jp/test.pdf"]');
  await expect(external).toHaveAttribute("target", "_blank");
});

test("dashboard enforcement card counts the next 30 days and groups by date", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/", BASE).toString());

  const card = page.getByTestId("dashboard-enforcement");
  await expect(card).toBeVisible({ timeout: 15_000 });
  // 60 日後の 1 件は 30 日以内に数えないが、全件には含める。
  await expect(card).toContainText("2");
  await expect(card).toContainText("30 日以内");
  await expect(card).toContainText("全 3 件");
  await expect(card).toContainText(SOON);
  await expect(card).toContainText("2 件");
});
