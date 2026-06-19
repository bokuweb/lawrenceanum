import { test, expect } from "@playwright/test";

// 議案 (GianView, 法案審議トラッキング) の e2e。gian の JSON を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const INDEX = {
  schema_version: 1,
  count: 2,
  bills: [
    {
      bill_id: "1DE153E",
      session: 221,
      bill_type: "衆法",
      number: "1",
      title: "政治資金規正法の一部を改正する法律案",
      committee: "政治改革に関する特別",
      result: null,
      status: null,
      latest_date: "2026-06-12",
      latest_event: "委員会付託(衆)",
      detail_url: "https://www.shugiin.go.jp/keika/1DE153E.htm",
    },
    {
      bill_id: "ABC999",
      session: 221,
      bill_type: "閣法",
      number: "15",
      title: "テスト閣法案",
      latest_date: "2026-06-05",
      latest_event: "公布",
      detail_url: "https://www.shugiin.go.jp/keika/ABC999.htm",
    },
  ],
};

const BILL = {
  schema_version: 1,
  bill_id: "1DE153E",
  session: 221,
  bill_type: "衆法",
  number: "1",
  title: "政治資金規正法の一部を改正する法律案",
  submitter: "落合 貴之君外四名",
  committee: "政治改革に関する特別",
  latest_date: "2026-06-12",
  latest_event: "委員会付託(衆)",
  fields: [
    { key: "議案件名", value: "政治資金規正法の一部を改正する法律案" },
    { key: "衆議院付託年月日／衆議院付託委員会", value: "令和 8年 6月12日 ／ 政治改革に関する特別" },
  ],
  source: { provider: "shugiin", fetched_at: "2026-06-19T00:00:00Z", detail_url: "https://www.shugiin.go.jp/keika/1DE153E.htm" },
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/gian/index.json", (r) => r.fulfill({ json: INDEX }));
  await page.route("**/gian/221/1DE153E.json", (r) => r.fulfill({ json: BILL }));
}

test("議案一覧に種別バッジ付きで法案が並ぶ", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/gian", BASE).toString());

  await expect(page.getByRole("heading", { name: "議案（法案審議）" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("政治資金規正法の一部を改正する法律案").first()).toBeVisible();
  await expect(page.getByText("テスト閣法案")).toBeVisible();
});

test("議案を選ぶと審議経過と原文リンクが出る", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/gian/221/1DE153E", BASE).toString());

  // 審議経過テーブル（KOMOKU 行）。
  await expect(page.getByText("衆議院付託年月日／衆議院付託委員会")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("政治改革に関する特別").first()).toBeVisible();
  // 原文 (衆議院 議案情報) リンク。
  const link = page.getByRole("link", { name: /衆議院 議案情報/ });
  await expect(link).toHaveAttribute("href", "https://www.shugiin.go.jp/keika/1DE153E.htm");
});

test("一覧から議案をクリックして詳細へ遷移する", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/gian", BASE).toString());
  await page.getByText("政治資金規正法の一部を改正する法律案").first().click();
  await expect(page).toHaveURL(/#\/gian\/221\/1DE153E/, { timeout: 15_000 });
});
