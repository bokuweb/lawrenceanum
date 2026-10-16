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
  await page.getByRole("tab", { name: /^官報/ }).click();
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });
  // fixture の記事タイトルと号・頁メタ。
  await expect(
    page.getByText("郵便法施行規則の一部を改正する省令（総務五八）"),
  ).toBeVisible();
  await expect(page.getByText("第1678号", { exact: false })).toBeVisible();
});

test("官報記事カードに官報PDFリンクがある", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
  await page.getByRole("tab", { name: /^官報/ }).click();
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });

  const pdf = page.getByRole("link", { name: "官報PDFを開く" });
  await expect(pdf).toBeVisible();
  await expect(pdf).toHaveAttribute("href", "https://www.kanpo.go.jp/20260402/x.pdf");
});

test("官報検索結果から改正対象法令へ逆引きできる", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
  await page.getByRole("tab", { name: /^官報/ }).click();
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });

  // fixture の linked_laws (郵便法施行規則 415M60000008005) への逆引きチップ。
  const lawBtn = page.getByRole("button", { name: "郵便法施行規則", exact: true });
  await expect(lawBtn).toBeVisible();
  await lawBtn.click();
  await expect(page).toHaveURL(/#\/laws\/415M60000008005/, { timeout: 15_000 });
});

test("検索の通達セクションに通達がヒットする", async ({ page }) => {
  await page.goto(new URL("#/search?q=住所の意義", BASE).toString());
  await page.getByRole("tab", { name: /^通達/ }).click();
  // fixture の tsutatsu_fts (所得税基本通達 2-1 住所の意義)。
  await expect(page.getByText(/通達 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });
  await expect(page.getByText("住所の意義")).toBeVisible();
  await expect(page.getByText("2-1", { exact: true })).toBeVisible();
});

// 官報の一覧と項目の表示 (#/kanpo)。fixture の kanpo/index.json と kanpo/2026-04-02/index.json を読む。
test("官報の一覧に最新の発行日の号と項目が並ぶ", async ({ page }) => {
  await page.goto(new URL("#/kanpo", BASE).toString());

  await expect(page.getByRole("combobox", { name: "発行日" })).toContainText("2026-04-02");
  await expect(page.getByRole("region", { name: "本紙 第1678号" })).toBeVisible();
  await expect(page.getByRole("region", { name: "号外 第70号" })).toBeVisible();
  await expect(page.getByRole("button", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
  await expect(page.getByRole("status", { name: "表示中の項目数" })).toHaveText("3件");
});

test("改め文の項目を選ぶと本文と改正対象の法令が出る", async ({ page }) => {
  await page.goto(new URL("#/kanpo", BASE).toString());
  await page.getByRole("button", { name: /郵便法施行規則の一部を改正する省令/ }).click();

  await expect(page).toHaveURL(/#\/kanpo\/2026-04-02\/0\/0$/);
  await expect(page.getByRole("heading", { name: "郵便法施行規則の一部を改正する省令（総務五八）" })).toBeVisible();
  await expect(page.getByText("一部を次のように改正する。")).toBeVisible();
  await expect(page.getByRole("link", { name: /官報PDF（2頁）/ })).toHaveAttribute("href", "https://www.kanpo.go.jp/20260402/x.pdf");

  await page.getByRole("button", { name: "郵便法施行規則", exact: true }).click();
  await expect(page).toHaveURL(/#\/laws\/415M60000008005/, { timeout: 15_000 });
});

test("本文を取り出せなかった項目は PDF への導線を出す", async ({ page }) => {
  await page.goto(new URL("#/kanpo/2026-04-02/0/1", BASE).toString());

  await expect(page.getByRole("heading", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
  await expect(page.getByText("この項目の本文は取り出せませんでした。")).toBeVisible();
  await expect(page.getByRole("link", { name: /官報PDF（3頁）/ })).toHaveAttribute("href", "https://www.kanpo.go.jp/20260402/y.pdf");
});

test("その日の項目を標題で絞り込める", async ({ page }) => {
  await page.goto(new URL("#/kanpo/2026-04-02", BASE).toString());
  await page.getByRole("textbox", { name: "この日の項目を絞り込む" }).fill("共同募金");

  await expect(page.getByRole("status", { name: "表示中の項目数" })).toHaveText("1件");
  await expect(page.getByRole("button", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /郵便法施行規則の一部を改正する省令/ })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "号外 第70号" })).toHaveCount(0);
});

test("官報の検索結果から項目の表示へ移れる", async ({ page }) => {
  await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
  await page.getByRole("tab", { name: /^官報/ }).click();
  await expect(page.getByText(/官報 \(\d+件\)/)).toBeVisible({ timeout: FTS_TIMEOUT });

  await page.getByRole("button", { name: "郵便法施行規則の一部を改正する省令（総務五八）" }).click();
  // 号・頁・標題で項目を引き、並び順の URL に置き換える。
  await expect(page).toHaveURL(/#\/kanpo\/2026-04-02\/0\/0$/, { timeout: 15_000 });
  await expect(page.getByRole("heading", { name: "郵便法施行規則の一部を改正する省令（総務五八）" })).toBeVisible();
});
