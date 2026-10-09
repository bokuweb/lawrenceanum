import { test, expect } from "@playwright/test";

// wiki (WikiView) の e2e。`lawpub wiki-export` の出力 JSON を route mock する。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const INDEX = {
  schema_version: 1,
  generated_at: "2026-10-08T00:00:00Z",
  pages: [
    { path: "laws/L1", type: "law", title: "予防接種法", description: "副反応救済をめぐる議論", date: null, tags: [] },
    { path: "meetings/kokkai/M1", type: "meeting", title: "参議院 厚生労働委員会 第1号", description: "山田議員が救済拡充を質疑", date: "2026-10-01", tags: ["副反応救済"] },
    { path: "index", type: "index", title: "lawrenceanum wiki", description: "", date: null, tags: [] },
  ],
};

const GRAPH = {
  schema_version: 1,
  nodes: [
    { id: "laws/L1", type: "law", title: "予防接種法", description: "" },
    { id: "meetings/kokkai/M1", type: "meeting", title: "参議院 厚生労働委員会 第1号", description: "" },
  ],
  links: [{ source: "laws/L1", target: "meetings/kokkai/M1" }],
};

const MEETING = {
  path: "meetings/kokkai/M1",
  frontmatter: {
    type: "meeting",
    title: "参議院 厚生労働委員会 第1号",
    description: "山田議員が救済拡充を質疑",
    resource: "https://kokkai.ndl.go.jp/txt/M1",
    date: "2026-10-01",
    tags: ["副反応救済"],
  },
  body: [
    "",
    "# 参議院 厚生労働委員会 第1号",
    "",
    "<!-- lawpub:begin meta -->",
    "| 項目 | 内容 |",
    "|---|---|",
    "| 言及法令 | [予防接種法](../../laws/L1.md) |",
    "<!-- lawpub:end meta -->",
    "",
    "## 要点",
    "",
    "- 山田太郎（無所属）は副反応の救済拡充を求めた[^1]。",
    "",
    "[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべきです」",
    "",
  ].join("\n"),
};

const LAW = {
  path: "laws/L1",
  frontmatter: { type: "law", title: "予防接種法", description: "副反応救済をめぐる議論", law_id: "L1" },
  body: "\n# 予防接種法\n\n## 時系列\n\n| 日付 | 会議 | 概要 |\n|---|---|---|\n| 2026-10-01 | [参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md) | 山田議員が救済拡充を質疑 |\n",
};

async function mock(page: import("@playwright/test").Page) {
  await page.route("**/wiki/index.json", (r) => r.fulfill({ json: INDEX }));
  await page.route("**/wiki/graph.json", (r) => r.fulfill({ json: GRAPH }));
  await page.route("**/wiki/page/meetings/kokkai/M1.json", (r) => r.fulfill({ json: MEETING }));
  await page.route("**/wiki/page/laws/L1.json", (r) => r.fulfill({ json: LAW }));
}

test("wiki トップにナレッジグラフと type 別の件数が出る", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());

  await expect(page.getByRole("heading", { name: "wiki", exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: "法令 1" })).toBeVisible();
  await expect(page.getByRole("button", { name: "会議 1" })).toBeVisible();
  await expect(page.getByTestId("wiki-graph").locator("canvas").first()).toBeVisible({ timeout: 15_000 });
});

test("一覧から会議ページを開くと要点・脚注の出典・つながりが表示される", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());

  await page.getByRole("tab", { name: "一覧" }).click();
  await page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ }).click();

  await expect(page).toHaveURL(/#\/wiki\/meetings\/kokkai\/M1$/);
  await expect(page.getByRole("heading", { name: "参議院 厚生労働委員会 第1号" })).toBeVisible();
  await expect(page.getByText("山田太郎（無所属）は副反応の救済拡充を求めた")).toBeVisible();
  // 脚注は「出典」として表示され、HTML コメント (lawpub マーカー) は出ない。
  await expect(page.getByRole("heading", { name: "出典" })).toBeVisible();
  await expect(page.getByText("「副反応の救済を拡充すべきです」")).toBeVisible();
  await expect(page.getByText("lawpub:begin")).toHaveCount(0);
  // 脚注参照をクリックしてもルーティングは変わらない。
  await page.locator("sup a").first().click();
  await expect(page).toHaveURL(/#\/wiki\/meetings\/kokkai\/M1$/);
  // グラフ上の隣接ページ。
  await expect(page.getByRole("complementary").getByRole("button", { name: /予防接種法/ })).toBeVisible();
});

test("本文の相対リンクで法令ページへ移動できる", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki/meetings/kokkai/M1", BASE).toString());

  await page.getByRole("link", { name: "予防接種法" }).click();
  await expect(page).toHaveURL(/#\/wiki\/laws\/L1$/);
  await expect(page.getByRole("heading", { name: "予防接種法", level: 1 })).toBeVisible();
  await expect(page.getByRole("link", { name: "参議院 厚生労働委員会 第1号" })).toBeVisible();
});

test("法令詳細に wiki への導線が出て、wiki ページへ移動できる", async ({ page }) => {
  const lawId = "129AC0000000089"; // fixture の民法
  await page.route(`**/wiki/page/laws/${lawId}.json`, (r) =>
    r.fulfill({
      json: {
        path: `laws/${lawId}`,
        frontmatter: { type: "law", title: "民法", description: "成年年齢の引下げをめぐる議論", law_id: lawId },
        body: "\n# 民法\n",
      },
    }),
  );
  await mock(page);
  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());

  const band = page.getByRole("button", { name: /wiki.*成年年齢の引下げをめぐる議論/ });
  await expect(band).toBeVisible({ timeout: 15_000 });
  await band.click();
  await expect(page).toHaveURL(new RegExp(`#/wiki/laws/${lawId}$`));
  await expect(page.getByRole("heading", { name: "民法", level: 1 })).toBeVisible();
});

test("wiki ページが無い法令には導線を出さない", async ({ page }) => {
  await page.route("**/wiki/page/laws/**", (r) => r.fulfill({ status: 404, body: "" }));
  await page.goto(new URL("#/laws/129AC0000000089", BASE).toString());
  await expect(page.getByRole("heading", { name: "民法" }).first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: /^wiki/ })).toHaveCount(0);
});
