import { test, expect } from "@playwright/test";

// 新着 — 規制変化フィード (FeedView) の e2e。feeds/recent.json を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const FEED = {
  schema_version: 1,
  generated_at: "2026-06-19T00:00:00Z",
  count: 7,
  items: [
    {
      kind: "bill",
      date: "2026-06-12",
      title: "テスト法案（審議トラッキング）",
      href: "https://www.shugiin.go.jp/keika/TEST.htm",
      internal: false,
      summary: "衆法 · 委員会付託(衆) · テスト特別委員会",
    },
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
      kind: "shingikai",
      date: "2026-06-18",
      title: "テスト審議会の議事録",
      href: "/shingikai/moj/test-meeting",
      internal: true,
      ministry: "法務省",
    },
    {
      kind: "proceedings",
      date: "2026-06-17",
      title: "衆議院 · テスト委員会 · 第1号",
      href: "/proceedings/test-meeting",
      internal: true,
    },
    {
      kind: "procurement",
      date: "2026-06-16",
      title: "テスト情報システム調達",
      href: "/procurement/test-notice",
      internal: true,
      ministry: "総務省",
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
      law_id: "TESTLAW2",
      law_title: "テスト対象法",
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

  const latest = page.getByRole("region", { name: "種類別の最新" });
  const list = page.getByRole("region", { name: "新着一覧", exact: true });
  await expect(latest.getByRole("button")).toHaveCount(7);
  for (const item of FEED.items) {
    await expect(latest.getByText(item.title)).toBeVisible();
    await expect(list.getByText(item.title)).toHaveCount(1);
  }
  await expect(list.getByRole("button").first()).toContainText("テスト民法改正パブコメ");
});

test("法案フィルタで法案だけに絞れる", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  await expect(page.getByRole("region", { name: "種類別の最新" })).toBeVisible({ timeout: 15_000 });

  await page.getByLabel("フィルタ-法案").click();
  await expect(page.getByText("テスト法案（審議トラッキング）")).toBeVisible();
  await expect(page.getByText("テスト改正法")).toHaveCount(0);
  await expect(page.getByText("テスト民法改正パブコメ")).toHaveCount(0);
});

test("種別フィルタで官報だけに絞れる", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  await expect(page.getByRole("region", { name: "種類別の最新" })).toBeVisible({ timeout: 15_000 });

  // 「官報」フィルタチップを押す (aria-label で一意化)。
  await page.getByLabel("フィルタ-官報").click();

  await expect(page.getByText("ある省令の一部を改正する省令")).toBeVisible();
  await expect(page.getByText("テスト改正法")).toHaveCount(0);
  await expect(page.getByText("テスト民法改正パブコメ")).toHaveCount(0);
});

test("内部アイテムをクリックすると該当ページへ遷移する", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());

  await page.getByRole("region", { name: "新着一覧", exact: true }).getByText("テスト改正法").click();
  await expect(page).toHaveURL(/#\/laws\/TESTLAW1/, { timeout: 15_000 });
});

test("追加した種別は複数選択でき、クリアで全種類に戻る", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  const shingikai = page.getByLabel("フィルタ-審議会");
  await shingikai.click();
  await expect(shingikai).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("region", { name: "種類別の最新" })).toHaveCount(0);
  const list = page.getByRole("region", { name: "新着一覧", exact: true });
  await expect(list.getByRole("button")).toHaveCount(1);
  await page.getByLabel("フィルタ-政府調達").click();
  await expect(list.getByRole("button")).toHaveCount(2);
  await expect(list.getByText("テスト情報システム調達")).toBeVisible();
  await page.getByRole("button", { name: "クリア", exact: true }).click();
  await expect(list.getByRole("button")).toHaveCount(7);
  await expect(page.getByRole("region", { name: "種類別の最新" })).toBeVisible();
});

for (const [title, href] of [
  ["テスト審議会の議事録", "/shingikai/moj/test-meeting"],
  ["衆議院 · テスト委員会 · 第1号", "/proceedings/test-meeting"],
  ["テスト情報システム調達", "/procurement/test-notice"],
]) {
  test(`種類別の最新から詳細に遷移: ${href}`, async ({ page }) => {
    await mockFeed(page);
    await page.goto(new URL("#/feed", BASE).toString());
    await page.getByRole("region", { name: "種類別の最新" }).getByText(title, { exact: true }).click();
    await expect(page).toHaveURL(new URL(`#${href}`, BASE).toString());
  });
}

test("狭い画面でも種類別の最新とフィルタが収まる", async ({ page }) => {
  await page.setViewportSize({ width: 768, height: 844 });
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  await expect(page.getByRole("region", { name: "種類別の最新" })).toBeVisible();
  await expect(page.getByLabel("フィルタ-政府調達")).toBeVisible();
  const widths = await page.evaluate(() => ({ content: document.documentElement.scrollWidth, viewport: window.innerWidth }));
  expect(widths.content).toBeLessThanOrEqual(widths.viewport);
});

test("官報項目の逆引きチップから改正対象法令へ遷移する", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());

  const lawChip = page.getByRole("link", { name: "テスト対象法" });
  await expect(lawChip).toBeVisible({ timeout: 15_000 });
  await lawChip.click();
  await expect(page).toHaveURL(/#\/laws\/TESTLAW2/, { timeout: 15_000 });
});
