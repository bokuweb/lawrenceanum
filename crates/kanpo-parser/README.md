# kanpo-parser

[デジタル官報](https://www.kanpo.go.jp/)の項目別 PDF（縦書き・多段組）から本文の読み順を復元し、
改め文・新旧対照表を構造化する Rust crate。

- 縦書きの読み順復元（段組みの分割・右→左の列並べ・ページ柱の除去・縦書き約物の正規化）
- 新旧対照表の改正後/改正前の分離（同じページの下段へ欄見出し無しで続く表、継続ページも含む）
- 条見出し・号ごとの改正後/改正前の行対応、表中の別表（罫線表）の 2D 復元
- 1 ページに同居する複数記事（〇…告示第…号 など）の分割

## 前提

PDF の解析に [poppler](https://poppler.freedesktop.org/) の `pdftotext` / `pdftocairo` を子プロセスで使う。

```bash
brew install poppler
```

```bash
sudo apt-get install -y poppler-utils
```

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

ページ PDF を 1 本で扱うなら `extract_document(&pdf)` が構造化と別表の復元をまとめて行う。
複数ページにまたがる項目は、ページのテキストを連結して `Document::from_text` した後、
各ページの PDF で `attach_tables(&mut doc, &pdf)` を呼ぶ。

`pdftotext -bbox-layout` の XHTML が手元にあれば、poppler 無しで `reconstruct_vertical(&xhtml)` を使える。

## デバッグ

```bash
pdftotext -bbox-layout -enc UTF-8 input.pdf out.bbox.html
cargo run -p kanpo-parser --example dump -- out.bbox.html
cargo run -p kanpo-parser --example structured -- out.bbox.html
```

## 解析過程の可視化（`analyze`）

`analyze(&xhtml)` は本文・記事・構造化結果に加えて、各語がどの領域（前文・改正後・改正前・
表外・本文）のどの行に組まれたか、捨てた語はその理由（余白・欄見出し・柱）を返す。
poppler を使わないので WASM（`crates/kanpo-parser-wasm`）でも動き、Web の playground
（`/kanpo-playground`）がこれを使って PDF の上に割り当てを重ねて表示する。

```bash
cargo run -p kanpo-parser --example trace -- out.bbox.html
```

## 回帰テスト用コーパス

`tests/corpus/<case>/` に実官報のページ（`page.pdf`・`page.bbox.html`）と、何を確かめるケースか
（`case.json`）、目視で確認した期待出力（`expected.txt`・`expected.json`）を置いている。
パーサを直して出力が変わったら、差分を確かめてから更新する。

```bash
UPDATE_EXPECT=1 cargo test -p kanpo-parser --test corpus
```

官報原本は GitHub Actions（`collect-kanpo-pdf.yml`）が毎日 R2 の `kanpo-pdf/` に蓄積している
（デジタル官報の公開は約 90 日）。手元に取るには:

```bash
cargo run -p lawpub-cli -- kanpo-archive --date 2026-10-05 --out .cache/kanpo-archive
cargo run --release -p kanpo-parser --example scan -- .cache/kanpo-archive > scan.tsv
```

`scan` は全ページを解析して怪しいページ（欄見出しがあるのに表にならない、改正後/改正前の
行数が極端に偏る、私用 CID の化け候補など）に印を付ける。新しいエッジケースを見つけたら
コーパスに足す。

## 既知の限界

- 縦書きと横書きが混在するページ（号外の決算公告など）は、どちらか多い方の読み方で組む
- 表の中の罫線表（セルが格子になった表）はセル構造までは復元しない（新旧対照表の `別表第○` 行の
  2D 表は `attach_tables` が罫線から復元する）
- ToUnicode の無い私用 CID は、原本と突き合わせて確かめた範囲だけ写像する
  （目次のページ番号の数字など、常用漢字と区別できない符号はそのまま）
