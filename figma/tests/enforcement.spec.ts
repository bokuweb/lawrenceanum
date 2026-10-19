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
      event_id: "stage-1",
      revision_id: "r1",
      explanation_id: "explanation-1",
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

const body = (change: string) => `## 変更内容
${change}[^1]。

## 経緯
確認できる資料がありません。

## 根拠
附則の施行期日による[^1]。

[^1]: [enforcement:LAW/stage#kanpo](https://www.kanpo.go.jp/source.pdf) 「申請期限を三十日から六十日に改める」`;

test("同じ法令の段階施行ごとに解説と出典を表示し、脚注が衝突しない", async ({ page }) => {
  const first = UPCOMING.items[0];
  await page.route("**/enforcement/upcoming.json", route => route.fulfill({ json: {
    ...UPCOMING, items: [first, { ...first, date: "2026-08-01", event_id: "stage-2", revision_id: "r2", explanation_id: "explanation-2" }],
  } }));
  let requests = 0;
  await page.route(`**/wiki/enforcements/${first.law_id}.json`, route => {
    requests++;
    return route.fulfill({ json: { schema_version: 1, law_id: first.law_id, items: [
      { date: first.date, event_id: "stage-1", revision_id: "r1", explanation_id: "explanation-1", path: "enforcements/LAW/one", body: body("申請期限を六十日に変更する") },
      { date: "2026-08-01", event_id: "stage-2", revision_id: "r2", explanation_id: "explanation-2", path: "enforcements/LAW/two", body: body("電子申請を導入する") },
    ] } });
  });
  await page.goto(new URL("#/enforcement", BASE).toString());
  const details = page.locator("details");
  await details.nth(0).locator("summary").click();
  await details.nth(1).locator("summary").click();
  await expect(details.nth(0)).toContainText("申請期限を六十日に変更する");
  await expect(details.nth(0)).not.toContainText("電子申請を導入する");
  await expect(details.nth(1)).toContainText("電子申請を導入する");
  await expect(details.nth(0).getByRole("link", { name: "enforcement:LAW/stage#kanpo" })).toHaveAttribute("href", "https://www.kanpo.go.jp/source.pdf");
  const footnotes = page.locator("a[data-footnote-ref]");
  const firstHref = await footnotes.first().getAttribute("href");
  const secondHref = await details.nth(1).locator("a[data-footnote-ref]").first().getAttribute("href");
  expect(firstHref).not.toBe(secondHref);
  const hash = page.url();
  await footnotes.first().click();
  await expect(page).toHaveURL(hash);
  expect(requests).toBe(1);
});

test("別の改正の解説で補完せず未生成を表示する", async ({ page }) => {
  await mock(page);
  await page.route("**/wiki/enforcements/323AC0000000082.json", route => route.fulfill({ json: {
    schema_version: 1, law_id: "323AC0000000082", items: [{
      date: "2026-07-01", event_id: "other", revision_id: "r1", explanation_id: "other", body: body("別改正の変更"),
    }],
  } }));
  await page.goto(new URL("#/enforcement", BASE).toString());
  await page.locator("details summary").first().click();
  await expect(page.getByText("この施行予定の出典付き解説はまだありません。")).toBeVisible();
  await expect(page.getByText("別改正の変更", { exact: false })).toHaveCount(0);
});

test("法令詳細に直近の施行予定の解説を出典付きで表示する", async ({ page }) => {
  const lawId = "129AC0000000089";
  await page.route(`**/laws/${lawId}/timeline.json`, route => route.fulfill({ json: {
    law_id: lawId, events: [{ event_id: "future-1", event_type: "amendment", target_law_id: lawId,
      effective_date: "2099-01-01", revision_id: "r1", amending_law_title: "民法改正", kanpo: { linked: false } }],
  } }));
  await page.route(`**/wiki/enforcements/${lawId}.json`, route => route.fulfill({ json: {
    schema_version: 1, law_id: lawId, items: [{ date: "2099-01-01", event_id: "future-1", revision_id: "r1",
      explanation_id: "future-1", path: `enforcements/${lawId}/future-1`, body: body("申請期限を六十日に変更する") }],
  } }));
  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());
  const section = page.getByRole("region", { name: "施行予定ごとの解説" });
  await expect(section.getByText("2099-01-01 施行", { exact: true })).toBeVisible();
  await expect(section.getByRole("heading", { name: "変更内容", exact: true })).toBeVisible();
  await expect(section).toContainText("申請期限を六十日に変更する");
});

test("解説の取得失敗は再試行できる", async ({ page }) => {
  await mock(page);
  let failed = true;
  await page.route("**/wiki/enforcements/323AC0000000082.json", route => failed
    ? route.fulfill({ status: 503, body: "unavailable" })
    : route.fulfill({ json: { law_id: "323AC0000000082", items: [] } }));
  await page.goto(new URL("#/enforcement", BASE).toString());
  await page.locator("details summary").first().click();
  await expect(page.getByText("解説を読み込めませんでした。", { exact: false })).toBeVisible();
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByText("この施行予定の出典付き解説はまだありません。")).toBeVisible();
});
