import { test, expect } from "@playwright/test";

// 新着 — 規制変化フィード (FeedView) の e2e。feeds/recent.json を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const FEED = {
  schema_version: 1,
  generated_at: "2026-06-19T00:00:00Z",
  count: 3,
  items: [
    {
      kind: "pubcomment",
      date: "2026-06-19",
      title: "テスト民法改正パブコメ",
      href: "/pubcomment/test-001",
      internal: true,
      ministry: "法務省",
      summary: "関連: 民法",
    },
    {
      kind: "law",
      date: "2026-06-10",
      title: "テスト改正法",
      href: "/laws/TESTLAW1",
      internal: true,
      law_id: "TESTLAW1",
      summary: "改正",
    },
    {
      kind: "kanpo",
      date: "2026-06-05",
      title: "ある省令の一部を改正する省令",
      href: "https://www.kanpo.go.jp/x/y.pdf",
      internal: false,
      ministry: "総務一",
      summary: "官報",
    },
  ],
};

async function mockFeed(page: import("@playwright/test").Page) {
  await page.route("**/feeds/recent.json", (route) => route.fulfill({ json: FEED }));
}

test("新着フィードが横断アイテムとRSS購読を表示する", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());

  await expect(page.getByRole("heading", { name: /規制変化フィード/ })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("link", { name: /RSS購読/ })).toBeVisible();

  // 3 種別のアイテムが出る。
  await expect(page.getByText("テスト民法改正パブコメ")).toBeVisible();
  await expect(page.getByText("テスト改正法")).toBeVisible();
  await expect(page.getByText("ある省令の一部を改正する省令")).toBeVisible();
});

test("種別フィルタで官報だけに絞れる", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  await expect(page.getByText("テスト改正法")).toBeVisible({ timeout: 15_000 });

  // 「官報」フィルタチップを押す (aria-label で一意化)。
  await page.getByLabel("フィルタ-官報").click();

  await expect(page.getByText("ある省令の一部を改正する省令")).toBeVisible();
  await expect(page.getByText("テスト改正法")).toHaveCount(0);
  await expect(page.getByText("テスト民法改正パブコメ")).toHaveCount(0);
});

test("内部アイテムをクリックすると該当ページへ遷移する", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());

  await page.getByText("テスト改正法").click();
  await expect(page).toHaveURL(/#\/laws\/TESTLAW1/, { timeout: 15_000 });
});
