import { test, expect } from "@playwright/test";

// 施行予定 (EnforcementView) の e2e。enforcement/upcoming.json を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const UPCOMING = {
  schema_version: 1,
  generated_at: "2026-06-19T00:00:00Z",
  as_of: "2026-06-19",
  count: 2,
  items: [
    {
      date: "2026-07-01",
      law_id: "323AC0000000082",
      title: "農薬取締法",
      amending_law_title: "環境省設置法の一部を改正する法律",
      date_kind: "effective",
    },
    {
      date: "2026-09-01",
      law_id: "TESTLAW9",
      title: "テスト施行予定法",
      amending_law_title: "テスト改正法",
      date_kind: "scheduled",
    },
  ],
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/enforcement/upcoming.json", (route) => route.fulfill({ json: UPCOMING }));
}

test("施行予定が日付グループで表示される", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/enforcement", BASE).toString());

  await expect(page.getByRole("heading", { name: "施行予定" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("2026-07-01")).toBeVisible();
  await expect(page.getByText("農薬取締法")).toBeVisible();
  await expect(page.getByText("環境省設置法の一部を改正する法律")).toBeVisible();
  // 別日付グループ (scheduled) の項目。
  await expect(page.getByText("2026-09-01")).toBeVisible();
  await expect(page.getByText("テスト施行予定法")).toBeVisible();
});

test("施行予定の項目をクリックすると法令へ遷移する", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/enforcement", BASE).toString());
  await page.getByText("農薬取締法").click();
  await expect(page).toHaveURL(/#\/laws\/323AC0000000082/, { timeout: 15_000 });
});
