import { test, expect } from "@playwright/test";

// パブコメ (PubcommentView) の e2e。pubcomment 系 JSON は fixture を置かず
// page.route で都度モックする (views.spec と同じ方式)。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const INDEX = {
  schema_version: 1,
  count: 1,
  cases: [
    {
      case_id: "2023-00001",
      title: "テスト民法改正パブリックコメント",
      ministry: "法務省",
      result_published: "2026-06-01",
      related_law_name: "民法",
    },
  ],
};

const DETAIL = {
  schema_version: 1,
  case_id: "2023-00001",
  title: "テスト民法改正パブリックコメント",
  ministry: "法務省",
  reception_start: "2026-04-01",
  reception_end: "2026-04-30",
  result_published: "2026-06-01",
  related_law_name: "民法",
  category: "民事",
  opinion_count: 1,
  opinions: [
    {
      item: "第1条関係",
      opinion: "基本原則をより明確にすべきである。",
      ministry_response: "ご意見を踏まえ条文を検討します。",
    },
  ],
  attachments: [
    { name: "結果公示PDF", url: "https://example.com/result.pdf" },
  ],
  source: {
    provider: "egov_pubcomment",
    fetched_at: "2026-06-01T00:00:00Z",
    detail_url: "https://example.com/detail",
  },
};

async function mockPubcomment(page: import("@playwright/test").Page) {
  await page.route("**/pubcomment/index.json", (route) =>
    route.fulfill({ json: INDEX }),
  );
  await page.route("**/pubcomment/2023-00001.json", (route) =>
    route.fulfill({ json: DETAIL }),
  );
}

test("パブコメ一覧に案件と関連法令チップが出る", async ({ page }) => {
  await mockPubcomment(page);
  await page.goto(new URL("#/pubcomment", BASE).toString());

  await expect(page.getByRole("heading", { name: "パブリックコメント" })).toBeVisible({ timeout: 15_000 });
  // 一覧アイテム: 案件名・所管省庁・関連法令名。
  await expect(page.getByText("テスト民法改正パブリックコメント").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("法務省").first()).toBeVisible();
});

test("案件を選ぶと意見と府省の考え方・添付PDF・関連法令が出る", async ({ page }) => {
  await mockPubcomment(page);
  // 直接ディープリンクで詳細を開く (HashRouter)。
  await page.goto(new URL("#/pubcomment/2023-00001", BASE).toString());

  // 意見と府省の考え方 (2 カラム)。
  await expect(page.getByText("寄せられた意見")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("府省の考え方")).toBeVisible();
  await expect(page.getByText("基本原則をより明確にすべきである。")).toBeVisible();
  await expect(page.getByText("ご意見を踏まえ条文を検討します。")).toBeVisible();

  // 添付 PDF リンク (実体は e-Gov の結果公示 PDF)。
  const pdf = page.getByRole("link", { name: /結果公示PDF/ });
  await expect(pdf).toBeVisible();
  await expect(pdf).toHaveAttribute("href", "https://example.com/result.pdf");

  // 関連法令ボタン (クリックで検索へ誘導)。一覧アイテムも「民法」を含むので exact 指定。
  await expect(page.getByRole("button", { name: "民法", exact: true })).toBeVisible();
});

test("一覧から案件をクリックして詳細へ遷移できる", async ({ page }) => {
  await mockPubcomment(page);
  await page.goto(new URL("#/pubcomment", BASE).toString());

  await page.getByText("テスト民法改正パブリックコメント").first().click();
  // URL が詳細へ。
  await expect(page).toHaveURL(/#\/pubcomment\/2023-00001/, { timeout: 15_000 });
  await expect(page.getByText("府省の考え方")).toBeVisible({ timeout: 15_000 });
});
