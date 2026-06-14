# 履歴(版閲覧＋任意2版diff)の本番化 引き継ぎ

> 目的: アプリで「法令の履歴閲覧」と「指定 2 版の diff」を、**できるだけ圧縮**して本番配信する。
> データ・圧縮・ビルド基盤は完成済み。残りは **SPA フロント実装＋配信結線**。
> 最終更新: 2026-06-14

---

## 0. 結論サマリー

- 全履歴 (45,885 revisions / 非圧縮 19GB) を **法令ごとの zstd 束 `history.ndjson.zst` = 計 72MB** に圧縮する仕組みを実装済み (`build-json` が生成)。
- per-file gzip だと 2.3GB・34万ファイルで Pages に載らないが、**72MB なら Pages にも載る**。
- SPA は束を 1 回取得 → 展開 (fzstd) → 全版を持つので **履歴閲覧＋任意 2 版 diff をクライアント側**で実現できる (precomputed per-pair diff 配信は不要)。

## 1. 完了済み (commit 済み, lawrenceanum)

| 内容 | commit |
|---|---|
| public 非追跡化 / CI gzip 配信 / push-deploy 除去 / gzip CLI＋SPA透過展開 | `059c7358` |
| build-json メモリ有界化 (16GB で全履歴ビルド可) | `664b80d9` |
| body_available 整合＋build-diffs 欠損耐性 | `d538ad77` |
| **履歴束 history.ndjson.zst 生成 (19GB→72MB)** | `3fb7b0bf` |
| git 履歴書き換え 978MB 回収 (main force-push 済) | (実施) |
| 32GB→160MB アーカイブ R2 格納 (`lawrenceanum-search/revisions.tar.zst`) | (実施) |

### 実証済み (内蔵ディスクで全件)
- build-json **45,885 revisions** 完走 (OOM なし) / search.db 233,510 articles
- **history.ndjson.zst 8,964 束 = 72MB** (民法 112KB)
- build-diffs 36,921 diff 生成 (※束方式では不要になる見込み)

### 圧縮率の根拠 (実測)
| 方式 | サイズ | 備考 |
|---|---|---|
| 非圧縮 | 19 GB | 45,885 revision ファイル |
| per-file gzip | 2.3 GB | gzip 窓 32KB で版間 dedup 不可 |
| **per-law zstd --long** | **72 MB** | 大窓で版間 dedup (gzip比 ~32x) |
| (将来) 条文 CAS | 推定さらに小 | 別タスク。72MB で十分なら不要 |

## 2. データ仕様 (実装済み)

各法令ディレクトリ `public/laws/{law_id}/`:
- `current.json` … 現行版本文 (既存)
- `versions.json` … 版メタ一覧 (`body_available` は実ファイルと一致, 既存・修正済)
- `timeline.json` … 改正タイムライン (既存)
- **`history.ndjson.zst`** … 全版を NDJSON 1 行 1 版でまとめ zstd(--long, window 23..27) 圧縮 (新規)
  - 各行 = `LawDocument` (compact JSON, `revision_id` と `status` 設定済み)
  - `revisions/{rev_id}.json` (per-file) も現状は併存して書かれるが、**配信には不要**
    (束だけ配ればよい。将来 build-json から per-file 出力を落としてよい)

`zstd_long()` は `crates/lawpub-cli/src/build.rs` 末尾。window は内容サイズに合わせ
23..=27 にクランプ (復号側のメモリ確保を内容相当に抑制)。標準 zstd フレームなので
`fzstd` 等で復号可能。

## 3. 残作業 — SPA フロント (要視覚テスト)

対象: `figma/`。既存の履歴/diff UI は `src/app/components/views/compare-view.tsx`,
`simple-views.tsx`, `browse-view.tsx`、データ層は `src/app/data/api.ts`。
現状は per-file revision / precomputed diff を前提にしている。

### 3.1 依存追加
- `fzstd` (純 JS zstd 復号, 小さい)。`pnpm --dir figma add fzstd`。

### 3.2 api.ts: 履歴束の取得＋展開
```ts
import { decompress } from 'fzstd'
export type Revision = LawDocumentRaw // revision_id/status 入り
export async function fetchHistory(lawId: string): Promise<Revision[]> {
  const url = new URL(`./laws/${lawId}/history.ndjson.zst`, document.baseURI).toString()
  const res = await fetch(url, { cache: 'force-cache' })
  if (!res.ok) throw new Error(`${res.status} for ${url}`)
  const zs = new Uint8Array(await res.arrayBuffer())
  const ndjson = new TextDecoder().decode(decompress(zs))
  return ndjson.split('\n').filter(Boolean).map((l) => JSON.parse(l))
}
```
- これで `api.revision()` / `api.diff()` / `api.diffsIndex()` の per-file 依存を置換できる。

### 3.3 クライアント側 diff (任意 2 版)
- `law-diff` crate (`crates/law-diff/src/lib.rs`) の article/paragraph diff を TS に移植、
  または article 単位の簡易 diff を実装 (added/removed/modified + paragraph テキスト diff)。
- 束から from/to の 2 版を引いて diff → 既存 `LawDiff` 型 (api.ts) に合わせると compare-view を流用しやすい。

### 3.4 UI
- 履歴閲覧: `versions.json` で版一覧、選んだ版本文は束から取得して表示。
- diff: 任意の from/to を選択 → クライアント diff → compare-view で表示。
- precomputed `diff/*.json` / `diffsIndex` への依存を外す。

## 4. 残作業 — 配信結線

### 4.1 prebuilt 履歴を R2 へ (CI は 32GB を展開できないため)
- 内蔵で生成した `public/laws/**/history.ndjson.zst` (72MB) を 1 アーカイブにして R2 へ:
  ```sh
  # 例: 束だけ集めて tar -> R2
  (cd /Users/bokuweb/lawpub-build/public && tar -cf - laws/*/history.ndjson.zst) | zstd -19 -o history-bundles.tar.zst
  wrangler r2 object put lawrenceanum-search/history-bundles.tar.zst --file=history-bundles.tar.zst --remote
  ```
  (R2 creds は owner 環境。`CLOUDFLARE_API_TOKEN`=Workers R2 Storage Edit, `R2_BUCKET=lawrenceanum-search`)

### 4.2 CI workflow (`.github/workflows/update-law-data.yml`)
- 既存「Restore lawpub cache」の隣に **history 束を R2 から取得して `public/laws/**/history.ndjson.zst` に展開**するステップを追加。
- daily build は現行版のみ生成 (軽量) し、履歴束は R2 prebuilt を流し込んで Pages artifact に同梱。
- 72MB なので Pages 配信で収まる (R2 直配信にしてもよい。その場合 SPA の base URL を分ける)。

### 4.3 容量/ファイル数
- 配信に含めるのは `history.ndjson.zst` (72MB / 8,964 ファイル) であって per-file revisions ではない。
  build-json の per-file revisions 出力は配信前に除外するか、将来 build-json 側で出力を止める。

## 5. 受け入れ条件
- [ ] 法令詳細で版一覧が出て、任意の版本文を表示できる
- [ ] 任意 2 版を選んで diff が表示される (条文 added/removed/modified)
- [ ] 配信サイズが Pages 上限内 (履歴束 72MB)
- [ ] 既存の現行版表示・全文検索 (search.db/R2) が従来どおり動く

## 6. 後片付け (任意)
- 外付け `.cache/revisions` (32GB) は R2 アーカイブ済＋再ビルド実証済なので削除可。
- 内蔵 `/Users/bokuweb/lawpub-build` (cache 32GB + public 19GB) も deploy 確定後に掃除可。
- git 履歴の完全縮小は force-push 後の fresh clone で。
