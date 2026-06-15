import { test, expect } from "@playwright/test";

// 履歴束 (history.ndjson.zst) を使った「版閲覧＋任意2版 diff」の e2e。
// 既定はローカル静的サーバ (fixture public)。CI も同じ BASE を webServer で立てる。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";
// 複数版を持つ法令 (民法)。fixture/本番いずれにも履歴束がある想定。
const LAW = process.env.HISTORY_LAW ?? "129AC0000000089";

test("compare view loads the zstd history bundle and diffs two revisions", async ({ page }) => {
  // 履歴束が実際に fetch されることをネットワークで確認する。
  const bundle = page.waitForResponse(
    (r) => r.url().includes(`/laws/${LAW}/history.ndjson.zst`) && r.status() === 200,
    { timeout: 30_000 },
  );

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());

  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  await bundle; // 束 (zstd) を取得できた = fzstd 復号経路が走る

  // モックフォールバックではない (= live 履歴を束から復号できている)。
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);

  // 実データの条数で比較している (民法は 1000 条超)。
  // 初期表示は mock の数条 → 束 (zstd) 展開後に実データへ更新される。
  // 更新前に assert すると落ちるので、3 桁以上の条数になるまでリトライ待ちする。
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

// 本番 Pages の不具合再現: versions.json が 404 でも、本文の真の在処である
// 履歴束から比較が成立する (モックに落ちない)。比較対象リストを versions.json
// に依存させていた旧実装はここで空になり、ずっとモックになっていた。
test("compare works when versions.json is missing (404) — bundle is the source of truth", async ({
  page,
}) => {
  await page.route(`**/laws/${LAW}/versions.json*`, (route) =>
    route.fulfill({ status: 404, contentType: "text/plain", body: "Not Found" }),
  );
  const bundle = page.waitForResponse(
    (r) => r.url().includes(`/laws/${LAW}/history.ndjson.zst`) && r.status() === 200,
    { timeout: 30_000 },
  );

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });
  await bundle;

  // versions.json 404 でもモックフォールバックしない。
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);
  // 実データの条数で比較できている。
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });
  // 版ラベルは revision_id から日付を起こす (例: "施行 2026-04-01")。
  await expect(page.locator("text=差分はありません")).toHaveCount(0);
  const s = await readSummary(page);
  expect(s.added + s.modified + s.removed).toBeGreaterThan(0);
});

// 履歴束のロード中は、モックではなく skeleton を表示する (初回の mock チラ見え防止)。
// 束のレスポンスをゲートで保留し、「ロード中」を観測可能にして検証する。
test("shows skeleton (not mock) while the history bundle is loading", async ({ page }) => {
  let release: () => void = () => {};
  const gate = new Promise<void>((res) => (release = res));
  await page.route(`**/laws/${LAW}/history.ndjson.zst`, async (route) => {
    await gate; // 解放するまで束を返さない → ロード中状態が続く
    await route.continue();
  });

  await page.goto(new URL(`#/laws/${LAW}/compare`, BASE).toString());
  await expect(page.getByRole("heading", { name: "バージョン比較" })).toBeVisible({
    timeout: 15_000,
  });

  // ロード中: skeleton が出ていて、モックは出ていない。
  await expect(page.locator('[data-slot="skeleton"]').first()).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator("text=モックでデモ表示")).toHaveCount(0);
  // モックの版ラベル "モック" もセレクタに出ていない。
  await expect(page.locator("text=· モック")).toHaveCount(0);

  // 束を解放 → 実データに置き換わり skeleton が消える。
  release();
  await expect(page.locator("text=条を比較")).toContainText(/\d{3,}\s*条を比較/, {
    timeout: 30_000,
  });
  await expect(page.locator('[data-slot="skeleton"]')).toHaveCount(0);
});
