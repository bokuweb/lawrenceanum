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

test("新規判例のWikiを判例フィルターから開き関連法令へ移動する", async ({ page }) => {
  await mock(page);
  const entry = { path: "hanrei/5", type: "hanrei", title: "損害賠償請求事件", description: "民法709条に関する判断", date: "2026-10-24", tags: [] };
  await page.route("**/wiki/index.json", r => r.fulfill({ json: { ...INDEX, pages: [...INDEX.pages, entry] } }));
  await page.route("**/wiki/page/hanrei/5.json", r => r.fulfill({ json: {
    path: entry.path, frontmatter: { ...entry, source_scope: "判示事項・裁判要旨（判決全文未収録）", summary_input: { total_chars: 50000, included_chars: 36000 }, source_fingerprint: "v1", llm_source_fingerprint: "v1" },
    body: "# 損害賠償請求事件\n\n## 要約\n\n民法709条に関する判断。\n\n## 裁判所の公開情報（原文）\n\n### 裁判要旨\n\n> 公開された裁判要旨です。\n\n[関連法令](../laws/L1.md)\n",
  } }));
  await page.goto(new URL("#/wiki", BASE).toString());
  await expect(page.getByRole("button", { name: /判例.*1/ })).toBeVisible();
  await page.getByRole("tab", { name: "一覧", exact: true }).click();
  await page.getByRole("button", { name: /損害賠償請求事件/ }).click();
  await expect(page).toHaveURL(/#\/wiki\/hanrei\/5$/);
  const evidence = page.getByTestId("wiki-hanrei-scope");
  await expect(evidence).toContainText("判示事項・裁判要旨（判決全文未収録）");
  await expect(evidence).toContainText("原資料 50,000 文字中 36,000 文字");
  await expect(evidence).toContainText("抜粋外の理由・留保・反対意見は未確認");
  await expect(evidence).toContainText("AIによる要約・解説");
  await expect(evidence).toContainText("LLM（大規模言語モデル）が生成しました。");
  await expect(page.getByRole("heading", { name: "裁判所の公開情報（原文）", exact: true })).toBeVisible();
  await expect(page.getByTestId("wiki-evidence").getByRole("alert")).toHaveCount(0);
  await page.getByRole("link", { name: "関連法令", exact: true }).click();
  await expect(page).toHaveURL(/#\/wiki\/laws\/L1$/);
});

test("本文に会議リンクがなくても引用元へ移動でき、未収録と要約前を区別する", async ({ page }) => {
  await mock(page);
  await page.route("**/wiki/page/laws/L1.json", r => r.fulfill({ json: {
    ...LAW, body: "救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「救済を拡充すべきです」",
    citation_sources: [
      { path: "meetings/kokkai/M1", title: MEETING.frontmatter.title, date: "2026-10-01", refs: ["kokkai:M1_001", "kokkai:M1_002"], availability: "available", summary_source_status: "stale" },
      { path: "meetings/shingikai/S1", title: "要約前の会議", date: null, refs: ["shingikai:S1#0"], availability: "stub", summary_source_status: "pending" },
      { path: "meetings/shingikai/missing", title: null, date: null, refs: ["shingikai:missing#0"], availability: "missing", summary_source_status: null },
    ],
  } }));
  await page.goto(new URL("#/wiki/laws/L1", BASE).toString());
  const sources = page.getByTestId("wiki-citation-sources");
  await expect(sources).toContainText("このページの記述・引用がその版に対応するとは限りません");
  await expect(sources).toContainText("kokkai:M1_001、kokkai:M1_002");
  await expect(sources.getByRole("alert")).toHaveCount(2);
  await expect(sources).toContainText("会議ページ未公開・未収録");
  await expect(sources).toContainText("会議の要約前");
  await expect(sources.getByRole("button")).toHaveCount(2);
  await sources.getByRole("button", { name: MEETING.frontmatter.title, exact: true }).click();
  await expect(page).toHaveURL(/#\/wiki\/meetings\/kokkai\/M1$/);
  await expect(page.getByRole("heading", { level: 1, name: MEETING.frontmatter.title, exact: true })).toBeVisible();
});

for (const status of ["current", "untracked"] as const) {
  test(`引用元の状態 ${status} から引用する側の版対応を推定しない`, async ({ page }) => {
    await mock(page);
    await page.route("**/wiki/page/laws/L1.json", r => r.fulfill({ json: {
      ...LAW, citation_sources: [{ path: "meetings/kokkai/M1", title: MEETING.frontmatter.title, date: "2026-10-01", refs: ["kokkai:M1_001"], availability: "available", summary_source_status: status }],
    } }));
    await page.goto(new URL("#/wiki/laws/L1", BASE).toString());
    const sources = page.getByTestId("wiki-citation-sources");
    await expect(sources).toBeVisible();
    await expect(sources.getByRole("alert")).toHaveCount(0);
    await expect(sources).toContainText("このページの記述・引用がその版に対応するとは限りません");
    if (status === "untracked") await expect(sources).toContainText("原資料の版は未記録");
    await expect(sources).not.toContainText("対応しています");
  });
}

test("wiki トップにナレッジグラフと type 別の件数が出る", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());

  await expect(page.getByRole("heading", { name: "wiki", exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: "法令 1" })).toBeVisible();
  await expect(page.getByRole("button", { name: "会議 1" })).toBeVisible();
  await expect(page.getByTestId("wiki-graph").locator("canvas").first()).toBeVisible({ timeout: 15_000 });
});

test("会議の未完了理由と集計範囲が読める", async ({ page }) => {
  await mock(page);
  await page.route("**/wiki/index.json", (r) => r.fulfill({ json: { ...INDEX, intake: {
    date: "2026-10-08", since: "2026-09-01", scope: "窓外・未収集資料は集計外。",
    entries: { "kokkai:M1": "completed", "kokkai:M2": "budget_pending", "kokkai:M3": "source_unavailable", "kokkai:M4": "summary_pending", "kokkai:M5": "source_unchanged" },
  } } }));
  await page.goto(new URL("#/wiki", BASE).toString());
  const intake = page.getByTestId("wiki-intake");
  await expect(intake.locator("summary")).toContainText("未完了 3 件");
  await intake.locator("summary").click();
  await expect(intake).toContainText("窓外・未収集資料は集計外");
  await expect(intake).toContainText("kokkai:M2：上限により繰り越し");
  await expect(intake).toContainText("kokkai:M3：資料取得待ち");
  await expect(intake).toContainText("kokkai:M4：要約未完了");
  await expect(intake).not.toContainText("kokkai:M1");
  await expect(intake).not.toContainText("kokkai:M5");
});

test("会議の原資料と要約の版の不一致を表示する", async ({ page }) => {
  await mock(page);
  for (const [source, summary, message] of [
    ["sha256:new", "sha256:old", "原資料の版に対応する要約の更新が未完了です"],
    ["sha256:new", "sha256:new", "最後に取得した資料の版に対応しています"],
    [undefined, undefined, "原資料の版の記録がありません"],
  ] as const) {
    await page.route("**/wiki/page/meetings/kokkai/M1.json", (r) => r.fulfill({ json: {
      ...MEETING, frontmatter: { ...MEETING.frontmatter, source_fingerprint: source, llm_source_fingerprint: summary },
    } }));
    await page.goto(new URL("#/wiki/meetings/kokkai/M1", BASE).toString());
    await page.reload();
    await expect(page.getByTestId("wiki-evidence")).toContainText(message);
    if (source && source !== summary) await expect(page.getByRole("alert")).toContainText(message);
  }
});

test("主張ごとに留保・意味の判定状態と立場を区別して原文へ戻れる", async ({ page }) => {
  await mock(page);
  const statuses = ["unreviewed", "needs_review", "unsupported", "indeterminate", "llm_supported"];
  const roles = ["government", "industry", "expert", "witness", "unknown"];
  await page.route("**/wiki/page/meetings/kokkai/M1.json", (r) => r.fulfill({ json: {
    ...MEETING,
    frontmatter: { ...MEETING.frontmatter, source_scope: { total_units: 8, included_units: 5, partial_refs: ["kokkai:M1_001"], omitted_refs: ["kokkai:M1_006", "kokkai:M1_007", "kokkai:M1_008"], limitations: "抜粋外の条件と反対意見は未確認。" } },
    claim_assessments: statuses.map((status, i) => ({
      text: `主張${i + 1}`, kind: i === 0 ? "government_view" : "opinion", attribution: `kokkai:M1_00${i + 1}`,
      review_status: status, review_note: "実施には条件があるため原文の確認が必要。",
      sources: [{ ref: `kokkai:M1_00${i + 1}`, url: `https://kokkai.ndl.go.jp/txt/M1/${i + 1}`, speaker: `発言者${i + 1}`, group: i === 1 ? "業界協会" : null, position: i === 0 ? "国務大臣" : null, date: "2026-10-01", role: roles[i], quote: "ただし財源の確保が条件です。" }],
    })),
  } }));
  await page.goto(new URL("#/wiki/meetings/kokkai/M1", BASE).toString());
  const evidence = page.getByTestId("wiki-evidence");
  await expect(evidence).toContainText("意味的な支持を保証しません");
  await expect(evidence).toContainText("独立レビューではありません");
  await evidence.getByText(/抜粋の確認範囲/).click();
  await expect(evidence).toContainText("全 8 発言中 5 発言、部分収録 1、未収録 3");
  await expect(evidence.getByText("未収録：kokkai:M1_006、kokkai:M1_007、kokkai:M1_008")).toBeVisible();
  const labels = ["支持関係は未審査", "要確認", "根拠が支持しないとの判定", "判定不能", "LLM は支持と判定（暫定・人による確認なし）"];
  for (let i = 0; i < labels.length; i++) {
    const summary = evidence.locator("summary").filter({ hasText: `主張${i + 1} — ${labels[i]}` });
    await expect(summary).toBeVisible();
    await summary.click();
  }
  await expect(evidence).toContainText("国務大臣 / 政府側（役職表記による） / 2026-10-01");
  await expect(evidence).toContainText("業界協会 / 役職未確認 / 業界側（所属表記による）");
  await expect(evidence).toContainText("有識者（役職表記による）");
  await expect(evidence).toContainText("参考人（立場未特定）");
  await expect(evidence).toContainText("立場未確認");
  await expect(evidence.getByRole("link", { name: "原文（kokkai:M1_002）" })).toHaveAttribute("href", "https://kokkai.ndl.go.jp/txt/M1/2");
});

test("グラフは表示範囲を絞って描き、ページからは隣接だけの図を開ける", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());

  // 毛玉にならないよう、既定ではつながりの多い順に一定数だけ描く。
  const budget = page.getByTestId("wiki-graph-budget");
  await expect(budget).toBeVisible({ timeout: 15_000 });
  await expect(budget).toHaveValue("250");
  await expect(page.getByText("2 件を表示")).toBeVisible();

  // 絞り込みはグラフにも効く (一致したページとその直接のつながりだけを描く)。
  await page.getByPlaceholder("タイトル・概要・タグで絞り込み（グラフにも効きます）").fill("予防接種法");
  await expect(page.getByTestId("wiki-graph-note")).toHaveText(
    "「予防接種法」に一致 1 件とそのつながり、あわせて 2 件を表示",
  );
  await page.getByPlaceholder("タイトル・概要・タグで絞り込み（グラフにも効きます）").fill("");
  await expect(page.getByTestId("wiki-graph-note")).toHaveText("2 件を表示");

  // ページを開いて、そのページとつながるぶんだけの図を出す。
  await page.getByRole("tab", { name: "一覧" }).click();
  await page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ }).click();
  await page.getByRole("button", { name: "つながりを図で見る" }).click();
  await expect(page.getByTestId("wiki-graph").locator("canvas").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("選んだページとつながる 2 件を表示しています。")).toBeVisible();
});

test("一覧から会議ページを開くと要点・脚注の出典・つながりが表示される", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki", BASE).toString());

  await page.getByRole("tab", { name: "一覧" }).click();
  await page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ }).click();

  await expect(page).toHaveURL(/#\/wiki\/meetings\/kokkai\/M1$/);
  await expect(page.getByRole("heading", { name: "参議院 厚生労働委員会 第1号" })).toBeVisible();
  await expect(page.getByText("山田太郎（無所属）は副反応の救済拡充を求めた")).toBeVisible();
  await expect(page.getByTestId("wiki-evidence")).toContainText("取得範囲・主張ごとの根拠情報は未整備");
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

for (const fixture of [
  { endpoint: "index.json", json: INDEX, path: "", message: "Wiki の一覧を読み込めませんでした。", heading: "wiki" },
  { endpoint: "graph.json", json: GRAPH, path: "", message: "グラフを読み込めませんでした。", heading: "wiki" },
  { endpoint: "page/laws/L1.json", json: LAW, path: "/laws/L1", message: "ページを読み込めませんでした:", heading: "予防接種法" },
]) {
  test(`Wiki の取得失敗から再試行できる: ${fixture.endpoint}`, async ({ page }) => {
    await mock(page);
    let fail = true;
    await page.route(`**/wiki/${fixture.endpoint}`, route => fail
      ? route.fulfill({ status: 503, body: "unavailable" })
      : route.fulfill({ json: fixture.json }));
    await page.goto(new URL(`#/wiki${fixture.path}`, BASE).toString());
    const alert = page.getByRole("alert").filter({ hasText: fixture.message });
    await expect(alert).toBeVisible();
    if (fixture.endpoint === "graph.json") {
      await page.getByRole("tab", { name: "一覧" }).click();
      await expect(page.getByRole("button", { name: /予防接種法/ })).toBeVisible();
      await page.getByRole("tab", { name: "グラフ" }).click();
    }
    fail = false;
    await alert.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(alert).toHaveCount(0);
    await expect(page.getByRole("heading", { name: fixture.heading, exact: true, level: 1 })).toBeVisible();
    if (fixture.endpoint === "graph.json") {
      await expect(page.getByTestId("wiki-graph").locator("canvas").first()).toBeVisible();
    } else if (fixture.endpoint === "index.json") {
      await expect(page.getByRole("button", { name: "法令 1", exact: true })).toBeVisible();
    }
  });
}

test("Wiki の外部リンクと不正なフラグメントを安全に扱う", async ({ page }) => {
  await mock(page);
  await page.route("**/wiki/page/laws/L1.json", route => route.fulfill({ json: {
    ...LAW, body: "[外部資料](//example.com/source)\n\n[壊れた参照](#%E0%A4%A)",
  } }));
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(new URL("#/wiki/laws/L1", BASE).toString());
  const external = page.getByRole("link", { name: "外部資料" });
  await expect(external).toHaveAttribute("href", "//example.com/source");
  await expect(external).toHaveAttribute("target", "_blank");
  await page.getByRole("link", { name: "壊れた参照" }).click();
  await expect(page).toHaveURL(/#\/wiki\/laws\/L1$/);
  expect(errors).toEqual([]);
});

test("動きを抑える設定では Wiki の脚注へ即座に移動する", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await mock(page);
  await page.goto(new URL("#/wiki/meetings/kokkai/M1", BASE).toString());
  await expect(page.locator("sup a").first()).toBeVisible();
  await page.evaluate(() => {
    const original = Element.prototype.scrollIntoView;
    (window as any).__scrollBehaviors = [];
    Element.prototype.scrollIntoView = function (options) {
      (window as any).__scrollBehaviors.push(typeof options === "object" ? options?.behavior : null);
      return original.call(this, options);
    };
  });
  await page.locator("sup a").first().click();
  expect(await page.evaluate(() => (window as any).__scrollBehaviors)).toContain("instant");
  await expect(page).toHaveURL(/#\/wiki\/meetings\/kokkai\/M1$/);
});


test("wiki のフィルター・検索語・タブ・表示範囲を URL に保存して復元する", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki?keep=value", BASE).toString());
  await page.getByTestId("wiki-graph-budget").selectOption("500");
  await page.getByRole("button", { name: "会議 1", exact: true }).click();
  await page.getByLabel(/要約前（一覧だけ）のページも表示/).check();
  await page.getByPlaceholder("タイトル・概要・タグで絞り込み（グラフにも効きます）").fill("予防接種法");
  await page.getByRole("tab", { name: "一覧", exact: true }).click();
  await expect(page).toHaveURL(/tab=list/);

  const url = page.url();
  const params = new URLSearchParams(new URL(url).hash.split("?")[1]);
  expect(params.get("q")).toBe("予防接種法");
  expect(params.get("types")?.split(",")).not.toContain("meeting");
  expect(params.get("stubs")).toBe("1");
  expect(params.get("tab")).toBe("list");
  expect(params.get("budget")).toBe("500");
  expect(params.get("keep")).toBe("value");

  // ローカル設定が無い別のブラウザでも、URL だけから同じ状態に戻る。
  await page.evaluate(() => localStorage.removeItem("wiki-show-stubs"));
  await page.reload();
  await expect(page.getByRole("tab", { name: "一覧", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByPlaceholder("タイトル・概要・タグで絞り込み（グラフにも効きます）")).toHaveValue("予防接種法");
  await expect(page.getByRole("button", { name: "会議 1", exact: true })).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByLabel(/要約前（一覧だけ）のページも表示/)).toBeChecked();
  await expect(page.getByRole("button", { name: /予防接種法/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ })).toHaveCount(0);
  await page.getByRole("tab", { name: "グラフ", exact: true }).click();
  await expect(page.getByTestId("wiki-graph-budget")).toHaveValue("500");
  await expect(page.getByTestId("wiki-graph-note")).toHaveText("「予防接種法」に一致 1 件とそのつながり、あわせて 1 件を表示");

  await page.getByTestId("wiki-graph-budget").selectOption("Infinity");
  await expect(page).toHaveURL(/budget=all/);
  await page.reload();
  await expect(page.getByTestId("wiki-graph-budget")).toHaveValue("Infinity");
  await page.getByLabel(/要約前（一覧だけ）のページも表示/).uncheck();
  await expect(page).toHaveURL(/stubs=0/);
  await page.reload();
  await expect(page.getByLabel(/要約前（一覧だけ）のページも表示/)).not.toBeChecked();
});

test("wiki の種類の全解除・全選択と検索解除を URL に反映する", async ({ page }) => {
  await mock(page);
  await page.goto(new URL("#/wiki?types=law&tab=list&q=予防接種法", BASE).toString());
  const law = page.getByRole("button", { name: "法令 1", exact: true });
  await law.click();
  await expect(page).toHaveURL(/types=(&|$)/);
  await page.reload();
  await expect(law).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByText("該当するページはありません。", { exact: true })).toBeVisible();
  const chips = page.locator("button[aria-pressed]");
  for (const chip of await chips.all()) await chip.click();
  expect(new URLSearchParams(new URL(page.url()).hash.split("?")[1]).has("types")).toBe(false);
  await page.getByPlaceholder("タイトル・概要・タグで絞り込み（グラフにも効きます）").fill("");
  expect(new URLSearchParams(new URL(page.url()).hash.split("?")[1]).has("q")).toBe(false);
  await page.reload();
  for (const chip of await chips.all()) await expect(chip).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: /参議院 厚生労働委員会 第1号/ })).toBeVisible();
});

test("wiki は URL の明示的な要約前の非表示を優先し、不正な値は既定に戻す", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("wiki-show-stubs", "on"));
  await mock(page);
  await page.goto(new URL("#/wiki?types=unknown&tab=unknown&budget=-1&stubs=0", BASE).toString());
  await expect(page.getByRole("tab", { name: "グラフ", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("wiki-graph-budget")).toHaveValue("250");
  await expect(page.getByLabel(/要約前（一覧だけ）のページも表示/)).not.toBeChecked();
  await expect(page.getByRole("button", { name: "法令 1", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "会議 1", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.goto(new URL("#/wiki?types=law,unknown&tab=list", BASE).toString());
  await expect(page.getByRole("button", { name: "会議 1", exact: true })).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByRole("button", { name: /予防接種法/ })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole("button", { name: "会議 1", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("tab", { name: "グラフ", exact: true })).toHaveAttribute("aria-selected", "true");
});
