# kanpo-parser

[デジタル官報](https://www.kanpo.go.jp/)の項目別 PDF（縦書き・多段組）から本文の読み順を復元し、
改め文・新旧対照表を構造化する Rust crate。

- 縦書きの読み順復元（段組みの分割・右→左の列並べ・ページ柱の除去・縦書き約物の正規化）
- 回転ページ・横書きページ（官庁報告「労働」の最低賃金公示、公告「諸事項」、号外の様式など）の
  読み順復元（列の検出・左→右の列並べ・左端帯の柱の除去・字間空けの見出しの連結）
- 新旧対照表の改正後/改正前の分離（同じページの下段へ欄見出し無しで続く表、継続ページも含む）
- 条見出し・号ごとの改正後/改正前の行対応、表中の別表（罫線表）の 2D 復元
- 1 ページに同居する複数記事（〇…告示第…号 など）の分割

## 前提

PDF から座標付きテキストと罫線を取り出すのは pure Rust の [kanpo-pdf](../kanpo-pdf/)（poppler も外部コマンドも
要らない。WASM でも動く）。出力は `pdftotext -bbox-layout` と同じ形の XHTML で、パーサはこれを読む。

## 依存に追加する

crates.io には公開していないので git 依存で使う。

```toml
[dependencies]
kanpo-parser = { git = "https://github.com/bokuweb/lawrenceanum" }
```

## 使い方

```rust,no_run
let pdf = std::fs::read("20261005h018010002.pdf")?;

// 読み順を復元したページ本文と形式（"prose" / "shinkyu" / "unknown"）
let ex = kanpo_parser::extract(&pdf)?;

// 記事ごとに分ける
for article in kanpo_parser::segment_articles(&ex.text) {
    // 新旧対照表・散文を構造化（serde で JSON 化できる）
    let doc = kanpo_parser::Document::from_text(&article);
    println!("{}", serde_json::to_string_pretty(&doc)?);
}
# Ok::<(), anyhow::Error>(())
```

複数ページにまたがる項目（告示など）は、ページごとの座標付きテキストと罫線をまとめて
`analyze_item` に渡す。ページで切れた新旧対照表は 1 つの表につながり、新旧対照表の帯の中の
罫線の表（別表など）はセルの結合まで含めて、本文の流れの中の表（`Run::table`）になる。

```rust,no_run
let pages: Vec<kanpo_parser::PageInput> = ["p1.pdf", "p2.pdf"]
    .iter()
    .map(|path| {
        let pdf = std::fs::read(path)?;
        Ok(kanpo_parser::PageInput {
            xhtml: kanpo_parser::bbox_xhtml(&pdf)?,
            rules: kanpo_parser::page_rules(&pdf).ok(),
        })
    })
    .collect::<anyhow::Result<_>>()?;
let analysis = kanpo_parser::analyze_item(&pages);
for article in &analysis.articles {
    println!("{}", serde_json::to_string_pretty(&article.document)?);
}
# Ok::<(), anyhow::Error>(())
```

ページごとの入力は `page_inputs(&pdf)` でもまとめて作れる。`pdftotext -bbox-layout` の XHTML
（以前の poppler の出力など）が手元にあれば、それを `reconstruct_vertical(&xhtml)` / `analyze(&xhtml)` に渡してもよい。

## デバッグ

```bash
cargo run -p kanpo-parser --example prepare -- input.pdf   # input.bbox.html と input.rules.json を作る
cargo run -p kanpo-parser --example dump -- out.bbox.html
cargo run -p kanpo-parser --example structured -- out.bbox.html
```

## 解析過程の可視化（`analyze`）

`analyze(&xhtml)` は本文・記事・構造化結果に加えて、各語がどの領域（前文・改正後・改正前・
表外・本文）のどの行に組まれたか、捨てた語はその理由（余白・欄見出し・柱）を返す。
WASM（`crates/kanpo-parser-wasm`）でも動き（PDF から直接解析する `analyze_pdf` もある）、Web の playground
（`/kanpo-playground`）がこれを使って PDF の上に割り当てを重ねて表示する。

```bash
cargo run -p kanpo-parser --example trace -- out.bbox.html
```

## 回帰テスト用コーパス

`tests/corpus/<case>/` に実官報のページ（`page.pdf`・`page.bbox.html`・`page.rules.json`。複数ページの
ケースは `page-2.*` …）と、何を確かめるケースか
（`case.json`）、目視で確認した期待出力（`expected.txt`・`expected.json`）を置いている。
`tests/corpus.rs` は `page.pdf` を kanpo-pdf で読んで解析する（`page.bbox.html`・`page.rules.json` は
playground が読むために置いている PDF からの生成物で、テストは PDF から作ったものと同じかも確かめる）。
パーサや kanpo-pdf を直して出力が変わったら、差分を確かめてから更新する。

```bash
# 期待出力と .bbox.html・.rules.json を作り直す
UPDATE_EXPECT=1 cargo test -p kanpo-parser --test corpus
```

官報原本は GitHub Actions（`collect-kanpo-pdf.yml`）が毎日 R2 の `kanpo-pdf/` に蓄積している
（デジタル官報の公開は約 90 日）。手元に取るには:

```bash
cargo run -p lawpub-cli -- kanpo-archive --date 2026-10-05 --out .cache/kanpo-archive
cargo run --release -p kanpo-parser --example scan -- .cache/kanpo-archive > scan.tsv
```

法令ワークフロー（`update-law-data.yml`）は、配信用の改め文（`.cache/kanpo/{date}.json`）が無い日付をこの原本から
抽出し直す（`lawpub kanpo-reextract`。`.cache` は Actions の cache なので追い出されると消える）。パーサを直したあとに
過去の項目へ反映するには、ワークフローを `kanpo_reextract_all: true` で手動実行する。手元では:

```bash
cargo run --release -p lawpub-cli -- kanpo-reextract --archive https://…r2.dev/kanpo-pdf --cache .cache --date 2026-10-09
```

パーサを直したら、蓄積した官報の全ページの本文を直す前後で書き出して比べる（`textdump` は 1 つの語を
2 つの行に組んだページも標準エラーに出す）:

```bash
cargo run --release -p kanpo-parser --example textdump -- .cache/kanpo-archive /tmp/after
diff -r /tmp/before /tmp/after
```

`scan` は全ページを解析して怪しいページ（欄見出しがあるのに表にならない、改正後/改正前の
行数が極端に偏る、私用 CID の化け候補など）に印を付ける。新しいエッジケースを見つけたら
コーパスに足す。

`egov_check` は、改め文の新旧対照表の改正後欄・改正前欄を、e-Gov（法令 API v2）の改正後・改正前の
法令本文と突き合わせる（欄の各行と、続く 2 行が本文に含まれるか）。含まれない行は読み順・字の誤りの
候補。様式の図・目次の書き方の違い・段階的に施行される改正（改正前の版の選び方）は e-Gov との違いとして
出る。e-Gov にまだ載っていない改正（施行前のもの）は見られない:

```bash
cargo run --release -p kanpo-parser --example egov_check -- <bbox.html のディレクトリ> .cache/egov-check
```

`egov_rows` は、蓄積した官報（`kanpo-archive` の出力）の改め文の項目を本番と同じく PDF から項目単位で解析し、
標題の記事の新旧対照表を e-Gov と突き合わせる。欄の各行が改正後・改正前の版の本文に含まれるかに加えて、
**行の組み**も確かめる: e-Gov の両方の版を条・項・号などの単位に分けて版どうしを組み、表の行の改正後欄と
改正前欄が当たる単位が組になっていれば「合う」、どちらも別の単位と組になっていれば「組み違い」と数える。
段階的に施行される改正では、同じ省令の版のうち欄の文がいちばん多く含まれるものを選ぶ。様式の欄・表の注記・
省略の行は比べない:

```bash
cargo run --release -p kanpo-parser --example egov_rows -- .cache/kanpo-archive .cache/egov-check
# 1 項目だけ: … .cache/egov-check 20260918g00209:55
```

## 蓄積した官報のスナップショットテスト

`tests/archive/` に、蓄積した官報から層別に選んだページ（号の種類 × 組み方 × 新旧対照表 ×
罫線の表 × 割注 × 私用 CID）の原本 PDF と、2〜6 ページの項目を置いている。`tests/archive_snapshots.rs` は
本番と同じく PDF を kanpo-pdf で読んで解析し、ページごとの本文と記事の
構造、項目ごとの標題の記事の構造を [insta](https://insta.rs) のスナップショット
（`tests/snapshots/`）と比べる。CI（`kanpo-parser-test.yml`）でも回る。

```bash
cargo test -p kanpo-parser --test archive_snapshots
# 出力が変わったら差分を目視で確かめて受け入れる
cargo insta review -p kanpo-parser
# 選び直す（アーカイブが増えたとき）
cargo run --release -p kanpo-parser --example snapshot_sample -- .cache/kanpo-archive crates/kanpo-parser/tests/archive 150
```

## 読み順の精度（監査）

蓄積した官報（2026 年 7 月〜10 月の約 11,800 ページ）から、それまでの監査で見ていないページを毎回 30 ページ
選び、原本の画像と出力を突き合わせて、ページごとに次の 3 段階で評価している（文字の層の無い本文や字形の
誤りは対象外）。読み順を変える修正を入れるたびに新しい 30 ページで測り直し、この表に加える。

- ok: 人が読む順のとおり（字間・空白などの見た目だけの違いは問わない）
- minor: 局所的な崩れで、意味は取れる（欄見出しのずれ、短い項目名と値の分離、行 1 つの位置違いなど）
- major: 別の記事・段・欄が混ざる、表の行が崩れて値と項目が対応しない、文の順が入れ替わるなど

| 監査 | ok | minor | major | ok か minor |
|---|---|---|---|---|
| 14 回目 | 23 | 5 | 2 | 93% |
| 15 回目 | 20 | 6 | 4 | 87% |
| 16 回目 | 22 | 4 | 4 | 87% |
| 17 回目 | 19 | 8 | 3 | 90% |
| 18 回目 | 22 | 5 | 3 | 90% |
| 19 回目 | 22 | 5 | 3 | 90% |
| 20 回目 | 21 | 7 | 2 | 93% |
| 21 回目 | 21 | 7 | 2 | 93% |
| 22 回目 | 26 | 4 | 0 | 100% |
| 23 回目 | 22 | 5 | 3 | 90% |
| 24 回目 | 22 | 5 | 3 | 90% |
| 25 回目 | 19 | 5 | 6 | 80% |
| 26 回目 | 24 | 6 | 0 | 100% |
| 27 回目 | 22 | 6 | 2 | 93% |
| 28 回目 | 21 | 7 | 2 | 93% |
| 29 回目 | 24 | 4 | 2 | 93% |
| 30 回目 | 22 | 5 | 3 | 90% |
| 31 回目 | 24 | 5 | 1 | 97% |
| 32 回目 | 23 | 5 | 2 | 93% |
| 33 回目 | 21 | 7 | 2 | 93% |
| 34 回目 | 24 | 4 | 2 | 93% |
| 35 回目 | 24 | 5 | 1 | 97% |
| 36 回目 | 22 | 6 | 2 | 93% |
| 37 回目 | 23 | 7 | 0 | 100% |
| 38 回目 | 23 | 5 | 2 | 93% |
| 39 回目 | 25 | 4 | 1 | 97% |
| 40 回目 | 21 | 9 | 0 | 100% |
| 41 回目 | 27 | 1 | 2 | 93% |
| 42 回目 | 23 | 4 | 3 | 90% |
| 43 回目 | 28 | 1 | 1 | 97% |
| 44 回目 | 23 | 5 | 2 | 93% |
| 45 回目 | 26 | 3 | 1 | 97% |
| 46 回目 | 27 | 1 | 2 | 93% |
| 47 回目 | 23 | 6 | 1 | 97% |
| 48 回目 | 28 | 2 | 0 | 100% |
| 49 回目 | 22 | 6 | 2 | 93% |
| 50 回目 | 24 | 5 | 1 | 97% |
| 51 回目 | 25 | 2 | 3 | 90% |
| 52 回目 | 20 | 9 | 1 | 97% |
| 53 回目 | 22 | 5 | 3 | 90% |
| 54 回目 | 20 | 7 | 3 | 90% |
| 55 回目 | 21 | 5 | 4 | 87% |
| 56 回目 | 21 | 6 | 3 | 90% |
| 57 回目 | 26 | 4 | 0 | 100% |
| 58 回目 | 23 | 4 | 3 | 90% |
| 59 回目 | 24 | 4 | 2 | 93% |
| 60 回目 | 20 | 5 | 5 | 83% |
| 61 回目 | 25 | 5 | 0 | 100% |
| 62 回目 | 24 | 5 | 1 | 97% |
| 63 回目 | 23 | 4 | 3 | 90% |
| 64 回目 | 21 | 6 | 3 | 90% |
| 65 回目 | 23 | 4 | 3 | 90% |
| 66 回目 | 27 | 2 | 1 | 97% |
| 67 回目 | 22 | 8 | 0 | 100% |
| 68 回目 | 22 | 7 | 1 | 97% |
| 69 回目 | 21 | 7 | 2 | 93% |
| 70 回目 | 25 | 4 | 1 | 97% |
| 71 回目 | 24 | 5 | 1 | 97% |
| 72 回目 | 26 | 2 | 2 | 93% |
| 73 回目 | 24 | 5 | 1 | 97% |
| 74 回目 | 24 | 4 | 2 | 93% |
| 75 回目 | 21 | 7 | 2 | 93% |
| 76 回目 | 25 | 5 | 0 | 100% |
| 77 回目 | 26 | 1 | 3 | 90% |
| 78 回目 | 27 | 3 | 0 | 100% |
| 79 回目 | 28 | 2 | 0 | 100% |

## 既知の限界

- 縦書きと横書きが混在するページ（号外の決算公告など）は、どちらか多い方の読み方で組む。縦書きのページの中の
  横組みの欄（題字の下の「（号外）」「発行　内閣府」、決算公告の貸借対照表の枠など、横組みの行が 2 行以上続く欄）は
  横の行に組んで欄の位置に置く。欄の中に縦組みの字があっても、10 字までの短い列で字数が欄の 5 分の 1 までなら（貸借対照表の
  「資産の部」「負債及び純資産の部」）欄の行に入れ、それより多ければ列のまま読む。幅の広い欄（決算公告の囲みが左右に並ぶ
  もの）は列の溝で左右に切ってから行に組むが、囲みの間に溝が無く行が揃っていると左右の囲みの行がつながる。横組みの
  ページの中の縦組みの欄（決算公告の囲みの中の合併公告・資本金の額の減少公告など、縦組みの列が 3 列以上・30 字以上
  並ぶ欄）は右の列から縦に読み、欄の左に同じ高さで並ぶ行（同じ囲みの決算公告）の後ろに置く
- 罫線の表を 2D 表として復元するのは新旧対照表の帯の中だけ（普通の本文の中の表は行のまま）
- 行を区切る横線の無い、縦の罫線だけで欄を区切った表（横倒しのページの財務諸表の損益計算書など）は、上下の端の揃う縦の
  罫線の間を欄とし、金額の欄（行の 8 割以上が金額だけの行）があるものだけ表に組む。科目の行は行間で分ける（行間を空けずに
  続く行は折り返しの続き）ので、どの行も行間を詰めて組んだ表では、金額の無い科目が前の科目の行につながる。外枠の線が取れず
  罫線のすぐ外に欄が残る表は表にしない
- ToUnicode の無い私用 CID は、原本と突き合わせて確かめた範囲だけ写像する。常用漢字と区別できない
  符号は文脈で決める（目次のページ番号の数字は、ほかのページ番号と下端がそろうか 2 桁以上を詰めて
  組んだものだけ。項目が 1 つだけの目次の 1 桁の番号はそのまま）
