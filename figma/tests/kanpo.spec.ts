import { test, expect } from "@playwright/test";

// 官報全文検索 (search-view の官報セクション / kanpo_fts) の e2e。
// fixture の search.db (kanpo_fts に 1 件) を sql.js-httpvfs 経由で検索する。
// fixture DB の再生成:
//   cargo run -p search-index --example build_kanpo_fixture -- \
//     figma/tests/fixtures/public/kanpo figma/tests/fixtures/public/search.db
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

// httpvfs 初期化 + 1.6MB DB の Range fetch + wasm コンパイルで初回は時間がかかるため
// 長めに待つ (ビルド直後のコールドスタート対策)。
const FTS_TIMEOUT = 60_000;

test("検索の官報セクションに改め文記事がヒットする", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());

  // 官報セクションのヘッダ (FTS5 が search.db を Range で読めている証拠)。
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });
  // fixture の記事タイトルと号・頁メタ。
  await expect(
    page.getByText("郵便法施行規則の一部を改正する省令（総務五八）"),
  ).toBeVisible();
  await expect(page.getByText("第1678号", { exact: false })).toBeVisible();
});

test("官報記事カードに官報PDFリンクがある", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });

  const pdf = page.getByRole("link", { name: "官報PDFを開く" });
  await expect(pdf).toBeVisible();
  await expect(pdf).toHaveAttribute("href", "https://www.kanpo.go.jp/20260402/x.pdf");
});

test("官報検索結果から改正対象法令へ逆引きできる", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });

  // fixture の linked_laws (郵便法施行規則 415M60000008005) への逆引きチップ。
  const lawBtn = page.getByRole("button", { name: "郵便法施行規則", exact: true });
  await expect(lawBtn).toBeVisible();
  await lawBtn.click();
  await expect(page).toHaveURL(/#\/laws\/415M60000008005/, { timeout: 15_000 });
});
