import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import type { FeedItem } from "../src/app/data/api";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const STORAGE = "lawrenceanum:topic-watch:v1";
const topic = { id: "administrative-welfare", title: "生活保護・行政救済", keywords: ["生活保護", "生活扶助"] };
const judgment: FeedItem = { kind: "hanrei", date: "2025-06-27", title: "生活保護の判例", summary: "生活扶助基準についての判示事項", href: "/hanrei/97208", internal: true };
const law: FeedItem = { kind: "law", date: "2026-10-23", title: "生活保護法", summary: "保存時の改正情報", href: "/laws/325AC0000000144", internal: true };
const initial = () => ({ schema_version: 1, topic, generated_at: "2026-10-24T00:00:00Z", items: [judgment, law], count: 2 });
const baselineButton = (page: Page) => page.getByRole("button", { name: "現在の一覧を次回の比較基準に保存", exact: true });
const comparison = (page: Page) => page.getByRole("region", { name: "前回の一覧との比較", exact: true });
const list = (page: Page) => page.getByRole("region", { name: "追跡資料一覧", exact: true });
const start = (page: Page) => page.goto(new URL("#/watch", BASE).href);

test("古い裁判日の追加と表示情報の変更を検出し、未読とは別に比較・出力・基準更新できる", async ({ page }) => {
  let current = initial();
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: current }));
  await start(page);
  await expect(comparison(page)).toContainText("比較基準はまだありません");
  await expect(page.getByLabel("追加・変更のみ", { exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "このテーマを追跡", exact: true }).click();
  await list(page).getByRole("button", { name: "既読にする", exact: true }).first().click();
  await list(page).getByRole("button", { name: "既読にする", exact: true }).click();
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(list(page).getByRole("article")).toHaveCount(0);
  await baselineButton(page).click();
  await expect(comparison(page)).toContainText("（2件）");
  const backfill = { ...judgment, date: "2001-01-01", title: "生活保護の過去の判例", href: "/hanrei/11111" };
  current = { ...current, generated_at: "2026-10-25T00:00:00Z", count: 3, items: [judgment, { ...law, summary: "現在の改正情報" }, backfill] };
  await page.reload();
  await expect(comparison(page)).toContainText("追加 1件 · 表示情報の変更 1件");
  await page.goto(new URL("#/", BASE).href);
  const card = page.getByRole("region", { name: "追跡中のテーマ", exact: true });
  await expect(card).toContainText("未読 1件");
  await expect(card).toContainText("追加 1件 · 表示情報の変更 1件");
  await card.getByRole("link").click();
  await expect(comparison(page)).toContainText("追加 1件 · 表示情報の変更 1件");
  await page.getByLabel("追加・変更のみ", { exact: true }).check();
  await expect(list(page).getByRole("article")).toHaveCount(2);
  const changed = list(page).getByRole("article").filter({ hasText: "保存した一覧から表示情報の変更" });
  await changed.getByText("基準保存時の表示情報を確認", { exact: true }).click();
  await expect(changed).toContainText("保存時の改正情報");
  await expect(changed).toContainText("現在の改正情報");
  await expect(list(page).getByRole("link", { name: "生活保護の過去の判例" })).toHaveAttribute("href", "#/hanrei/11111");
  const waiting = page.waitForEvent("download");
  await page.getByRole("button", { name: "表示中の資料を調査メモに出力", exact: true }).click();
  const report = await readFile((await (await waiting).path())!, "utf8");
  expect(report).toContain("出力件数: 2");
  expect(report).toContain("比較基準のフィード生成日時: 2026-10-24T00:00:00Z");
  expect(report).toContain("保存時の改正情報");
  expect(report).toContain("現在の改正情報");
  expect(report).toContain("保存した一覧との比較: 追加");
  expect(report).toContain("裁判所の原文・出典: https://www.courts.go.jp/hanrei/11111/detail2/index.html");
  expect(report).toContain("本文・Wiki・条文の変更を検出したものではありません");
  expect(report).toContain("LLMによる要約・解説");
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(list(page).getByRole("article")).toHaveCount(1);
  await baselineButton(page).click();
  await expect(comparison(page)).toContainText("（3件）");
  await expect(comparison(page)).toContainText("追加 0件 · 表示情報の変更 0件");
  await expect(list(page)).toContainText("本文の更新がないことを意味するものではありません");
  await page.reload();
  await expect(comparison(page)).toContainText("追加 0件 · 表示情報の変更 0件");
  await page.getByLabel("未読のみ", { exact: true }).check();
  await expect(list(page).getByRole("article")).toHaveCount(1);
});

test("取得失敗・古い配信を変更なしと扱わず、保存済みの比較基準を保持する", async ({ page }) => {
  let current = initial();
  let fail = false;
  await page.route("**/feeds/topics/administrative-welfare.json", route => fail ? route.fulfill({ status: 503 }) : route.fulfill({ json: current }));
  await start(page);
  await page.getByRole("button", { name: "このテーマを追跡", exact: true }).click();
  await baselineButton(page).click();
  await expect(comparison(page)).toContainText("（2件）");
  const saved = await page.evaluate(key => JSON.parse(localStorage.getItem(key)!).checkpoint, STORAGE);
  fail = true;
  await page.reload();
  await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
  await expect(baselineButton(page)).toBeDisabled();
  await expect(comparison(page)).not.toContainText("追加 0件");
  fail = false;
  current = { ...current, generated_at: "2026-10-23T00:00:00Z", items: [] };
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(comparison(page)).toContainText("比較基準より古いフィード");
  await expect(comparison(page)).not.toContainText("追加 0件");
  await expect(baselineButton(page)).toBeDisabled();
  await expect(page.getByLabel("追加・変更のみ", { exact: true })).toBeDisabled();
  expect(await page.evaluate(key => JSON.parse(localStorage.getItem(key)!).checkpoint, STORAGE)).toEqual(saved);
  await page.goto(new URL("#/", BASE).href);
  await expect(page.getByRole("region", { name: "追跡中のテーマ", exact: true })).toContainText("比較基準より古い配信");
  current = { ...initial(), generated_at: "2026-10-25T00:00:00Z", items: [{ ...judgment, summary: "生活扶助の修正された判示事項" }, law] };
  await start(page);
  await expect(comparison(page)).toContainText("追加 0件 · 表示情報の変更 1件");
});

test("不正な比較基準だけを無視し、既存の調査メモを読み込める", async ({ page }) => {
  await page.addInitScript(({ key, topicId }) => localStorage.setItem(key, JSON.stringify({ version: 1, followed: true, read: [], note: "以前の調査メモ", checkpoint: {
    topicId, generatedAt: "2026-10-24T00:00:00Z", checkedAt: "2026-10-24T01:00:00Z", entries: { '["hanrei","2025-06-27","/hanrei/97208"]': 'null' },
  } })), { key: STORAGE, topicId: topic.id });
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: initial() }));
  await start(page);
  await expect(comparison(page)).toContainText("比較基準はまだありません");
  await expect(page.getByRole("textbox", { name: /自分の調査メモ/ })).toHaveValue("以前の調査メモ");
  await expect(list(page).getByRole("article")).toHaveCount(2);
  await baselineButton(page).click();
  await expect(comparison(page)).toContainText("（2件）");
});

test("保存拒否時は比較基準の永続化に失敗したことを知らせる", async ({ page }) => {
  await page.addInitScript(() => { Storage.prototype.setItem = () => { throw new Error("blocked"); }; });
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: initial() }));
  await start(page);
  await baselineButton(page).click();
  await expect(comparison(page)).toContainText("比較基準をブラウザに保存できていない可能性があります");
  await page.reload();
  await expect(comparison(page)).toContainText("比較基準はまだありません");
});

test("収録範囲から外れた資料を削除扱いせず、同じ資料の日付変更は追加候補として扱う", async ({ page }) => {
  let current = initial();
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: current }));
  await start(page);
  await baselineButton(page).click();
  current = { ...initial(), generated_at: "2026-10-25T00:00:00Z", items: [judgment] };
  await page.reload();
  await expect(comparison(page)).toContainText("追加 0件 · 表示情報の変更 0件");
  await expect(comparison(page)).toContainText("一覧から外れた資料を削除とは判断しません");
  current = { ...current, items: [{ ...law, date: "2026-10-25" }, judgment] };
  await page.reload();
  await expect(comparison(page)).toContainText("追加 1件 · 表示情報の変更 0件");
});

test("スマートフォンで比較基準と長い変更前情報を確認できる", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  let current = initial();
  current.items[0] = { ...judgment, summary: `生活扶助${"長い判示事項".repeat(40)}` };
  await page.route("**/feeds/topics/administrative-welfare.json", route => route.fulfill({ json: current }));
  await start(page);
  await baselineButton(page).click();
  current = { ...initial(), generated_at: "2026-10-25T00:00:00Z" };
  await page.reload();
  await expect(comparison(page)).toContainText("表示情報の変更 1件");
  await page.getByLabel("追加・変更のみ", { exact: true }).check();
  await list(page).getByText("基準保存時の表示情報を確認", { exact: true }).click();
  await expect(list(page)).toContainText("長い判示事項".repeat(40));
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
  await page.screenshot({ path: test.info().outputPath("topic-changes-mobile.png"), fullPage: true });
  await comparison(page).scrollIntoViewIfNeeded();
  await page.screenshot({ path: test.info().outputPath("topic-checkpoint-mobile.png"), fullPage: true });
});
