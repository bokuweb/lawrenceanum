# 公共コーパス拡張 ロードマップ & 引き継ぎ

> このリポジトリ（lawrenceanum）を「法令専用」から **「公共法政コーパス（法令＋官報＋国会＋パブコメ＋例規）」** へ拡張するための引き継ぎ。
> 新しいセッション/担当が *cold start* で着手できるよう自己完結で書いてある。
> 既存のフェーズ別詳細は [`proceedings-plan.md`](proceedings-plan.md)（国会/議事録）・[`reiki-plan.md`](reiki-plan.md)（例規）・[`plan.md`](plan.md)（全体）を正とする。本書はその上位の **順序・共通方針・最初の一手**。
> 最終更新: 2026-06-13

---

## 0. いま立っている地点

- ✅ **法令**（e-Gov 法令 API v2）: `egov-client` → `law-normalizer` → diff/snapshot → `search-index` → `public/` 静的 JSON → SPA。
- ✅ **官報**: `kanpo-client` / `kanpo-linker`。
- 📋 **国会/議事録**: `proceedings-plan.md` に設計あり（未実装）。
- 📋 **例規**: `reiki-plan.md` に設計あり（未実装）。
- 🆕 **パブコメ**: 未計画。本書 §5 で新規提案。

土台パターン: **1 ソース = client crate**。取得元が違うだけで後段（正規化スキーマ / diff / snapshot / `search-index` / SPA / CI・Pages 配信 / 相互参照グラフ）は共有する。

## 1. 中心原則（これだけは外さない）

1. **集約ではなく関連付け（cross-link）が価値。** ソースを足すこと自体は誰でもできる。堀は
   `条文 ↔ 改正官報 ↔ 国会答弁 ↔ パブコメ府省回答 ↔ 自治体例規` を結ぶ `links/` グラフ。
2. **リポジトリを fork しない／スキーマを揃える。** 新ソースで書くのは原則 `<source>-client` ＋ 正規化マッピングだけ。
   JSON スキーマを既存 `LawDocument` 系に寄せないと cross-link が成立しない。
3. **index 時に LLM を呼ばない。** 抽出は aho-corasick / 形態素 / 静的辞書。意味処理が要るなら配信後の下流（検索時）で。
4. **静的配信を維持。** GitHub Actions → JSON commit → Pages → ブラウザ FTS5。サーバ常駐を増やさない（容量超過時のみ R2 を検討、wrangler は既に package.json にある）。
5. **TDD（t_wada 流 Red→Green→Refactor）。** オフライン固定 fixture と `MockProvider` で純粋テスト、実 API 取得は `#[ignore]`。

## 2. 推奨ビルド順（外部摩擦の低さ × 既存資産への接続価値で並べる）

| 順 | 作るもの | crate | なぜこの順 |
|---|---|---|---|
| **1** | 国会会議録 取得＋正規化 | `kokkai-client` (+ `kokkai-normalizer`) | **公式無料 JSON API**で `egov-client` と同形＝摩擦最小（スクレイプ/ToS/容量問題なし）。既存**法令に直結** |
| **2** | 法令↔国会 リンカ | `linking` | 「関連付けが本体」を体現する最初の cross-link。`amendment_law_num` 突合 |
| **3** | パブコメ 取得＋正規化＋リンカ | `pubcomment-client` ほか | 唯一無二の「民意＋府省回答」層（§5）。scrape |
| **4** | 例規 | `reiki-client` 系 | `reiki-plan.md` 準拠。ぎょうせい adapter で 3 自治体→47。容量は R2 |
| 後回し | 地方議会議事録 | — | 1,700 分散・容量・Chiholog が無料で 798。提携 or 最後 |

> 注: 既存 docs では 例規=Phase 2 / 国会=Phase 3 だが、**国会は公式 API で外部摩擦が最小**なため、本ロードマップでは **国会を先に着手することを推奨**。`reiki-plan.md` はそのまま後続スペックとして温存（順番を入れ替えるだけ、内容は破棄しない）。

## 3. 既存の作法（新 client を書くときに踏襲する型）

`egov-client` を雛形にする。要点:

- **provider trait + 2 実装**:
  ```rust
  pub trait EgovProvider: Send + Sync {
      fn fetch_update(&self, date: &str) -> Result<UpdateBatch>;
  }
  pub struct MockProvider;  // 組み込みサンプル（オフラインテスト用）
  pub struct HttpProvider;  // 実 API。base_url は env で上書き可
  ```
- **戻り値は生データ＋出所**: `FetchedLaw { law_id, xml, source_url }` / `UpdateBatch { date, laws }`。
- **base URL は env 上書き**（`LAWPUB_EGOV_BASE_URL`）。新ソースも同様に `LAWPUB_KOKKAI_BASE_URL` 等。
- **parse は寛容に、失敗は個別 skip**（1 件の不正データで全体を落とさない）。
- **CLI は `lawpub` の clap `Subcommand` に 1 エントリ追加**。各サブコマンドは `--cache`（既定 `.cache`）/ `--public`（既定 `public`）/ `--provider`（`mock`|`http`, env `LAWPUB_PROVIDER`）を持つ。実体は `lawpub-cli/src/<area>.rs` に `run_*` 関数で置く（例: `kanpo.rs` を参照）。

## 4. 最初の一手 — `kokkai-client`（詳細は proceedings-plan.md）

### 4.1 API
- ベース: `https://kokkai.ndl.go.jp/api/`
- 発言単位: `GET /api/speech?...`（会議単位は `/api/meeting`、軽量一覧は `/api/meeting_list`）
- `recordPacking=json` で JSON。鍵不要、レートゆるめ。1 リクエスト最大 100 件、`startRecord` でページング。
- 主パラメータ: `nameOfMeeting`（会議名）, `from` / `until`（開会日 `YYYY-MM-DD`）, `sessionFrom`/`sessionTo`（国会回次）, `maximumRecords`, `startRecord`。

### 4.2 crate 構成
```
crates/kokkai-client/        # 取得（KokkaiProvider: Mock + Http）
crates/kokkai-normalizer/    # API レスポンス → 安定 JSON（proceedings-plan §4 の Meeting スキーマ）
crates/linking/              # 法令 ↔ 議事録（後続。§2 の手順2）
```

### 4.3 正規化スキーマ（proceedings-plan.md §4 と一致させる）
会議単位 `Meeting { schema_version, meeting_id, session, house, committee, date, issue, speeches[], source }`、
`Speech { speech_id, order, speaker, speaker_id, speaker_group, speaker_position, speech }`。
配信先: `proceedings/{meeting_id}.json`, `proceedings/index.json`。

### 4.4 CLI（proceedings-plan.md §8）
```
lawpub proceedings-fetch --session 215 --cache .cache --provider http
lawpub proceedings-build-json --input .cache --output public
lawpub link-laws-and-proceedings --output public
```

### 4.5 TDD の切り方（最初のコミット群）
1. **Red**: `kokkai-normalizer` に「固定 JSON fixture（1 会議分）→ `Meeting`」のテストを 1 本書いて落とす。fixture は `crates/kokkai-normalizer/tests/fixtures/` に実 API の 1 レスポンスを保存。
2. **Green**: 最小の正規化を実装して通す（fake→triangulation→generalize）。
3. **Refactor**: 重複除去・命名。
4. `kokkai-client`: `MockProvider`（fixture を返す）で取得経路のテスト。`HttpProvider` の実取得は `#[ignore]`（`cargo test -- --ignored`）。
5. `lawpub proceedings-fetch`/`-build-json` を配線、`proceedings/` に JSON が並ぶことを確認。

### 4.6 受け入れ条件（proceedings-plan.md §9）
- [ ] 直近 1 国会会期分の会議録が `proceedings/` 配下に JSON で並ぶ
- [ ] 民法改正案を含む会議が `links/law-to-proceedings/129AC0000000089.json` から辿れる
- [ ] 1 改正法で「公布 → 該当審議 → 条文 diff」が UI でつながる

### 4.7 スコープ注意
- まず **直近 3 国会会期**に絞る（過去全期は数十 GB、Pages に収まらない → R2 は後）。
- 非ゴール: 地方議会 / 議員横断スタンス追跡 / 発言の意味検索・要約 / 表決結果（proceedings-plan §10）。

## 5. パブコメ層（新規提案・未計画ぶん）

- 入手: `public-comment.e-gov.go.jp`。公式 API 無し → HTML スクレイプ（`scraper` 既存依存）。
- データ: 案件メタ（所管府省/案件番号/募集期間/結果公示日）、**提出意見**、**意見に対する府省の考え方（回答）**、関連制定/改正法令名。
- crate: `pubcomment-client`（取得）/ 正規化 / リンカ。配信 `pubcomment/{case_id}.json`、`links/law-to-pubcomment/{law_id}.json`。
- リンク: 法令名/改正法番号マッチで国会リンカ（aho-corasick）を流用。
- 法務: 公文書。提出意見中の個人情報は reiki-plan §12 と同じ黒塗り検出 → skip。
- 初手スコープ: 直近 1〜2 年の **結果公示済み**案件のみ。

## 6. リポジトリの正体の更新（任意・いつでも）

国会を入れた時点で「法令専用」ではなくなる。`README.md` / `docs/plan.md` のスコープ記述に
「法令＋官報＋国会会議録（＋将来パブコメ・例規）」と追記しておくと混乱しない（実装後でよい）。

## 7. 下流の消費（参考・このリポジトリの責務外）

これら静的 JSON API は、別のローカルデスクトップアプリが source として取り込み、利用者のローカル文書と混在検索する想定。
本リポジトリの責務は **公開データの正規化・関連付け・静的配信まで**。下流の都合をここに持ち込まない（スキーマ安定性だけ守る）。

## 8. やらないことリスト

- ❌ index 時の LLM 呼び出し（鉄則）
- ❌ ベンダ例規 DB の直叩き（必ず自治体公式経由・robots 厳守: reiki-plan §4.2）
- ❌ ソースごとのリポジトリ分割（cross-link が死ぬ）
- ❌ 地方議会の 1,700 フルクロール（まず国会・パブコメ・例規 50 で価値を出す）
