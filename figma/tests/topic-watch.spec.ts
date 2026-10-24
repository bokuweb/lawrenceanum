import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const STORAGE = "lawrenceanum:topic-watch:v1";
const topic = { id: "administrative-welfare", title: "生活保護・行政救済", description: "試用テーマ", keywords: ["生活保護", "生活扶助"] };
const judgment = { kind: "hanrei", date: "2026-10-23", title: "保護変更決定取消請求事件", summary: "生活扶助基準の改定に関する判示事項", href: "/hanrei/97208", internal: true };
const law = { kind: "law", date: "2026-10-22", title: "生活保護法", summary: "改正", href: "/laws/325AC0000000144", internal: true };
const feed = { schema_version: 1, generated_at: "2026-10-24T00:00:00Z", topic, count: 3, items: [judgment, law, { ...law, title: "無関係な資料", href: "/laws/other" }] };

async function mock(page: Page, value = feed) {
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: value }));
}

test("テーマの根拠・収録範囲・判例へのサイト内リンクとRSSを表示する", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  await expect(page.getByRole("heading", { name: topic.title })).toBeVisible();
  const list = page.getByRole("region", { name: "追跡資料一覧" });
  await expect(list.getByRole("article")).toHaveCount(2);
  await expect(list).toContainText("裁判所公表の判示事項");
  await expect(list).toContainText("一致した語: 生活扶助");
  await expect(list.getByRole("link", { name: /保護変更決定/ })).toHaveAttribute("href", "#/hanrei/97208");
  await expect(page.getByRole("region", { name: "収録範囲" })).toContainText("公表日・取得日ではありません");
  await expect(page.getByRole("link", { name: "テーマ別RSSを購読" })).toHaveAttribute("href", "./feeds/topics/administrative-welfare.xml");
});

test("追跡・既読・メモが再訪後も残りダッシュボードの未読に反映される", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  await page.getByRole("button", { name: "このテーマを追跡", exact: true }).click();
  await page.getByRole("button", { name: "既読にする", exact: true }).first().click();
  await page.getByRole("textbox", { name: /自分の調査メモ/ }).fill("確認に10分。経過措置を次に読む。");
  await page.reload();
  await expect(page.getByRole("button", { name: "追跡中（解除する）" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("textbox", { name: /自分の調査メモ/ })).toHaveValue("確認に10分。経過措置を次に読む。");
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(1);
  await page.goto(new URL("#/", BASE).href);
  await expect(page.getByRole("region", { name: "追跡中のテーマ" })).toContainText("未読 1件");
  await page.getByRole("region", { name: "追跡中のテーマ" }).getByRole("link").click();
  await page.getByRole("button", { name: "追跡中（解除する）" }).click();
  await page.goto(new URL("#/", BASE).href);
  await expect(page.getByRole("region", { name: "追跡中のテーマ" })).toHaveCount(0);
});

test("同じ資料の日付が変わる更新は再び未読になり、出力は表示中だけを含む", async ({ page }) => {
  let current = { ...feed, count: 2, items: [judgment, law] };
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: current }));
  await page.goto(new URL("#/watch", BASE).href);
  await page.getByRole("button", { name: "既読にする", exact: true }).first().click();
  await page.getByRole("button", { name: "既読にする", exact: true }).click();
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(page.getByRole("region", { name: "追跡資料一覧" })).toContainText("未読の資料はありません");
  current = { ...current, items: [{ ...judgment, date: "2026-10-24" }, law] };
  await page.reload();
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(1);
  await page.getByRole("textbox", { name: /自分の調査メモ/ }).fill("原文を確認");
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "表示中の資料を調査メモに出力" }).click();
  const download = await waiting;
  const report = await readFile((await download.path())!, "utf8");
  expect(download.suggestedFilename()).toMatch(/^administrative-welfare-.*\.md$/);
  expect(report).toContain("出力件数: 1");
  expect(report).toContain("原文を確認");
  expect(report).toContain("裁判所公表の判示事項");
  expect(report).toContain("https://www.courts.go.jp/hanrei/97208/detail2/index.html");
  expect(report).toContain(`${BASE}#/hanrei/97208`);
  expect(report).toContain("LLMによる要約・解説");
  expect(report).toContain("適用版が確定していない");
  expect(report).not.toContain("#/laws/325AC0000000144");
});

test("取得失敗は空一覧と区別し、再試行できる", async ({ page }) => {
  let fail = true;
  await page.route("**/feeds/topics/administrative-welfare.json", route => fail ? route.fulfill({ status: 503, body: "unavailable" }) : route.fulfill({ json: { ...feed, count: 0, items: [] } }));
  await page.goto(new URL("#/watch", BASE).href);
  await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
  await expect(page.getByRole("button", { name: "表示中の資料を調査メモに出力" })).toBeDisabled();
  fail = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("region", { name: "追跡資料一覧" })).toContainText("更新がないことを意味するものではありません");
});

test("保存が拒否されても操作でき、保存失敗を案内する", async ({ page }) => {
  await page.addInitScript(() => { Storage.prototype.setItem = () => { throw new Error("blocked"); }; });
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  await page.getByRole("button", { name: "このテーマを追跡", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("読み書きできません");
  await page.getByRole("button", { name: "既読にする", exact: true }).first().click();
  await expect(page.getByRole("button", { name: "未読に戻す" })).toHaveCount(1);
});

test("破損した保存状態を読み込んでも画面が壊れない", async ({ page }) => {
  await page.addInitScript(key => localStorage.setItem(key, "broken"), STORAGE);
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  await expect(page.getByRole("status").filter({ hasText: "読み書きできません" })).toBeVisible();
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(2);
});

test("スマートフォンの幅で追跡操作と資料が収まる", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await mock(page);
  await page.goto(new URL("#/watch", BASE).href);
  await expect(page.getByRole("region", { name: "追跡資料一覧" }).getByRole("article")).toHaveCount(2);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
  await page.getByRole("button", { name: "このテーマを追跡", exact: true }).click();
  await expect(page.getByRole("button", { name: "追跡中（解除する）" })).toHaveAttribute("aria-pressed", "true");
  await page.screenshot({ path: test.info().outputPath("topic-watch-mobile.png"), fullPage: true });
});
