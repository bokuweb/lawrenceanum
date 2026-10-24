import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const STORAGE = "lawrenceanum:topic-watch:v1";
const caseIds = [97208, 97209, 97210];
const source = (id: number) => ({ case_id: id, case_name: `生活保護の試験判例${id}`, case_number: `試験第${id}号`, judged_on: "2025-06-27", court: "試験裁判所", result: `比較結果${id}`, holding: "生活扶助基準", summary: `公表要旨${id}`, referenced_laws: "生活保護法", judgment_type: "判決", reporter: "", lower_court: "", lower_case_number: "", lower_judged_on: "", detail_url: "", pdf_url: null });

async function mock(page: Page) {
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: { schema_version: 1, topic: { id: "administrative-welfare" }, generated_at: "2026-10-24T00:00:00Z", count: 3, items: caseIds.map(id => ({ kind: "hanrei", date: "2025-06-27", title: source(id).case_name, summary: "生活扶助基準", href: `/hanrei/${id}`, internal: true })) } }));
  await page.route(/\/hanrei\/\d+\.json(?:\?.*)?$/, route => route.fulfill({ json: source(Number(route.request().url().match(/\/(\d+)\.json/)![1])) }));
  await page.route("**/wiki/index.json", route => route.fulfill({ json: { schema_version: 1, pages: [] } }));
  await page.route("**/links/hanrei-to-laws/*.json", route => route.fulfill({ json: { schema_version: 1, case_id: Number(route.request().url().match(/\/(\d+)\.json/)![1]), citations: [], laws: [] } }));
}

async function start(page: Page) {
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  const list = page.getByRole("region", { name: "追跡資料一覧" });
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await list.getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
}

async function create(page: Page, title: string) {
  await page.getByRole("textbox", { name: "新しい案件名", exact: true }).fill(title);
  await page.getByRole("button", { name: "現在の比較から案件を作成", exact: true }).click();
  await expect(page.getByRole("region", { name: "調査案件", exact: true }).getByRole("heading", { name: title, exact: true })).toBeVisible();
}

test("同じ判例のメモも案件別に保存し、再訪・切替後に元の比較を再開できる", async ({ page }) => {
  await start(page);
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("案件外のメモ");
  await create(page, "処分Aの調査");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("処分Aの理由を確認する");
  await page.getByRole("textbox", { name: "確認した出典・箇所", exact: true }).fill("97208の原文3頁");
  await page.getByRole("textbox", { name: "未確認の点・次に調べること", exact: true }).fill("処分日の経過措置");
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("Aだけの相違点");
  await create(page, "処分Bの調査");
  await expect(page.getByRole("textbox", { name: "調査する問い", exact: true })).toHaveValue("");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("Bの論点");
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("Bだけの相違点");
  await page.getByRole("button", { name: "判例97209を比較から外す", exact: true }).click();
  await page.getByRole("region", { name: "追跡資料一覧" }).getByRole("button", { name: "比較に追加", exact: true }).last().click();
  const picker = page.getByRole("combobox", { name: "調査案件を選択", exact: true });
  await picker.selectOption({ label: "処分Aの調査" });
  await page.reload();
  await expect(page.getByRole("textbox", { name: "調査する問い", exact: true })).toHaveValue("処分Aの理由を確認する");
  await expect(page.getByRole("textbox", { name: "確認した出典・箇所", exact: true })).toHaveValue("97208の原文3頁");
  await expect(page.getByRole("textbox", { name: "未確認の点・次に調べること", exact: true })).toHaveValue("処分日の経過措置");
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("Aだけの相違点");
  const comparison = page.getByRole("region", { name: "判例の比較", exact: true });
  await expect(comparison).toContainText("比較結果97209");
  await expect(comparison).not.toContainText("比較結果97210");
  await picker.selectOption({ label: "処分Bの調査" });
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("Bだけの相違点");
  await expect(comparison).toContainText("比較結果97210");
  await picker.selectOption("");
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("案件外のメモ");
  await expect(comparison).toContainText("比較結果97209");
});

test("案件の問い・出典確認記録・未確認事項を公式要旨と区別して出力する", async ({ page }) => {
  await start(page);
  await create(page, "案件A");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("事案の相違点は何か");
  await page.getByRole("textbox", { name: "確認した出典・箇所", exact: true }).fill("私の記録：原文3頁の理由");
  await page.getByRole("textbox", { name: "未確認の点・次に調べること", exact: true }).fill("経過措置は未確認");
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("Aの判例メモ");
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "比較メモを出力", exact: true }).click();
  const download = await waiting;
  const report = await readFile((await download.path())!, "utf8");
  expect(report).toContain("## 利用者の調査記録");
  expect(report).toContain("案件名: 案件A");
  expect(report).toContain("事案の相違点は何か");
  expect(report).toContain("私の記録：原文3頁の理由");
  expect(report).toContain("経過措置は未確認");
  expect(report).toContain("Aの判例メモ");
  expect(report).toContain("公表要旨97208");
  expect(report).toContain("https://www.courts.go.jp/hanrei/97208/detail2/index.html");
  expect(report).toContain("LLMによる要約・解説");
  expect(report).toContain("案件保存時の資料を固定したものではありません");
});

test("保存失敗時にも案件を操作・出力でき、保存済みと案内しない", async ({ page }) => {
  await page.addInitScript(() => { Storage.prototype.setItem = () => { throw new DOMException("denied", "SecurityError"); }; });
  await start(page); await create(page, "画面内の案件");
  const projects = page.getByRole("region", { name: "調査案件", exact: true });
  await expect(projects.getByRole("status")).toContainText("保存できていない可能性");
  await expect(projects).not.toContainText("保存日時:");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("この画面で記録する");
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeEnabled();
});

test("資料の取得失敗時も案件の記録と比較から外した判例メモを出力できる", async ({ page }) => {
  await start(page); await create(page, "取得失敗でも持ち出す案件");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("理由の違いを調べる");
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("比較から外しても残すメモ");
  await page.getByRole("button", { name: "判例97208を比較から外す", exact: true }).click();
  await page.route(/\/hanrei\/\d+\.json(?:\?.*)?$/, route => route.fulfill({ status: 503, body: "unavailable" }));
  await page.reload();
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("判例を読み込めないか");
  await expect(page.getByRole("button", { name: "比較メモを出力", exact: true })).toBeDisabled();
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "案件の記録を出力", exact: true }).click();
  const report = await readFile((await (await waiting).path())!, "utf8");
  expect(report).toContain("理由の違いを調べる");
  expect(report).toContain("比較対象: 1件");
  expect(report).toContain("https://www.courts.go.jp/hanrei/97209/detail2/index.html");
  expect(report).toContain("## 比較対象から外した判例のメモ");
  expect(report).toContain("比較から外しても残すメモ");
  expect(report).not.toContain("公表要旨97209");
});

test("破損した案件を除外して既存の記録を維持し、案件数の上限を守る", async ({ page }) => {
  await page.addInitScript(key => {
    if (localStorage.getItem(key)) return;
    const valid = { id: "saved", title: "保存済み案件", updatedAt: "2026-10-24T00:00:00Z", question: "保存済みの問い", comparison: ["97208", "97208", "bad", "97209", "97210"], caseNotes: { "97208": "保持するメモ" } };
    localStorage.setItem(key, JSON.stringify({ version: 1, followed: true, read: [], note: "元の記録", activeProject: "missing", projects: [null, { ...valid, id: "broken", updatedAt: "invalid" }, valid, valid, ...Array.from({ length: 11 }, (_, i) => ({ ...valid, id: `project-${i}`, title: `案件${i}` }))] }));
  }, STORAGE);
  await mock(page); await page.goto(new URL("#/watch", BASE).href);
  const picker = page.getByRole("combobox", { name: "調査案件を選択", exact: true });
  await expect(picker).toHaveValue("");
  await expect(picker.getByRole("option")).toHaveCount(11);
  await expect(page.getByRole("textbox", { name: "自分の調査メモ（ブラウザ内に保存）", exact: true })).toHaveValue("元の記録");
  await page.getByRole("textbox", { name: "新しい案件名", exact: true }).fill("上限を超える案件");
  await expect(page.getByRole("button", { name: "現在の比較から案件を作成", exact: true })).toBeDisabled();
  await picker.selectOption("saved");
  await expect(page.getByRole("textbox", { name: "調査する問い", exact: true })).toHaveValue("保存済みの問い");
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("保持するメモ");
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("比較結果97209");
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).not.toContainText("比較結果97210");
});

test("案件削除は取り消せて、確定しても案件外の記録と別案件を維持する", async ({ page }) => {
  await start(page);
  await page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true }).fill("案件外に残す記録");
  await create(page, "残す案件");
  await create(page, "削除する案件");
  const projects = page.getByRole("region", { name: "調査案件", exact: true });
  await projects.getByRole("button", { name: "案件を削除", exact: true }).click();
  await projects.getByRole("button", { name: "削除をやめる", exact: true }).click();
  await expect(projects.getByRole("heading", { name: "削除する案件", exact: true })).toBeVisible();
  await projects.getByRole("button", { name: "案件を削除", exact: true }).click();
  await projects.getByRole("button", { name: "この案件を削除", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "調査案件を選択", exact: true })).toHaveValue("");
  await expect(page.getByRole("textbox", { name: "判例97208の比較メモ", exact: true })).toHaveValue("案件外に残す記録");
  await page.reload();
  await expect(projects.getByRole("option", { name: "削除する案件", exact: true })).toHaveCount(0);
  await expect(projects.getByRole("option", { name: "残す案件", exact: true })).toHaveCount(1);
});

test("空の案件も作成でき、スマートフォンで記録を保存して比較を追加できる", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await mock(page); await page.goto(new URL("#/watch", BASE).href);
  await expect(page.getByRole("button", { name: "現在の比較から案件を作成", exact: true })).toBeDisabled();
  await create(page, "最初に問いを決める案件");
  await page.getByRole("textbox", { name: "調査する問い", exact: true }).fill("生活扶助基準の改定について調べる");
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "案件の記録を出力", exact: true }).click();
  const report = await readFile((await (await waiting).path())!, "utf8");
  expect(report).toContain("比較対象: 0件");
  expect(report).toContain("生活扶助基準の改定について調べる");
  await page.getByRole("region", { name: "追跡資料一覧" }).getByRole("button", { name: "比較に追加", exact: true }).first().click();
  await page.reload();
  await expect(page.getByRole("textbox", { name: "調査する問い", exact: true })).toHaveValue("生活扶助基準の改定について調べる");
  await expect(page.getByRole("region", { name: "判例の比較", exact: true })).toContainText("比較結果97208");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
  await page.getByRole("region", { name: "調査案件", exact: true }).evaluate(element => element.scrollIntoView({ block: "start" }));
  await page.screenshot({ path: test.info().outputPath("topic-projects-mobile.png") });
});
