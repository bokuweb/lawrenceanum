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
    { path: "bills/221/B9", type: "bill", title: "要約前の議案", description: "", date: "2026-10-05", tags: [], stub: true },
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

test("要約前 (一覧だけ) のページは既定で一覧に出さず、チェックで表示できる", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());
  await page.getByRole("tab", { name: "一覧" }).click();
  await expect(page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: /要約前の議案/ })).toHaveCount(0);
  // 件数のチップも要約済みだけを数える。
  await expect(page.getByRole("button", { name: "議案 0" })).toBeVisible();

  await page.getByLabel(/要約前（一覧だけ）のページも表示/).check();
  await expect(page.getByRole("button", { name: /要約前の議案/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "議案 1" })).toBeVisible();

  // 切り替えはブラウザに保存され、再読み込みしても維持される。
  await page.reload();
  await expect(page.getByLabel(/要約前（一覧だけ）のページも表示/)).toBeChecked({ timeout: 15_000 });
});

test("要約前の法令ページには法令詳細から導線を出さない", async ({ page }) => {
  const lawId = "129AC0000000089";
  await page.route(`**/wiki/page/laws/${lawId}.json`, (r) =>
    r.fulfill({ json: { path: `laws/${lawId}`, frontmatter: { type: "law", title: "民法", description: "" }, body: "\n# 民法\n", stub: true } }),
  );
  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());
  await expect(page.getByRole("heading", { name: "民法" }).first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: /^wiki/ })).toHaveCount(0);
});

test("「要約前のページも表示」が有効なら、要約前の法令ページにも法令詳細から行ける", async ({ page }) => {
  const lawId = "129AC0000000089";
  await page.addInitScript(() => localStorage.setItem("wiki-show-stubs", "on"));
  await page.route(`**/wiki/page/laws/${lawId}.json`, (r) =>
    r.fulfill({ json: { path: `laws/${lawId}`, frontmatter: { type: "law", title: "民法", description: "" }, body: "\n# 民法\n", stub: true } }),
  );
  await page.goto(new URL(`#/laws/${lawId}`, BASE).toString());
  const band = page.getByRole("button", { name: /^wiki.*要約前（一覧のみ）/ });
  await expect(band).toBeVisible({ timeout: 15_000 });
  await band.click();
  await expect(page).toHaveURL(new RegExp(`#/wiki/laws/${lawId}$`));
  await expect(page.getByTestId("wiki-stub-note")).toBeVisible();
});

test("官報ページは type「官報」で一覧に出て、要約・出典・官報の本文（自動抽出）を表示する", async ({ page }) => {
  // `lawpub wiki-export` が書く kanpo ページ (要約済み) の形。
  const KANPO = {
    path: "kanpo/2026-10-09/g223-p0002-bdc0e8a0",
    frontmatter: {
      type: "kanpo",
      title: "輸出貿易管理令の一部を改正する政令（三二五）",
      description: "別表第二の三の品目を改め規制対象を広げる（公布から7日後に施行）",
      resource: "https://www.kanpo.go.jp/20261009/20261009g00223/pdf/20261009g002230002.pdf",
      date: "2026-10-09",
      laws: ["L1"],
    },
    body: [
      "",
      "# 輸出貿易管理令の一部を改正する政令（三二五）",
      "",
      "## 改正の内容",
      "",
      "<!-- llm:begin -->",
      "**施行**: 公布の日から起算して七日を経過した日から施行する[^1]。",
      "",
      "[^1]: [kanpo:2026-10-09/g223-p0002-bdc0e8a0](https://www.kanpo.go.jp/20261009/20261009g00223/pdf/20261009g002230002.pdf) 「この政令は、公布の日から起算して七日を経過した日から施行する。」",
      "<!-- llm:end -->",
      "",
      "<!-- lawpub:begin meta -->",
      "## 官報の情報（一覧）",
      "",
      "| 項目 | 内容 |",
      "|---|---|",
      "| 改正対象 | [予防接種法](../../laws/L1.md) |",
      "<!-- lawpub:end meta -->",
      "",
      "<!-- lawpub:begin source -->",
      "## 官報の本文（自動抽出）",
      "",
      "````text",
      "輸出貿易管理令（昭和二十四年政令第三百七十八号）の一部を次のように改正する。",
      "附　則",
      "この政令は、公布の日から起算して七日を経過した日から施行する。",
      "````",
      "<!-- lawpub:end source -->",
      "",
    ].join("\n"),
    stub: false,
  };
  await mock(page);
  await page.route("**/wiki/index.json", (r) =>
    r.fulfill({
      json: {
        ...INDEX,
        pages: [...INDEX.pages, { path: KANPO.path, type: "kanpo", title: KANPO.frontmatter.title, description: KANPO.frontmatter.description, date: "2026-10-09", tags: [] }],
      },
    }),
  );
  await page.route(`**/wiki/page/${KANPO.path}.json`, (r) => r.fulfill({ json: KANPO }));

  await page.goto(new URL("#/wiki", BASE).toString());
  await expect(page.getByRole("button", { name: "官報 1" })).toBeVisible({ timeout: 15_000 });
  await page.getByRole("tab", { name: "一覧" }).click();
  await page.getByRole("button", { name: /輸出貿易管理令の一部を改正する政令/ }).click();

  await expect(page).toHaveURL(/#\/wiki\/kanpo\/2026-10-09\/g223-p0002-bdc0e8a0$/);
  await expect(page.getByRole("heading", { name: "輸出貿易管理令の一部を改正する政令（三二五）", level: 1 })).toBeVisible();
  await expect(page.getByRole("heading", { name: "出典" })).toBeVisible();
  // 本文はコードブロック (折り返し・高さ制限付き) で、Markdown として解釈しない。
  const source = page.locator("pre");
  await expect(source).toContainText("輸出貿易管理令（昭和二十四年政令第三百七十八号）の一部を次のように改正する。");
  await expect(source).toHaveCSS("white-space", "pre-wrap");
  await page.getByRole("link", { name: "予防接種法" }).click();
  await expect(page).toHaveURL(/#\/wiki\/laws\/L1$/);
});
