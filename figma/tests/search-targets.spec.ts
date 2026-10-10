import { test, expect } from "@playwright/test";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

for (const [query, label, title, path] of [
  ["添付原文", "パブコメ", "検索テストの意見募集案件", "/pubcomment/search-case"],
  ["提出理由", "議案", "検索テストの法律案", "/gian/221/search-bill"],
  ["配布原文", "審議会資料", "検索テストの審議会", "/shingikai/moj/search-meeting"],
]) {
  test(`${label}の本文・添付から検索して詳細へ移動できる`, async ({ page }) => {
    let reikiRequests = 0;
    page.on("request", req => { if (req.url().includes("reiki-search.db")) reikiRequests++; });
    await page.goto(new URL("#/search", BASE).toString());
    await page.getByRole("button", { name: "解除", exact: true }).click();
    await page.getByRole("checkbox", { name: label, exact: true }).check();
    await page.getByPlaceholder(/検索/).fill(query);
    await expect(page.getByText(`${label} (1件)`, { exact: true })).toBeVisible({ timeout: 20_000 });
    expect(reikiRequests).toBe(0);
    await expect(page.getByText("該当する条文がありません")).toHaveCount(0);
    await page.getByText(title, { exact: true }).click();
    await expect(page).toHaveURL(new URL(`#${path}`, BASE).toString());
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
  });
}

test("例規DBの読み込みが遅くても次の検索と対象切り替えを止めない", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  let reikiStarted = () => {};
  const started = new Promise<void>(resolve => { reikiStarted = resolve; });
  await page.route("**/reiki-search.db", async route => {
    reikiStarted();
    await gate;
    await route.continue();
  });
  try {
    await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
    await started;
    await expect(page.getByText("官報 (1件)", { exact: true })).toBeVisible({ timeout: 20_000 });
    await page.getByPlaceholder(/検索/).fill("添付原文");
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("官報 (1件)", { exact: true })).toHaveCount(0);
    await page.getByRole("checkbox", { name: "パブコメ", exact: true }).uncheck();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toHaveCount(0);
    // 再選択しても同じ検索結果を再利用できる。
    await page.getByRole("checkbox", { name: "パブコメ", exact: true }).check();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toBeVisible({ timeout: 10_000 });
  } finally { release(); }
});

test("検索対象の解除と短い検索語を案内する", async ({ page }) => {
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await expect(page.getByText("検索対象を選んでください")).toBeVisible();
  await page.getByRole("checkbox", { name: "パブコメ", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("法");
  await expect(page.getByText("2 文字以上で検索してください")).toBeVisible();
});
