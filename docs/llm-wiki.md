# LLM wiki (OKF)

国会会議録・審議会議事録のうち法令に言及した発言を、「いつ・どの会議で・誰が・どんな理由で・
どの法令について何を述べ、どうなったか」を後から追える wiki に日次で積み上げる。
形式は [Open Knowledge Format](https://agentic-ai.readthedocs.io/en/latest/Standards/open-knowledge-format/)
(Markdown + YAML frontmatter、必須は `type`)。

[public-corpus-roadmap.md](public-corpus-roadmap.md) の原則どおり、wiki は正規化コーパスの**下流の派生物**で、
正本は常に Pages の静的 JSON。wiki は消して作り直せる。

## 置き場所

- 本体: orphan ブランチ `wiki`（main と履歴を分け、日次コミットで main を汚さない）
- CI では `git worktree` で `wiki/` に展開する。作業領域 `.wiki-work/` とともに main では追跡しない。

```
wiki/
├── index.md                     # type: index（生成）
├── log.md                       # type: log（生成・日次追記）
├── laws/{law_id}.md             # type: law   — 経緯の要約 (LLM) + 時系列 (生成)
├── meetings/kokkai/{id}.md      # type: meeting — 要点 (LLM) + メタ (生成)
├── meetings/shingikai/{id}.md
├── bills/{回次}/{議案ID}.md     # type: bill   — 概要と経緯 (LLM: 提出理由・主な内容・附帯決議を引用付きで) + 審議経過・会派の賛否（生成）
├── pubcomments/{案件ID}.md      # type: pubcomment — 概要と結果 (LLM: 何の案か・主な意見と考え方・修正の有無) + 募集情報（生成）
├── kanpo/{日付}/{号-ページ-hash}.md # type: kanpo — 改正の内容 (LLM: 何をどう改め、いつ施行するか) + 掲載情報・官報の本文（生成）
├── committees/{会議体}.md       # type: committee — 開催回・扱った法令・発言者（生成）
├── parties/{会派}.md            # type: party   — 発言した人物・議案への賛否・提出した議案（生成）
├── people/{氏名}.md             # type: person — 法令に言及した発言の一覧（生成）。国会は議事進行を除く発言者全員、
│                                #   審議会は官職者（例: 森光健康・生活衛生局長）。姓だけの委員は会議体ページに載せる
├── topics/{論点}.md             # type: topic  — 論点ごとのまとめ (LLM)
├── .lawpub/state.json           # 処理済み会議
└── .lawpub/roster/{corpus}/{会議ID}.json  # 発言者名簿（生成）— 人物・会議体ページの元
```

### 会派（政党）ページ

`parties/{会派}.md` は、発言者名簿・会議ページが持つ発言者の会派と、議案ページの frontmatter
`votes`（院ごとの賛成・反対会派）・`proposer_groups`（提出会派）から作る。LLM は使わない。

- この会派として発言した人物（人物ページへのリンクと発言した会議数）
- 議案への賛否（日付・議案・院・賛否）と、提出した議案
- よく言及した法令（上位 30）

会派名は会議録・議案の**表記のまま**にする。院や会期で表記が変わる（「自由民主党・無所属の会」(衆)
と「自由民主党」(参) など）が、勝手に名寄せすると出典と食い違うので、名寄せは今はしない。
人物ページからは、最新の会派のページへリンクする。

### 人物・会議体ページはコーパスの全会議から作る

LLM が要点を書けるのは 1 日数件 (`WIKI_MAX_ITEMS`) で、対象も `lookback_days` の窓の中だけなので、
人物ページを会議ページの frontmatter からだけ作っていた頃は、窓からこぼれた会議の発言者が永久に載らなかった
（コーパスに 1,000 件以上の会議があるのに人物ページが 3 件、という状態になる）。

そこで発言者の抽出を LLM から切り離し、`wiki-plan` が**コーパスの全会議**について発言者名簿
`.lawpub/roster/{corpus}/{会議ID}.json`（会議の表記・原文 URL・言及した法令・発言者と会派）を作る。
`wiki-finalize` は会議ページと名簿を合わせて人物・会議体ページを作り、会議ページがまだ無い回は原文
（国会会議録・議事録）へ直接つなぐ。

- 日付の窓は無い。`lookback_days` は「LLM に要点を書かせる会議」の窓のままにする。
- 会議録は公開後に変わらないので、一度作った名簿は作り直さない（`ROSTER_VERSION` を上げたときだけ）。
- 国会は法令リンクの無い回の本体を取りに行かない（「法令に言及した発言者」の定義どおり）。
  リンクが無いことも名簿に残して、翌日以降に引き直さない。
- 審議会は件数が少なく資料だけでは中身がわからないので、議事録があれば必ず読む。
- 1 回の run で作る名簿は `WIKI_MAX_ROSTER`（既定 1200）件まで。LLM は使わず、1 会議あたり HTTP 数回。

## アプリでの閲覧

法令ワークフロー (`update-law-data.yml`) が Pages へデプロイするとき、`wiki` ブランチを
`lawpub wiki-export` で SPA 用の JSON に変換して `public/wiki/` に同梱する。

```
public/wiki/index.json                 全ページの一覧 (type / title / description / date / tags)
public/wiki/graph.json                 ナレッジグラフ (ノード = ページ、エッジ = ページ間リンク)
public/wiki/page/{path}.json           frontmatter と本文 (Markdown)
```

SPA の `#/wiki` で、type 別に色分けしたナレッジグラフと一覧、各ページ（要点・出典の脚注・
つながっているページ）を閲覧できる。法令詳細画面には、wiki ページがある法令だけ「wiki」への導線が出る。
wiki の更新 (JST 09:00) は、次の法令ワークフローのデプロイ (JST 12:30 など) で反映される。

## 自治体の対応（例規）

`wiki-plan` は週 1 回、R2 に配信している自治体例規（約 300 自治体・22 万件）の題名を、wiki に法令ページがある法令の名前と
最長一致で照合し（4 文字未満の法令名は誤一致しやすいので除く）、法令ページに「自治体の対応（例規）」節（件数・自治体数・
最近の制定例）を載せる。LLM は使わない。ワークフローは Secret `R2_PUBLIC_URL` から配信元を組み立てる。

## 官報

法令ページの時系列に、その法令を改正する官報の項目と、その法令に基づく告示の改正を載せる。LLM は使わない。

- `kanpo-link`（法令ワークフロー）が、官報の目次の標題から改正対象の法令を引いて `kanpo/{date}/index.json` の
  `linked_laws` に入れる（`Xの一部を改正する政令` → X、`Xの一部を改正する法律の一部の施行期日を定める政令` → X）。
  e-Gov の改正履歴との突合は e-Gov への反映が数か月遅れて直近の官報に当たらないため、標題からも引く。
  告示は e-Gov に無いので、告示の改正は標題の先頭の根拠法令（`外国為替令第七条の規定に基づき、…`）を
  `basis_laws` に入れる。帰化・破産などの公告は改正の形の標題にならないので、どの法令にもつながらない。
  配信している日付の一覧は `kanpo/index.json`。
- `wiki-plan` が、wiki に法令ページがある法令について、その項目を frontmatter の `kanpo` にためる。
  デジタル官報の公開は約 90 日で配信側からも消えうるため、取り込んだ項目は wiki 側に残し続ける
  （対応付けの規則を変えたときは `KANPO_VERSION` を上げると引き直す）。官報だけでは法令ページを作らない。
- 改め文の本文が取れている項目は、官報ページ `kanpo/{date}/{slug}.md` を作る（wiki に法令ページがある法令に
  つながるものだけ）。ページには掲載情報と官報の本文（自動抽出）を載せ、`WIKI_MAX_KANPO`（既定 3）件ずつ
  LLM に「何をどう改め、いつ施行するか」を書かせ、法令ページの経緯にも追記させる。
- 官報の引用 `[kanpo:{date}/{slug}](PDF) 「…」` は、配信側ではなく**官報ページに保存した本文**と照合する。
  デジタル官報は約 90 日で消え、パーサの改善で抽出結果も変わるため。要約済みのページは本文を固定する。
- `wiki-finalize` が時系列表に描く。e-Gov に反映済みの公布の行には「官報」へのリンクを付け、
  まだ反映されていない改正と告示の改正は「官報」の行として足す。官報ページがあればそちらを指し、
  LLM が書いた 1 行を添える。

## 日次の流れ (`.github/workflows/update-wiki.yml`, JST 09:00)

| 段階 | 実行者 | 内容 |
|---|---|---|
| `lawpub wiki-plan` | 決定的 | 未処理の会議を新しい順に見て、法令リンクのあるものを最大 `WIKI_MAX_ITEMS` 件選ぶ。法令名を含む発言だけを前後の文脈付きで抜粋したソースバンドルと、ページの雛形を作る。あわせて議案（件名から対象法令を決める）と法令の改正履歴（直近 10 年の公布・施行）と官報の項目、コーパス全会議の発言者名簿を取り込む |
| Claude Code | LLM | `.github/wiki-agent.md` に従い、抜粋だけを根拠に、会議の要点と、法令・会議体・人物・論点ページの「経緯」（古い順の日付見出し。誰が・何について・どんな理由で・何を述べ・どうなったか）を引用付きで書き足す。使えるツールは wiki/ の Read/Edit/Write のみ（Bash・ネットワークは無し）。タスクは `wiki-plan` が 4 件ずつのチャンク（`.wiki-work/chunks/NNN.md`）に分け、チャンクごとに新しい Claude Code（CLI）を呼ぶ（一度に何十件も渡すと途中で切り上げてしまうため） |
| `lawpub wiki-finalize` | 決定的 | 法令ページの時系列表（会議・議案の経過・公布/施行・官報を 1 本に。改正法は法律番号で議案ページにつなぐ）、人物・会議体ページ（会議ページ＋発言者名簿）、index、log を再生成する。未完了の会議は雛形を消して翌日に回す |
| `lawpub wiki-check` | 決定的 | OKF frontmatter・相対リンク・引用を検証する。失敗したら LLM に 1 回だけ修正させ、それでも駄目なら push しない |

### 引用の検証

LLM の記述には必ず脚注で根拠を付ける。

```
[^1]: [kokkai:122105261X01720260727_044](https://kokkai.ndl.go.jp/txt/122105261X01720260727/44) 「今直ちに新たな有識者会議を設置する考えはございません」
```

`wiki-check` は、発言 ID が実在すること、URL がその発言のものであること、「」内が発言本文の
連続した一節であること（空白は無視）を照合する。言い換えや捏造された引用は push 前に落ちる。

### コスト上限

- 1 回あたりの会議数: `WIKI_MAX_ITEMS`（既定 5）、議案数: `WIKI_MAX_BILLS`（既定 5。経過が進んだ議案も再要約）、パブコメ数: `WIKI_MAX_PUBCOMMENTS`（既定 3。結果公示済みの案件）、官報の項目数: `WIKI_MAX_KANPO`（既定 3。1 項目あたり本文 12,000 字まで）。1 会議ごとに会議・法令・会議体・人物ページへ経緯を追記する。法令リンクの無い会議は LLM に渡さない。
- LLM に渡すのは 1 会議あたり最大 24 発言・14,000 字の抜粋だけで、会議全文は渡さない。
- 未処理の残りは翌日以降に新しい順で消化する（`lookback_days` 既定 45 日）。
- 発言者名簿はこの上限の外。LLM を呼ばないので、コーパスの全会議が対象。

## セットアップ

1. Claude の認証を Secrets に登録する。個人の Pro / Max プランで動かす場合（推奨）:

   ```bash
   claude setup-token                          # ブラウザでログインし、1 年有効の OAuth トークンを発行
   gh secret set CLAUDE_CODE_OAUTH_TOKEN       # 表示されたトークンを貼り付ける
   ```

   利用量はそのプランの上限から消費される。従量課金にしたい場合は代わりに `ANTHROPIC_API_KEY` を登録する。
2. 任意で Variables に `WIKI_MODEL`（既定 `claude-sonnet-5-5`）と `WIKI_MAX_ITEMS` を設定する。
3. Actions → "Update LLM wiki" を手動実行する。初回は `wiki` ブランチが orphan として作成される。
   - 試運転は `push_mode: pr` にすると、`wiki` 宛ての PR で差分を確認できる（2 回目以降。PR を
     マージするまで state が進まないので、日次運用は `direct` にする）。
   - PR モードを使う場合は、Settings → Actions → "Allow GitHub Actions to create and approve pull requests" を有効にする。

## ローカルで試す

```bash
cargo build --release -p lawpub-cli
git init wiki   # --changed で変更ページを判定するため
./target/release/lawpub wiki-plan --max-items 3
claude -p "\`.github/wiki-agent.md\` の指示に従い、\`.wiki-work/plan.md\` の全タスクについて wiki を更新してください。" \
  --allowedTools "Read,Glob,Grep,Edit(wiki/**),Write(wiki/**)" --disallowedTools "Bash,WebFetch,WebSearch"
./target/release/lawpub wiki-finalize
./target/release/lawpub wiki-check --changed
```

`--base-url` にローカルの `public/` を渡せば、Pages ではなく手元のビルド結果から作れる。

## 今後

- 官報の遡及: R2 に蓄積した官報原本（約 90 日分）から、配信側に無い日付の項目も取り込む。
- 週次の整備ジョブ: 重複した論点ページの統合、記述の矛盾の検出。
- 審議会の人物同定（議事録は「森委員」のように姓＋役職のため、現状は官職者しか人物ページを作らない）。
- 会派の名寄せ（「自由民主党・無所属の会」(衆) と「自由民主党」(参)、会期で変わる表記）。
  今は会議録・議案の表記のまま 1 会派 1 ページにしている。
