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

function publishedExplanation() {
  return {
    path: "hanrei/92002", frontmatter: {
      type: "hanrei", case_id: 92002, source_fingerprint: "v1", llm_source_fingerprint: "v1",
      source_scope: "判示事項・裁判要旨（判決全文未収録）", summary_input: { total_chars: 50000, included_chars: 36000 },
    },
    body: "## 裁判所の公開情報（原文）\n\n公式原文の区間はカードに複製しない。",
    hanrei_explanation: "## 要約\n\n生活扶助基準の改定について判断した[^1]。\n\n## 判断と理由\n\n基準の改定理由を検討した[^1]。\n\n[関連法令](../laws/325AC0000000144.md)\n\n[^1]: [hanrei:92002#holding](https://www.courts.go.jp/hanrei/92002/) 「生活扶助基準の改定について判断した。」\n",
  };
}

test("判例本文の前に公開Wikiの解説・引用・入力範囲を表示し、Wikiと行き来できる", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: publishedExplanation() }));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const card = page.getByTestId("hanrei-explanation");
  await expect(card).toContainText("生活扶助基準の改定について判断した");
  await expect(card).toContainText("判決全文未収録");
  await expect(card).toContainText("50,000 文字中 36,000 文字");
  await expect(card).toContainText("抜粋外の理由・留保・反対意見は未確認");
  await expect(card).not.toContainText("公式原文の区間はカードに複製しない");
  await expect(card.getByRole("heading", { name: "出典", exact: true })).toBeVisible();
  await expect(card.getByRole("link", { name: "hanrei:92002#holding", exact: true })).toHaveAttribute("href", "https://www.courts.go.jp/hanrei/92002/");
  await card.locator("a[data-footnote-ref]").first().click();
  await expect(page).toHaveURL(/#\/hanrei\/92002$/);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await card.getByRole("link", { name: "関連法令", exact: true }).click();
  await expect(page).toHaveURL(/#\/wiki\/laws\/325AC0000000144$/);
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  await page.getByRole("link", { name: "解説・関連判例をWikiで読む", exact: true }).click();
  await expect(page).toHaveURL(/#\/wiki\/hanrei\/92002$/);
  await page.getByRole("link", { name: "判例本文・公式要旨を読む", exact: true }).click();
  await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機");
});

function citation(field: "holding" | "summary" | "body_text", quote: string, label = "1") {
  return { label, field, quote, reference: `hanrei:92002#${field === "body_text" ? "body" : field}` };
}

test("AIの引用を判示事項・裁判要旨・本文で照合し、原文区間へ移動できる", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route("**/hanrei/92002.json", r => r.fulfill({ json: {
    ...fixture("92002"), holding, summary: "前の記述。原告の請求を認めた。後の記述。",
  } }));
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: {
    ...publishedExplanation(), hanrei_citations: [
      citation("holding", holding), citation("summary", "原告の請求を認めた。", "2"),
      citation("body_text", "電子計算機の利用と立証責任", "3"),
      { ...citation("holding", holding, "other"), reference: "hanrei:92001#holding" },
    ],
  } }));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const context = page.getByTestId("hanrei-citation-context");
  await context.getByText("引用箇所を原文で確認（3件）", { exact: true }).click();
  await expect(context.locator("mark")).toHaveText([holding, "原告の請求を認めた。", "電子計算機の利用と立証責任"]);
  await expect(context).toContainText("前の記述。原告の請求を認めた。後の記述。");
  for (const [label, field] of [["判示事項", "holding"], ["裁判要旨", "summary"], ["本文（PDF抽出）", "body_text"]]) {
    await context.getByRole("button", { name: `${label}の原文へ移動`, exact: true }).click();
    await expect(page.locator(`#hanrei-source-${field}`)).toBeFocused();
    await expect(page).toHaveURL(/#\/hanrei\/92002$/);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

test("更新された原文や未収録の本文に引用を無理に対応付けない", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route("**/hanrei/92002.json", r => r.fulfill({ json: { ...fixture("92002"), holding, body_text: null } }));
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: {
    ...publishedExplanation(), hanrei_citations: [
      citation("holding", "保存時の判示事項の引用"), citation("body_text", "保存時の判決本文の引用", "2"),
    ],
  } }));
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const context = page.getByTestId("hanrei-citation-context");
  await context.getByText("引用箇所を原文で確認（2件）", { exact: true }).click();
  await expect(context).toContainText("保存時の判示事項の引用");
  await expect(context).toContainText("保存時の判決本文の引用");
  await expect(context.getByText(/配信中の原文には完全一致する箇所がありません/)).toHaveCount(2);
  await expect(context.locator("mark")).toHaveCount(0);
  await expect(context.getByRole("button")).toHaveCount(0);
  await expect(page.getByTestId("hanrei-explanation").getByRole("link", { name: "hanrei:92002#holding", exact: true })).toBeVisible();
});

test("引用データのない旧配信でも解説の脚注を読める", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: publishedExplanation() }));
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  await expect(page.getByTestId("hanrei-explanation").getByRole("heading", { name: "出典", exact: true })).toBeVisible();
  await expect(page.getByTestId("hanrei-citation-context")).toHaveCount(0);
});

function relatedCase() {
  return {
    path: "hanrei/92001", case_id: 92001, title: "旧データの行政事件",
    judged_on: "令和7年1月15日", court: "東京地方裁判所", description: "行政処分の基準について検討した。",
    common_articles: [{ law_id: lawId, title: "生活保護法", article: "8" }],
  };
}

async function mockRelatedCases(page: import("@playwright/test").Page, cases: ReturnType<typeof relatedCase>[]) {
  await mockCaseLinks(page);
  await page.route("**/wiki/index.json", r => r.fulfill({ json: { schema_version: 1, pages: [
    { path: "hanrei/92002" }, { path: "hanrei/92001" }, { path: `laws/${lawId}` },
  ] } }));
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: { ...publishedExplanation(), hanrei_related_cases: cases } }));
}

test("共通条文を持つ判例の概要・裁判日・裁判所を表示し、本文・Wiki・事件の条文候補をたどれる", async ({ page }) => {
  await mockRelatedCases(page, [relatedCase()]);
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: historicalLaw() }));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const related = page.getByTestId("hanrei-related-cases");
  await expect(related).toContainText("令和7年1月15日・東京地方裁判所");
  await expect(related).toContainText("AIによる概要：行政処分の基準について検討した。");
  await expect(related).toContainText("先例関係・同じ争点や結論・適用法令の版の一致を示すものではありません。");
  await expect(related.getByRole("link", { name: "旧データの行政事件のWiki", exact: true })).toHaveAttribute("href", "#/wiki/hanrei/92001");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await related.getByRole("link", { name: "生活保護法 第8条", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`#/hanrei/92002/laws/${lawId}/8$`));
  await expect(page.getByTestId("case-law-article")).toContainText("過去版の第2項の本文");
  await page.getByRole("link", { name: "判例に戻る", exact: true }).click();
  await related.getByRole("link", { name: "旧データの行政事件", exact: true }).click();
  await expect(page).toHaveURL(/#\/hanrei\/92001$/);
  await expect(page.getByRole("heading", { name: "旧データの行政事件", exact: true })).toBeVisible();
  await expect(page.getByTestId("hanrei-explanation")).not.toContainText("生活扶助基準の改定について判断した");
  await expect(related).toHaveCount(0);
});

test("公開索引にない関連判例・自分自身・事件IDと異なるパスは表示せず、欠落メタデータを明示する", async ({ page }) => {
  const valid = { ...relatedCase(), judged_on: "", court: "" };
  await mockRelatedCases(page, [valid,
    { ...relatedCase(), path: "hanrei/92002", case_id: 92002, title: "自分自身" },
    { ...relatedCase(), path: "hanrei/99999", case_id: 99999, title: "公開対象から外れた判例" },
    { ...relatedCase(), path: "hanrei/92001", case_id: 92003, title: "事件ID不一致" },
  ]);
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const related = page.getByTestId("hanrei-related-cases");
  await expect(related.getByRole("listitem")).toHaveCount(1);
  await expect(related).toContainText("裁判日未記録・裁判所未記録");
  await expect(related).not.toContainText("自分自身");
  await expect(related).not.toContainText("公開対象から外れた判例");
  await expect(related).not.toContainText("事件ID不一致");
});

test("解説取得の失敗から再試行でき、判例本文は引き続き読める", async ({ page }) => {
  await mockCaseLinks(page);
  let fail = true;
  await page.route("**/wiki/page/hanrei/92002.json", r => fail ? r.fulfill({ status: 503 }) : r.fulfill({ json: publishedExplanation() }));
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  const card = page.getByTestId("hanrei-explanation");
  await expect(card.getByRole("alert")).toBeVisible();
  await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機");
  fail = false;
  await card.getByRole("button", { name: "解説を再試行", exact: true }).click();
  await expect(card).toContainText("生活扶助基準の改定について判断した");
  await expect(card.getByRole("alert")).toHaveCount(0);
});

for (const mismatch of ["case", "source", "stub"] as const) {
  test(`対応を確認できない解説を本文に表示しない: ${mismatch}`, async ({ page }) => {
    await mockCaseLinks(page);
    const wiki = publishedExplanation();
    if (mismatch === "case") wiki.frontmatter.case_id = 92001;
    if (mismatch === "source") wiki.frontmatter.llm_source_fingerprint = "old";
    await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: { ...wiki, stub: mismatch === "stub" } }));
    await page.goto(new URL("#/hanrei/92002", BASE).toString());
    const card = page.getByTestId("hanrei-explanation");
    await expect(card.getByRole("alert")).toContainText("対応を確認できませんでした");
    await expect(card).not.toContainText("生活扶助基準の改定について判断した");
    await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機");
  });
}

test("未公開の解説は取得せず、旧配信形式ではWikiへの導線を表示する", async ({ page }) => {
  await mockCaseLinks(page);
  let requests = 0;
  const wiki = publishedExplanation();
  await page.route("**/wiki/page/hanrei/92002.json", r => { requests++; return r.fulfill({ json: { ...wiki, hanrei_explanation: undefined } }); });
  await page.route("**/wiki/index.json", r => r.fulfill({ json: { schema_version: 1, pages: [] } }));
  await page.goto(new URL("#/hanrei/92002", BASE).toString());
  await expect(page.getByText("判例wikiの公開をまだ確認できません。", { exact: true })).toBeVisible();
  await expect(page.getByTestId("hanrei-explanation")).toHaveCount(0);
  expect(requests).toBe(0);
  await page.unroute("**/wiki/index.json");
  await page.route("**/wiki/index.json", r => r.fulfill({ json: { schema_version: 1, pages: [{ path: wiki.path }] } }));
  await page.reload();
  await expect(page.getByTestId("hanrei-explanation")).toContainText("この配信版の解説は判例Wikiで確認できます。");
});

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

test("サイト内で候補を切り替え、版と項の指定を維持する", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: historicalLaw() }));
  await page.goto(new URL(`#/hanrei/92002/laws/${lawId}/8?paragraph=2`, BASE).toString());
  await expect(page.locator('a[href*="laws.e-gov.go.jp"]')).toHaveCount(0);
  await expect(page.getByTestId("case-law-article")).toBeVisible();
  await page.getByRole("combobox", { name: "表示する版候補" }).selectOption(missingRevision);
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
  expect(fetched).toEqual([]);
});

test("判例wikiの版候補リンクからサイト内の過去版条文を開く", async ({ page }) => {
  await mockCaseLinks(page);
  await page.route("**/wiki/page/hanrei/92002.json", r => r.fulfill({ json: {
    path: "hanrei/92002", frontmatter: { type: "hanrei", title: "判例wiki" },
    body: `[条番号 8・版候補 ${oldRevision}](#/hanrei/92002/laws/${lawId}/8?revision=${oldRevision})`,
  } }));
  await page.route(`**/laws/${lawId}/revisions/${oldRevision}.json`, r => r.fulfill({ json: historicalLaw() }));
  await page.goto(new URL("#/wiki/hanrei/92002", BASE).toString());
  const link = page.getByRole("link", { name: `条番号 8・版候補 ${oldRevision}`, exact: true });
  await expect(link).toHaveAttribute("href", `#/hanrei/92002/laws/${lawId}/8?revision=${oldRevision}`);
  await link.click();
  await expect(page).toHaveURL(new URL(`#/hanrei/92002/laws/${lawId}/8?revision=${oldRevision}`, BASE).toString());
  await expect(page.getByTestId("case-law-article")).toContainText("過去版の第2項の本文");
});
