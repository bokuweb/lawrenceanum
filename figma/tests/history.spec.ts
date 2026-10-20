import { test, expect } from "@playwright/test";

// 「版閲覧＋任意2版 diff」(compare view) の e2e。
// compare view は versions.json で版一覧を引き、選択中の 2 版の本文だけを
// 個別 revision JSON (`revisions/{id}.json`) として on-demand 取得して diff する。
// (旧実装は全版を 1 つの history.ndjson.zst に束ねブラウザで zstd 展開していたが、
//  大法令だと展開後 ~200MB・fzstd が大窓 LDM を誤展開して数秒固まり本文も壊れて
//  比較が成立しなかった。比較に要るのは常に 2 版なので個別取得に切り替えた。)
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
// 複数版を持つ法令 (民法)。fixture/本番いずれにも versions.json + revisions がある想定。
const LAW = process.env.HISTORY_LAW ?? "129AC0000000089";

const OTHER_LAW = "comparison-other";
const comparisonVersions = (lawId: string) => ({ law_id: lawId, versions: ["20200101_old", "20210101_new"].map(suffix => ({
  revision_id: `${lawId}_${suffix}`, path: `laws/${lawId}/revisions/${lawId}_${suffix}.json`,
})) });
const comparisonBody = (lawId: string, revision: string) => ({
  law_id: lawId, title: lawId === LAW ? "比較用アルファ法" : "比較用ベータ法", revision_id: revision,
  articles: [{ article_id: "art_1", article_no: "第一条", caption: "比較", paragraphs: [{ paragraph_no: "1", text: `${lawId} ${revision.endsWith("old") ? "改正前" : "改正後"}の条文です。` }] }],
});
async function comparisonFixture(page: import("@playwright/test").Page) {
  await page.route("**/laws/index.json", route => route.fulfill({ json: { laws: [
    { law_id: LAW, title: "比較用アルファ法" }, { law_id: OTHER_LAW, title: "比較用ベータ法" },
  ] } }));
  await page.route("**/laws/*/versions.json", route => {
    const lawId = new URL(route.request().url()).pathname.split("/").at(-2)!;
    return route.fulfill({ json: comparisonVersions(lawId) });
  });
  await page.route("**/laws/*/revisions/*.json", route => {
    const parts = new URL(route.request().url()).pathname.split("/");
    return route.fulfill({ json: comparisonBody(parts.at(-3)!, parts.at(-1)!.replace(/\.json$/, "")) });
  });
}

test("比較の版一覧を再試行し、取得失敗をサンプル条文に置き換えない", async ({ page }) => {
  await comparisonFixture(page);
  let failed = true;
  await page.route(`**/laws/${LAW}/versions.json`, route => failed
    ? route.fulfill({ status: 503 }) : route.fulfill({ json: comparisonVersions(LAW) }));
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("alert")).toContainText("版一覧を取得できませんでした");
  await expect(page.getByText(/条を比較/)).toHaveCount(0);
  await expect(page.getByText(/モック/)).toHaveCount(0);
  failed = false;
  await page.getByRole("alert").getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByText(/条を比較/)).toContainText("比較用アルファ法 · 1 条を比較");
});

for (const failBoth of [false, true]) {
  test(`比較本文の${failBoth ? "両方" : "片方"}の取得失敗を再試行できる`, async ({ page }) => {
    await comparisonFixture(page);
    let failed = true;
    const requests: string[] = [];
    await page.route(`**/laws/${LAW}/revisions/*.json`, route => {
      const revision = new URL(route.request().url()).pathname.split("/").at(-1)!.replace(/\.json$/, "");
      requests.push(revision);
      return failed && (failBoth || revision.endsWith("old"))
        ? route.fulfill({ status: 503 }) : route.fulfill({ json: comparisonBody(LAW, revision) });
    });
    await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("比較する本文を取得できませんでした");
    await expect(page.getByText(/条を比較/)).toHaveCount(0);
    await expect(page.getByText(/モック/)).toHaveCount(0);
    failed = false;
    await page.getByRole("alert").getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByText(/条を比較/)).toContainText("1 条を比較");
    if (!failBoth) expect(requests.filter(id => id.endsWith("new"))).toHaveLength(1);
  });
}

test("比較可能な版が1つの場合はサンプル差分ではなく空状態を表示する", async ({ page }) => {
  await comparisonFixture(page);
  await page.route(`**/laws/${LAW}/versions.json`, route => route.fulfill({ json: {
    law_id: LAW, versions: comparisonVersions(LAW).versions.slice(0, 1),
  } }));
  const bodies: string[] = [];
  page.on("request", req => { if (req.url().includes("/revisions/")) bodies.push(req.url()); });
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByText("比較には本文を取得できる版が2つ以上必要です。")).toBeVisible();
  await expect(page.getByText(/条を比較/)).toHaveCount(0);
  await expect(page.getByText(/モック/)).toHaveCount(0);
  expect(bodies).toHaveLength(0);
});

test("他方の比較本文が応答しなくても失敗を表示して再試行できる", async ({ page }) => {
  await comparisonFixture(page);
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  let failed = true;
  await page.route(`**/laws/${LAW}/revisions/*.json`, async route => {
    const revision = new URL(route.request().url()).pathname.split("/").at(-1)!.replace(/\.json$/, "");
    if (revision.endsWith("new")) await gate;
    return failed && revision.endsWith("old") ? route.fulfill({ status: 503 })
      : route.fulfill({ json: comparisonBody(LAW, revision) });
  });
  try {
    await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("比較する本文を取得できませんでした");
  } finally {
    release();
  }
  failed = false;
  await page.getByRole("alert").getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByText(/条を比較/)).toContainText("1 条を比較");
});

test("比較トップで法令一覧の取得を再試行して比較を開始できる", async ({ page }) => {
  await comparisonFixture(page);
  let failed = true;
  await page.route("**/laws/index.json", route => failed ? route.fulfill({ status: 503 }) : route.fulfill({ json: {
    laws: [{ law_id: LAW, title: "比較用アルファ法" }],
  } }));
  await page.goto(new URL("#/compare", BASE).toString());
  await expect(page.getByRole("alert")).toContainText("法令一覧を取得できませんでした");
  await expect(page.locator('[data-slot="skeleton"]')).toHaveCount(0);
  failed = false;
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByText(/条を比較/)).toContainText("比較用アルファ法 · 1 条を比較");
});

test("比較する法令の選択をURLと戻る・進むへ反映する", async ({ page }) => {
  await comparisonFixture(page);
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByText(/条を比較/)).toContainText("比較用アルファ法");
  await page.getByRole("combobox", { name: "法令", exact: true }).click();
  await page.getByRole("option", { name: "比較用ベータ法", exact: true }).click();
  await expect(page).toHaveURL(new URL(`#/laws/${OTHER_LAW}/compare`, BASE).toString());
  await expect(page.getByText(/条を比較/)).toContainText("比較用ベータ法");
  await page.goBack();
  await expect(page.getByText(/条を比較/)).toContainText("比較用アルファ法");
  await page.goForward();
  await expect(page.getByText(/条を比較/)).toContainText("比較用ベータ法");
  await page.reload();
  await expect(page.getByText(/条を比較/)).toContainText("比較用ベータ法");
});

test("比較対象の一覧が取得できなくても直接指定した法令を比較できる", async ({ page }) => {
  await comparisonFixture(page);
  let failed = true;
  await page.route("**/laws/index.json", route => failed ? route.fulfill({ status: 503 }) : route.fulfill({ json: {
    laws: [{ law_id: LAW, title: "比較用アルファ法" }],
  } }));
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByText(/条を比較/)).toContainText("比較用アルファ法 · 1 条を比較");
  await expect(page.getByRole("alert")).toContainText("法令一覧を取得できませんでした");
  await expect(page.getByRole("combobox", { name: "法令", exact: true })).toContainText("比較用アルファ法");
  failed = false;
  await page.getByRole("alert").getByRole("button", { name: "再試行", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "法令", exact: true })).toContainText("比較用アルファ法");
});

test("小画面でも比較の選択欄と長い条文を画面内で読める", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await comparisonFixture(page);
  await page.route(`**/laws/${LAW}/revisions/*.json`, route => {
    const revision = new URL(route.request().url()).pathname.split("/").at(-1)!.replace(/\.json$/, "");
    const doc = comparisonBody(LAW, revision);
    doc.articles[0].paragraphs[0].text += "a".repeat(400);
    return route.fulfill({ json: doc });
  });
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByText(/条を比較/)).toContainText("1 条を比較");
  expect(await page.locator("main").evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  for (const name of ["法令", "基準版 (Before)", "比較版 (After)"]) {
    const box = await page.getByRole("combobox", { name, exact: true }).boundingBox();
    expect(box!.width).toBeGreaterThan(150);
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
  }
});

test("compare view fetches per-revision JSON and diffs two revisions", async ({ page }) => {
  // 版一覧 (versions.json) と、選択 2 版の本文 (revisions/*.json) が fetch される。
  const versions = page.waitForResponse(
    (r) => r.url().includes(`/laws/${LAW}/versions.json`) && r.status() === 200,
    { timeout: 30_000 },
  );
  const revision = page.waitForResponse(
    (r) => /\/laws\/[^/]+\/revisions\/[^/]+\.json/.test(r.url()) && r.status() === 200,
    { timeout: 30_000 },
  );

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());

  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  await versions; // 版一覧を取得
  await revision; // 個別版の本文を取得 (= 実比較の経路が走る)

  // モックフォールバックではない (= 実データを取得できている)。
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);

  // 実データの条数で比較している (民法は 1000 条超)。
  // 初期表示は skeleton → 本文取得後に実データへ更新される。
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });

  // diff サマリ (追加/変更/削除) のバッジが出ている。
  await expect(page.locator("text=/追加 \\d+/")).toBeVisible();
  await expect(page.locator("text=/変更 \\d+/")).toBeVisible();
  await expect(page.locator("text=/削除 \\d+/")).toBeVisible();
});

// サマリバッジ "追加 N" / "変更 N" / "削除 N" の N を読む。
// 条文カードの状態バッジ ("変更" 等) は数字を伴わないので、数字つきの
// サマリだけにマッチする。
async function readSummary(page: import("@playwright/test").Page) {
  const num = async (label: string) => {
    const txt = await page.locator(`text=/${label} \\d+/`).first().innerText();
    return Number(/(\d+)/.exec(txt)?.[1] ?? "0");
  };
  return {
    added: await num("追加"),
    modified: await num("変更"),
    removed: await num("削除"),
  };
}

// 既定で選ばれる 2 版が本文同一だと「差分なし」で空に見える回帰
// (e-Gov の隣接版は本文が同一なことが多い)。既定は「本文が実際に異なる
// 直近の版どうし」を選ぶので、差分は非空でなければならない。
test("compare view defaults to a non-empty diff (differing revisions)", async ({ page }) => {
  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });

  // 「差分はありません」の空状態に陥っていない。
  await expect(page.locator("text=差分はありません")).toHaveCount(0);
  // 差分カード (2 カラムの条文比較) が 1 つ以上描画されている。
  await expect(page.getByText(/^(追加|変更|削除)$/).first()).toBeVisible();
  // 追加/変更/削除 の合計が 0 でない (= 中身の違う版を既定選択している)。
  const s = await readSummary(page);
  expect(s.added + s.modified + s.removed).toBeGreaterThan(0);
});

// 性能の要: 全版を束ねた巨大な history.ndjson.zst をクライアントで展開しない。
// 比較に要るのは 2 版だけなので、束は一切 fetch せず個別 revision を取る。
// (束は大法令で展開後 ~200MB になり fzstd 誤復号で固まる元凶だった。)
test("does not download the full history bundle (loads only selected revisions)", async ({
  page,
}) => {
  const bundleRequests: string[] = [];
  page.on("request", (req) => {
    if (req.url().includes("history.ndjson.zst")) bundleRequests.push(req.url());
  });

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  // 実データで比較できるところまで待つ。
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });

  // 束は一度も要求していない。
  expect(bundleRequests).toHaveLength(0);
});

// 本文取得中は、モックではなく skeleton を表示する (初回の mock チラ見え防止)。
// 個別 revision のレスポンスをゲートで保留し、「取得中」を観測可能にして検証する。
test("shows skeleton (not mock) while revision bodies are loading", async ({ page }) => {
  let release: () => void = () => {};
  const gate = new Promise<void>((res) => (release = res));
  await page.route(`**/laws/${LAW}/revisions/**`, async (route) => {
    await gate; // 解放するまで本文を返さない → 取得中状態が続く
    await route.continue();
  });

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });

  // 取得中: skeleton が出ていて、モックは出ていない。
  await expect(page.locator('[data-slot="skeleton"]').first()).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);
  // モックの版ラベル "モック" もセレクタに出ていない。
  await expect(page.locator("text=· モック")).toHaveCount(0);

  // 本文を解放 → 実データに置き換わり skeleton が消える。
  release();
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });
  await expect(page.locator('[data-slot="skeleton"]')).toHaveCount(0);
});
