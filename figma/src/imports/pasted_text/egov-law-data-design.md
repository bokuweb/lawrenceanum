# e-Gov法令データ自動最新化・JSON配信基盤 設計書

## 1. 目的

GitHub Actionsでe-Gov法令データを定期取得し、Rust製CLIで正規化したJSONをGitHub Pages上に静的配置する。利用者はHTTP GETで法令本文、条文、改正履歴、差分、官報リンク情報をJSONとして取得できる。

本設計のゴールは、agentがこのMarkdownをもとにリポジトリ初期化、Rust実装、GitHub Actions、GitHub Pages配信まで実装できる状態にすることである。

---

## 2. 基本方針

### 2.1 ホスティング方針

GitHub Pagesで静的JSONを配信する。

想定URL:

```text
https://<owner>.github.io/<repo>/index.json
https://<owner>.github.io/<repo>/laws/{law_id}/current.json
https://<owner>.github.io/<repo>/laws/{law_id}/versions.json
https://<owner>.github.io/<repo>/laws/{law_id}/timeline.json
https://<owner>.github.io/<repo>/laws/{law_id}/articles/{article_id}.json
https://<owner>.github.io/<repo>/updates/latest.json
https://<owner>.github.io/<repo>/updates/{yyyy-mm-dd}.json
https://<owner>.github.io/<repo>/kanpo/{yyyy-mm-dd}/index.json
```

### 2.2 更新方針

- GitHub Actionsの`on.schedule`で毎日または数時間ごとに実行する。
- 手動再実行用に`workflow_dispatch`も用意する。
- 初回または必要時のみ全件バルクを取得する。
- 通常更新はe-Govの最新更新データを日付単位で取得する。
- 取得済みデータはSHA-256で重複判定する。
- 変更があった場合のみ`public/`を更新し、Pagesへdeployする。
- 大容量のraw ZIPやraw XMLは原則Git管理しない。必要な場合はGitHub Actions artifactまたはRelease assetを検討する。

### 2.3 データ方針

- e-Gov XMLを一次ソースとして扱う。
- e-Gov JSONは取得できる場合も補助扱いにする。
- 配信用JSONは独自の安定スキーマに正規化する。
- すべての配信JSONに`source`メタデータを含める。

---

## 3. 全体アーキテクチャ

```text
GitHub Actions
  |
  | schedule / workflow_dispatch
  v
Rust CLI: lawpub
  |
  | fetch e-Gov bulk/update data
  | parse XML/CSV
  | normalize stable JSON
  | build index files
  | optionally fetch/link Kanpo PDFs
  v
public/
  index.json
  laws/
  updates/
  kanpo/
  schema/
  health.json
  manifest.json
  v
GitHub Pages
  v
Static JSON API over HTTPS
```

---

## 4. リポジトリ構成

```text
.
├── .github/
│   └── workflows/
│       ├── update-law-data.yml
│       └── pages.yml
├── crates/
│   ├── egov-client/
│   ├── law-normalizer/
│   ├── kanpo-client/
│   ├── kanpo-linker/
│   └── lawpub-cli/
├── public/
│   ├── index.json
│   ├── manifest.json
│   ├── health.json
│   ├── schema/
│   ├── laws/
│   ├── updates/
│   └── kanpo/
├── state/
│   └── latest.json
├── docs/
│   ├── api.md
│   └── schema.md
├── Cargo.toml
├── Cargo.lock
└── README.md
```

### 4.1 Git管理するもの

- Rustソースコード
- `public/**/*.json`
- `state/latest.json`
- スキーマ定義
- GitHub Actions workflow

### 4.2 Git管理しないもの

- e-Govのraw ZIP
- 官報PDF
- 一時展開済みXML
- 中間キャッシュ

ただし、官報PDFを長期保存したい場合は、GitHub PagesではなくRelease assetまたは別ストレージを使う。

---

## 5. Rust CLI仕様

CLI名は仮に`lawpub`とする。

### 5.1 コマンド一覧

```bash
lawpub fetch-update --date 2026-05-01 --cache .cache
lawpub fetch-range --from 2026-04-01 --to 2026-05-01 --cache .cache
lawpub fetch-bulk --cache .cache
lawpub build-json --input .cache --output public
lawpub build-index --output public
lawpub kanpo-fetch --date 2026-05-01 --cache .cache
lawpub kanpo-link --output public
lawpub validate --public public
lawpub update --public public --cache .cache
```

### 5.2 最重要コマンド

agentはまず以下を実装する。

```bash
lawpub update --public public --cache .cache
```

このコマンドは次を行う。

1. `state/latest.json`を読む。
2. 前回取得日以降のe-Gov更新データを取得する。
3. 未取得またはSHA-256が異なるデータだけ処理する。
4. XML/CSVをparseする。
5. 安定JSONに正規化する。
6. `public/`配下にJSONを書き出す。
7. `public/manifest.json`と`public/health.json`を更新する。
8. `state/latest.json`を更新する。
9. 変更がない場合はexit code 0で終了し、ファイル更新しない。

---

## 6. e-Gov取得仕様

### 6.1 取得対象

初期版では以下を対象にする。

- 最新更新データ
- 法令本文XML
- 法令一覧CSV

将来対応:

- 全件バルク
- 分類別バルク
- e-Gov JSONファイル

### 6.2 更新取得の考え方

通常実行では以下の期間を取得する。

```text
from = state.latest_successful_update_date - 3 days
to   = today in Asia/Tokyo
```

理由:

- GitHub Actionsの失敗や遅延に備える。
- e-Gov側の掲載タイミングの揺れに備える。
- 重複取得はSHA-256で排除する。

### 6.3 取得結果メタデータ

取得したファイルごとに以下を記録する。

```json
{
  "source": "egov",
  "kind": "daily_update_zip",
  "date": "2026-05-01",
  "url": "...",
  "sha256": "...",
  "fetched_at": "2026-05-02T00:10:00Z",
  "bytes": 1234567
}
```

---

## 7. 配信用JSON設計

### 7.1 `public/index.json`

全体の入口。

```json
{
  "version": 1,
  "generated_at": "2026-05-02T00:10:00Z",
  "base_url": "https://<owner>.github.io/<repo>/",
  "endpoints": {
    "laws": "laws/index.json",
    "updates_latest": "updates/latest.json",
    "manifest": "manifest.json",
    "health": "health.json"
  }
}
```

### 7.2 `public/manifest.json`

配信ファイル一覧と検証用情報。

```json
{
  "version": 1,
  "generated_at": "2026-05-02T00:10:00Z",
  "files": [
    {
      "path": "laws/129AC0000000089/current.json",
      "sha256": "...",
      "bytes": 12345,
      "content_type": "application/json"
    }
  ]
}
```

### 7.3 `public/health.json`

監視用。

```json
{
  "ok": true,
  "generated_at": "2026-05-02T00:10:00Z",
  "latest_egov_update_date": "2026-05-01",
  "law_count": 12345,
  "file_count": 54321,
  "errors": []
}
```

### 7.4 `public/laws/index.json`

法令一覧。

```json
{
  "version": 1,
  "generated_at": "2026-05-02T00:10:00Z",
  "laws": [
    {
      "law_id": "129AC0000000089",
      "law_num": "明治二十九年法律第八十九号",
      "title": "民法",
      "current": "laws/129AC0000000089/current.json",
      "timeline": "laws/129AC0000000089/timeline.json",
      "versions": "laws/129AC0000000089/versions.json"
    }
  ]
}
```

### 7.5 `public/laws/{law_id}/current.json`

現在有効な法令。

```json
{
  "schema_version": 1,
  "law_id": "129AC0000000089",
  "law_num": "明治二十九年法律第八十九号",
  "title": "民法",
  "revision_id": "...",
  "promulgation_date": "1896-04-27",
  "effective_date": "...",
  "status": "current",
  "articles": [
    {
      "article_id": "art_1",
      "article_no": "第一条",
      "caption": "基本原則",
      "paragraphs": [
        {
          "paragraph_no": "1",
          "text": "私権は、公共の福祉に適合しなければならない。"
        }
      ]
    }
  ],
  "source": {
    "provider": "egov",
    "raw_xml_sha256": "...",
    "fetched_at": "2026-05-02T00:10:00Z"
  }
}
```

### 7.6 `public/laws/{law_id}/versions.json`

法令バージョン一覧。

```json
{
  "law_id": "129AC0000000089",
  "versions": [
    {
      "revision_id": "...",
      "effective_date": "2026-04-01",
      "promulgation_date": "2025-12-20",
      "path": "laws/129AC0000000089/revisions/<revision_id>.json",
      "source_update_date": "2026-04-01"
    }
  ]
}
```

### 7.7 `public/laws/{law_id}/timeline.json`

改正イベントの時系列。

```json
{
  "law_id": "129AC0000000089",
  "events": [
    {
      "event_id": "...",
      "event_type": "partial_amendment",
      "target_law_id": "129AC0000000089",
      "amending_law_num": "令和八年法律第三十号",
      "promulgation_date": "2026-05-01",
      "effective_date": "2026-06-01",
      "revision_id": "...",
      "status": "promulgated_not_yet_effective",
      "kanpo": {
        "linked": true,
        "path": "kanpo/2026-05-01/index.json",
        "confidence": 0.95
      }
    }
  ]
}
```

### 7.8 `public/updates/latest.json`

直近更新サマリ。

```json
{
  "generated_at": "2026-05-02T00:10:00Z",
  "latest_update_date": "2026-05-01",
  "updated_laws": [
    {
      "law_id": "...",
      "title": "...",
      "change_type": "modified",
      "current": "laws/.../current.json"
    }
  ]
}
```

---

## 8. 官報連携仕様

### 8.1 基本方針

e-Gov改正情報から取得できる以下をキーに、官報発行サイト上の該当日PDFを探索・突合する。

- 公布日
- 法令番号
- 法令名
- 改正対象法令名
- 「一部を改正する法律」等のタイトル

### 8.2 初期実装範囲

MVPでは官報PDF自体はPagesに置かない。

実装するもの:

- 官報発行日のindex取得
- 本紙・号外・特別号外のPDF URL収集
- PDF URL、号数、種別、SHA-256、取得時刻をJSON化
- e-Gov改正イベントとの簡易マッチング

実装しないもの:

- PDF全文OCR
- PDF本文の長期ホスティング
- 全文検索

### 8.3 `public/kanpo/{yyyy-mm-dd}/index.json`

```json
{
  "date": "2026-05-01",
  "generated_at": "2026-05-02T00:10:00Z",
  "issues": [
    {
      "issue_type": "extra",
      "issue_no": "第101号",
      "pdf_url": "https://...",
      "sha256": "...",
      "matched_law_events": [
        {
          "law_id": "...",
          "revision_id": "...",
          "amending_law_num": "令和八年法律第三十号",
          "confidence": 0.95,
          "match_reasons": ["promulgation_date", "law_num"]
        }
      ]
    }
  ]
}
```

### 8.4 マッチングスコア

```text
+0.40 公布日一致
+0.35 法令番号一致
+0.15 法令名一致
+0.10 改正対象法令名一致
```

`confidence >= 0.80`なら自動リンク扱いにする。

---

## 9. GitHub Actions設計

### 9.1 更新workflow

ファイル: `.github/workflows/update-law-data.yml`

```yaml
name: Update law JSON

on:
  workflow_dispatch:
    inputs:
      from_date:
        description: "Optional start date YYYY-MM-DD"
        required: false
        type: string
      to_date:
        description: "Optional end date YYYY-MM-DD"
        required: false
        type: string
  schedule:
    - cron: "30 0,6,12,18 * * *"
      timezone: "Asia/Tokyo"

permissions:
  contents: write
  pages: write
  id-token: write

concurrency:
  group: update-law-json
  cancel-in-progress: false

jobs:
  update:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v6

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Build
        run: cargo build --release

      - name: Update JSON
        run: |
          ./target/release/lawpub update --public public --cache .cache

      - name: Validate public JSON
        run: |
          ./target/release/lawpub validate --public public

      - name: Commit changes
        id: commit
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git add public state
          if git diff --cached --quiet; then
            echo "changed=false" >> "$GITHUB_OUTPUT"
          else
            git commit -m "Update law JSON data"
            git push
            echo "changed=true" >> "$GITHUB_OUTPUT"
          fi

  deploy:
    needs: update
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pages: write
      id-token: write
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Checkout updated branch
        uses: actions/checkout@v6

      - name: Configure Pages
        uses: actions/configure-pages@v5

      - name: Upload Pages artifact
        uses: actions/upload-pages-artifact@v4
        with:
          path: public

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

### 9.2 注意

上記は基本形。実装時には以下も検討する。

- `deploy`を毎回実行するか、変更時のみ実行するか。
- `git push`による再帰実行を避けるため、workflow triggerを`schedule`と`workflow_dispatch`のみにする。
- 大量ファイル更新時はPages artifact方式を優先し、`gh-pages`ブランチ方式は避ける。
- ファイル数が増えすぎる場合は、法令IDごとに分割しつつ`manifest.json`で索引する。

---

## 10. Pages配信設定

GitHub repository settingsで以下を設定する。

```text
Settings > Pages > Build and deployment > Source = GitHub Actions
```

配信対象は`public/`ディレクトリ。

---

## 11. API利用例

### 11.1 全体index取得

```bash
curl https://<owner>.github.io/<repo>/index.json
```

### 11.2 法令一覧取得

```bash
curl https://<owner>.github.io/<repo>/laws/index.json
```

### 11.3 法令の現行本文取得

```bash
curl https://<owner>.github.io/<repo>/laws/129AC0000000089/current.json
```

### 11.4 直近更新取得

```bash
curl https://<owner>.github.io/<repo>/updates/latest.json
```

### 11.5 官報リンク取得

```bash
curl https://<owner>.github.io/<repo>/kanpo/2026-05-01/index.json
```

---

## 12. 実装タスク分解

### Phase 1: 最小JSON配信

- [ ] Rust workspace作成
- [ ] `lawpub` CLI作成
- [ ] e-Gov更新ZIP取得処理
- [ ] ZIP展開処理
- [ ] CSV/XML読取処理
- [ ] `public/index.json`生成
- [ ] `public/laws/index.json`生成
- [ ] `public/laws/{law_id}/current.json`生成
- [ ] `public/manifest.json`生成
- [ ] `public/health.json`生成
- [ ] JSON validation実装
- [ ] GitHub Actions更新workflow作成
- [ ] GitHub Pages deploy確認

### Phase 2: 履歴・差分

- [ ] `versions.json`生成
- [ ] `timeline.json`生成
- [ ] revision単位JSON生成
- [ ] 条文単位JSON生成
- [ ] 条文差分JSON生成
- [ ] `updates/{date}.json`生成

### Phase 3: 官報リンク

- [ ] 官報発行サイトの日付ページ取得
- [ ] PDF URL収集
- [ ] 官報issue metadata生成
- [ ] e-Gov改正イベントとの突合
- [ ] `kanpo/{date}/index.json`生成

### Phase 4: 品質改善

- [ ] schema versioning
- [ ] JSON Schema生成
- [ ] 取得失敗時のretry/backoff
- [ ] 過去数日再取得
- [ ] 変更なし時のno-op化
- [ ] サイズ削減のためのminify
- [ ] 必要に応じて`.json.gz`も生成

---

## 13. Rust実装詳細

### 13.1 主要クレート

```toml
[workspace]
members = [
  "crates/egov-client",
  "crates/law-normalizer",
  "crates/kanpo-client",
  "crates/kanpo-linker",
  "crates/lawpub-cli"
]
```

推奨依存:

```toml
anyhow = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
quick-xml = { version = "0.37", features = ["serialize"] }
csv = "1"
zip = "2"
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }
scraper = "0.22"
walkdir = "2"
```

### 13.2 データ型例

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawSummary {
    pub law_id: String,
    pub law_num: Option<String>,
    pub title: String,
    pub current: String,
    pub timeline: String,
    pub versions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    pub provider: String,
    pub raw_xml_sha256: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawDocument {
    pub schema_version: u32,
    pub law_id: String,
    pub law_num: Option<String>,
    pub title: String,
    pub revision_id: Option<String>,
    pub promulgation_date: Option<String>,
    pub effective_date: Option<String>,
    pub status: String,
    pub articles: Vec<Article>,
    pub source: SourceMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub article_id: String,
    pub article_no: String,
    pub caption: Option<String>,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paragraph {
    pub paragraph_no: Option<String>,
    pub text: String,
}
```

---

## 14. エラー処理方針

- 取得失敗は最大3回retryする。
- 一部日付の取得に失敗しても、既存JSONは壊さない。
- 生成中は`public.tmp/`に書き、成功後に`public/`へatomic replaceする。
- validationに失敗したらcommit/deployしない。
- `health.json`に直近エラーを記録する。

---

## 15. セキュリティ・運用

- `GITHUB_TOKEN`は最小権限にする。
- Pages deploy jobには`pages: write`と`id-token: write`を付与する。
- repository contentsへcommitするjobには`contents: write`を付与する。
- 外部URLから取得したZIP/PDFは必ずサイズ上限を設ける。
- ZIP展開時はZip Slip対策を行う。
- 生成JSONにはHTMLをそのまま埋め込まない。

---

## 16. 受け入れ条件

### 16.1 MVP受け入れ条件

- [ ] `cargo build --release`が成功する。
- [ ] `lawpub update --public public --cache .cache`が成功する。
- [ ] `public/index.json`が生成される。
- [ ] `public/laws/index.json`が生成される。
- [ ] 少なくとも1法令について`current.json`が生成される。
- [ ] `public/manifest.json`のSHA-256が実ファイルと一致する。
- [ ] GitHub Actionsで定期実行できる。
- [ ] GitHub Pages上でJSONをHTTP GETできる。

### 16.2 Phase 2受け入れ条件

- [ ] `versions.json`が生成される。
- [ ] `timeline.json`が生成される。
- [ ] 過去版または将来施行版を区別できる。
- [ ] 直近更新一覧を`updates/latest.json`で取得できる。

### 16.3 Phase 3受け入れ条件

- [ ] 官報の日付別index JSONが生成される。
- [ ] e-Gov改正イベントと官報PDF候補がリンクされる。
- [ ] confidence scoreとmatch理由が保存される。

---

## 17. agentへの実装指示

以下の順で実装すること。

1. Rust workspaceと`lawpub` CLIを作る。
2. まず外部取得をmockして、`public/`配下のJSON生成だけを通す。
3. 次にe-Gov取得処理を実装する。
4. 取得データから`current.json`と`laws/index.json`を生成する。
5. `manifest.json`と`health.json`を生成する。
6. `validate`コマンドを実装する。
7. GitHub Actions workflowを追加する。
8. GitHub Pages deployを確認する。
9. その後、履歴・差分・官報リンクを追加する。

初期PRでは官報PDF本文抽出やOCRは実装しなくてよい。まずはJSON配信基盤を完成させる。

