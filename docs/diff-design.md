# 任意バージョン間 diff 設計 (Phase 1)

## 1. 目的

ある法令の **任意の 2 リビジョン**、もしくは **任意の 2 時点 (日付)** を指定すると、条文単位の構造化された差分が取得できる API / UI を提供する。e-Gov v2 が提供しない領域に踏み込み、本アプリの存在意義の中核とする。

## 2. ユースケース

1. **施行日スナップショット**: 「2018-04-01 時点の民法第709条」を1クエリで取れる。
2. **改正前後の比較**: 「この条はどの改正で何が変わったか」を条単位で表示。
3. **長期 diff**: 「2000年と現在で第3編はどう変わったか」を一括で見る。
4. **未施行改正の重ね合わせ**: 公布済みだが未施行の改正を含めた仮想スナップショット。

## 3. 配信 URL 設計

### 3.1 リビジョン直接指定

```
laws/{law_id}/diff/{from_rev}..{to_rev}.json
```

例:
```
laws/129AC0000000089/diff/129AC0000000089_20170601_xxx..129AC0000000089_20200401_yyy.json
```

### 3.2 日付指定スナップショット

```
laws/{law_id}/at/{yyyy-mm-dd}.json           # その時点で施行されていた版
laws/{law_id}/at/{yyyy-mm-dd}/diff/{yyyy-mm-dd}.json   # 2時点 diff
```

実体は revision にリダイレクトされる軽量 JSON。

```json
{
  "law_id": "129AC0000000089",
  "as_of": "2018-04-01",
  "resolved_revision_id": "129AC0000000089_20170601_xxx",
  "include_unenforced": false,
  "current": "laws/129AC0000000089/revisions/129AC0000000089_20170601_xxx.json"
}
```

### 3.3 隣接 diff インデックス (precomputed)

```
laws/{law_id}/diffs.json
```

各 revision とその前 revision の diff path をまとめた索引。UI のタイムライン表示で使う。

```json
{
  "law_id": "129AC0000000089",
  "diffs": [
    {
      "from_revision_id": "...",
      "to_revision_id": "...",
      "effective_date": "2020-04-01",
      "path": "laws/.../diff/from..to.json",
      "summary": { "added": 2, "removed": 1, "modified": 12 }
    }
  ]
}
```

## 4. Diff データモデル

### 4.1 トップレベル

```json
{
  "schema_version": 1,
  "law_id": "...",
  "from": {
    "revision_id": "...",
    "effective_date": "2017-06-01",
    "promulgation_date": "..."
  },
  "to": {
    "revision_id": "...",
    "effective_date": "2020-04-01",
    "promulgation_date": "..."
  },
  "summary": {
    "articles_added": 2,
    "articles_removed": 1,
    "articles_modified": 12,
    "articles_renumbered": 0,
    "articles_unchanged": 850
  },
  "articles": [ ... ],
  "source": { ... }
}
```

### 4.2 条文単位の差分

各エントリは以下のいずれかの `change_type` を持つ。

| change_type | 意味 |
|---|---|
| `unchanged` | 条文全体が完全一致 (デフォルトでは配信 JSON から省略可) |
| `added` | to にのみ存在 |
| `removed` | from にのみ存在 |
| `modified` | 同じ article_id で内容が異なる |
| `renumbered` | テキストはほぼ一致だが article_no が変わった (将来) |
| `moved` | 章/節構造が変わった (将来) |

### 4.3 modified の項単位 diff

```json
{
  "article_id": "art_709",
  "change_type": "modified",
  "from": { "article_no": "第七百九条", "caption": "..." },
  "to":   { "article_no": "第七百九条", "caption": "..." },
  "paragraphs": [
    {
      "paragraph_no": "1",
      "change_type": "modified",
      "text_diff": [
        { "op": "equal", "text": "故意又は過失によって" },
        { "op": "delete", "text": "他人の権利" },
        { "op": "insert", "text": "他人の権利又は法律上保護される利益" },
        { "op": "equal", "text": "を侵害した者は、" }
      ]
    }
  ]
}
```

`text_diff` は文字単位の Myers diff (Rust `similar` クレート想定)。

### 4.4 added / removed

```json
{ "article_id": "art_709_2", "change_type": "added",
  "to": { "article_no": "第七百九条の二", "caption": "...", "paragraphs": [...] } }
```

## 5. Diff アルゴリズム

### 5.1 条文マッチング

1. **第一段階**: `article_id` で完全マッチ (現行 normalizer は `art_{Num}` を出すので、改正で番号変更がなければ通る)
2. **第二段階 (将来)**: `article_no` テキスト + caption + 段落数のヒューリスティック
3. **第三段階 (将来)**: 本文のシングル類似度 (`strsim`) で renumbered を検出

MVP は第一段階のみで十分。

### 5.2 段落マッチング

paragraph_no が両方に存在すればそれで対応付け。無い場合は順序で対応付け。

### 5.3 テキスト diff

- 段落単位で `similar::TextDiff::from_chars` を使用
- 結果を `op: equal/insert/delete` の配列に変換
- パフォーマンス対策: 1段落が極端に長い場合 (>100KB) は単語単位にフォールバック

## 6. スナップショット解決

### 6.1 解決ルール

`as_of` (日付) を入力に取り、次を満たす最新の revision を返す:

```
effective_date <= as_of  AND
(repeal_date IS NULL OR repeal_date > as_of)
```

未施行改正を含めるオプション (`include_unenforced=true`) では `promulgation_date <= as_of` に切り替える。

### 6.2 廃止法令

`repeal_status` が立っている revision 以降は、`as_of >= repeal_date` の場合 410 相当 (`status: "repealed"`) を返す JSON を出す。

## 7. crate / CLI 拡張

### 7.1 新規 crate: `law-diff`

```
crates/law-diff/
  src/
    lib.rs         # diff データ型、diff 関数
    article.rs     # 条文マッチング
    paragraph.rs   # 項マッチング
    text.rs        # similar ラッパ
```

公開 API:

```rust
pub struct LawDiff { ... }

pub fn diff_documents(from: &LawDocument, to: &LawDocument) -> LawDiff;
```

依存:
- `similar = "2"`
- `serde`, `serde_json`
- workspace 共通

### 7.2 CLI コマンド追加

```bash
lawpub build-diffs --public public    # 全法令の隣接 diff を再計算
lawpub diff --law 129AC0000000089 \
            --from <rev> --to <rev>   # 単発 diff (stdout)
lawpub build-snapshots --public public \
       --dates 2018-04-01,2020-04-01  # 任意日付スナップショット
```

### 7.3 `lawpub update` への組み込み

`update` 実行時に、変更のあった法令についてのみ隣接 diff を再生成する。
ファイル数増加を抑えるため、デフォルトでは「隣接 diff」のみ事前生成し、任意ペア diff はオンデマンドで生成する `diff` API を別途用意する (Phase 1.5)。

## 8. ファイル数とサイズの見積もり

- 法令数 ≒ 10,000
- 平均 revision 数 ≒ 5 (古い法令は 1、最近のは 10+)
- 隣接 diff 数 ≒ 法令数 × (revisions - 1) ≒ 50,000 程度
- 1 diff JSON ≒ 数 KB 〜 数百 KB (大きな改正でも 1MB 以下)

合計で 1〜2GB 程度を想定。GitHub Pages の制約 (1GB) に近づくため、以下を検討:

- 大きすぎる diff は段落 text_diff を省略し、段落単位の change_type のみ保持
- gzip 配信 (`.json.gz` も併置)
- 古い法令のうち revision が 1 つしかないものは diff 不要

## 9. UI (figma/) への組み込み

1. 法令詳細ページに「改正履歴」タブを追加
2. timeline.json の各イベント横に「← 前版と比較」ボタン
3. 比較ビューは左右 2 カラム or 統合ビューを切り替え
4. 日付指定ピッカー (`as_of`) で任意時点のスナップショットを表示
5. 条文単位で「この条はいつ追加 / 変更されたか」をホバーで表示

## 10. 受け入れ条件

- [ ] `law-diff` crate が単体テスト付きで存在する
- [ ] 民法 (129AC0000000089) について隣接 diff が全 revision で生成される
- [ ] `diffs.json` 索引が法令単位で生成される
- [ ] スナップショット `at/{date}.json` が生成される
- [ ] UI から 2 つの版を選んで diff が表示できる
- [ ] schema/diff.schema.json が公開される

## 11. 非ゴール (この Phase でやらない)

- 改正法 (amending law) の本文を読んで diff を生成する方式 (今は結果ベース)
- 構造変更 (章節の追加・削除) の検出
- 別法令間の diff (e.g. 旧法 ↔ 新法 の対応関係)
- WYSIWYG エディタ的な inline edit ビュー

これらは Phase 1.5 以降で検討。
