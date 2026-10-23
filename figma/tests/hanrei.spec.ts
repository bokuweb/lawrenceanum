import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const fixture = (name: string) => JSON.parse(readFileSync(new URL(`./fixtures/public/hanrei/${name}.json`, import.meta.url), "utf8"));

test("左メニューの判例から一覧・絞り込み・詳細本文を開いて一覧に戻れる", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("navigation").getByRole("link", { name: "判例", exact: true }).click();
  await expect(page.getByRole("heading", { name: "判例", exact: true })).toBeVisible();
  await expect(page.getByText("2 / 2件")).toBeVisible();
  await page.getByRole("textbox", { name: "判例一覧の絞り込み" }).fill("損害賠償");
  await expect(page.getByText("1 / 2件")).toBeVisible();
  await expect(page.getByRole("link", { name: /旧データの行政事件/ })).toHaveCount(0);
  await page.getByRole("link", { name: /検索テストの損害賠償請求事件/ }).click();
  await expect(page).toHaveURL(/#\/hanrei\/92002\?filter=/);
  await expect(page.getByRole("heading", { name: "本文", exact: true })).toBeVisible();
  await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機の利用と立証責任");
  await expect(page.getByTestId("hanrei-body")).toContainText('<script>alert("本文")</script>');
  await expect(page.getByTestId("hanrei-body").locator("script")).toHaveCount(0);
  await expect(page.getByText("東京高等裁判所", { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveAttribute("href", fixture("92002").pdf_url);
  await page.getByRole("link", { name: "判例一覧に戻る" }).click();
  await expect(page.getByRole("textbox", { name: "判例一覧の絞り込み" })).toHaveValue("損害賠償");
});

test("裁判所で絞り込み、旧データでは本文未取得を案内する", async ({ page }) => {
  await page.goto(new URL("#/hanrei", BASE).toString());
  await page.getByRole("combobox", { name: "裁判所", exact: true }).click();
  await page.getByRole("option", { name: "東京地方裁判所", exact: true }).click();
  await expect(page.getByText("1 / 2件")).toBeVisible();
  await page.getByRole("link", { name: /旧データの行政事件/ }).click();
  await expect(page.getByTestId("hanrei-body")).toContainText("本文テキストは未取得です。");
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveCount(0);
});

for (const resource of ["index", "92002"]) {
  test(`判例${resource === "index" ? "一覧" : "詳細"}の読み込み失敗から再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(`**/hanrei/${resource}.json`, r => fail
      ? r.fulfill({ status: 503, body: "unavailable" })
      : r.fulfill({ json: fixture(resource) }));
    await page.goto(new URL(`#/hanrei${resource === "index" ? "" : "/92002"}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByRole("alert")).toHaveCount(0);
    if (resource === "index") await expect(page.getByText("2 / 2件")).toBeVisible();
    else await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機");
  });
}

test("本文だけの検索語でヒットし、該当する本文の抜粋から詳細へ移動できる", async ({ page }) => {
  await page.route("**/search.db", r => r.continue({ url: new URL("hanrei-search.db", BASE).toString() }));
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await page.getByRole("checkbox", { name: "判例", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("電子計算機");
  const hits = page.getByTestId("hanrei-hits");
  await expect(hits).toContainText("判例 (1件)", { timeout: 20_000 });
  await expect(hits.getByTestId("hanrei-body-excerpt")).toContainText("本文（PDFから抽出・抜粋）");
  await expect(hits.locator("mark")).toHaveText("電子計算機");
  await expect(hits.getByTestId("hanrei-body-excerpt")).toContainText("立証責任");
  await hits.getByRole("link", { name: "検索テストの損害賠償請求事件", exact: true }).click();
  await expect(page).toHaveURL(/#\/hanrei\/92002$/);
  await expect(page.getByTestId("hanrei-body")).toContainText("本件上告を棄却する。");
});

test("スマートフォンで判例一覧と本文が画面幅に収まる", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/hanrei", BASE).toString());
  await page.getByRole("link", { name: /検索テストの損害賠償請求事件/ }).click();
  const body = page.getByTestId("hanrei-body");
  await expect(body).toContainText("電子計算機");
  const box = await body.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(390);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

const lawId = "325AC0000000144";
const oldRevision = `${lawId}_20140101_A`;
const missingRevision = `${lawId}_20150101_B`;
const holding = "生活保護法３条、８条２項";
function caseLinks() {
  return {
    schema_version: 1, case_id: 92002, wiki_path: "hanrei/92002",
    notice: "条文への言及を機械抽出した版候補です。経過措置・実際の適用法条は原文で確認してください。過去版を現行条文で代用しません。",
    citations: [
      { field: "holding", law_id: lawId, article: "3", paragraph: null, start: 0, end: 7, text: "生活保護法３条" },
      { field: "holding", law_id: lawId, article: "8", paragraph: 2, start: 8, end: 12, text: "８条２項" },
    ],
    laws: [{ law_id: lawId, title: "生活保護法", articles: ["3", "8"], wiki_path: `laws/${lawId}`, basis: "event_date_candidate", evidence: [{ from: "2013-01-01", to: "2015-12-31", text: "平成２５年から平成２７年にかけて本件改定" }], incomplete: true,
      versions: [{ revision_id: oldRevision, effective_date: "2014-01-01", body_available: true }, { revision_id: missingRevision, effective_date: "2015-01-01", body_available: false }] }],
  };
}
const historicalLaw = () => ({ law_id: lawId, revision_id: oldRevision, articles: [{ article_id: "art_8", article_no: "第八条", caption: "（基準及び程度の原則）", paragraphs: [ { paragraph_num: "1", paragraph_no: null, text: "過去版の第1項" }, { paragraph_num: "2", paragraph_no: "２", text: "過去版の第2項の本文" } ] }] });

async function mockCaseLinks(page: import("@playwright/test").Page) {
  await page.route("**/hanrei/92002.json", r => r.fulfill({ json: { ...fixture("92002"), holding } }));
  await page.route("**/links/hanrei-to-laws/92002.json", r => r.fulfill({ json: caseLinks() }));
  await page.route("**/wiki/index.json", r => r.fulfill({ json: { schema_version: 1, pages: [{ path: "hanrei/92002" }, { path: `laws/${lawId}` }] } }));
}

test("省略された法令名・項をリンクし、版候補・根拠と過去版本文、wikiの紐付けを表示する", async ({ page }) => {
  await mockCaseLinks(page);
  let currentRequests = 0;
  await page.route(`**/laws/${lawId}/current.json`, r => { currentRequests++; return r.fulfill({ json: {} }); });
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: historicalLaw() }));
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  await expect(page.getByRole("link", { name: "判例のLLM wikiを開く" })).toHaveAttribute("href", "#/wiki/hanrei/92002");
  await expect(page.getByRole("link", { name: "関連法令のwikiを開く" })).toHaveAttribute("href", `#/wiki/laws/${lawId}`);
  await page.getByRole("link", { name: "８条２項", exact: true }).click();
  await expect(page).toHaveURL(/paragraph=2/);
  await expect(page.getByText("本文の出来事の日付による候補", { exact: true })).toBeVisible();
  await expect(page.getByText(/平成２５年から平成２７年/)).toBeVisible();
  await expect(page.getByTestId("case-law-article").locator('[data-highlighted="true"]')).toHaveText("２ 過去版の第2項の本文");
  await expect(page.getByText(/表示版:/)).toContainText(oldRevision);
  await expect(page.getByRole("link", { name: "この版候補をe-Govで確認" })).toHaveAttribute("href", `https://laws.e-gov.go.jp/law/${lawId}/20140101_A`);
  await expect(page.getByRole("link", { name: "裁判所の原文ページ", exact: true })).toHaveAttribute("href", fixture("92002").detail_url);
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveAttribute("href", fixture("92002").pdf_url);
  expect(currentRequests).toBe(0);
  await page.getByRole("link", { name: "判例に戻る", exact: true }).click();
  await expect(page.getByRole("heading", { name: "検索テストの損害賠償請求事件" })).toBeVisible();
});

test("候補の本文が未収録なら現行版や別の過去版を取得しない", async ({ page }) => {
  await mockCaseLinks(page);
  const fetched: string[] = [];
  await page.route(`**/laws/${lawId}/**`, r => { fetched.push(r.request().url()); return r.fulfill({ json: {} }); });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL(`#/hanrei/92002/laws/${lawId}/8?revision=${missingRevision}&paragraph=2`, BASE).toString());
  await expect(page.getByText("この過去版の本文は未収録です。現行条文での代用は行っていません。", { exact: true })).toBeVisible();
  await expect(page.getByText(/複数の版が候補/)).toBeVisible();
  await expect(page.getByRole("link", { name: "この版候補をe-Govで確認" })).toHaveAttribute("href", `https://laws.e-gov.go.jp/law/${lawId}/20150101_B`);
  expect(fetched).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

test("配信された本文の版が候補と異なる場合は引用せず再試行できる", async ({ page }) => {
  await mockCaseLinks(page);
  let fail = true;
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: { ...historicalLaw(), revision_id: fail ? "current" : oldRevision } }));
  await page.goto(new URL(`#/hanrei/92002/laws/${lawId}/8`, BASE).toString());
  await expect(page.getByRole("alert")).toContainText("過去版本文を読み込めませんでした");
  await expect(page.getByTestId("case-law-article")).toHaveCount(0);
  fail = false;
  await page.getByRole("button", { name: "本文を再試行" }).click();
  await expect(page.getByTestId("case-law-article")).toContainText("過去版の第2項");
});

test("候補の切り替えに合わせて公式リンクを更新し、項の指定を維持する", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: historicalLaw() }));
  await page.goto(new URL(`#/hanrei/92002/laws/${lawId}/8?paragraph=2`, BASE).toString());
  await expect(page.getByTestId("case-law-article")).toBeVisible();
  await page.getByRole("combobox", { name: "表示する版候補" }).selectOption(missingRevision);
  await expect(page.getByRole("link", { name: "この版候補をe-Govで確認" })).toHaveAttribute("href", `https://laws.e-gov.go.jp/law/${lawId}/20150101_B`);
  await expect(page.getByTestId("case-law-article")).toHaveCount(0);
  await expect(page).toHaveURL(/paragraph=2&revision=/);
  await page.getByRole("combobox", { name: "表示する版候補" }).selectOption(oldRevision);
  await expect(page.getByTestId("case-law-article").locator('[data-highlighted="true"]')).toContainText("過去版の第2項");
});

test("過去版を特定できなくても裁判所の原文とPDF、wikiを確認できる", async ({ page }) => {
  await mockCaseLinks(page);
  const links = caseLinks();
  links.laws[0].versions = [];
  links.laws[0].basis = "unresolved";
  await page.route("**/links/hanrei-to-laws/92002.json", r => r.fulfill({ json: links }));
  const fetched: string[] = [];
  await page.route(`**/laws/${lawId}/**`, r => { fetched.push(r.request().url()); return r.fulfill({ json: {} }); });
  await page.goto(new URL(`#/hanrei/92002/laws/${lawId}/8`, BASE).toString());
  await expect(page.getByText("版を特定できません", { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "裁判所の原文ページ", exact: true })).toHaveAttribute("href", fixture("92002").detail_url);
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveAttribute("href", fixture("92002").pdf_url);
  await expect(page.getByRole("link", { name: "判例のLLM wiki", exact: true })).toHaveAttribute("href", "#/wiki/hanrei/92002");
  await expect(page.getByRole("link", { name: "この版候補をe-Govで確認" })).toHaveCount(0);
  expect(fetched).toEqual([]);
});
