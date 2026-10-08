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
├── bills/{回次}/{議案ID}.md     # type: bill   — 審議経過・会派の賛否・対象法令（生成、LLM 不使用）
├── committees/{会議体}.md       # type: committee — 開催回・扱った法令・発言者（生成）
├── people/{氏名}.md             # type: person — 法令に言及した発言の一覧（生成）。国会は議事進行を除く発言者全員、
│                                #   審議会は官職者（例: 森光健康・生活衛生局長）。姓だけの委員は会議体ページに載せる
├── topics/{論点}.md             # type: topic  — 論点ごとのまとめ (LLM)
└── .lawpub/state.json           # 処理済み会議
```

## アプリでの閲覧

法令ワークフロー (`update-law-data.yml`) が Pages へデプロイするとき、`wiki` ブランチを
`lawpub wiki-export` で SPA 用の JSON に変換して `public/wiki/` に同梱する。

```
public/wiki/index.json                 全ページの一覧 (type / title / description / date / tags)
public/wiki/graph.json                 ナレッジグラフ (ノード = ページ、エッジ = ページ間リンク)
public/wiki/page/{path}.json           frontmatter と本文 (Markdown)
```

SPA の `#/wiki` で、type 別に色分けしたナレッジグラフと一覧、各ページ（要点・出典の脚注・
つながっているページ）を閲覧できる。法令詳細画面には、wiki ページがある法令だけ「経緯 wiki」への導線が出る。
wiki の更新 (JST 09:00) は、次の法令ワークフローのデプロイ (JST 12:30 など) で反映される。

## 日次の流れ (`.github/workflows/update-wiki.yml`, JST 09:00)

| 段階 | 実行者 | 内容 |
|---|---|---|
| `lawpub wiki-plan` | 決定的 | 未処理の会議を新しい順に見て、法令リンクのあるものを最大 `WIKI_MAX_ITEMS` 件選ぶ。法令名を含む発言だけを前後の文脈付きで抜粋したソースバンドルと、ページの雛形を作る。あわせて議案（件名から対象法令を決める）と法令の改正履歴（直近 10 年の公布・施行）を取り込む |
| Claude Code | LLM | `.github/wiki-agent.md` に従い、抜粋だけを根拠に、会議の要点と、法令・会議体・人物・論点ページの「経緯」（古い順の日付見出し。誰が・何について・どんな理由で・何を述べ・どうなったか）を引用付きで書き足す。使えるツールは wiki/ の Read/Edit/Write のみ（Bash・ネットワークは無し） |
| `lawpub wiki-finalize` | 決定的 | 法令ページの時系列表（会議・議案の経過・公布/施行を 1 本に。改正法は法律番号で議案ページにつなぐ）、人物ページ、index、log を再生成する。未完了の会議は雛形を消して翌日に回す |
| `lawpub wiki-check` | 決定的 | OKF frontmatter・相対リンク・引用を検証する。失敗したら LLM に 1 回だけ修正させ、それでも駄目なら push しない |

### 引用の検証

LLM の記述には必ず脚注で根拠を付ける。

```
[^1]: [kokkai:122105261X01720260727_044](https://kokkai.ndl.go.jp/txt/122105261X01720260727/44) 「今直ちに新たな有識者会議を設置する考えはございません」
```

`wiki-check` は、発言 ID が実在すること、URL がその発言のものであること、「」内が発言本文の
連続した一節であること（空白は無視）を照合する。言い換えや捏造された引用は push 前に落ちる。

### コスト上限

- 1 回あたりの会議数: `WIKI_MAX_ITEMS`（既定 5）。1 会議ごとに会議・法令・会議体・人物ページへ経緯を追記する。法令リンクの無い会議は LLM に渡さない。
- LLM に渡すのは 1 会議あたり最大 24 発言・14,000 字の抜粋だけで、会議全文は渡さない。
- 未処理の残りは翌日以降に新しい順で消化する（`lookback_days` 既定 45 日）。

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

- 「どうなったか」の強化: 議案 (gian) の審議経過・官報・法令の改正履歴を時系列に合流させる。
- 週次の整備ジョブ: 重複した論点ページの統合、記述の矛盾の検出。
- 審議会の人物同定（議事録は「森委員」のように姓＋役職のため、現状は人物ページを作らない）。
