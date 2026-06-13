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
