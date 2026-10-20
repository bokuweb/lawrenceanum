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

// 同じ PDF 頁に国会事項・人事異動・叙勲などが同居する場合も、現在の画面で本文を読める。
test("同じPDF頁の他項目の本文を開き、項目へ移動できる", async ({ page }) => {
  const pdf = "https://www.kanpo.go.jp/20261019/20261019h01810/pdf/20261019h018100007.pdf";
  await page.route("**/kanpo/2026-10-19/index.json*", async route => {
    if (route.request().url().endsWith(".gz")) return route.fulfill({ status: 404 });
    await route.fulfill({ json: {
      date: "2026-10-19",
      issues: [{
        issue_type: "regular", issue_no: "第1810号", titles: [],
        items: [
          ...Array.from({ length: 4 }, (_, i) => ({ title: `前の項目${i}`, page: i + 1, pdf_url: `${pdf}?other=${i}` })),
          { title: "国会事項", page: 7, pdf_url: pdf,
            amend_text: "衆議院\n質問書提出\n十月十五日議員から提出した質問主意書は次のとおりである。\n政府提出の「消費税・支援金法案」に関する質問主意書（田村智子外三名提出）" },
          { title: "内閣", page: 7, pdf_url: pdf,
            amend_text: "財務大臣臨時代理\n国務大臣藤井比早之" },
          { title: "叙位・叙勲", page: 7, pdf_url: pdf,
            amend_document: { format: "prose", blocks: [{ kind: "paragraph", runs: [{ text: "旭日小綬章を授ける（各通）" }] }] } },
          // 頁番号だけが同じでも、別 PDF は同居項目に含めない。
          { title: "別のPDFの項目", page: 7, pdf_url: `${pdf}?other=7`, amend_text: "別のPDFの本文" },
        ],
      }],
    } });
  });
  await page.goto(new URL("#/kanpo/2026-10-19/0/4", BASE).toString());
  const own = page.getByRole("region", { name: "国会事項の本文", exact: true });
  await expect(own).toContainText("田村智子外三名提出");
  await expect(own).not.toContainText("財務大臣臨時代理");
  const others = page.getByRole("region", { name: "同じPDF頁の他の掲載内容" });
  await expect(others.locator("summary")).toHaveText(["内閣", "叙位・叙勲"]);
  await expect(others.getByText("国務大臣藤井比早之")).not.toBeVisible();
  await others.locator("summary").filter({ hasText: "内閣" }).click();
  await expect(others.getByText(/国務大臣藤井比早之/)).toBeVisible();
  await others.locator("summary").filter({ hasText: "叙位・叙勲" }).click();
  await expect(others.getByText("旭日小綬章を授ける（各通）")).toBeVisible();
  await others.locator("details").filter({ has: page.locator("summary", { hasText: "内閣" }) })
    .getByRole("button", { name: "この項目を開く" }).click();
  await expect(page).toHaveURL(/#\/kanpo\/2026-10-19\/0\/5$/);
  await expect(page.getByRole("region", { name: "内閣の本文", exact: true })).toContainText("国務大臣藤井比早之");
  await expect(others.getByText("旭日小綬章を授ける（各通）")).not.toBeVisible();
});

test("官報の日付一覧は取得失敗から絞り込みを保って再試行できる", async ({ page }) => {
  let failed = true;
  await page.route("**/kanpo/index.json*", route => failed
    ? route.fulfill({ status: 503 }) : route.continue());
  await page.goto(new URL("#/kanpo", BASE).toString());
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("発行日を読み込めませんでした");
  const query = page.getByRole("textbox", { name: "この日の項目を絞り込む" });
  await query.fill("共同募金");
  failed = false;
  await alert.getByRole("button", { name: "再試行" }).click();
  await expect(page.getByRole("status", { name: "表示中の項目数" })).toHaveText("1件");
  await expect(page.getByRole("button", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
  await expect(query).toHaveValue("共同募金");
  await expect(alert).toHaveCount(0);
});

test("官報の項目は取得失敗から選択中のURLと絞り込みを保って再試行できる", async ({ page }) => {
  let failed = true;
  await page.route("**/kanpo/2026-04-02/index.json*", route => failed
    ? route.fulfill({ status: 503 }) : route.continue());
  await page.goto(new URL("#/kanpo/2026-04-02/0/0", BASE).toString());
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("この日の官報を読み込めませんでした");
  const query = page.getByRole("textbox", { name: "この日の項目を絞り込む" });
  await query.fill("郵便法");
  failed = false;
  await alert.getByRole("button", { name: "再試行" }).click();
  await expect(page.getByRole("heading", { name: "郵便法施行規則の一部を改正する省令（総務五八）" })).toBeVisible();
  await expect(page.getByRole("status", { name: "表示中の項目数" })).toHaveText("1件");
  await expect(query).toHaveValue("郵便法");
  await expect(page).toHaveURL(/#\/kanpo\/2026-04-02\/0\/0$/);
  await expect(alert).toHaveCount(0);
});

test("発行日一覧が失敗してもURLの日付の官報は読める", async ({ page }) => {
  await page.route("**/kanpo/index.json*", route => route.fulfill({ status: 503 }));
  await page.goto(new URL("#/kanpo/2026-04-02/0/0", BASE).toString());
  await expect(page.getByRole("alert")).toContainText("発行日を読み込めませんでした");
  await expect(page.getByRole("heading", { name: "郵便法施行規則の一部を改正する省令（総務五八）" })).toBeVisible();
  await expect(page.getByRole("button", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
});

test("発行日一覧の応答を待たずにURLの日付の官報一覧と本文を読める", async ({ page }) => {
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  await page.route("**/kanpo/index.json*", async route => { await pending; await route.continue(); });
  try {
    await page.goto(new URL("#/kanpo/2026-04-02/0/0", BASE).toString());
    await expect(page.getByRole("heading", { name: "郵便法施行規則の一部を改正する省令（総務五八）" })).toBeVisible();
    await expect(page.getByRole("button", { name: /共同募金会が募集する寄附金/ })).toBeVisible();
  } finally {
    release();
  }
});

test("スマートフォンで官報の一覧と長い題名・本文を画面内で読める", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const title = "長い官報の標題".repeat(40);
  const body = "LONG_KANPO_BODY_".repeat(80);
  await page.route("**/kanpo/2026-04-02/index.json*", route => route.request().url().endsWith(".gz")
    ? route.fulfill({ status: 404 }) : route.fulfill({ json: {
      date: "2026-04-02", issues: [{ issue_type: "regular", issue_no: "第1678号", items: [{
        title, page: 1, agency_hint: "AGENCY_".repeat(50), amend_text: body,
      }] }],
    } }));
  await page.goto(new URL("#/kanpo", BASE).toString());
  const row = page.getByRole("button", { name: new RegExp(title) });
  await expect(row).toBeInViewport();
  const bounds = await row.boundingBox();
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
  await row.click();
  await expect(page.getByRole("textbox", { name: "この日の項目を絞り込む" })).toBeVisible();
  await expect(page.getByRole("heading", { name: title, exact: true })).toBeInViewport();
  const text = page.getByRole("region", { name: `${title}の本文`, exact: true }).locator("pre");
  await text.scrollIntoViewIfNeeded();
  await expect(text).toBeInViewport();
  const viewport = text.locator("xpath=ancestor::*[@data-slot='scroll-area-viewport'][1]");
  const size = await viewport.evaluate(el => ({ client: el.clientWidth, scroll: el.scrollWidth }));
  expect(size.scroll).toBeLessThanOrEqual(size.client + 1);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

test("官報の幅広い表はスマートフォンで表の中を横スクロールできる", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route("**/kanpo/2026-04-02/index.json*", route => route.request().url().endsWith(".gz")
    ? route.fulfill({ status: 404 }) : route.fulfill({ json: {
      date: "2026-04-02", issues: [{ issue_type: "regular", issue_no: "第1678号", items: [{
        title: "別表を改正する件", page: 1, amend_document: { format: "prose", blocks: [{
          kind: "paragraph", runs: [{ text: "", table: {
            rows: [Array.from({ length: 12 }, (_, i) => [{ text: `項目${i + 1}` }])],
          } }],
        }] },
      }] }],
    } }));
  await page.goto(new URL("#/kanpo/2026-04-02/0/0", BASE).toString());
  const table = page.getByRole("region", { name: "別表を改正する件の本文" }).getByRole("table");
  await expect(table).toBeVisible();
  const viewport = table.locator("xpath=ancestor::*[@data-slot='scroll-area-viewport'][1]");
  const size = await viewport.evaluate(el => ({ client: el.clientWidth, scroll: el.scrollWidth }));
  expect(size.scroll).toBeLessThanOrEqual(size.client + 1);
  const canScroll = await table.evaluate(el => {
    for (let parent = el.parentElement; parent && !parent.dataset.slot; parent = parent.parentElement) {
      if (getComputedStyle(parent).overflowX !== "auto") continue;
      parent.scrollLeft = parent.scrollWidth;
      return parent.scrollLeft > 0;
    }
    return false;
  });
  expect(canScroll).toBe(true);
});
