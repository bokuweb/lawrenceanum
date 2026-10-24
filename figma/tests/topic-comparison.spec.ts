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
const wiki = { schema_version: 1, generated_at: feed.generated_at, pages: [meta(97208), meta(97209, true), meta(97210)] };

async function mock(page: Page) {
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: feed }));
  await page.route("**/wiki/index.json", route => route.fulfill({ json: wiki }));
  await page.route("**/wiki/graph.json", route => route.fulfill({ json: { nodes: [], links: [] } }));
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
    hanrei_citations: [{ label: "1", reference: "hanrei:97208#holding", field: "holding", quote: source(97208).holding }],
    hanrei_related_cases: [{ ...meta(97210), case_id: 97210, judged_on: "2025-06-27", court: "関連候補の裁判所", common_articles: [{ law_id: "325AC0000000144", title: "生活保護法", article: "8" }] }],
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

for (const surface of ["hanrei", "wiki/hanrei"] as const) {
  test(`${surface}から一覧にない関連判例を調査案件へ追加できる`, async ({ page }) => {
    await mock(page);
    await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: { ...feed, items: [], count: 0 } }));
    await page.goto(new URL(`#/${surface}/97208`, BASE).href);
    await expect(page.getByRole("link", { name: "この判例を調査案件で比較", exact: true })).toHaveAttribute("href", "#/watch?case=97208");
    const related = page.getByTestId("hanrei-related-cases");
    await expect(related).toContainText("先例関係・同じ争点や結論・適用法令の版の一致を示すものではありません");
    await related.getByRole("link", { name: "判例97210と表示中の判例を比較", exact: true }).click();
    const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
    await expect(request).toBeFocused();
    await expect(request).toContainText(source(97210).case_name);
    await expect(request).toContainText("裁判所公表の判示事項");
    expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBeNull();
    await request.getByRole("button", { name: "比較対象に追加して保存", exact: true }).click();
    await expect(page).toHaveURL(/#\/watch$/);
    const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
    await expect(comparison).toBeFocused();
    await expect(comparison).toContainText("比較用の結論97210");
    await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(0);
    await page.reload();
    await expect(comparison).toContainText("比較用の結論97208");
    await expect(comparison).toContainText("比較用の結論97210");
  });
}

test("単独の判例を既存の比較に追加し、重複とURLの再訪で選択を増やさない", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/hanrei/97208", BASE).href);
  await page.getByRole("link", { name: "この判例を調査案件で比較", exact: true }).click();
  await page.getByRole("button", { name: "比較対象に追加して保存", exact: true }).click();
  await page.goto(new URL("#/watch?case=97209&case=97209", BASE).href);
  const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
  await expect(request).toContainText("追加後の比較対象: 判例97208・判例97209");
  await request.getByRole("button", { name: "比較対象に追加して保存", exact: true }).click();
  await page.goto(new URL("#/watch?case=97209&filter=keep", BASE).href);
  await expect(request).toContainText("すでに比較対象に含まれています");
  await request.getByRole("button", { name: "比較画面へ進む", exact: true }).click();
  await expect(page).toHaveURL(/#\/watch\?filter=keep$/);
  expect(await page.evaluate(key => JSON.parse(localStorage.getItem(key)!).comparison, STORAGE)).toEqual(["97208", "97209"]);
});

test("保存先を切り替えて入れ替えを確認し、別案件と外した判例のメモを保持する", async ({ page }) => {
  await mock(page); await selectTwo(page);
  await page.getByRole("textbox", { name: "判例97209の比較メモ", exact: true }).fill("外しても残すメモ");
  await page.getByRole("textbox", { name: "新しい案件名", exact: true }).fill("処分Aの調査");
  await page.getByRole("button", { name: "現在の比較から案件を作成", exact: true }).click();
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("Aの問いを保持");
  await page.getByRole("textbox", { name: "新しい案件名", exact: true }).fill("処分Bの調査");
  await page.getByRole("button", { name: "現在の比較から案件を作成", exact: true }).click();
  await page.goto(new URL("#/watch?case=97208&case=97210", BASE).href);
  const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
  await expect(request).toContainText("保存先: 調査案件「処分Bの調査」");
  await page.getByRole("combobox", { name: "調査案件を選択", exact: true }).selectOption({ label: "処分Aの調査" });
  await expect(request).toContainText("保存先: 調査案件「処分Aの調査」");
  await expect(request).toContainText("現在の比較対象: 判例97208・判例97209");
  await expect(request).toContainText("次の2件に入れ替えます: 判例97208・判例97210");
  await page.setViewportSize({ width: 375, height: 812 });
  await request.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
  await page.screenshot({ path: test.info().outputPath("comparison-request-mobile.png") });
  await request.getByRole("button", { name: "比較対象を2件に入れ替える", exact: true }).click();
  const saved = await page.evaluate(key => JSON.parse(localStorage.getItem(key)!), STORAGE);
  expect(saved.comparison).toEqual(["97208", "97209"]);
  const a = saved.projects.find((item: { title: string }) => item.title === "処分Aの調査");
  const b = saved.projects.find((item: { title: string }) => item.title === "処分Bの調査");
  expect(a.comparison).toEqual(["97208", "97210"]);
  expect(a.question).toBe("Aの問いを保持");
  expect(a.caseNotes["97209"]).toBe("外しても残すメモ");
  expect(b.comparison).toEqual(["97208", "97209"]);
  await page.reload();
  await expect(page.getByRole("textbox", { name: "調査する問い", exact: true })).toHaveValue("Aの問いを保持");
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("比較用の結論97210");
});

test("候補を閉じる操作と不正なURLでは既存の比較を変更しない", async ({ page }) => {
  await mock(page); await selectTwo(page);
  const before = await page.evaluate(key => localStorage.getItem(key), STORAGE);
  await page.goto(new URL("#/watch?case=97210", BASE).href);
  await page.getByRole("button", { name: "候補を閉じる", exact: true }).click();
  expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBe(before);
  const cases: string[] = [];
  page.on("request", request => { if (/\/hanrei\/\d+\.json/.test(request.url())) cases.push(request.url()); });
  for (const query of ["case=0", "case=097208", "case=not-an-id", "case=97208&case=97209&case=97210"]) {
    await page.goto(new URL(`#/watch?${query}`, BASE).href);
    await expect(page.getByRole("region", { name: "判例の追加確認", exact: true }).getByRole("alert")).toContainText("指定を確認できません");
    expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBe(before);
  }
  expect(cases.some(url => url.includes("/97210.json"))).toBe(false);
});

test("別事件の応答を候補として保存せず、取得の再試行で再開できる", async ({ page }) => {
  await mock(page);
  let fail = true;
  await page.route("**/hanrei/97210.json", route => route.fulfill({ json: source(fail ? 97209 : 97210) }));
  await page.goto(new URL("#/watch?case=97208&case=97210", BASE).href);
  const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
  await expect(request.getByRole("alert")).toContainText("事件IDとの対応を確認できませんでした");
  await expect(request.getByRole("button", { name: "比較対象に追加して保存", exact: true })).toBeDisabled();
  await expect(request).not.toContainText(source(97209).case_name);
  expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBeNull();
  fail = false;
  await request.getByRole("button", { name: "候補を再試行", exact: true }).click();
  await request.getByRole("button", { name: "比較対象に追加して保存", exact: true }).click();
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("比較用の結論97210");
});

test("保存失敗を成功扱いせず、再試行後に比較候補を閉じる", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/watch?case=97208", BASE).href);
  await page.evaluate(() => {
    const setItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key, value) {
      if ((window as unknown as { failSave: boolean }).failSave) throw new DOMException("denied", "SecurityError");
      return setItem.call(this, key, value);
    };
    (window as unknown as { failSave: boolean }).failSave = true;
  });
  const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
  await request.getByRole("button", { name: "比較対象に追加して保存", exact: true }).click();
  await expect(request.getByRole("status")).toContainText("ブラウザに保存できませんでした");
  await expect(page).toHaveURL(/case=97208$/);
  expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBeNull();
  await page.evaluate(() => { (window as unknown as { failSave: boolean }).failSave = false; });
  await request.getByRole("button", { name: "比較対象の保存を再試行", exact: true }).click();
  await expect(page).toHaveURL(/#\/watch$/);
  await expect(page.getByText("比較対象をブラウザに保存しました。", { exact: true })).toBeVisible();
  expect(await page.evaluate(key => JSON.parse(localStorage.getItem(key)!).comparison, STORAGE)).toEqual(["97208"]);
});

test("保存データを読み込めない場合はURLからの比較追加を停止する", async ({ page }) => {
  await mock(page);
  await page.addInitScript(key => localStorage.setItem(key, "broken-state"), STORAGE);
  await page.goto(new URL("#/watch?case=97208", BASE).href);
  const request = page.getByRole("region", { name: "判例の追加確認", exact: true });
  await expect(request).toContainText("保存データを読み込めていないため");
  await expect(request.getByRole("button", { name: "比較対象に追加して保存", exact: true })).toBeDisabled();
  expect(await page.evaluate(key => localStorage.getItem(key), STORAGE)).toBe("broken-state");
});
