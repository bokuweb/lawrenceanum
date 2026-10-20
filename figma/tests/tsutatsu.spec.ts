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

test("通達集の選択をURL・再読み込み・戻る進むで復元する", async ({ page }) => {
  await mock(page);
  await page.route("**/tsutatsu/index.json", r => r.fulfill({ json: {
    ...INDEX,
    count: 2,
    sets: [...INDEX.sets, { tax: "hojin", name: "法人税基本通達", count: 1 }],
  } }));
  await page.route("**/tsutatsu/hojin.json", r => r.fulfill({ json: {
    ...SET,
    tax: "hojin",
    name: "法人税基本通達",
    items: [{ ...SET.items[0], caption: "法人税の項目" }],
  } }));
  await page.goto(new URL("#/tsutatsu?source=test", BASE).toString());
  await expect(page.getByText("住所の意義")).toBeVisible();
  await expect(page).toHaveURL(/#\/tsutatsu\?source=test&tax=shotoku$/);

  await page.getByRole("combobox", { name: "通達集" }).click();
  await page.getByRole("option", { name: /法人税基本通達/ }).click();
  await expect(page.getByText("法人税の項目")).toBeVisible();
  await expect(page).toHaveURL(/source=test&tax=hojin$/);

  await page.goBack();
  await expect(page.getByText("住所の意義")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "通達集" })).toContainText("所得税基本通達");
  await page.goForward();
  await expect(page.getByText("法人税の項目")).toBeVisible();
  await page.reload();
  await expect(page.getByText("法人税の項目")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "通達集" })).toContainText("法人税基本通達");
});

for (const [reference, articleId] of [["法第２条の３", "art_2_3"], ["法第2条の3の4", "art_2_3_4"]]) {
  test(`枝番号付きの参照 ${reference} を正しい条文へリンクする`, async ({ page }) => {
    await mock(page);
    await page.route("**/tsutatsu/shotoku.json", r => r.fulfill({ json: {
      ...SET,
      items: [{ ...SET.items[0], text: `${reference}による。民法第22条の2及び措置法第3条の2は対象外。` }],
    } }));
    await page.goto(new URL("#/tsutatsu?tax=shotoku", BASE).toString());
    await expect(page.getByRole("button", { name: "法第22条の2", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "法第3条の2", exact: true })).toHaveCount(0);
    await page.getByRole("button", { name: reference, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`#${articleId}$`));
  });
}

for (const resource of ["index", "shotoku"]) {
  test(`通達の${resource}読み込み失敗を表示し再試行できる`, async ({ page }) => {
    await mock(page);
    let attempts = 0;
    await page.route(`**/tsutatsu/${resource}.json`, r => {
      attempts++;
      return attempts === 1 ? r.fulfill({ status: 503, body: "unavailable" }) :
        r.fulfill({ json: resource === "index" ? INDEX : SET });
    });
    await page.goto(new URL("#/tsutatsu?tax=shotoku", BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    await expect(page.getByText("該当する通達がありません")).toHaveCount(0);
    await page.getByRole("button", { name: "再試行" }).click();
    await expect(page.getByText("住所の意義")).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(0);
    expect(attempts).toBe(2);
  });
}

test("本文末まで表示した検索抜粋に省略記号を付けない", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/tsutatsu?tax=shotoku", BASE).toString());
  await expect(page.getByText("住所の意義")).toBeVisible();
  await page.getByPlaceholder("番号・見出し・本文で絞り込み…").fill("生活の本拠");
  await expect(page.locator("p").filter({ hasText: "生活の本拠" })).toHaveText(SET.items[0].text);
});
