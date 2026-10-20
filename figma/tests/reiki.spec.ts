import { test, expect } from "@playwright/test";

// 自治体例規 (ReikiView) と検索の例規セクションの e2e。
//
// tests/fixtures/public/reiki/** と reiki-search.db は mock プロバイダで生成した 2 自治体分:
//   lawpub reiki-fetch --provider mock --registry <2自治体の tenants.json> --cache <tmp>
//   lawpub reiki-build-json --cache <tmp> --public figma/tests/fixtures/public --registry <同上>
//   lawpub reiki-build-search-db --cache <tmp> --out figma/tests/fixtures/public/reiki-search.db
// 条・項・号・表・附則の描画は、より豊かな本文を route mock して確かめる。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const RICH_DOC = {
  schema_version: 2,
  reiki_id: "121002_g002RG00000853",
  municipality_code: "121002",
  municipality_name: "千葉市",
  prefecture: "千葉県",
  title: "千葉市アイススケート場設置管理条例",
  reiki_number: "条例第34号",
  kind: "条例",
  promulgated_date: "2004-09-29",
  current_as_of: "2026-07-01",
  preamble: [],
  articles: [
    {
      article_no: "第1条",
      article_id: "art_1",
      caption: "設置",
      paragraphs: [{ num: null, text: "本市は、次のとおりスケート場を設置する。\n名称\t位置\n千葉アイススケート場\t千葉市美浜区新港224番地1" }],
    },
    {
      article_no: "第3条",
      article_id: "art_3",
      caption: "業務の範囲",
      heading: "第2章　指定管理者",
      paragraphs: [
        {
          num: null,
          text: "指定管理者が行う業務の範囲は、次のとおりとする。",
          items: [
            { num: "(1)", text: "使用の許可に関する業務" },
            { num: "(2)", text: "維持管理に関する業務", subitems: [{ num: "ア", text: "清掃" }] },
          ],
        },
        { num: "2", text: "市長は、必要があると認めるときは、指示することができる。" },
      ],
    },
  ],
  supplementary: [
    { title: "附則", articles: [{ article_no: "", article_id: "s1_p1", caption: null, paragraphs: [{ num: null, text: "この条例は、規則で定める日から施行する。" }] }] },
  ],
  content_sha256: "x",
  source: {
    provider: "gyosei",
    fetched_at: "2026-10-01T00:00:00Z",
    checked_at: "2026-10-08T00:00:00Z",
    detail_url: "https://www1.g-reiki.net/chiba/reiki_honbun/g002RG00000853.html",
    municipality_official_site: "https://www1.g-reiki.net/chiba/reiki_menu.html",
  },
};

test("自治体一覧 → 例規一覧 → 本文へ辿れる", async ({ page }) => {
  await page.goto(new URL("#/reiki", BASE).toString());
  await expect(page.getByRole("heading", { name: "自治体例規" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("2自治体 / 2件")).toBeVisible();

  await page.getByRole("button", { name: /千葉市/ }).click();
  await expect(page).toHaveURL(/#\/reiki\/121002$/);
  await expect(page.getByRole("heading", { name: "千葉県 千葉市" })).toBeVisible();

  await page.getByRole("button", { name: /千葉市個人情報保護条例/ }).click();
  await expect(page).toHaveURL(/#\/reiki\/121002\/121002_jourei_sample$/);
  await expect(page.getByTestId("reiki-title")).toHaveText("千葉市個人情報保護条例");
  await expect(page.getByText("この条例は、個人情報の保護に関し必要な事項を定める。")).toBeVisible();
  await expect(page.getByText("（趣旨）")).toBeVisible();
  await expect(page.getByRole("button", { name: /他自治体の「個人情報保護条例」を探す/ })).toBeVisible();
});

test("自治体名・都道府県で絞り込める", async ({ page }) => {
  await page.goto(new URL("#/reiki", BASE).toString());
  await expect(page.getByRole("button", { name: /留萌市/ })).toBeVisible({ timeout: 15_000 });
  await page.getByPlaceholder("自治体名…").fill("千葉");
  await expect(page.getByRole("button", { name: /千葉市/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /留萌市/ })).toHaveCount(0);
});

test("自治体内の本文検索では件数欄が本文ヒット数になる", async ({ page }) => {
  await page.goto(new URL("#/reiki/121002", BASE).toString());
  const box = page.getByPlaceholder("題名で絞り込み（Enter で本文検索）");
  await expect(box).toBeVisible({ timeout: 15_000 });
  await box.fill("個人情報");
  await box.press("Enter");
  await expect(page.getByTestId("reiki-list-count")).toHaveText(/^本文 \d+件$/, { timeout: 30_000 });
});

test("本文の条・項・号・表・附則を描画し、原文へリンクする", async ({ page }) => {
  await page.route("**/reiki/121002/121002_g002RG00000853.json", (r) => r.fulfill({ json: RICH_DOC }));
  await page.goto(new URL("#/reiki/121002/121002_g002RG00000853?a=art_3", BASE).toString());

  await expect(page.getByTestId("reiki-title")).toHaveText("千葉市アイススケート場設置管理条例", { timeout: 15_000 });
  await expect(page.getByText("制定 2004-09-29")).toBeVisible();
  await expect(page.getByText("例規集 2026-07-01 現在")).toBeVisible();
  // 表はセルとして描画される
  await expect(page.getByRole("cell", { name: "千葉市美浜区新港224番地1" })).toBeVisible();
  // 章見出し・号・細目・第2項
  await expect(page.getByRole("heading", { name: "第2章　指定管理者" })).toBeVisible();
  await expect(page.getByText("維持管理に関する業務")).toBeVisible();
  await expect(page.getByText("清掃", { exact: true })).toBeVisible();
  await expect(page.getByText("市長は、必要があると認めるときは、指示することができる。")).toBeVisible();
  // ?a= で指定した条が強調される
  await expect(page.locator("section#art_3")).toHaveClass(/bg-amber/);
  // 附則は折りたたみ
  await page.getByText("附則（1）").click();
  await expect(page.getByText("この条例は、規則で定める日から施行する。")).toBeVisible();
  await expect(page.getByRole("link", { name: /例規集で原文を見る/ })).toHaveAttribute("href", /g-reiki\.net\/chiba\/reiki_honbun/);
  await expect(page.getByText(/非公式の写し/)).toBeVisible();
});

test("検索の自治体例規セクションに複数自治体の例規がヒットし、本文へ遷移する", async ({ page }) => {
  await page.goto(new URL("#/search?q=個人情報保護", BASE).toString());
  await page.getByRole("tab", { name: /^自治体例規/ }).click();
  const section = page.getByTestId("reiki-hits");
  await expect(section).toBeVisible({ timeout: 30_000 });
  await expect(section.getByText("千葉県 千葉市")).toBeVisible();
  await expect(section.getByText("北海道 留萌市")).toBeVisible();
  await section.getByText("留萌市個人情報保護条例").first().click();
  await expect(page).toHaveURL(/#\/reiki\/012122\/012122_jourei_sample/);
  await expect(page.getByTestId("reiki-title")).toHaveText("留萌市個人情報保護条例");
});

test("サイドバーの自治体例規から例規ビューを開ける", async ({ page }) => {
  await page.goto(new URL("#/", BASE).toString());
  // ダッシュボードのコーパスカードにも同名のリンクがあるので、サイドバーに絞る。
  const link = page.getByRole("navigation").getByRole("link", { name: "自治体例規" });
  await expect(link).toBeVisible({ timeout: 15_000 });
  await link.click();
  await expect(page).toHaveURL(/#\/reiki$/);
  await expect(page.getByRole("heading", { name: "自治体例規" })).toBeVisible();
});

for (const target of [
  { name: "自治体一覧", path: "reiki/index.json", hash: "#/reiki", query: "自治体名…", text: "千葉", result: "千葉市" },
  { name: "例規一覧", path: "reiki/121002/index.json", hash: "#/reiki/121002", query: "題名で絞り込み（Enter で本文検索）", text: "個人情報", result: "千葉市個人情報保護条例" },
]) {
  test(`${target.name}は取得失敗から絞り込みを保って再試行できる`, async ({ page }) => {
    let failed = true;
    await page.route(`**/${target.path}`, route => failed
      ? route.fulfill({ status: 503 }) : route.continue());
    await page.goto(new URL(target.hash, BASE).toString());
    const alert = page.getByRole("alert");
    await expect(alert).toContainText("読み込めませんでした");
    const query = page.getByPlaceholder(target.query);
    await query.fill(target.text);
    failed = false;
    await alert.getByRole("button", { name: "再試行" }).click();
    await expect(page.getByRole("button", { name: new RegExp(target.result) })).toBeVisible();
    await expect(query).toHaveValue(target.text);
    await expect(alert).toHaveCount(0);
  });
}

test("例規本文は取得失敗から同じURLで再試行できる", async ({ page }) => {
  let failed = true;
  await page.route("**/reiki/121002/121002_jourei_sample.json", route => failed
    ? route.fulfill({ status: 503 }) : route.continue());
  await page.goto(new URL("#/reiki/121002/121002_jourei_sample?a=art_1", BASE).toString());
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("本文を読み込めませんでした");
  failed = false;
  await alert.getByRole("button", { name: "再試行" }).click();
  await expect(page.getByTestId("reiki-title")).toHaveText("千葉市個人情報保護条例");
  await expect(page).toHaveURL(/#\/reiki\/121002\/121002_jourei_sample\?a=art_1$/);
});

test("附則への直接リンクは折りたたみを開いて該当条を表示する", async ({ page }) => {
  await page.route("**/reiki/121002/121002_g002RG00000853.json", route => route.fulfill({ json: RICH_DOC }));
  await page.goto(new URL("#/reiki/121002/121002_g002RG00000853?a=s1_p1", BASE).toString());
  await expect(page.locator("section#s1_p1")).toBeVisible();
  await expect(page.locator("section#s1_p1")).toHaveClass(/bg-amber/);
});

test("スマートフォンで例規一覧と長い題名・本文を画面内に表示する", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route("**/reiki/121002/121002_g002RG00000853.json", route => route.fulfill({ json: {
    ...RICH_DOC, title: "長い題名".repeat(40), reiki_number: "条例第" + "1234567890".repeat(15) + "号",
  } }));
  await page.goto(new URL("#/reiki/121002/121002_g002RG00000853", BASE).toString());
  const title = page.getByTestId("reiki-title");
  await expect(title).toBeVisible();
  const bounds = await title.boundingBox();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
  await expect(page.getByPlaceholder("題名で絞り込み（Enter で本文検索）")).toBeVisible();
  const body = page.getByText("本市は、次のとおりスケート場を設置する。", { exact: true });
  await body.scrollIntoViewIfNeeded();
  await expect(body).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

test("本文検索中に入力を変えたら古い結果を表示しない", async ({ page }) => {
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route("**/reiki-search.db*", async route => {
    await gate;
    await route.continue().catch(() => {});
  });
  try {
    await page.goto(new URL("#/reiki/121002", BASE).toString());
    const box = page.getByPlaceholder("題名で絞り込み（Enter で本文検索）");
    await expect(page.getByTestId("reiki-list-count")).toHaveText("1件");
    await box.fill("個人情報");
    const started = page.waitForRequest(/reiki-search\.db/);
    await box.press("Enter");
    await started;
    await box.fill("一致しない題名");
    await expect(page.getByText("該当する例規がありません")).toBeVisible();
    release();
    await box.fill("個人情報");
    await box.press("Enter");
    await expect(page.getByTestId("reiki-list-count")).toHaveText(/^本文 [1-9]\d*件$/, { timeout: 30_000 });
  } finally {
    release();
  }
});

test("本文検索の失敗を該当なしと区別して再試行できる", async ({ page }) => {
  let failed = true;
  await page.route("**/reiki-search.db*", route => failed ? route.fulfill({ status: 503 }) : route.continue());
  await page.goto(new URL("#/reiki/121002", BASE).toString());
  const box = page.getByPlaceholder("題名で絞り込み（Enter で本文検索）");
  await box.fill("個人情報");
  await box.press("Enter");
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("本文検索に失敗しました", { timeout: 20_000 });
  await expect(page.getByText("本文に該当する例規がありません")).toHaveCount(0);
  failed = false;
  await alert.getByRole("button", { name: "再試行" }).click();
  await expect(page.getByTestId("reiki-list-count")).toHaveText(/^本文 [1-9]\d*件$/, { timeout: 30_000 });
  await expect(box).toHaveValue("個人情報");
});

test("例規の本文検索は日本語変換中のEnterでは実行しない", async ({ page }) => {
  await page.goto(new URL("#/reiki/121002", BASE).toString());
  const box = page.getByPlaceholder("題名で絞り込み（Enter で本文検索）");
  await expect(page.getByTestId("reiki-list-count")).toHaveText("1件");
  await box.fill("個人情報");
  await box.dispatchEvent("keydown", { key: "Enter", isComposing: true });
  await expect(page.getByRole("button", { name: "本文検索", exact: true })).toBeEnabled();
  await box.dispatchEvent("keydown", { key: "Enter", keyCode: 229 });
  await expect(page.getByRole("button", { name: "本文検索", exact: true })).toBeEnabled();
  await box.press("Enter");
  await expect(page.getByTestId("reiki-list-count")).toHaveText(/^本文 [1-9]\d*件$/, { timeout: 30_000 });
});

test("例規の本文検索結果でも選択した種別を保つ", async ({ page }) => {
  await page.route("**/reiki/121002/index.json", async route => {
    const response = await route.fetch();
    const index = await response.json();
    await route.fulfill({ json: { ...index, count: 2, reiki: [
      ...index.reiki,
      { ...index.reiki[0], reiki_id: "rule", title: "千葉市個人情報保護規則", kind: "規則" },
    ] } });
  });
  await page.goto(new URL("#/reiki/121002", BASE).toString());
  await page.getByRole("combobox").click();
  await page.getByRole("option", { name: "規則", exact: true }).click();
  const box = page.getByPlaceholder("題名で絞り込み（Enter で本文検索）");
  await box.fill("個人情報");
  await box.press("Enter");
  await expect(page.getByTestId("reiki-list-count")).toHaveText("本文 0件", { timeout: 30_000 });
  await expect(page.getByRole("button", { name: /千葉市個人情報保護条例/ })).toHaveCount(0);
  await page.getByRole("combobox").click();
  await page.getByRole("option", { name: "全種別", exact: true }).click();
  await expect(page.getByTestId("reiki-list-count")).toHaveText(/^本文 [1-9]\d*件$/);
});
