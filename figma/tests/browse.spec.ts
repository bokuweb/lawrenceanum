import { test, expect } from "@playwright/test";

// 法令閲覧 (一覧) の e2e。fixture の laws/index.json (民法 + テスト法令3件) を使う。
// 検証: ロード中は skeleton (mock を出さない) / 既定は更新順ソート / タイトル絞り込み。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

test("law list shows skeleton (not mock) while index.json is loading", async ({ page }) => {
  // index.json のレスポンスをゲートで保留し、ロード中状態を観測可能にする。
  let release: () => void = () => {};
  const gate = new Promise<void>((res) => (release = res));
  await page.route("**/laws/index.json", async (route) => {
    await gate;
    await route.continue();
  });

  await page.goto(new URL("#/laws", BASE).toString());
  await expect(page.getByRole("heading", { name: "法令閲覧" })).toBeVisible({ timeout: 15_000 });

  // ロード中: skeleton が出ている。
  await expect(page.locator('[data-slot="skeleton"]').first()).toBeVisible({ timeout: 10_000 });
  // mock 一覧 (LAWS) の law がチラ見えしていない。fixture に無い "労働基準法" が
  // 出ていれば mock フォールバックが描画された証拠。
  await expect(page.locator("text=労働基準法")).toHaveCount(0);

  // index 解放 → 実データのカードに置き換わり skeleton が消える。
  release();
  await expect(page.locator("text=アルファ法")).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('[data-slot="skeleton"]')).toHaveCount(0);
});

test("law list defaults to update-order sort (newest last_updated first)", async ({ page }) => {
  await page.goto(new URL("#/laws", BASE).toString());
  await expect(page.locator("text=アルファ法")).toBeVisible({ timeout: 15_000 });

  // カードのタイトル順を取得。last_updated 降順:
  //   アルファ法 (2026-06-15) > ベータ法 (2026-06-10) > ガンマ法 (2026-06-01) > 民法 (null=末尾)
  const titles = await page.locator(".grid .text-base").allInnerTexts();
  const idx = (t: string) => titles.findIndex((x) => x.trim() === t);
  expect(idx("アルファ法")).toBe(0);
  expect(idx("アルファ法")).toBeLessThan(idx("ベータ法"));
  expect(idx("ベータ法")).toBeLessThan(idx("ガンマ法"));
  // last_updated が無い民法は末尾に回る。
  expect(idx("ガンマ法")).toBeLessThan(idx("民法"));
});

test("law list filters by title", async ({ page }) => {
  await page.goto(new URL("#/laws", BASE).toString());
  await expect(page.locator("text=アルファ法")).toBeVisible({ timeout: 15_000 });

  await page.getByPlaceholder("タイトル・法令番号で絞り込み").fill("ベータ");

  await expect(page.locator(".grid .text-base")).toHaveCount(1);
  await expect(page.locator("text=ベータ法")).toBeVisible();
  await expect(page.locator("text=アルファ法")).toHaveCount(0);
  await expect(page.locator("text=民法")).toHaveCount(0);
});

// 法令詳細の「この法令の官報掲載」ブロック。timeline.json の官報リンク済み
// イベント (kanpo.linked && pdf_url) を拾って表示する。current.json は fixture に
// 無く 404 → offlineFallback だが、timeline は独立取得されるのでブロックは出る。
test("法令詳細に「この法令の官報掲載」が出て官報PDFへリンクする", async ({ page }) => {
  const lawId = "129AC0000000089"; // fixture index に居る民法
  await page.route(`**/laws/${lawId}/timeline.json`, (route) =>
    route.fulfill({
      json: {
        law_id: lawId,
        events: [
          {
            event_id: "ev-kanpo-1",
            event_type: "amendment",
            target_law_id: lawId,
            amending_law_num: "令和八年政令第九六号",
            amending_law_title: "民法の一部を改正する政令",
            promulgation_date: "2026-04-24",
            effective_date: "2026-04-24",
            revision_id: "r1",
            status: "current",
            kanpo: {
              linked: true,
              path: "kanpo/2026-04-24/index.json",
              confidence: 0.95,
              pdf_url: "https://www.kanpo.go.jp/20260424/x.pdf",
              page: 3,
              amend_format: "prose",
            },
          },
        ],
      },
    }),
  );

  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());

  await expect(page.getByText(/この法令の官報掲載 \(\d+\)/)).toBeVisible({ timeout: 15_000 });
  const link = page.getByRole("link", { name: /民法の一部を改正する政令/ });
  await expect(link).toHaveAttribute("href", "https://www.kanpo.go.jp/20260424/x.pdf");
});
