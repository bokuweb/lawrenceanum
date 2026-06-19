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
  await expect(page.locator(".grid.grid-cols-2").first()).toBeVisible();
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
