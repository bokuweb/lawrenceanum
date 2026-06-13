# 議事録連結 (Phase 3) 設計

## 1. 戦略的位置づけ

議事録を**単独アプリ**にすると価値が薄い (国会会議録検索システムが既にある)。
本アプリの強みは **法令 / 例規データを既に持っていること** なので、
「法案 ↔ 議事録の連結」「条文 ↔ 審議発言の連結」を売りにする。

## 2. データソース

### 2.1 国会会議録 (国会会議録検索システム API)
- 公開 API: `https://kokkai.ndl.go.jp/api/`
- 形式: XML / JSON
- ライセンス: パブリックドメイン
- 採れるもの: 発言単位 (speaker, group, speech) と会議メタデータ
- API キー不要、レート制限ゆるめ

### 2.2 地方議会議事録 (Phase 3.5)
- ベンダ製 (会議録検索システム) が大半: 三菱電機ISC、富士ソフト等
- API は基本なし、HTML スクレイピング
- 公開ポリシーは自治体次第

→ Phase 3 では **国会会議録のみ** をスコープにする。

## 3. アーキテクチャ方針 (前回の議論より)

- データは別 DB / 別サービス
- UI は統合
- 連結レイヤは独立した第三のストア

### 3.1 リポジトリ構成

```
crates/
├── kokkai-client/        # 国会会議録 API クライアント
├── kokkai-normalizer/    # API レスポンス → 安定 JSON
├── linking/              # 法令 ↔ 議事録の連結データ生成
```

### 3.2 配信 URL

```
proceedings/index.json
proceedings/{meeting_id}.json                  # 会議単位 (発言全リスト)
proceedings/speakers/{speaker_id}.json         # 議員別発言集約 (Phase 3.5)
links/law-to-proceedings/{law_id}.json         # 法令 → 該当審議
links/proceedings-to-law/{meeting_id}.json     # 会議 → 言及された法令
links/article-to-speeches/{law_id}/{article_id}.json   # 条文 → 該当発言群 (Phase 3.5)
```

## 4. データモデル

### 4.1 会議

```json
{
  "schema_version": 1,
  "meeting_id": "120020241101200X06_001",
  "session": 215,
  "house": "shugiin",         // shugiin | sangiin | both
  "committee": "法務委員会",
  "date": "2024-11-01",
  "issue": "第6号",
  "speeches": [
    {
      "speech_id": "...",
      "order": 12,
      "speaker": "山田太郎",
      "speaker_id": "...",     // 議員 ID (議員DBと突合)
      "speaker_group": "自民",
      "speaker_position": "委員",
      "speech": "ご質問の趣旨は..."
    }
  ],
  "source": { "provider": "kokkai_ndl", "fetched_at": "..." }
}
```

### 4.2 連結

```json
{
  "law_id": "129AC0000000089",
  "linked_proceedings": [
    {
      "meeting_id": "...",
      "date": "2024-11-01",
      "house": "shugiin",
      "committee": "法務委員会",
      "relevance": "amendment_debate",  // amendment_debate | reference_only
      "amendment_law_num": "令和六年法律第○号",
      "speech_count_mentioning": 12,
      "confidence": 0.92,
      "match_reasons": ["amendment_law_num", "law_title_in_topic"]
    }
  ]
}
```

## 5. リンク生成アルゴリズム

1. 各会議の `topic` (議題) と `speeches[].speech` から法令名 / 改正法名を抽出
2. 抽出は aho-corasick で全法令タイトル + 改正法番号を一括マッチ
3. timeline.json の amendment_law_num と突き合わせて高信頼マッチ
4. それ以外は法令名一致のみで「reference_only」扱い
5. 自動マッチは confidence で閾値付け、手動レビュー UI で上書き可能に

## 6. UI 統合

- 法令詳細ページに「審議で見る」タブ
- timeline の各イベント横に「該当審議」リンク
- 条文ホバーで「直近で議論された会議」を表示 (Phase 3.5)
- 会議録ビュー: 会議単位で発言を流し読み、左サイドに該当法令リスト

## 7. ファイル数とサイズ

- 1 国会会期 ≒ 500 会議
- 1 会議の JSON ≒ 数百KB〜数MB
- 過去 30 年で 15,000 会議程度 → 数十 GB

→ Phase 3 では **直近 3 国会会期** に絞ってまず動かす。
過去全期は Phase 3.5 で R2 ホスティング前提で再検討。

## 8. CLI

```bash
lawpub proceedings-fetch --session 215 --cache .cache
lawpub proceedings-build-json --input .cache --output public
lawpub link-laws-and-proceedings --output public
```

## 9. 受け入れ条件

- [ ] 直近 1 国会会期分の会議録が `proceedings/` 配下に JSON で並ぶ
- [ ] 民法改正案を含む会議が、民法の `links/law-to-proceedings/129AC0000000089.json` から辿れる
- [ ] 1 つ以上の改正法について「公布 → 該当審議 → 条文 diff」が UI でつながる

## 10. 非ゴール

- 地方議会 (Phase 3.5)
- 議員 (speaker) を横断する縦軸 (発言履歴・スタンス追跡)
- 発言の意味的検索 / 要約 (将来 LLM 統合)
- 起立採決等の表決結果 (別データソースが必要)

## 11. 実装順

1. kokkai-client で 1 会議分の生 JSON 取得 + 正規化
2. 1 会期分一括取得 + proceedings/ 出力
3. linking crate: 法令名抽出 + amendment_law_num マッチング
4. links/law-to-proceedings/{law_id}.json 生成
5. UI に「審議で見る」タブを追加
6. confidence 評価、手動上書きデータの取り込み口を用意
