import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const STORAGE = "lawrenceanum:topic-watch:v1";
const topic = { id: "administrative-welfare", title: "生活保護・行政救済", keywords: ["生活保護"] };
const ids = [97208, 97209, 97210];
const citation = "生活保護法８条２項";
const source = (id: number) => ({
  case_id: id, case_name: `生活保護の比較用判例${id}`, case_number: `試験第${id}号`, judged_on: "2025-06-27", court: "比較用の裁判所",
  result: `比較用の結論${id}`, holding: `判示事項${id}：生活扶助の基準。`, summary: `${citation}に関する裁判要旨${id}。`, referenced_laws: citation,
  judgment_type: "判決", reporter: "", lower_court: "", lower_case_number: "", lower_judged_on: "", detail_url: "", pdf_url: null, body_text: "比較用の判決本文",
});
const feed = { schema_version: 1, topic, generated_at: "2026-10-24T00:00:00Z", count: 3, items: ids.map(id => ({ kind: "hanrei", date: "2025-06-27", title: source(id).case_name, summary: source(id).holding, href: `/hanrei/${id}`, internal: true })) };
const meta = (id: number, stub = false) => ({ path: `hanrei/${id}`, type: "hanrei", title: `判例${id}`, description: `AIの概要${id}`, date: null, tags: [], stub });
const wiki = { schema_version: 1, generated_at: feed.generated_at, pages: [meta(97208), meta(97209, true)] };

async function mock(page: Page) {
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: feed }));
  await page.route("**/wiki/index.json", route => route.fulfill({ json: wiki }));
  await page.route(/\/hanrei\/\d+\.json(?:\?.*)?$/, route => {
    const id = Number(route.request().url().match(/\/(\d+)\.json/)![1]);
    return route.fulfill({ json: source(id) });
  });
  await page.route("**/links/hanrei-to-laws/*.json", route => {
    const id = Number(route.request().url().match(/\/(\d+)\.json/)![1]);
    return route.fulfill({ json: { schema_version: 1, case_id: id, notice: "版候補", wiki_path: `hanrei/${id}`, laws: [], citations: [{ field: "summary", law_id: "325AC0000000144", article: "8", paragraph: 2, start: 0, end: citation.length, text: citation }] } });
  });
  await page.route("**/wiki/page/hanrei/97208.json", route => route.fulfill({ json: {
    path: "hanrei/97208", frontmatter: { type: "hanrei", case_id: 97208, source_fingerprint: "v1", llm_source_fingerprint: "v1", source_scope: "判示事項・裁判要旨" }, body: "",
    hanrei_explanation: `## 要約\n\n出典と照合するLLM解説です[^1]。\n\n[^1]: [hanrei:97208#holding](https://www.courts.go.jp/hanrei/97208/) 「${source(97208).holding}」`,
    hanrei_citations: [{ label: "1", reference: "hanrei:97208#holding", field: "holding", quote: source(97208).holding }], hanrei_related_cases: [],
  } }));
}

async function selectTwo(page: Page) {
  await page.goto(new URL("#/watch", BASE).href);
  const list = page.getByRole("region", { name: "追跡資料一覧" });
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
}

test("2判例の公式要旨・結果・条文候補とLLM解説を同じ画面で確認できる", async ({ page }) => {
  await mock(page); await selectTwo(page);
  const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
  await expect(comparison).toContainText("比較用の結論97208");
  await expect(comparison).toContainText("比較用の結論97209");
  await expect(comparison.getByRole("heading", { name: "裁判所公表の裁判要旨", exact: true })).toHaveCount(2);
  await expect(comparison.getByRole("link", { name: citation, exact: true }).first()).toHaveAttribute("href", "#/hanrei/97208/laws/325AC0000000144/8?paragraph=2");
  await expect(comparison.getByRole("link", { name: "裁判所の原文", exact: true }).first()).toHaveAttribute("href", "https://www.courts.go.jp/hanrei/97208/detail2/index.html");
  await expect(comparison).toContainText("AI概要（LLM生成）");
  await expect(comparison).toContainText("公開済みのAI解説は未収録");
  await expect(comparison).not.toContainText("AIの概要97209");
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("button", { name: "比較に追加", exact: true })).toBeDisabled();
  await comparison.getByRole("button", { name: "AI解説・引用根拠をこの場で読む" }).click();
  await expect(comparison.getByTestId("hanrei-explanation")).toContainText("出典と照合するLLM解説");
  await expect(comparison.getByTestId("hanrei-explanation")).toContainText("LLM（大規模言語モデル）が生成");
  const inline = comparison.getByTestId("hanrei-citation-inline");
  await inline.locator("summary").click();
  await expect(inline.locator("mark")).toHaveText(source(97208).holding);
  await inline.getByRole("button", { name: "判示事項の原文へ移動", exact: true }).click();
  await expect(page).toHaveURL(/#\/hanrei\/97208\?source=holding$/);
  await expect(page.locator("#hanrei-source-holding")).toBeFocused();
});

test("以前の保存状態を維持し、比較対象とメモは一覧から外れた後も再開できる", async ({ page }) => {
  await page.addInitScript(key => {
    if (!localStorage.getItem(key)) localStorage.setItem(key, JSON.stringify({ version: 1, followed: true, read: [], note: "以前からの調査メモ" }));
  }, STORAGE);
  await mock(page); await selectTwo(page);
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("原文確認に12分。処分日と経過措置を比較する。");
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: { ...feed, count: 0, items: [] } }));
  await page.reload();
  await expect(page.getByRole("textbox", { name: /自分の調査メモ/ })).toHaveValue("以前からの調査メモ");
  await expect(page.getByRole("button", { name: "追跡中（解除する）" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("原文確認に12分。処分日と経過措置を比較する。");
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(0);
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("比較用の結論97208");
});

test("比較した2件とメモ・原文・Wikiの出典を出力し、AI本文と未収録Wikiを混ぜない", async ({ page }) => {
  await mock(page); await selectTwo(page);
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("私が確認した相違点");
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("AIの概要97208");
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "比較メモを出力", exact: true }).click();
  const download = await waiting;
  const report = await readFile((await download.path())!, "utf8");
  expect(download.suggestedFilename()).toBe("hanrei-comparison-97208-97209.md");
  expect(report).toContain("裁判要旨97208");
  expect(report).toContain("比較用の結論97209");
  expect(report).toContain("私が確認した相違点");
  expect(report).toContain("https://www.courts.go.jp/hanrei/97208/detail2/index.html");
  expect(report).toContain(`${BASE}#/hanrei/97209?source=summary`);
  expect(report).toContain(`${BASE}#/wiki/hanrei/97208`);
  expect(report).toContain("適用版が確定していない");
  expect(report).toContain("LLMによる要約・解説");
  expect(report).not.toContain("97210");
  expect(report).not.toContain("AIの概要97208");
  expect(report).not.toContain("#/wiki/hanrei/97209");
});

test("別判例の応答や取得失敗を表示せず、再試行後に比較を再開できる", async ({ page }) => {
  await mock(page);
  let fail = true;
  await page.route("**/hanrei/97209.json", route => fail ? route.fulfill({ json: source(97210) }) : route.fulfill({ json: source(97209) }));
  await page.goto(new URL("#/watch", BASE).href);
  const list = page.getByRole("region", { name: "追跡資料一覧" });
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
  await expect(comparison.getByRole("alert")).toContainText("比較対象との対応を確認できません");
  await expect(comparison).not.toContainText("比較用の結論97210");
  await expect(comparison.getByRole("button", { name: "比較メモを出力", exact: true })).toBeDisabled();
  fail = false;
  await comparison.getByRole("button", { name: "判例を再試行", exact: true }).click();
  await expect(comparison.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
  await expect(comparison).toContainText("比較用の結論97209");
});

test("Wikiの取得失敗とAI解説の版不一致を区別して原文比較を継続できる", async ({ page }) => {
  await mock(page);
  let fail = true;
  await page.route("**/wiki/index.json", route => fail ? route.fulfill({ status: 503 }) : route.fulfill({ json: wiki }));
  await page.route("**/wiki/page/hanrei/97208.json", route => route.fulfill({ json: { path: "hanrei/97208", frontmatter: { type: "hanrei", case_id: 97208, source_fingerprint: "new", llm_source_fingerprint: "old" }, hanrei_explanation: "対応していない解説" } }));
  await selectTwo(page);
  const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
  await expect(comparison).toContainText("AI解説の収録状況を確認できません");
  await expect(comparison).not.toContainText("公開済みのAI解説は未収録");
  await expect(comparison).toContainText("比較用の結論97208");
  fail = false;
  await comparison.getByRole("button", { name: "Wikiの確認を再試行" }).click();
  await comparison.getByRole("button", { name: "AI解説・引用根拠をこの場で読む" }).click();
  await expect(comparison.getByRole("alert")).toContainText("資料の版との対応を確認できません");
  await expect(comparison).not.toContainText("対応していない解説");
});

test("スマートフォンでも比較を読めて選択を入れ替えられる", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await mock(page); await selectTwo(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
  await page.getByRole("button", { name: "判例97208を比較から外す", exact: true }).click();
  const list = page.getByRole("region", { name: "追跡資料一覧" });
  await list.getByRole("button", { name: "比較に追加", exact: true }).last().click();
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
  const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
  await expect(comparison).toContainText("比較用の結論97210");
  await expect(comparison).not.toContainText("比較用の結論97208");
  await comparison.evaluate(element => element.scrollIntoView({ block: "start" }));
  await page.screenshot({ path: test.info().outputPath("topic-comparison-mobile.png") });
});
