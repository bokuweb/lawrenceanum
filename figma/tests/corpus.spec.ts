import { test, expect } from "@playwright/test";

// ダッシュボードのコーパスタイルから開く 政府調達 / 審議会 / 財政統計 ビューの e2e。
// 各コーパスの JSON は route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const PROCUREMENT_INDEX = {
  schema_version: 1,
  count: 2,
  items: [
    { item_id: "aXRlbTE", title: "庁舎清掃業務 一式", organization: "国土交通省関東地方整備局", notice_type: "役務", publish_date: "2026-10-06" },
    { item_id: "aXRlbTI", title: "%E5%85%A5%E6%9C%AD%E5%85%AC%E5%91%8A%E5%85%A5%E6%9C%AD%E5%85%AC%E5%91%8A", organization: "放送大学学園", notice_type: "不明", publish_date: "2026-10-05" },
  ],
};

const PROCUREMENT_ITEM = {
  schema_version: 1,
  ...PROCUREMENT_INDEX.items[0],
  deadline: "2026-10-20",
  contract_amount: null,
  contractor: null,
  contract_date: null,
  detail_url: "https://www.kkj.go.jp/example.pdf",
  source: { provider: "kkj_go_jp", fetched_at: "2026-10-06T20:23:12Z" },
};

const SHINGIKAI_INDEX = {
  schema_version: 3,
  count: 2,
  minutes: [
    { minutes_id: "mhlw_x_20261014_212", ministry: "mhlw", committee_id: "x", committee: "労働条件分科会", date: "2026-10-14", status: "scheduled", title: "労働条件分科会 第212回", attachment_count: 0, has_minutes: false, detail_url: "https://www.mhlw.go.jp/a.html" },
    { minutes_id: "shingi06100001_00164", ministry: "moj", committee_id: "y", committee: "法制審議会ー刑事法部会", date: "2026-07-30", status: "held", title: "刑事法部会第１回会議", attachment_count: 1, has_minutes: true, detail_url: "https://www.moj.go.jp/b.html" },
  ],
};

const SHINGIKAI_MEETING = {
  schema_version: 3,
  ...SHINGIKAI_INDEX.minutes[1],
  agenda: "部会長の選出等について",
  summary: null,
  body_text: "",
  minutes_text: "法務大臣からの諮問について説明がなされた。",
  attachments: [
    { attachment_id: "a1", kind: "minutes_text", label: "TXT版", source_url: "https://www.moj.go.jp/b.txt", bytes: 58000 },
  ],
  source: { provider: "shingikai_moj", fetched_at: "2026-10-06T00:00:00Z", detail_url: "https://www.moj.go.jp/b.html" },
};

const BUDGET_INDEX = {
  schema_version: 1,
  count: 1,
  datasets: [{ stats_data_id: "0003360064", title: "国有財産統計（政府出資等の推移）", value_count: 4 }],
};

const value = (category: string, time: string, v: string) => ({
  area: null,
  time,
  category,
  dimensions: { "政府出資・有価証券": category, "時間軸（年度次）": time },
  value: v,
  unit: "億円",
});

const BUDGET_DATASET = {
  schema_version: 2,
  stats_data_id: "0003360064",
  title: "国有財産統計（政府出資等の推移）",
  values: [
    value("合計（Ａ）", "2024年度", "1065891"),
    value("合計（Ａ）", "2023年度", "1047528"),
    value("政府出資", "2024年度", "900000"),
    value("政府出資", "2023年度", "880000"),
  ],
  source: { provider: "estat", fetched_at: "2026-10-06T20:34:26Z", stats_data_id: "0003360064" },
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/procurement/index.json", (r) => r.fulfill({ json: PROCUREMENT_INDEX }));
  await page.route("**/procurement/aXRlbTE.json", (r) => r.fulfill({ json: PROCUREMENT_ITEM }));
  await page.route("**/shingikai/index.json", (r) => r.fulfill({ json: SHINGIKAI_INDEX }));
  await page.route("**/shingikai/moj/shingi06100001_00164.json", (r) => r.fulfill({ json: SHINGIKAI_MEETING }));
  await page.route("**/budget/index.json", (r) => r.fulfill({ json: BUDGET_INDEX }));
  await page.route("**/budget/0003360064.json", (r) => r.fulfill({ json: BUDGET_DATASET }));
}

test("政府調達: 一覧から公告を選ぶと詳細と原文リンクが出る", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/procurement", BASE).toString());
  await expect(page.getByRole("heading", { name: "政府調達" })).toBeVisible({ timeout: 15_000 });

  await page.getByText("庁舎清掃業務 一式").click();
  await expect(page).toHaveURL(/#\/procurement\/aXRlbTE/);
  await expect(page.getByText("2026-10-20")).toBeVisible();
  await expect(page.getByRole("link", { name: /公告原文/ })).toHaveAttribute("href", "https://www.kkj.go.jp/example.pdf");
});

test("政府調達: 改行できない長い件名でも一覧が横にはみ出さない", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/procurement", BASE).toString());
  const row = page.getByRole("button", { name: /放送大学学園/ });
  await expect(row).toBeVisible({ timeout: 15_000 });
  // 一覧カラムは w-96 (384px)。件名が折り返されず行が広がると、カラム外にはみ出す。
  const box = await row.boundingBox();
  expect(box!.width).toBeLessThanOrEqual(384);
});

test("審議会: 府省・開催予定バッジ付きで並び、会議を開くと議題と議事録が出る", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/shingikai", BASE).toString());
  await expect(page.getByRole("heading", { name: "審議会議事録" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("開催予定")).toBeVisible();

  await page.getByText("刑事法部会第１回会議").click();
  await expect(page).toHaveURL(/#\/shingikai\/moj\/shingi06100001_00164/);
  await expect(page.getByText("部会長の選出等について")).toBeVisible();
  await expect(page.getByText("法務大臣からの諮問について説明がなされた。")).toBeVisible();
  await expect(page.getByRole("link", { name: /TXT版/ })).toHaveAttribute("href", "https://www.moj.go.jp/b.txt");
});

test("財政統計: 統計を選ぶと既定の系列の時系列が出て、次元を切り替えられる", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/budget/0003360064", BASE).toString());
  await expect(page.getByRole("heading", { name: "国有財産統計（政府出資等の推移）" })).toBeVisible({ timeout: 15_000 });

  await expect(page.getByRole("cell", { name: "1,065,891" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "900,000" })).toHaveCount(0);

  await page.getByRole("combobox").click();
  await page.getByRole("option", { name: "政府出資" }).click();
  await expect(page.getByRole("cell", { name: "900,000" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "1,065,891" })).toHaveCount(0);
});

test("財政統計: 区分変更時に地域を有効な組み合わせへ補正する", async ({ page }) => {
  await mock(page);
  await page.route("**/budget/0003360064.json", r => r.fulfill({ json: {
    ...BUDGET_DATASET,
    values: [
      { ...value("政府出資", "2024年度", "100"), dimensions: { 区分: "政府出資", 地域: "東京", 年度: "2024年度" } },
      { ...value("有価証券", "2024年度", "200"), dimensions: { 区分: "有価証券", 地域: "大阪", 年度: "2024年度" } },
    ],
  } }));
  await page.goto(new URL("#/budget/0003360064", BASE).toString());
  await expect(page.getByRole("cell", { name: "100", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "地域", exact: true })).toContainText("東京");

  await page.getByRole("combobox", { name: "区分", exact: true }).click();
  await page.getByRole("option", { name: "有価証券", exact: true }).click();
  await expect(page.getByRole("cell", { name: "200", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "地域", exact: true })).toContainText("大阪");
  await page.getByRole("combobox", { name: "地域", exact: true }).click();
  await expect(page.getByRole("option", { name: "東京", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.getByRole("combobox", { name: "区分", exact: true }).click();
  await page.getByRole("option", { name: "政府出資", exact: true }).click();
  await expect(page.getByRole("cell", { name: "100", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "地域", exact: true })).toContainText("東京");
});

test("財政統計: 小数・大きな整数・欠測表記を元の精度のまま表示する", async ({ page }) => {
  await mock(page);
  const numbers = ["1234.567890", "9007199254740993", "-1234567.89012", "***", "-", "1.23e20"];
  await page.route("**/budget/0003360064.json", r => r.fulfill({ json: {
    ...BUDGET_DATASET, values: numbers.map((n, i) => value("合計", `${2024 - i}年度`, n)),
  } }));
  await page.goto(new URL("#/budget/0003360064", BASE).toString());
  for (const formatted of ["1,234.567890", "9,007,199,254,740,993", "-1,234,567.89012", "***", "-", "1.23e20"]) {
    await expect(page.getByRole("cell", { name: formatted, exact: true })).toBeVisible();
  }
});

for (const resource of ["index", "0003360064"]) {
  test(`財政統計: ${resource} の取得失敗を区別し再試行できる`, async ({ page }) => {
    await mock(page);
    let attempts = 0;
    await page.route(`**/budget/${resource}.json`, r => ++attempts === 1
      ? r.fulfill({ status: 503, body: "unavailable" })
      : r.fulfill({ json: resource === "index" ? BUDGET_INDEX : BUDGET_DATASET }));
    await page.goto(new URL(`#/budget${resource === "index" ? "" : "/0003360064"}`, BASE).toString());
    const alert = page.getByRole("alert");
    await expect(alert).toContainText("読み込めませんでした");
    await expect(page.getByText("データがありません", { exact: true })).toHaveCount(0);
    await alert.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(alert).toHaveCount(0);
    if (resource === "index") {
      await expect(page.getByRole("button", { name: /国有財産統計/ })).toBeVisible();
    } else {
      await expect(page.getByRole("cell", { name: "1,065,891", exact: true })).toBeVisible();
    }
    expect(attempts).toBe(2);
  });
}

test("財政統計: 狭い画面でも一覧と時系列表を画面内で操作できる", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mock(page);
  await page.goto(new URL("#/budget", BASE).toString());
  await page.getByRole("button", { name: /国有財産統計/ }).click();
  const table = page.getByRole("table");
  await expect(page.getByRole("cell", { name: "1,065,891", exact: true })).toBeVisible();
  const bounds = await table.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
});

for (const config of [
  { label: "政府調達", path: "procurement", detailPath: "procurement/aXRlbTE", index: PROCUREMENT_INDEX, detail: PROCUREMENT_ITEM,
    indexURL: "**/procurement/index.json", detailURL: "**/procurement/aXRlbTE.json", placeholder: "件名・調達機関…",
    query: "庁舎", title: PROCUREMENT_ITEM.title, otherTitle: PROCUREMENT_INDEX.items[1].title, detailText: "2026-10-20", source: "公告原文" },
  { label: "審議会", path: "shingikai", detailPath: "shingikai/moj/shingi06100001_00164", index: SHINGIKAI_INDEX, detail: SHINGIKAI_MEETING,
    indexURL: "**/shingikai/index.json", detailURL: "**/shingikai/moj/shingi06100001_00164.json", placeholder: "会議名・審議会…",
    query: "刑事法", title: SHINGIKAI_MEETING.title, otherTitle: SHINGIKAI_INDEX.minutes[0].title, detailText: "法務大臣からの諮問について説明がなされた。", source: "法務省 会議ページ" },
]) {
  test(`${config.label}: 一覧の取得失敗を表示し、絞り込みを維持して再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(config.indexURL, route => fail
      ? route.fulfill({ status: 503, body: "unavailable" })
      : route.fulfill({ json: config.index }));
    await page.goto(new URL(`#/${config.path}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    await page.getByPlaceholder(config.placeholder).fill(config.query);
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByRole("button", { name: new RegExp(config.title) })).toBeVisible();
    await expect(page.getByText(config.otherTitle, { exact: true })).toHaveCount(0);
    await expect(page.getByPlaceholder(config.placeholder)).toHaveValue(config.query);
    await expect(page.getByRole("alert")).toHaveCount(0);
  });

  test(`${config.label}: 詳細の取得失敗からその場で再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(config.indexURL, route => route.fulfill({ json: config.index }));
    await page.route(config.detailURL, route => fail
      ? route.fulfill({ status: 503, body: "unavailable" })
      : route.fulfill({ json: config.detail }));
    await page.goto(new URL(`#/${config.detailPath}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByText(config.detailText, { exact: true })).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(page).toHaveURL(new URL(`#/${config.detailPath}`, BASE).toString());
  });

  test(`${config.label}: スマートフォンでも一覧と詳細が画面内で操作できる`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.route(config.indexURL, route => route.fulfill({ json: config.index }));
    await page.route(config.detailURL, route => route.fulfill({ json: config.detail }));
    await page.goto(new URL(`#/${config.path}`, BASE).toString());
    const row = page.getByRole("button", { name: new RegExp(config.title) });
    await expect(row).toBeVisible();
    const box = await row.boundingBox();
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
    await row.click();
    const source = page.getByRole("link", { name: config.source, exact: true });
    await expect(source).toBeVisible();
    const detailBox = await source.boundingBox();
    expect(detailBox!.x).toBeGreaterThanOrEqual(0);
    expect(detailBox!.x + detailBox!.width).toBeLessThanOrEqual(390);
    expect(detailBox!.y + detailBox!.height).toBeLessThanOrEqual(844);
    await expect(page.getByText(config.detailText, { exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
}
