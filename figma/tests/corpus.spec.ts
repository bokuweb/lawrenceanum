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
