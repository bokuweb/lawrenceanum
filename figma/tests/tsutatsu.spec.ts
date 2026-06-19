import { test, expect } from "@playwright/test";

// 通達 (TsutatsuView) の e2e。tsutatsu の JSON を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const INDEX = {
  schema_version: 1,
  count: 1,
  sets: [{ tax: "shotoku", name: "所得税基本通達", count: 2 }],
};

const SET = {
  schema_version: 1,
  name: "所得税基本通達",
  tax: "shotoku",
  parent_law_id: "340AC0000000033",
  parent_law_title: "所得税法",
  items: [
    { tax: "shotoku", number: "2-1", caption: "住所の意義", text: "法第2条第1項に規定する住所とは各人の生活の本拠をいい、民法第22条に定めるところによる。", source_url: "https://www.nta.go.jp/law/tsutatsu/kihon/shotoku/01/02.htm" },
    { tax: "shotoku", number: "2-2", caption: "再入国した場合の居住期間", text: "国内に居所を有していた者が国外に赴き再び入国した場合…", source_url: "https://www.nta.go.jp/law/tsutatsu/kihon/shotoku/01/03.htm" },
  ],
  source: { provider: "nta", fetched_at: "2026-06-19T00:00:00Z", index_url: "https://www.nta.go.jp/law/tsutatsu/kihon/shotoku/01.htm" },
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/tsutatsu/index.json", (r) => r.fulfill({ json: INDEX }));
  await page.route("**/tsutatsu/shotoku.json", (r) => r.fulfill({ json: SET }));
}

test("通達ビューに番号・見出し付きで項目が並ぶ", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/tsutatsu", BASE).toString());

  await expect(page.getByRole("heading", { name: "通達" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("住所の意義")).toBeVisible();
  await expect(page.getByText("2-1", { exact: true })).toBeVisible();
  // 原文リンク。
  const link = page.getByRole("link", { name: "国税庁 原文" }).first();
  await expect(link).toHaveAttribute("href", /nta\.go\.jp/);
});

test("通達本文の「法第N条」が親法令の条文へリンクする（民法等は除外）", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/tsutatsu?tax=shotoku", BASE).toString());

  // 「法＝所得税法」の親法令ヒント。
  await expect(page.getByRole("button", { name: /法＝所得税法/ })).toBeVisible({ timeout: 15_000 });

  // 「法第2条」はリンク (button)、隣接の「民法第22条」はリンクにならない。
  const ref = page.getByRole("button", { name: "法第2条", exact: true });
  await expect(ref).toBeVisible();
  await expect(page.getByRole("button", { name: "民法第22条" })).toHaveCount(0);

  // クリックで親法令の該当条文 (art_2) へ遷移。
  await ref.click();
  await expect(page).toHaveURL(/#\/laws\/340AC0000000033#art_2/, { timeout: 15_000 });
});

test("番号・見出しで絞り込める", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/tsutatsu", BASE).toString());
  await expect(page.getByText("住所の意義")).toBeVisible({ timeout: 15_000 });

  await page.getByPlaceholder("番号・見出し・本文で絞り込み…").fill("再入国");
  await expect(page.getByText("再入国した場合の居住期間")).toBeVisible();
  await expect(page.getByText("住所の意義")).toHaveCount(0);
});
