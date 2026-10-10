# WebMCP

SPA を開くと、対応ブラウザの `document.modelContext` に読み取り専用ツールを登録する。旧 API の `navigator.modelContext` も検出する。API がないブラウザでは通常の UI を利用できる。

実装は [Chrome の Imperative API](https://developer.chrome.com/docs/ai/webmcp/imperative-api) に準拠する。WebMCP を有効にした対応ブラウザと、ツールを呼び出せるエージェント環境が必要。独立した HTTP MCP サーバーの起動は不要で、ツールはページを開いている間利用できる。

## 公開ツール

読み取り専用の19ツールを公開する。

| ツール | 用途 |
| --- | --- |
| `list_data_sources` | 対応コーパス、全文検索可否、必須引数、文書内の区分を案内 |
| `search_legal_data` | 法令・会議録・官報・通達・自治体例規・パブコメ・議案・審議会の全文検索 |
| `list_legal_data` | 12種類のデータの一覧、メタデータ検索、属性フィルター、ページング |
| `get_legal_document` | IDから詳細・本文・出典を取得。法令は過去版も指定可能 |
| `list_document_entries` | 文書内の項目を検索・絞り込み・並べ替え・ページング |
| `get_document_entry` | 発言・通達・統計値・添付などの1項目を取得 |
| `get_law_article` | 法令の条文・附則・別表を取得 |
| `list_law_versions` | 法令の版一覧と本文の取得可否 |
| `get_law_timeline` | 制定・改正・廃止の履歴 |
| `list_law_diffs` | 配信済みの版間差分と変更件数の一覧 |
| `get_law_diff` | 版間差分を条文単位で取得・絞り込み |
| `get_law_snapshot` | 配信済みの指定日時点の版と、任意でその本文を取得 |
| `get_law_references` | 法令全体・条文の参照先・被参照一覧 |
| `get_related_documents` | 法令から関連会議録・パブコメ・通達、会議録から言及法令を取得 |
| `get_wiki_links` | Wikiの関連ページを無向グラフから取得 |
| `get_law_updates` | 日付別の法令追加・変更・削除。省略時は最新更新日 |
| `get_recent_changes` | 新着フィード |
| `get_upcoming_enforcements` | 施行予定 |
| `get_data_status` | 更新日時・収集状況・件数・収集エラー |

すべて `readOnlyHint: true`、`untrustedContentHint: true`。画面遷移は行わず、既存の公開 JSON と SQLite 検索 DB を利用する。ツールの登録は非同期で、ページ内の画面切り替えをまたいで利用できる。

## 一覧から詳細へ

`corpus` の省略値は `laws`。一覧・検索・関連資料の応答に含まれる `document_args` を、そのまま `get_legal_document` に渡せる。対応データの種類と必須引数は `list_data_sources` でも取得できる。手動で指定する場合は以下の値を使う。

| corpus | データ | `id` に渡す値 | 追加の引数 |
| --- | --- | --- | --- |
| `laws` | 法令 | `law_id` | 過去版は `revision_id` |
| `proceedings` | 国会会議録 | `meeting_id` | — |
| `pubcomment` | パブコメ | `case_id` | — |
| `gian` | 議案 | `bill_id` | `session`（文字列） |
| `tsutatsu` | 通達集 | `tax` | — |
| `municipalities` | 自治体 | `municipality_code` | — |
| `reiki` | 自治体例規 | `reiki_id` | `municipality_code` |
| `procurement` | 政府調達 | `item_id` | — |
| `shingikai` | 審議会 | `minutes_id` | `ministry` |
| `budget` | 財政・統計 | `stats_data_id` | — |
| `wiki` | 解説 | `path` | — |
| `kanpo` | 官報 | 不要 | `date`（YYYY-MM-DD） |

`reiki` の一覧にも `municipality_code`、`kanpo` の一覧にも `date` が必要。通達の詳細は通達集を返し、官報の詳細は指定日の号と記事を返す。パブコメ・議案・審議会の全文検索結果には `document_id` と画面の `route` が含まれ、議案の回次・審議会の府省も `document_args` に設定される。会議録・通達・例規の項目ヒットは `entry_args`、法令の条文ヒットは `article_args` を返す。

一覧系ツールの `query` はメタデータの部分一致、空白区切りは AND 条件。`filters` はフィールド名と文字列値の完全一致条件（例: `{"category":"民事"}`）。ネストした値は `{"dimensions.category":"教育"}` のように指定できる。`sort_by` と `sort_order`（`asc` / `desc`）で並べ替え可能。数値フィールドは数値順、文字列は日本語の文字列順。`limit` は1〜100（既定20）、`offset` は0以上。応答の `total` は絞り込み後の件数で、`next_offset` が `null` になるまで続きを取得できる。版一覧・引用リンクはページングのみ対応する。

全文検索の検索語は2文字以上。順位順に1ページ最大100件を返す。`next_offset` を次の呼び出しの `offset` に指定して続きを取得する。分類 `categories` は法令のみ。例規は `municipality_code` または `prefecture` で絞り込み、`title_only: true` で題名だけを検索できる。例規の検索・ページングは先頭2000候補内（応答の `candidate_limit`）に限定され、全国の全件を列挙するものではない。件数が多い場合は自治体・都道府県で絞り込む。本文検索には配信済みの検索 DB が必要で、DBが利用できない場合はエラーを返す。

法令の版を省略すると配信中の `current.json` を返す。取得した `revision_id`・`status`・出典を確認する。過去版は `list_law_versions` の `body_available` と `path` を確認して取得する。

## 文書内の項目

`list_document_entries` と `get_document_entry` は詳細取得と同じ引数に、次の `section` を指定する。省略時は各行の先頭の区分。

| corpus | section |
| --- | --- |
| `laws` | `articles`（本則）、`supplementary`（附則）、`appendices`（別表） |
| `proceedings` | `speeches`（発言） |
| `pubcomment` | `opinions`（意見と回答）、`attachments`（添付・抽出本文） |
| `gian` | `fields`（審議情報の項目） |
| `tsutatsu` | `items`（通達項目） |
| `reiki` | `articles`（本則）、`supplementary`（附則） |
| `shingikai` | `attachments`（添付・抽出本文） |
| `budget` | `values`（統計値と分類） |
| `kanpo` | `items`（当日の各号の記事） |

応答には文書のメタデータと出典を `document` として含める。附則には親区分の情報、官報記事には号の情報も付く。`entry_index` はフィルター・並べ替え前の区分内の0始まりの位置で固定。`get_document_entry` には `entry_index` または `entry_id` の一方を渡す。意見・統計値・官報記事などIDがない項目、同じIDが複数ある項目は `entry_index` を使う。添付は配信済みのメタデータ・抽出本文を返し、外部ファイルの新規ダウンロードや抽出は行わない。

## 履歴・関連資料

`list_law_diffs` の版ペアを `get_law_diff` に渡すと、差分の概要と条文ごとの変更が返る。`filters: {"change_type":"modified"}` や、入れ子の差分本文への `query` で絞り込める。配信されている差分ペアのみ取得できる。

`get_law_snapshot` は配信済みの `laws/<law_id>/at/<date>.json` を読む。指定日が未配信ならエラー。`include_body: true` では解決された版の本文を追加し、該当版がない日付では `document: null`、版はあるが本文が未配信ならエラー。`include_unenforced`・`as_of` など配信側の条件もそのまま返す。

`get_related_documents` は `laws → proceedings / pubcomment / tsutatsu` と `proceedings → laws` に対応する。関連度・照合理由は公開データの値を返す。`get_wiki_links` の配信グラフは無向なので、関連ページを方向の区別なく返す（`graph_type: "undirected"`）。`get_law_references` は `article_id` を省略すると法令全体の引用・被引用を取得する。

## 呼び出し例

対応ブラウザでは DevTools からもツールを検出・実行できる。

```js
const tools = await document.modelContext.getTools();
async function call(name, args = {}) {
  const tool = tools.find(tool => tool.name === name);
  const result = await document.modelContext.executeTool(tool, args);
  if (result === null) return null;
  const data = JSON.parse(result.content[0].text);
  if (result.isError) throw new Error(data.error);
  return data;
}

await call('search_legal_data', { corpus: 'laws', query: '個人情報', limit: 10 });
const laws = await call('list_legal_data', { corpus: 'laws', query: '民法', limit: 5 });
const law = await call('get_legal_document', laws.items[0].document_args);
await call('get_law_article', { law_id: law.law_id, article_id: law.articles[0].article_id });
await call('list_legal_data', { corpus: 'municipalities', query: '千葉市' });
await call('list_legal_data', { corpus: 'reiki', municipality_code: '121002' });

const speeches = await call('search_legal_data', { corpus: 'proceedings', query: '教育', limit: 5 });
if (speeches.items[0]?.entry_args) await call('get_document_entry', speeches.items[0].entry_args);
if (speeches.next_offset !== null) {
  await call('search_legal_data', { corpus: 'proceedings', query: '教育', limit: 5, offset: speeches.next_offset });
}
await call('get_related_documents', { id: law.law_id, target: 'proceedings', limit: 5 });
await call('list_document_entries', { corpus: 'budget', id: '配信済みのstats_data_id', filters: { area: '東京都' }, limit: 10 });
```

成功時は `content[0].text` に JSON、失敗時は `isError: true` と `{"error":"..."}` を返す。未配信の詳細、引数不備、DB取得失敗を空の検索結果と区別できる。

実行コンテキストの `AbortSignal` に対応する。開始前の中止はデータ取得を行わず、実行中の中止は応答を待たずにエラーを返す。共有DB処理や開始済みの取得処理はバックグラウンドで完了する場合がある。

## 検証

```sh
cd figma
pnpm test:e2e:history webmcp.spec.ts
```

Playwright ではブラウザ API のテスト用ハーネスで登録を再現し、実際の静的 JSON と WASM SQLite を使って取得・検索を検証する。ブラウザ本体やエージェントとの接続の確認は、WebMCP が有効なブラウザで行う。
