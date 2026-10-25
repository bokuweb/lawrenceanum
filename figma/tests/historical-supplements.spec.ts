import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { administrativeAndLaborSupplements } from "../src/app/data/administrative-labor-historical-articles";
import { historicalArticleSupplement } from "../src/app/data/historical-article-supplements";
import type { CaseLawReference } from "../src/app/data/api";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const fixture = JSON.parse(readFileSync(new URL("./fixtures/public/hanrei/92002.json", import.meta.url), "utf8"));
function reference(lawId: string, articles: string[], from: string, to = from): CaseLawReference {
  return { law_id: lawId, title: "照合対象", articles, wiki_path: `laws/${lawId}`, basis: "unresolved", evidence: [{ from, to, text: "判例から抽出した期間" }], incomplete: true, versions: [], reference_versions: [] };
}
const adjacent = (date: string, days: number) => new Date(Date.parse(date) + days * 86400000).toISOString().slice(0, 10);

test("追加した全11条を確認期間内だけ補完し、別条・範囲をまたぐ証拠・選定済み版には流用しない", () => {
  expect(administrativeAndLaborSupplements).toHaveLength(11);
  for (const s of administrativeAndLaborSupplements) {
    const law = reference(s.lawId, [s.article], s.from, s.to);
    expect(historicalArticleSupplement(law, s.article)).toEqual(s);
    for (const date of [s.from, s.to]) expect(historicalArticleSupplement(reference(s.lawId, [s.article], date), s.article)).toEqual(s);
    for (const date of [adjacent(s.from, -1), adjacent(s.to, 1)]) expect(historicalArticleSupplement(reference(s.lawId, [s.article], date), s.article)).toBeUndefined();
    expect(historicalArticleSupplement({ ...law, evidence: [...law.evidence, { from: adjacent(s.from, -1), to: s.from, text: "境界をまたぐ別の根拠" }] }, s.article)).toBeUndefined();
    expect(historicalArticleSupplement({ ...law, articles: ["999"] }, s.article)).toBeUndefined();
    expect(historicalArticleSupplement(law, "999")).toBeUndefined();
    expect(historicalArticleSupplement({ ...law, versions: [{ revision_id: "selected", effective_date: s.from, body_available: true }] }, s.article)).toBeUndefined();
  }
  expect(historicalArticleSupplement(reference("322AC0000000049", ["114"], "2015-05-19"), "114")).toBeUndefined();
  expect(historicalArticleSupplement(reference("337AC0000000139", ["3"], "2013-07-04"), "3")).toBeUndefined();
});

for (const target of [
  { id: 85738, lawId: "337AC0000000139", title: "行政事件訴訟法", article: "14", paragraph: "3", date: "2016-03-10", text: "六箇月を経過したとき", caption: "出訴期間", period: "2005-04-01〜2017-03-31" },
  { id: 84502, lawId: "337AC0000000139", title: "行政事件訴訟法", article: "12", paragraph: "4", date: "2014-09-25", text: "特定管轄裁判所", caption: "管轄", period: "2005-04-01〜2017-03-31" },
  { id: 85148, lawId: "322AC0000000049", title: "労働基準法", article: "75", paragraph: "2", date: "2015-06-08", text: "厚生労働省令で定める", caption: "療養補償", period: "2001-01-06〜2016-03-31" },
]) {
  test(`${target.title}${target.article}条を判例${target.id}から内部リンクで読み、項・出典・法令別の注意を確認できる`, async ({ page }) => {
    const holding = `${target.title}第${target.article}条第${target.paragraph}項`;
    const law = { ...reference(target.lawId, target.id === 85148 ? ["19", "75", "81"] : [target.article], target.date), title: target.title };
    const links = { schema_version: 1, case_id: target.id, wiki_path: `hanrei/${target.id}`, notice: "機械抽出した版候補です。", laws: [law], citations: [{ field: "holding", law_id: target.lawId, article: target.article, paragraph: Number(target.paragraph), start: 0, end: holding.length, text: holding }] };
    await page.route(`**/hanrei/${target.id}.json`, r => r.fulfill({ json: { ...fixture, case_id: target.id, holding } }));
    await page.route(`**/links/hanrei-to-laws/${target.id}.json`, r => r.fulfill({ json: links }));
    await page.route("**/wiki/index.json", r => r.fulfill({ json: { schema_version: 1, pages: [{ path: `hanrei/${target.id}` }] } }));
    let lawRequests = 0;
    await page.route(`**/laws/${target.lawId}/**`, r => { lawRequests++; return r.fulfill({ status: 503 }); });
    await page.goto(new URL(`#/hanrei/${target.id}`, BASE).toString());
    await expect(page.getByText(/には公的資料で文言を照合した補完条文/)).toBeVisible();
    await page.getByRole("link", { name: holding, exact: true }).click();
    const panel = page.getByTestId("historical-article-supplement");
    await expect(panel.getByRole("heading", { name: `第${target.article}条（${target.caption}）` })).toBeVisible();
    await expect(panel).toContainText(target.period);
    await expect(panel.locator('[data-highlighted="true"]')).toContainText(target.text);
    await expect(panel).not.toContainText("生活扶助の金額");
    await expect(panel).toContainText(target.title === "行政事件訴訟法" ? "改正前の規定が適用される場合" : "労災保険給付");
    await panel.locator("summary").click();
    await expect(panel).toContainText("LLMによる要約ではありません");
    await expect(panel.getByRole("link", { name: /厚生労働省/ })).toHaveAttribute("href", /mhlw\.go\.jp/);
    await expect(page.getByText("版を特定できません", { exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "判例のLLM wiki", exact: true })).toHaveAttribute("href", `#/wiki/hanrei/${target.id}`);
    if (target.id === 85148) {
      for (const [article, text] of [["19", "産前産後の女性"], ["81", "千二百日分の打切補償"]]) {
        await page.goto(new URL(`#/hanrei/${target.id}/laws/${target.lawId}/${article}`, BASE).toString());
        await expect(panel).toContainText(text);
      }
    }
    expect(lawRequests).toBe(0);
  });
}
