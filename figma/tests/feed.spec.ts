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

test("判例を絞り込み、裁判所の判示事項とサイト内リンクを表示する", async ({ page }) => {
  const judgment = { kind: "hanrei", date: "2026-06-19", title: "生活扶助基準の事件", href: "/hanrei/97208", internal: true, summary: "生活保護法の基準に関する判示事項" };
  await page.route("**/feeds/recent.json", route => route.fulfill({ json: { ...FEED, count: 8, items: [judgment, ...FEED.items] } }));
  await page.goto(new URL("#/feed", BASE).href);
  await page.getByLabel("フィルタ-判例").click();
  const list = page.getByRole("region", { name: "新着一覧", exact: true });
  await expect(list.getByRole("link")).toHaveCount(1);
  await expect(list.getByRole("link")).toHaveAttribute("href", "#/hanrei/97208");
  await expect(list).toContainText("裁判所公表の判示事項");
});

test("新着フィードが横断アイテムとRSS購読を表示する", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());

  await expect(page.getByRole("heading", { name: /規制変化フィード/ })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("link", { name: /RSS購読/ })).toBeVisible();

  const latest = page.getByRole("region", { name: "種類別の最新" });
  const list = page.getByRole("region", { name: "新着一覧", exact: true });
  await expect(latest.getByRole("link")).toHaveCount(7);
  for (const item of FEED.items) {
    await expect(latest.getByText(item.title)).toBeVisible();
    await expect(list.getByText(item.title)).toHaveCount(1);
  }
  await expect(list.getByRole("link").first()).toContainText("テスト民法改正パブコメ");
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
  await expect(list.getByRole("link")).toHaveCount(1);
  await page.getByLabel("フィルタ-政府調達").click();
  await expect(list.getByRole("link")).toHaveCount(2);
  await expect(list.getByText("テスト情報システム調達")).toBeVisible();
  await page.getByRole("button", { name: "クリア", exact: true }).click();
  await expect(list.getByRole("link")).toHaveCount(8);
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

test("フィード取得失敗から再試行でき、選択中のフィルタを維持する", async ({ page }) => {
  let fail = true;
  await page.route("**/feeds/recent.json", route => fail
    ? route.fulfill({ status: 503, body: "unavailable" })
    : route.fulfill({ json: FEED }));
  await page.goto(new URL("#/feed", BASE).toString());
  await expect(page.getByRole("alert")).toContainText("フィードを読み込めませんでした");
  await page.getByLabel("フィルタ-政府調達").click();
  fail = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  const list = page.getByRole("region", { name: "新着一覧", exact: true });
  await expect(list.getByText("テスト情報システム調達")).toBeVisible();
  await expect(list.getByRole("link")).toHaveCount(1);
  await expect(page.getByLabel("フィルタ-政府調達")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("最新カードと新着一覧がコピー可能な内部・外部リンクを持つ", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  for (const name of ["種類別の最新", "新着一覧"]) {
    const region = page.getByRole("region", { name, exact: true });
    await expect(region.getByRole("link", { name: /テスト改正法/ })).toHaveAttribute("href", "#/laws/TESTLAW1");
    const external = region.getByRole("link", { name: /テスト法案（審議トラッキング）/ });
    await expect(external).toHaveAttribute("href", FEED.items[0].href);
    await expect(external).toHaveAttribute("target", "_blank");
    await expect(external).toHaveAttribute("rel", /noopener/);
  }
});

test("官報の対象法令の修飾クリックを妨げず元の画面を保つ", async ({ page }) => {
  await mockFeed(page);
  await page.goto(new URL("#/feed", BASE).toString());
  // document まで到達した時点の状態を記録し、テスト中のタブ生成だけを抑える。
  const prevented = await page.getByRole("link", { name: "テスト対象法", exact: true }).evaluate(element => {
    let prevented = false;
    document.addEventListener("click", event => {
      prevented = event.defaultPrevented;
      event.preventDefault();
    }, { once: true });
    element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, metaKey: true }));
    return prevented;
  });
  expect(prevented).toBe(false);
  await expect(page).toHaveURL(/#\/feed$/);
});
