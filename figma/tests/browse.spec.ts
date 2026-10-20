import { test, expect } from "@playwright/test";

// 法令閲覧 (一覧) の e2e。fixture の laws/index.json (民法 + テスト法令3件) を使う。
// 検証: ロード中は skeleton (mock を出さない) / 既定は更新順ソート / タイトル絞り込み。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const recoveredLaw = {
  schema_version: 1, law_id: "129AC0000000089", title: "取得した法令の正式名称",
  law_num: "取得した法令番号", revision_id: null, status: "current",
  promulgation_date: "1896-04-27", effective_date: "1898-07-16",
  articles: [{ article_id: "art_2", article_no: "第二条", caption: null,
    paragraphs: [{ paragraph_no: null, text: "取得に成功した本文です。" }] }],
};

test("law index failure offers retry without sample laws and keeps the filter", async ({ page }) => {
  let failed = true;
  await page.route("**/laws/index.json", route => failed
    ? route.fulfill({ status: 503, body: "unavailable" }) : route.continue());
  await page.goto(new URL("#/laws", BASE).toString());
  await expect(page.getByRole("alert")).toContainText("法令一覧を取得できませんでした");
  await expect(page.getByText("労働基準法", { exact: true })).toHaveCount(0);
  await page.getByPlaceholder("タイトル・法令番号で絞り込み").fill("ベータ");
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.locator(".grid .text-base")).toHaveText(["ベータ法"]);
  await expect(page.getByPlaceholder("タイトル・法令番号で絞り込み")).toHaveValue("ベータ");
});

test("law body failure retries at the same URL without fabricated articles", async ({ page }) => {
  let failed = true;
  await page.route("**/laws/index.json", route => route.fulfill({ status: 503 }));
  await page.route("**/laws/129AC0000000089/current.json", route => failed
    ? route.fulfill({ status: 503 }) : route.fulfill({ json: recoveredLaw }));
  await page.goto(new URL("#/laws/129AC0000000089#art_2", BASE).toString());
  await expect(page.getByRole("alert")).toContainText("法令本文を取得できませんでした");
  await expect(page.locator("article")).toHaveCount(0);
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByRole("heading", { name: recoveredLaw.title })).toBeVisible();
  await expect(page.getByText(recoveredLaw.law_num, { exact: true })).toBeVisible();
  await expect(page.locator("#art_2")).toContainText("取得に成功した本文です。");
  await expect(page).toHaveURL(/#\/laws\/129AC0000000089#art_2$/);
});

test("law body is readable while optional history and versions are pending", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route("**/laws/129AC0000000089/current.json", route => route.fulfill({ json: recoveredLaw }));
  await page.route(/\/laws\/129AC0000000089\/(timeline|versions)\.json$/, async route => {
    await gate;
    await route.fulfill({ status: 503 });
  });
  try {
    await page.goto(new URL("#/laws/129AC0000000089", BASE).toString());
    await expect(page.locator("#art_2")).toContainText("取得に成功した本文です。");
    await expect(page.getByText("本文を読み込み中…")).toHaveCount(0);
    await page.getByRole("tab", { name: "改正履歴", exact: true }).click();
    await expect(page.getByRole("tabpanel")).toContainText("改正履歴を読み込み中…");
    await page.getByRole("tab", { name: "バージョン", exact: true }).click();
    await expect(page.getByRole("tabpanel")).toContainText("バージョン情報を読み込み中…");
  } finally { release(); }
});

test("empty law timeline and failed timeline never fall back to sample amendments", async ({ page }) => {
  let failed = true;
  await page.route("**/laws/129AC0000000089/timeline.json", route => failed
    ? route.fulfill({ status: 503 }) : route.fulfill({ json: { law_id: recoveredLaw.law_id, events: [] } }));
  await page.goto(new URL("#/laws/129AC0000000089", BASE).toString());
  await page.getByRole("tab", { name: "改正履歴", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("改正履歴を取得できませんでした");
  await expect(page.getByRole("tabpanel").locator('[data-slot="card"]')).toHaveCount(0);
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByRole("tabpanel")).toContainText("改正履歴はありません");
  await expect(page.getByRole("tabpanel").locator('[data-slot="card"]')).toHaveCount(0);
});

test("version failure retries independently of the loaded law body", async ({ page }) => {
  let failed = true;
  let bodyRequests = 0;
  await page.route("**/laws/129AC0000000089/current.json", route => {
    bodyRequests++;
    return route.fulfill({ json: recoveredLaw });
  });
  await page.route("**/laws/129AC0000000089/versions.json", route => failed
    ? route.fulfill({ status: 503 }) : route.fulfill({ json: { current_revision_id: null, versions: [] } }));
  await page.goto(new URL("#/laws/129AC0000000089", BASE).toString());
  await expect(page.locator("#art_2")).toBeVisible();
  await page.getByRole("tab", { name: "バージョン", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("バージョン情報を取得できませんでした");
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByRole("tabpanel")).toContainText("バージョン情報はありません");
  await page.getByRole("tab", { name: "本文", exact: true }).click();
  await expect(page.locator("#art_2")).toBeVisible();
  expect(bodyRequests).toBe(1);
});

test("law cards fit a phone and can be opened with the keyboard", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("#/laws", BASE).toString());
  const card = page.getByRole("link", { name: /アルファ法/ });
  await expect(card).toHaveAttribute("href", "#/laws/900AC0000000001");
  const bounds = await card.boundingBox();
  expect(bounds!.width).toBeGreaterThan(300);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
  await card.focus();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#\/laws\/900AC0000000001$/);
});

test("title search can recover its law index while full text search is unavailable", async ({ page }) => {
  let failed = true;
  await page.route("**/search.db", route => route.abort());
  await page.route("**/laws/index.json", route => failed
    ? route.fulfill({ status: 503 }) : route.continue());
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await page.getByRole("checkbox", { name: "法令・条文", exact: true }).check();
  await page.getByPlaceholder("法令・資料名・本文を検索...").fill("ベータ");
  const indexError = page.getByRole("alert").filter({ hasText: "法令一覧を取得できませんでした" });
  await expect(indexError).toBeVisible();
  failed = false;
  await indexError.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByText("ベータ法", { exact: true })).toBeVisible();
  await expect(indexError).toHaveCount(0);
  await expect(page.getByPlaceholder("法令・資料名・本文を検索...")).toHaveValue("ベータ");
});

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
// 無く 404 だが、timeline は独立取得されるのでブロックは出る。
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

// 法令詳細の「この法令の通達」ブロック (law→tsutatsu クロスリンク)。
// links/law-to-tsutatsu/{law_id}.json を route で返し、チップ表示と
// 通達ビュー (/tsutatsu?tax=...) への遷移を検証する。
test("法令詳細に「この法令の通達」が出て通達ビューへ遷移する", async ({ page }) => {
  const lawId = "129AC0000000089"; // fixture index に居る民法
  await page.route(`**/links/law-to-tsutatsu/${lawId}.json`, (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        law_id: lawId,
        linked_tsutatsu: [
          {
            tax: "shotoku",
            name: "所得税基本通達",
            count: 638,
            relevance: "interpretive_circular",
            confidence: 0.95,
          },
        ],
      },
    }),
  );

  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());

  await expect(page.getByText(/この法令の通達 \(\d+\)/)).toBeVisible({ timeout: 15_000 });
  const chip = page.getByRole("button", { name: /所得税基本通達/ });
  await expect(chip).toBeVisible();
  await chip.click();
  await expect(page).toHaveURL(/#\/tsutatsu\?tax=shotoku/, { timeout: 15_000 });
});

// 法令詳細の別表 (current.json の appendix_tables)。号列挙形式と表形式の両方を
// 本文末尾に描画し、条文目次から #appdx_{n} へ飛べることを検証する。
test("法令詳細に別表 (appendix_tables) が表示される", async ({ page }) => {
  const lawId = "900AC0000000001"; // fixture index に居るアルファ法
  await page.route(`**/laws/${lawId}/current.json`, (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        law_id: lawId,
        law_num: "テスト法律第一号",
        title: "アルファ法",
        revision_id: null,
        promulgation_date: null,
        effective_date: null,
        status: "current",
        articles: [
          {
            article_id: "art_2",
            article_no: "第二条",
            caption: null,
            paragraphs: [{ paragraph_no: null, text: "別表に掲げるもの" }],
          },
        ],
        appendix_tables: [
          {
            appdx_id: "appdx_1",
            index: 1,
            title: "別表",
            related_article_num: "（第二条関係）",
            items: [
              { title: "一", text: "刑法（明治四十年法律第四十五号）" },
              { title: "八", text: "前各号に掲げるもののほか、政令で定めるもの" },
            ],
          },
          {
            appdx_id: "appdx_2",
            index: 2,
            title: "別表第二",
            related_article_num: null,
            rows: [
              [{ text: "区分", header: true }, { text: "金額", header: true }],
              [{ text: "甲" }, { text: "千円" }],
            ],
            remarks: ["金額は消費税を含む。"],
          },
        ],
        source: { provider: "egov", raw_xml_sha256: null, fetched_at: "2026-10-01T00:00:00Z" },
      },
    }),
  );

  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());

  const appdx1 = page.locator("#appdx_1");
  await expect(appdx1).toBeVisible({ timeout: 15_000 });
  await expect(appdx1.getByRole("heading", { name: "別表" })).toBeVisible();
  await expect(appdx1).toContainText("（第二条関係）");
  await expect(appdx1).toContainText("刑法（明治四十年法律第四十五号）");

  const appdx2 = page.locator("#appdx_2");
  await expect(appdx2.locator("th")).toHaveText(["区分", "金額"]);
  await expect(appdx2.locator("td")).toHaveText(["甲", "千円"]);
  await expect(appdx2).toContainText("金額は消費税を含む。");

  // 条文目次にも別表が並ぶ。
  await expect(page.getByRole("button", { name: /別表第二/ })).toBeVisible();
});

// e-Gov 出典表記 (公共データ利用規約 第1.0版)。サイドバー (「最新同期」横のヘルプの tooltip) には health.json の
// latest_egov_update_date を、法令詳細には current.json の source.fetched_at (JST 日付) を添える。
test("出典表記 (e-Gov法令検索・加工の旨) がサイドバーと法令詳細に出る", async ({ page }) => {
  const lawId = "129AC0000000089"; // fixture index に居る民法
  await page.route(`**/laws/${lawId}/current.json`, (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        law_id: lawId,
        law_num: "明治二十九年法律第八十九号",
        title: "民法",
        revision_id: "r1",
        promulgation_date: null,
        effective_date: null,
        status: "current",
        articles: [{ article_id: "a1", article_no: "第一条", caption: null, paragraphs: [{ paragraph_no: "1", text: "私権は、公共の福祉に適合しなければならない。" }] }],
        source: { provider: "egov", raw_xml_sha256: null, fetched_at: "2026-06-13T20:00:00+00:00" },
      },
    }),
  );

  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());
  await expect(page.getByText("私権は、公共の福祉に適合しなければならない。")).toBeVisible({ timeout: 15_000 });

  // サイドバーの出典は「最新同期」横のヘルプの tooltip に入っているので、開くまでは法令詳細の 1 つだけ。
  const notes = page.getByTestId("egov-attribution");
  await expect(notes).toHaveCount(1);
  // 法令詳細: fetched_at 2026-06-13T20:00Z → JST 2026-06-14。
  await expect(notes.first()).toContainText("加工して作成（2026-06-14 時点）");
  await expect(notes.first().getByRole("link", { name: "e-Gov法令検索" })).toHaveAttribute("href", "https://laws.e-gov.go.jp/");
  await expect(notes.first().getByRole("link", { name: "公共データ利用規約（第1.0版）" })).toHaveAttribute("href", "https://www.e-gov.go.jp/terms");

  // サイドバー: ヘルプを hover すると fixture health.json の latest_egov_update_date 付きで出る。
  await page.locator("aside").getByRole("button", { name: "データの出典" }).hover();
  const sidebarNote = page.locator("[data-slot=tooltip-content]").getByTestId("egov-attribution");
  await expect(sidebarNote).toContainText("e-Gov法令検索（デジタル庁）のデータを lawrenceanum が加工して作成（2026-06-14 時点）");
  await expect(sidebarNote.getByRole("link", { name: "公共データ利用規約（第1.0版）" })).toHaveAttribute("href", "https://www.e-gov.go.jp/terms");
  await expect(notes).toHaveCount(2);
});
