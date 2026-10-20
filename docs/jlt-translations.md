# 法令英訳（日本法令外国語訳データベース）の取り込み

> Issue #197。法務省「日本法令外国語訳データベース」(JLT, <https://www.japaneselawtranslation.go.jp>)
> の英訳を条単位で取り込み、法令詳細で日本語の条文に英訳を併記し、英語の全文検索に使う。
> 最終確認: 2026-10-20

## 1. 利用条件（再配布可・出典と加工の旨の記載が条件）

| 確認先 | 内容 |
|---|---|
| [サイト利用規約](https://www.japaneselawtranslation.go.jp/ja/index/terms) | 掲載情報は権利表記がない限り **公共データ利用規約（第1.0版）(PDL1.0)** を適用。著作権は法務省。適用除外はロゴマーク・キャラクターのみ |
| 各ページの「ご利用上の注意」 | 「掲載しているデータは、利用規約に従い、引用し、複製し又は転載して差し支えありません」 |
| [PDL1.0](https://www.digital.go.jp/resources/open_data/public_data_license_v1.0) | 複製・公衆送信・翻案・商用利用を認める。CC BY 4.0 互換。条件は ①出典の記載 ②編集・加工した場合はその旨と主体の記載 ③加工物を国が作成したかのように公表しない |
| 注意書き（翻訳について） | 翻訳は正文ではなく、最終改正版でないものも含む参考資料。法的効力は日本語の法令のみ。「（暫定版）」はネイティブ・専門家のチェック前 |
| robots.txt | 不在（404） |
| API / 一括ダウンロード | 無い。法令ごとに txt / docx / pdf / tmx のダウンロードはあるが、条単位の日英対応は「日英併記」HTML で取る |

結論: **再配布可**。次を守る。

- `laws/{law_id}/en.json` と `jlt/index.json` の `source` に出典名・URL・PDL1.0・加工の旨
  （条単位への分割と e-Gov 法令 ID への対応付けは lawrenceanum が加工）・参考訳である旨（日英）を入れる。
- SPA の英訳表示には出典リンク・PDL1.0 リンク・加工の旨・参考訳の注意書き・翻訳日・
  反映済みの最終改正・暫定版の印を出す。各条に JLT の該当箇所へのリンクを付ける。
- 英訳本文は改変しない（空白の正規化と、号・号の細分を `(i) …` の行に並べる整形のみ）。

## 2. 取得経路

- 一覧: `POST /ja/laws/result/?page=N`。フォームの `_csrfToken` と同名 cookie が要る
  （`GET /ja/laws/search-cat` で得る）。分野 `bu`（2, 4, …, 4096 の 12 区分）ごとに引く
  （全分野同時は `error/too_many`）。検索条件はサーバに保存されないのでページ送りでも毎回送る。
  `ia=03` で暫定版も含める。1 行に翻訳版 ID・法令名・法令番号・翻訳日・辞書バージョン。
- 本文: `GET /ja/laws/view/{jlt_id}/je`（日英併記）。`jlt_id` は翻訳版ごとの ID で、翻訳が
  更新されると新しい ID になる（旧 ID は最新版へ redirect）。`#mp`（本則）の `div.Article` に
  日英の `ArticleCaption` / `ParagraphSentence` / `ItemSentence` / `Subitem*Sentence` が交互に並ぶ。
  附則は載っていない。
- 2026-10-20 時点で 12 分野の和集合は **1,042 翻訳**。

## 3. パイプライン

```
lawpub jlt-fetch       → .cache/jlt/list.json + .cache/jlt/laws/{jlt_id}.json   (corpus workflow, R2: jlt-cache.tar.zst)
lawpub jlt-build-json  → public/laws/{law_id}/en.json + public/jlt/index.json    (law workflow, build-json の後)
lawpub build-search-db → search.db の laws_en_fts (porter unicode61)
```

- `jlt-fetch`: 一覧は毎回引き直し、本文は未取得の翻訳版 ID だけ取る（2 秒間隔、1 run 200 件）。
  一覧取得に失敗した分野は前回の一覧の行を引き継ぐ。`--refresh` で取り直し、`--categories 8,64` で分野を絞る。
- `jlt-build-json`: `public/laws/index.json` の `law_num` で law_id に対応付ける。
  1. 手動対応表 `crates/jlt-client/data/law_id_overrides.json`（`{"<jlt_id>": {"law_id": "...", "note": "..."}}`）
  2. 法令番号の一致（表記ゆれ: 空白、共同省令の区切り `、` → `・`）。同番号が複数なら題名で絞る
  3. 題名が一意に一致し、法令番号の末尾 `第…号` も一致（JLT 側の発令者・年の誤記を救う。
     全部改正で番号の変わった旧法の訳を新法に付けないよう、番号の末尾までは一致を求める）
  
  対応表は `public/jlt/index.json` の `translations[]`（`law_id` / `matched_by` / `unresolved`）。
  1 法令に複数の翻訳（民法の第一〜三編と第四・五編など）があれば条単位でまとめ、同じ条は翻訳日の新しいほうを採る。
- 条の対応: 日本語の条名（`第三条の二`）を e-Gov の `article_no` と文字列で突き合わせ、無ければ漢数字から
  `art_3_2`（削除範囲は `art_38:84`）を導く。`translations[].matched_article_count` は e-Gov 現行版に
  存在した条数。

## 4. 実測（2026-10-20, 全 1,042 翻訳の一覧 + 本文 30 件）

- 対応付け: 950 / 1,042（91%）が e-Gov の 941 法令に対応（法令番号 943・題名 7）。
  未解決 92 件の内訳は、法令番号の無い「告示等」（ガイドライン・様式など）49 件と、
  e-Gov に無い番号 43 件（告示・最高裁判所規則 22 件、廃止法令（外国人登録法、
  行政機関個人情報保護法など）ほか 21 件）。※ 照合に使った e-Gov 索引は R2 の
  `revisions_meta.jsonl` から作ったもので、一部法令（歯科医師法など）が欠けていた。本番の
  `laws/index.json` ではもう少し上がる見込み。
- 条の一致: 本文取得 30 件で 999 / 1,194 条（84%）。外れは翻訳が旧版のままの法令
  （翻訳後に条が追加・削除された）と、条を持たない法令（本則が項だけの政令・省令）。

## 5. 既知の制約・今後

- 翻訳は現行版と食い違うことがある（例: 民法第4条は翻訳では成年年齢 18 歳、e-Gov の旧版表示では二十歳）。
  SPA は「○○までの改正を反映」を出すだけで、版の差分は検出しない。
- 本則が項だけで条の無い法令、附則、別表は英訳を出していない。
- 定義語のハイライト（定義規定の `"…" means …` を抽出して本文で強調）は未実装。
- SPA の検索は「法令英訳」対象で `laws_en_fts` を引く（英単語を含むクエリだけ。機能語は AND から外す）。
  結果から法令詳細を開くと英訳併記を有効にして該当条へ飛ぶ。
