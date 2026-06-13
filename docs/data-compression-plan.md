# データ容量 圧縮計画

> `public/` の配信データ容量を削減し、GitHub Pages 上限内で 例規・議事録・全リビジョン履歴まで
> 載せられるようにする。実測（2026-06-13, 法令コーパス）に基づく。
> 関連: [`plan.md`](plan.md) / [`diff-design.md`](diff-design.md) / [`public-corpus-roadmap.md`](public-corpus-roadmap.md)

---

## 0. 結論

- **整形除去（minify）は無意味**（本文が長い日本語テキスト主体で 5% しか効かない）。
- 効くのは **①重複排除（現状の約半分が文字通りのコピー）** と **②brotli 事前圧縮（約16×）**。
- 現状 **2.5GB → 重複排除で〜680MB → brotli 配信で〜40–90MB** まで圧縮可能。
- 将来の履歴全期/例規/議事録を載せるには **③条文 content-addressing（CAS）** が必須。

## 1. 実測（法令コーパス, 2026-06-13）

### 1.1 `public/laws` の内訳（論理バイト）

| 役割 | サイズ | ファイル数 | 問題 |
|---|---|---|---|
| `current.json` | 477 MB | 8,990 | **最新リビジョンの完全コピー**（同一 md5 を確認） |
| `revisions/*` | 645 MB | 9,563 | 記述名と**ハッシュ名で同一本文を二重保存**（差は `revision_id`/`status`/`raw_xml_sha256`/`fetched_at` のみ） |
| `articles/*` | 321 MB | **254,841** | 本文は `current.json` にインライン済み＝**本文テキストの分割コピー** |
| `versions.json` | 32 MB | 8,990 | — |
| **論理合計** | **≒ 1.48 GB** | — | — |
| `manifest.json` | 61 MB | 1 | 全ファイル sha256。SPA は debug リンクのみで常用しない |
| `du` 実測 | **2.4 GB** | 309,372 | 差 ~1GB は 25万個の極小ファイルの**ブロック余白** |

### 1.2 圧縮率（同一 `current.json` で比較）

| 手法 | 後サイズ比 | 倍率 |
|---|---|---|
| **brotli -q11 / zstd -19** | **6%** | **約16×** |
| gzip -9 | 12% | 約8× |
| minify (pretty→compact) | 95% | 1.05×（実質無効） |

### 1.3 構造的事実

- `public/`（291,416 ファイル / 2.5GB）が **git に丸ごとコミットされている** → clone 毎に全量、再生成毎に履歴肥大。
- JSON はすべて `serde_json::to_vec_pretty`（pretty-print）。

## 2. 施策（ROI 順）

### 🟢 Tier A — 桁が変わる（最優先）

#### A-1. brotli 事前圧縮配信（`.json.br`）
- 公開 JSON を **brotli で事前圧縮して `*.json.br` を出力**、SPA は fetch → 展開して使う。
- 単独で **約16×**（1.48GB → ~90MB）。
- 理由: GitHub Pages は転送時に gzip はするが **(a) 保存容量は減らない (b) brotli 非対応**。容量上限が制約なので *リポジトリ/Pages 側に圧縮済みを置く* のが効く。
- クライアント実装:
  - gzip なら **依存ゼロ**（`DecompressionStream('gzip')` がブラウザ標準）。手軽さ優先ならまず gzip（8×）。
  - brotli は `brotli-wasm` / `fflate` 等を同梱（16×、+数十KB）。
- 実装ポイント: `lawpub-cli` の書き出し（`build.rs` / `diffs.rs` / `snapshots.rs` / `kanpo.rs`）に圧縮ライタを噛ませる。`Content-Type`/拡張子規約を `figma/src/app/data/api.ts` の fetch 層と合わせる。
- 受け入れ: SPA が `.json.br`（or `.json.gz`）から法令詳細・diff・検索を従来どおり表示できる。

#### A-2. `public/` を git 追跡から外し CI ビルド＆デプロイ
- `public/` を `.gitignore` に入れ、GitHub Actions でビルド → **`actions/upload-pages-artifact` + `actions/deploy-pages`**（または orphan `gh-pages` を force-push）でデプロイ。
- 効果: 2.5GB の履歴肥大を根絶。clone が軽くなる。圧縮とは独立に効く。
- 再現性の担保: 生データは `.cache` バンドル（既存 `bundle-revisions-meta` / R2 wrangler）で保全。
- 受け入れ: main から `public/` が消え、Pages は CI 成果物で更新される。

### 🟡 Tier B — 論理サイズ半減 ＋ ファイル数激減

#### B-1. `current.json` の本文コピー廃止 （−477MB）
- `current.json` を**最新リビジョンへの薄い参照**にする（`{ revision_id, status, "$ref": "revisions/<hash>.json" }`）、
  または `current.json` を正として**記述名リビジョンを出力しない**（どちらか一方を canonical に）。

#### B-2. `articles/*` 分割の廃止 （−321MB, ファイル 291k→~37k）
- 条文は `current.json` / リビジョン本文に**インライン済み**なので、`articles/{art_id}.json` は冗長。
- 条文への deep-link は **SPA 側で本文 JSON から該当条文を切り出す**方式に変更。
- 効果: 本文コピー削減に加え、**25万ファイルのブロック余白 ~1GB・61MB の `manifest.json`・git tree** が一気に縮む。

#### B-3. リビジョンを content-addressed 一本化
- リビジョンは **ハッシュ名（`raw_xml_sha256` ベース等）に統一**し、記述名 `revision_id` は `versions.json` でマップ。
- 記述名⇔ハッシュ名の二重保存を排除。

### 🔵 Tier C — 履歴/例規/議事録を載せる前に必須

#### C-1. 条文単位 content-addressing（CAS）
- 現状は 1 リビジョン = 全条文の完全保存。一部改正は数条しか変わらないため、**全リビジョン履歴を入れると数十GB に爆発**（plan 既知）。
- **条文本文をハッシュで一意保存**（`articles/blobs/{hash}.json`）し、リビジョン = `[{article_no, caption, blob_hash}]` の並びにする。
  - リビジョン間で 90%+ の条文を共有 → dedup が効く。
  - B-2 の 254k ファイル問題も同時に解消（一意 blob のみ保存）。
- 既存 `law-diff` は「表示用 diff」として併存可（保存方式は CAS、差分表示は law-diff）。

### ⚪ やらないこと
- **minify**（5% しか効かず可読性を失う割に合わない）。
- 自前のサーバ常駐圧縮（静的配信の利点を捨てる）。

## 3. 想定削減

| 状態 | サイズ | 主因 |
|---|---|---|
| 現状（git, 全コピー, 非圧縮） | 2.5 GB | 4 重コピー＋25万ファイル |
| ＋ B-1/B-2/B-3（重複排除） | 〜680 MB | コピー除去 |
| ＋ A-1（brotli 配信） | **〜40–90 MB** | 16×（gzipなら~180MB） |
| ＋ C-1（CAS）で全リビジョン履歴投入 | 数百MB 規模に抑制 | 条文共有 |

## 4. 実装順（推奨）

1. **A-1 brotli（or gzip）配信** — 単独で最大効果。SPA fetch 層と書き出し層に圧縮を導入。
2. **A-2 git 非追跡 + CI デプロイ** — 履歴肥大を止める（早いほど良い）。
3. **B-2 articles 廃止 → B-1 current 参照化 → B-3 リビジョン CAS 化** — ファイル数とコピーを削減。
4. **C-1 条文 CAS** — 例規/議事録/全履歴を載せる前に必須。

## 4.5 検索への影響と不変条件（重要）

検索と詳細表示は別経路なので、圧縮/重複排除をしても**検索はこれまで通り動く**。ただし守るべき不変条件がある。

### 2 層の分離

1. **全文検索 = `search.db`（FTS5 / `sql.js-httpvfs`）**
   - 1 行 = 1 条文の専用 SQLite インデックス。条文テキストはここに入る。
   - `HTTP Range` で 4KB ページ単位取得（`VITE_SEARCH_DB_URL`=R2、未設定なら `./search.db`）。
   - **per-law JSON（current/revisions/articles）とは独立** → 重複排除しても索引は不変＝検索はそのまま。
2. **詳細表示 = per-law JSON**（`figma/src/app/data/api.ts` が fetch）。brotli/重複排除の対象はこちら。

### ⚠️ 禁止事項
- **`search.db` を丸ごと gzip/brotli しない。** `sql.js-httpvfs` は無圧縮ファイルへの Range ランダムアクセス前提。
  外側圧縮すると全 DL＋全展開になり Range の利点が消える。
  小さくしたいときは **`VACUUM` ＋不要列削除（schema スリム化）** で行う。
- per-law JSON は全取得なので brotli して OK（この非対称を間違えない）。

### 不変条件（守れば検索＋詳細が従来どおり）
- `search.db` のビルド経路は不変。外側圧縮しない（VACUUM/スリム化はOK）。
- **bigram トークナイザを Rust(索引) ↔ TS(クエリ) で一致**させたまま（既存不変条件）。
- `api.ts` が fetch する `current.json` / `versions.json` / `revisions/{revId}.json` / `diff/*` / `at/*` は
  **到達可能に保つ**（重複排除では“余分なコピー”のみ削除）。revId をハッシュ化するなら
  `versions.json` と `all.ndjson` の id を揃える。
- 展開は **`api.ts` の `getJson()` を 1 箇所だけラップ**（`.json.br`/`.gz` → `DecompressionStream` → `json()`）。
  `search-engine.ts` は触らない。
- `articles/*` 削除（B-2）は検索に無害（条文テキストは `search.db` 内）。詳細側の条文ジャンプは
  `current.json` から切り出す。

## 5. 検証

- 各施策後に `lawpub validate` と `du -sh public` / `find public -type f | wc -l` を記録。
- SPA の主要動線（法令詳細・任意日付スナップショット・diff・FTS5 検索・cross-ref）が圧縮後データで動くことを e2e で確認。
- 圧縮率・ファイル数・容量の before/after をこの doc の §3 表に追記（append）。
