import { test, expect } from "@playwright/test";

const BASE = process.env.LAWRENCEANUM_BASE ?? "https://bokuweb.github.io/lawrenceanum/";

test("dashboard shows law count from live JSON", async ({ page }) => {
  // health.json の law_count を直接読んで baseline にする。
  const health = await (await page.request.get(new URL("./health.json", BASE).toString())).json();
  console.log("[health]", JSON.stringify(health));

  await page.goto(BASE);
  // ダッシュボード stat カードが live を反映するまで待つ。"—" が消えるまで。
  const lawValue = page.locator('text=登録法令数').locator('..').locator('div').nth(1);
  await expect(lawValue).not.toHaveText("—", { timeout: 15_000 });
  const value = (await lawValue.textContent())?.replace(/,/g, "").trim() ?? "";
  console.log("[ui] 登録法令数 =", value);
  expect(Number(value)).toBe(health.law_count);
});

test("search '民法' returns hits", async ({ page }) => {
  await page.goto(new URL("#/search?q=民法", BASE).toString());
  await expect(page.locator('text=FTS5 / 法令')).toBeVisible({ timeout: 30_000 });
  const text = (await page.locator('text=件の結果').first().textContent()) ?? "";
  console.log("[search]", text);
  const m = text.match(/^(\d+) 件/);
  expect(m).not.toBeNull();
  // 0 件 = bulk fetch に取りこぼし、または FTS5 indexing バグ
  expect(Number(m![1])).toBeGreaterThan(0);
});

test("browse 民法 詳細表示", async ({ page }) => {
  await page.goto(new URL("#/laws/129AC0000000089", BASE).toString());
  // 第一条が出れば本文 OK
  await expect(page.locator('h2', { hasText: '第一条' })).toBeVisible({ timeout: 15_000 });
});
