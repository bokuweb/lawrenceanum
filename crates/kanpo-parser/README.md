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
