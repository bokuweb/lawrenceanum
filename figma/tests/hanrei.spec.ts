import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
const fixture = (name: string) => JSON.parse(readFileSync(new URL(`./fixtures/public/hanrei/${name}.json`, import.meta.url), "utf8"));

test("左メニューの判例から一覧・絞り込み・詳細本文を開いて一覧に戻れる", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("navigation").getByRole("link", { name: "判例", exact: true }).click();
  await expect(page.getByRole("heading", { name: "判例", exact: true })).toBeVisible();
  await expect(page.getByText("2 / 2件")).toBeVisible();
  await page.getByRole("textbox", { name: "判例一覧の絞り込み" }).fill("損害賠償");
  await expect(page.getByText("1 / 2件")).toBeVisible();
  await expect(page.getByRole("link", { name: /旧データの行政事件/ })).toHaveCount(0);
  await page.getByRole("link", { name: /検索テストの損害賠償請求事件/ }).click();
  await expect(page).toHaveURL(/#\/hanrei\/92002\?filter=/);
  await expect(page.getByRole("heading", { name: "本文", exact: true })).toBeVisible();
  await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機の利用と立証責任");
  await expect(page.getByTestId("hanrei-body")).toContainText('<script>alert("本文")</script>');
  await expect(page.getByTestId("hanrei-body").locator("script")).toHaveCount(0);
  await expect(page.getByText("東京高等裁判所", { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveAttribute("href", fixture("92002").pdf_url);
  await page.getByRole("link", { name: "判例一覧に戻る" }).click();
  await expect(page.getByRole("textbox", { name: "判例一覧の絞り込み" })).toHaveValue("損害賠償");
});

test("裁判所で絞り込み、旧データでは本文未取得を案内する", async ({ page }) => {
  await page.goto(new URL("#/hanrei", BASE).toString());
  await page.getByRole("combobox", { name: "裁判所", exact: true }).click();
  await page.getByRole("option", { name: "東京地方裁判所", exact: true }).click();
  await expect(page.getByText("1 / 2件")).toBeVisible();
  await page.getByRole("link", { name: /旧データの行政事件/ }).click();
  await expect(page.getByTestId("hanrei-body")).toContainText("本文テキストは未取得です。");
  await expect(page.getByRole("link", { name: "全文PDF", exact: true })).toHaveCount(0);
});

for (const resource of ["index", "92002"]) {
  test(`判例${resource === "index" ? "一覧" : "詳細"}の読み込み失敗から再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(`**/hanrei/${resource}.json`, r => fail
      ? r.fulfill({ status: 503, body: "unavailable" })
      : r.fulfill({ json: fixture(resource) }));
    await page.goto(new URL(`#/hanrei${resource === "index" ? "" : "/92002"}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByRole("alert")).toHaveCount(0);
    if (resource === "index") await expect(page.getByText("2 / 2件")).toBeVisible();
    else await expect(page.getByTestId("hanrei-body")).toContainText("電子計算機");
  });
}

test("本文だけの検索語でヒットし、該当する本文の抜粋から詳細へ移動できる", async ({ page }) => {
  await page.route("**/search.db", r => r.continue({ url: new URL("hanrei-search.db", BASE).toString() }));
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await page.getByRole("checkbox", { name: "判例", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("電子計算機");
  const hits = page.getByTestId("hanrei-hits");
  await expect(hits).toContainText("判例 (1件)", { timeout: 20_000 });
  await expect(hits.getByTestId("hanrei-body-excerpt")).toContainText("本文（PDFから抽出・抜粋）");
  await expect(hits.locator("mark")).toHaveText("電子計算機");
  await expect(hits.getByTestId("hanrei-body-excerpt")).toContainText("立証責任");
  await hits.getByRole("link", { name: "検索テストの損害賠償請求事件", exact: true }).click();
  await expect(page).toHaveURL(/#\/hanrei\/92002$/);
  await expect(page.getByTestId("hanrei-body")).toContainText("本件上告を棄却する。");
});

test("スマートフォンで判例一覧と本文が画面幅に収まる", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/hanrei", BASE).toString());
  await page.getByRole("link", { name: /検索テストの損害賠償請求事件/ }).click();
  const body = page.getByTestId("hanrei-body");
  await expect(body).toContainText("電子計算機");
  const box = await body.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(390);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});
