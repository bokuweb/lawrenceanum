# 例規 (自治体条例・規則) 取り込み設計 (Phase 2)

## 1. なぜやるか

- e-Gov v2 は国の法令のみ。自治体例規 (条例・規則・要綱・要領) は完全な空白地帯。
- 全 1,700+ 自治体を横断検索する公式手段は存在しない。
- ユーザー (自治体職員・士業・議員・コンサル) の需要は明確で、「他自治体はどう書いてるか」「国法との縦串」は誰もが欲しがる。
- 法令側で確立した正規化 JSON / diff / スナップショット基盤がそのまま再利用できる。

## 2. ゴール (この Phase)

- 50 自治体ぶんの例規を取り込んで、`public/reiki/{municipality_id}/` 配下に法令と同じ形のJSON で配信。
- 法令で実装済みの全機能 (current.json / versions.json / timeline.json / diff / at/{date}.json) を例規にも適用。
- 国法との縦串 (「この条例は何法の委任か」) のリンクテーブルを別途持つ。

## 3. 非ゴール

- 1,700 自治体フル対応 (Phase 2.5 以降)
- 議会議事録 (Phase 3)
- 国法 → 条例の自動委任関係推論 (手動メタデータから始める)

## 4. データソースの実態

### 4.1 自治体例規システムのベンダ別シェア (概算)

| ベンダ | 想定シェア | 特徴 |
|---|---|---|
| ぎょうせい (e-Reiki / 例規集システム) | 〜50% | HTML、フレーム構造、検索フォームは POST 主体。URL構造はほぼ統一されている。 |
| 第一法規 (D1-Law 自治体版) | 〜25% | JS 重め、SPA に近い構造。ベンダ DB 側に直接当てるのは規約 NG。 |
| ジャパンシステム / その他 | 〜15% | 自治体独自カスタム多い。 |
| 自治体独自 (静的 HTML / PDF) | 〜10% | 都道府県の一部・小規模自治体。 |

→ **ぎょうせい型に絞れば 1 adapter で半数カバー** できる。ここを最優先。

### 4.2 法的整理

- 例規本文そのものに著作権は発生しない (著作権法 13 条 1 号 2 号)。
- ただしベンダの例規集**システムからスクレイプ**するとベンダ ToS 違反になり得る。
- **必ず「自治体公式サイトに掲載されている例規集ページ」から取得**する建付けにする。
  自治体公式の利用規約は通常スクレイピング禁止条項を含まない (情報公開の趣旨)。
- robots.txt と Crawl-delay は厳守。1 自治体あたり 1 req/sec を上限とする。
- 取得元 URL とサイトポリシーへの参照を `source` メタに必ず残す。

## 5. アーキテクチャ

### 5.1 新規 crate

```
crates/
├── reiki-client/      # 自治体例規システムからの取得 (adapter pattern)
│   ├── adapters/
│   │   ├── gyosei.rs       # ぎょうせい例規集
│   │   ├── d1law.rs        # 第一法規 (将来)
│   │   └── generic_html.rs # フォールバック
│   └── lib.rs
├── reiki-normalizer/  # ベンダ HTML → LawDocument 互換 JSON
└── reiki-publisher/   # public/reiki/{id}/ への書き出し (law-publisher と共通基盤化検討)
```

LawDocument の構造を**そのまま再利用**する。`law_id` の代わりに `reiki_id` を導入:

```
reiki_id = "{municipality_code}_{reiki_code}"
  例: "131016_jourei_kojin_jouhou_hogo"
       (千代田区 個人情報保護条例)
```

municipality_code は総務省の全国地方公共団体コード (6 桁) を使う。

### 5.2 配信 URL

```
reiki/index.json                        # 全自治体一覧
reiki/{municipality_code}/index.json    # その自治体の例規一覧
reiki/{municipality_code}/{reiki_id}/current.json
reiki/{municipality_code}/{reiki_id}/versions.json
reiki/{municipality_code}/{reiki_id}/diff/{from}..{to}.json
reiki/{municipality_code}/{reiki_id}/at/{yyyy-mm-dd}.json
```

法令側の URL 設計とパラレル。

### 5.3 国法 ↔ 例規 縦串リンク

```
links/law-to-reiki/{law_id}.json
links/reiki-to-law/{reiki_id}.json
```

中身:

```json
{
  "law_id": "129AC0000000089",
  "linked_reiki": [
    {
      "reiki_id": "131016_jourei_xxx",
      "municipality": { "code": "131016", "name": "千代田区" },
      "relation": "delegation",   // delegation / reference / both
      "article_links": [
        { "law_article_id": "art_709", "reiki_article_id": "art_3" }
      ],
      "confidence": 1.0,
      "source": "manual"
    }
  ]
}
```

初期は **手動メタデータ + LLM 補助**。自動推論はやらない。
法令本文の「〜条例で定めるところにより」や例規本文の「〜法第○条の規定に基づき」を aho-corasick で抽出するのは Phase 2.5。

## 6. パイロット自治体の選定

ぎょうせい例規集を使い、かつ規模が異なる **3 自治体** から始める:

1. **東京都千代田区** (131016): 都心、例規数が比較的少なく検証しやすい
2. **横浜市** (141003): 政令市、例規数多めでスケール確認
3. **長野県松本市** (202010): 一般市・地方の典型

3 自治体で adapter が動いたら、ぎょうせい型 47 自治体まで横展開する。

## 7. CLI 拡張

```bash
lawpub reiki-fetch --municipality 131016 --cache .cache
lawpub reiki-build-json --input .cache --output public
lawpub reiki-build-diffs --public public
lawpub reiki-build-snapshots --dates 2020-04-01 --public public
lawpub reiki-link --output public         # 縦串リンク再生成
```

`law-diff` / snapshot resolver はそのまま reiki 側でも使える (LawDocument を使い回すため)。

## 8. スケジューラと負荷

- 自治体公式サイトはレートが厳しい。GitHub Actions の cron で `1 自治体 / 日` の頻度から始める。
- 全件再取得は週次。差分は If-Modified-Since / ETag を尊重。
- ZIP/PDF を提供している自治体はそちらを優先 (1 req で済むため)。

## 9. ファイルサイズ見積

- 1 自治体あたり例規数 ≒ 200〜2,000 (規模による)
- 全国 1,700 自治体に対し平均 500 とすると総数 ≒ 85万件
- 1 件あたり current.json + versions.json + revisions/ で 50〜200KB
- 総容量: 40〜170GB → **GitHub Pages では収まらない**

→ Phase 2 では **50 自治体 (= 全体の 3%)** に絞り、Pages 容量内に収める。
Phase 2.5 で R2 ホスティングへ移行する判断 (既に R2 用 wrangler が package.json にある)。

## 10. UI

- ヘッダーに「法令 / 例規」タブを追加
- 法令詳細ページに「この法令を根拠とする条例」セクション (links/law-to-reiki)
- 例規ブラウザは自治体ごと → 例規一覧 → 詳細の階層
- 検索は法令と同じ FTS5 を反復適用
- 「自治体横断比較」: 同種条例 (例: 個人情報保護条例) を 5 自治体並べて表示

## 11. 受け入れ条件

- [ ] `crates/reiki-client` の Gyosei adapter が動く
- [ ] 千代田区の全例規が `reiki/131016/` 配下に出る
- [ ] 横浜市・松本市でも同じ adapter が動く
- [ ] 例規にも diff / snapshot が適用される
- [ ] 国法 → 例規の links JSON が手動データから生成される
- [ ] SPA から例規を閲覧・検索できる

## 12. リスクと対策

| リスク | 対策 |
|---|---|
| ベンダが ToS でスクレイプ拒否 | 必ず自治体公式サイト経由。robots.txt 順守。代理取得業者 API は使わない。 |
| HTML 構造の自治体ローカルカスタム | 1 adapter で完全網羅は諦め、未対応自治体は skip + ログ。 |
| 個人情報・要配慮情報 (要綱で個人名が入る稀ケース) | 取り込み前に黒塗りパターン検出、見つけたら自動 skip + 通知。 |
| 容量超過 | Phase 2.5 で R2 へ。それまでは 50 自治体上限。 |
| 法的責任 (「最新版である」の保証は誰がするか) | 全配信 JSON に "this is unofficial mirror" の免責を入れる。`source.official_url` を必ず明示。 |

## 13. 実装順

1. ぎょうせい型 1 自治体 (千代田区) で end-to-end 通す
2. normalizer の互換性を確認 (article_id が安定するか)
3. 残り 2 自治体 (横浜市・松本市) で adapter の汎用性を検証
4. links JSON のスキーマ確定と手動データ投入 (5 件程度)
5. SPA に「例規」タブを追加
6. ぎょうせい型 47 自治体まで横展開

## 14. 実装状況（2026-10）

当初の「50 自治体・Pages 配信」から方針を変え、**RILG リンク集に載る全国の公開例規集を収集し R2 から配信する**形で実装した。

### 収集対象

- `lawpub reiki-discover` が RILG「全国自治体例規集リンク集」を読み、総務省の全国地方公共団体コード（`crates/reiki-client/data/municipality_codes.csv`、令和6年1月1日現在）と都道府県＋団体名で突き合わせて `crates/reiki-client/data/tenants.json` を作る。
- 対応形式は 2 つ。2026-10-08 時点で **1,177 自治体**（RILG 掲載の団体コード付き 1,755 のうち 67%）。
  - ぎょうせい Reiki-Base（`.../reiki_menu.html`）: 950（`www1.g-reiki.net` 501 + 自治体ドメイン）
  - 第一法規 例規類集 HTML 版（`.../d1w_reiki/`）: 227（`en3-jg.d1-law.com` 199 + 自治体ドメイン）
- 対象外（578）: 第一法規の検索アプリ型（`ops-jg.d1-law.com/opensearch`）、Legal Square、条例 Web アーカイブ型、北海道町村会、自治体独自 HTML/PDF など。`tenants.json` の `unsupported` に残している。一部事務組合・広域連合は団体コード表に無いので未収録。

### 取得の作法

- robots.txt を尊重する（5xx・到達不能は RFC 9309 に従い全面禁止扱い）。例外は `en3-jg.d1-law.com` だけで、存在しないパスすべてに空応答を返すホストのため「robots.txt なし」として扱う（`http.rs` の `HOSTS_WITHOUT_ROBOTS`）。
- **1 ホストあたり 1 req/sec 以下**。429/503 を受けたらそのホストの間隔を倍にし（上限 30 秒、`Retry-After` に従う）、約 1 分間成功が続くごとに 3/4 ずつ戻す。成功が続いていた間隔で 429 を受けたら、その 5/4 倍を下限にして持続できる速度に収束させる。`www1.g-reiki.net` は 1 req/sec で 7 分ほど（400 件前後）続けると 429 になり、その後 7 分ほど 30 秒間隔でも 429 が続いた（2026-10-08 実測）。
- 例規集トップの「内容現在」日が前回と同じ自治体は、1 リクエストで巡回を省く。変わっていなくても 60 日ごとに全件を確認し直す。
- 一覧の一部ページが取れなかったときと、本文の取得失敗が 5% を超えたときは、その周回を完了扱いにしない。この場合、例規の削除（廃止）判定もしない。

### 構造化

ベンダごとの HTML を文書順の「ブロック列」（題名・制定日・番号・章見出し・条見出し・条・項・号・表・附則・注記）にし、共通の `structure::build` で条 → 項 → 号（→ 細目）の階層と附則にまとめる。ぎょうせい版は項・号が `div.article` の外にある兄弟要素として並ぶため、旧実装はこれらを取りこぼしていた。保存するのは本文と制定情報だけで、目次体系などベンダの付加情報は持たない。

### 保存と配信

| 置き場所 | 内容 |
|---|---|
| R2 `reiki-cache/state.json` | 自治体ごとの巡回状態（内容現在日・周回開始・完了・公開済み時刻・未削除の廃止例規） |
| R2 `reiki-cache/tenants/{code}.ndjson.zst` | 自治体の全例規（1 行 1 文書）。日次同期は巡回した自治体の分だけ |
| R2 `reiki/index.json.gz` ほか | 配信 JSON（`reiki/{code}/index.json.gz`, `reiki/{code}/{reiki_id}.json.gz`）。前回公開以降に内容が変わった例規だけを上げる |
| R2 `reiki-search.db` | 例規専用の全文検索 DB（contentless FTS5 + 自治体ごとの rowid 範囲）。法令用 `search.db` とは別ファイル |
| Pages `reiki/index.json` | 全国一覧のみ（ダッシュボードの収録状況用） |

SPA は `VITE_REIKI_BASE_URL` / `VITE_REIKI_SEARCH_DB_URL`（`R2_PUBLIC_URL` から設定）で R2 を読む。

### 規模の目安（2 自治体・1,178 件の実測から外挿）

1 件あたり、収集キャッシュ約 1.7KB（zstd）、配信 JSON 約 5KB（gzip）、検索 DB 約 15KB。全国 150〜180 万件で、それぞれ約 3GB / 8GB / 25GB になる。

全国検索で頻出語に当たると FTS5 の doclist の読み込みが大きくなるので、SPA は候補を MATCH の先頭 2,000 件に絞ってから rank で並べる。自治体・都道府県で絞るときは rowid 範囲を渡す。規模が大きくなって遅い場合は、都道府県単位で DB を分割するのが次の手。

### ワークフロー

`collect-reiki.yml`（JST 23:00 / 11:00）の流れ:

1. R2 から収集キャッシュを同期する。
2. `reiki-fetch` で最大 300 分巡回する。8 ホストを並列に処理し、同じホストは直列。
3. キャッシュを R2 に同期する。巡回が失敗しても実行する。
4. `reiki-build-json --pending-only` で変更分を作り、gzip して R2 に上げる。廃止された例規は R2 から削除する。
5. `reiki-mark-published` で公開済みを記録する。
6. 変更があった日だけ、別ジョブで `reiki-search.db` を /mnt 上に作り直して R2 に上げる。

`update-corpus-data.yml` からは例規を外した。`update-law-data.yml` は R2 の `reiki/index.json.gz` を Pages に同梱するだけにした。

### 未対応（次の候補）

- 第一法規 検索アプリ型（`ops-jg.d1-law.com/opensearch`、約 190 自治体）。動的検索画面のため、利用条件の確認が先。
- 国法 ↔ 例規の縦串リンク（§5.3）、版管理・差分（例規集は現行版のみ公開のため、収集時点ごとのスナップショットの蓄積から始める）。
- 一部事務組合・広域連合（団体コードの別表が必要）。
