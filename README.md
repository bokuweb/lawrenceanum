# lawrenceanum

e-Gov 法令データを GitHub Actions で定期取得し、Rust 製 CLI (`lawpub`) で
正規化したJSONをGitHub Pages上に静的配信するための基盤。

詳細設計は [docs/plan.md](docs/plan.md) を参照。

## ステータス

Phase 1 (最小JSON配信) を実装中。e-Gov の実HTTP取得は未実装で、Phase 1 では
組み込みのモック provider が `民法` 1件分のサンプル XML を返す。

## ローカル実行

```bash
cargo build --release -p lawpub-cli
./target/release/lawpub update --public public --cache .cache
./target/release/lawpub validate --public public
```

生成されるファイル:

```
public/
├── index.json
├── manifest.json
├── health.json
├── laws/
│   ├── index.json
│   └── 129AC0000000089/
│       ├── current.json
│       └── articles/
│           └── art_*.json
└── updates/latest.json
state/latest.json
```

## CLI

```text
lawpub update         --public public --cache .cache [--provider mock] [--date YYYY-MM-DD]
lawpub fetch-update   --date YYYY-MM-DD --cache .cache
lawpub fetch-range    --from YYYY-MM-DD --to YYYY-MM-DD --cache .cache
lawpub build-json     --input .cache --output public
lawpub build-index    --output public
lawpub validate       --public public
```

`fetch-bulk` / `kanpo-*` は Phase 2 / 3 用のスタブ。

## ワークスペース構成

| crate | 役割 |
|---|---|
| `crates/egov-client`     | e-Gov 取得 (現状は Mock のみ) |
| `crates/law-normalizer`  | LawXML → 正規化済み LawDocument |
| `crates/kanpo-client`    | 官報サイト取得 (Phase 3) |
| `crates/kanpo-linker`    | 改正イベント ↔ 官報PDF マッチング (Phase 3) |
| `crates/search-index`    | bigram トークナイザ + SQLite FTS5 ビルダ |
| `crates/lawpub-cli`      | CLI バイナリ `lawpub` |

## ブラウザ検索 (SQLite FTS5 + 条項間参照グラフ)

`lawpub` ビルド時に `public/search.db` (SQLite + FTS5) を生成し、ブラウザは
`sql.js` (**WASM SQLite** = sqlite.org の Emscripten ビルド) で読み込む。
ネイティブ rusqlite は build pipeline 限定で、配信される実行体は WASM のみ。

- 索引対象は各法令の現行版条文。`law_id` / `article_id` / `article_no` /
  `caption` / `title_tokens` / `content_tokens` を持つ FTS5 仮想テーブル
- 日本語は文字 bigram で前段分割 (`crates/search-index::tokenize` と
  `figma/src/app/data/search-engine::tokenize` が同一実装)
- クエリも同じ bigram で分割して FTS5 に投げる、`snippet()` 関数でハイライト
- `meta` テーブルに `built_at` / `law_count` / `article_count` / `ref_count` を保存
- `refs` テーブルに条項間参照を格納:
  ```sql
  CREATE TABLE refs (
    from_law_id TEXT, from_article_id TEXT,
    to_law_id   TEXT, to_article_id   TEXT,
    ref_text TEXT, ref_type TEXT  -- 'self_article' (Phase 1)
  );
  ```
  Phase 1 は同一法令内の「第○条」を `article_no` 部分一致で抽出。
  ブラウザ側は `getOutgoingRefs` / `getIncomingRefs` / `getRefsForLaw` で取得し、
  Browse 詳細ビューで本文中をリンク化 (クリックで `#article_id` にスクロール) +
  被参照バッジを各条文ヘッダに表示する。

`/search` ルートを開くと sql-wasm + `search.db` を遅延ロード。失敗時はモック
LawSummary フィルタへフォールバックするので Pages 配信前のローカル開発でも動く。
参考: [`../ellisii/crates/jp-tokenizer-bigram`](../ellisii/crates/jp-tokenizer-bigram/) と
[`store-sqlite`](../ellisii/crates/store-sqlite/) の同型 FTS5 構成。

## Web UI (SSG)

`figma/` がデザインの正本兼 UI 実装。Vite + React + Tailwind v4 + shadcn/ui で
組み、ビルド成果物は `lawpub` の生成 JSON と同じ `public/` へ統合する。

- `base: './'` で相対パス参照、サブパス Pages にも対応
- `outDir: ../public`, `emptyOutDir: false` で JSON を保護
- `publicDir: false` で静的アセットの再帰コピーを抑止
- dev mode は `lawpubJsonDevServer` middleware が `../public/*.json` をその場で配信

### ローカル

```bash
cargo run --release -p lawpub-cli -- update --public public --cache .cache
cd figma && pnpm install && pnpm dev   # http://localhost:5173/
# あるいは本番ビルド
pnpm build                              # ../public/{index.html,assets/} を出力
```

### CI ステップ順

1. `lawpub update` (JSON 生成、`public/` を atomic rename で差し替え)
2. `lawpub kanpo-link` (timeline へ官報突合を反映、manifest 再計算)
3. **変更検知** (`state/last_run.json` の `.changed` を判定) — false なら以降スキップ
4. `pnpm build` (HTML + assets を `public/` へ追加 — JSON は残る)
5. `lawpub validate` (manifest の sha256 を実ファイルと照合)
6. `actions/configure-pages` → `actions/upload-pages-artifact`
7. `git commit && git push` (`public/` と `state/latest.json`)
8. 別ジョブ `deploy`: `actions/deploy-pages` (アップロード済 artifact を消費)

## 自動最新化

GitHub Actions の `update-law-data.yml` が以下のトリガで動作する:

| トリガ | 動作 |
|---|---|
| `schedule` (JST 06:30 / 12:30 / 18:30 / 00:30) | e-Gov API から更新取得 → 変更があれば commit + deploy |
| `push` (main へ merge) | 既に main に居る `public/` を rebuild + deploy (e-Gov へは触らない) |
| `workflow_dispatch` | provider/date/force を選んで手動実行 |

main merge では auto-commit せず deploy のみ走る (cron の方で state は管理)。
GITHUB_TOKEN による自動 commit は workflow を再トリガしないので、cron の
auto-commit が deploy ループに入ることはない。

### 変更検知によるノーオペ削減

`lawpub update` は実行ごとに `state/last_run.json` (gitignore) を出力する:

```json
{
  "version": 1,
  "ran_at": "2026-05-09T03:30:00Z",
  "provider": "http",
  "dates": ["2026-05-06", "2026-05-07", "2026-05-08", "2026-05-09"],
  "new_xmls": 0,
  "errors": [],
  "changed": false
}
```

`.cache/revisions/{law_id}/{rev_id}.xml` の sha256 で重複判定し、新規 XML が
0 件かつ `public/manifest.json` が存在する場合は `changed=false` を返して
以降の build/commit/deploy をスキップする。これにより e-Gov 側に動きが無い
時間帯でも commit が膨らまない。

### フェイルセーフ

- HttpProvider は 3 回のリトライ + 指数バックオフ。それでも失敗した日付は
  `errors` に記録するだけで、他の日付の処理は継続する (plan §14)。
- `public/` の atomic rename: `public.tmp/` → `public.bak/` 経由で差し替え、
  失敗時は backup から戻す。
- `concurrency: update-law-json` でスケジュールの重複実行を直列化。

### 手動実行

```text
gh workflow run update-law-data.yml \
  -f provider=http \
  -f date=2026-05-01 \
  -f force=true
```
