# kanpo-pdf

[デジタル官報](https://www.kanpo.go.jp/)の PDF から、文字の座標と罫線を取り出す pure Rust の PDF リーダ。
kanpo-parser が poppler（`pdftotext -bbox-layout` / `pdftocairo -svg`）の代わりに使う。

- C のライブラリにも外部コマンドにも頼らないので、`wasm32-unknown-unknown` でもそのまま動く
- poppler（GPL）のコードは使っていない。poppler と同じ形の出力（`bbox` モジュール）は、poppler の出力を
  外から観察して合わせたもの

## できること

- PDF の読み込み: xref 表・xref ストリーム・オブジェクトストリーム、壊れた xref の作り直し
- 暗号化: 標準セキュリティハンドラ（AES-256 の V5/R5・R6、AES-128、RC4）を空パスワードで復号する。
  官報の PDF は AES-256（R6）で暗号化されている
- フィルタ: Flate（PNG / TIFF 予測子）・LZW・ASCIIHex・ASCII85・RunLength（画像のフィルタは扱わない）
- フォント: Type0（CID）の Identity-H/V と埋め込みの CMap、字幅（`W`/`DW`）と縦書きの字送り
  （`W2`/`DW2`）、ToUnicode、Adobe-Japan1 の CID → Unicode。単純フォント（Type1・TrueType・Type3）の字幅と
  基本的な対応
- コンテンツストリーム: テキストの演算子一式、`cm`・`q`/`Q`、パス、フォーム XObject、インライン画像の読み飛ばし
- 出力（座標はページ座標: /Rotate を適用した表示の向きで、左上原点・y 下向き・pt）
  - [`Page::glyphs`]: 1 文字ずつの Unicode・原点・字送り・大きさ・縦書きか・字面の枠
  - [`Page::paths`]: 描かれたパス
  - [`page_rules`]: 罫線（縦線・横線）
  - [`bbox::bbox_layout`]: `pdftotext -bbox-layout` と同じ形の XHTML（kanpo-parser の入力）

```rust,no_run
let pdf = std::fs::read("20261005h018010002.pdf")?;
for page in kanpo_pdf::extract_pages(&pdf)? {
    for g in &page.glyphs {
        println!("{} ({:.1}, {:.1}) {}pt", g.text, g.x, g.y, g.size);
    }
    let rules = kanpo_pdf::page_rules(&page);
    println!("{} 本の縦線, {} 本の横線", rules.vertical.len(), rules.horizontal.len());
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

## poppler との違い

`bbox` は kanpo-parser が poppler の出力に合わせて作られてきたので、語の枠と切れ目を poppler に寄せている。

- 縦書きの語の枠は、poppler と同じく字の原点（上端の中央）を左上とする正方形をつないだもの。実際の字面より
  右へ半 em、上へ 1 em ずれる（kanpo-parser はこのずれを前提に補正している）。[`Glyph::bbox`] は実際の字面
- 語の切れ目は字間の空きで決める（横書き 0.21 em、縦書き 0.35 em まで同じ語）。poppler は同じ字間でも
  つないだり切ったりするので完全には一致しない
- poppler が読めずに捨てるフォント（対応表の無い CID の文字集合で ToUnicode も無いもの。官報では傍線などの
  字形の `Adobe-Npb1`）の字は、poppler と同じく語に入れない（[`Page::glyphs`] には入る）
- Adobe-Japan1 の CID → Unicode は Adobe の公開データから作っている。poppler の表と違い、ハイフンは
  U+002D、下線は U+005F になる（poppler は U+2011・U+0332）

- 同じ字がほぼ同じ位置に重ねて描かれていたら 2 つ目を捨てる（poppler と同じ）。横書きで私用領域の字を
  前の字と同じ位置に重ねたもの（私用 CID の 2 字で 1 字を作る組み方）は、その字だけで 1 語にする

kanpo-parser のスナップショットテストのページ（313 ページ）と、それ以外の官報 6 日分（1004 ページ）で
poppler と比べた結果:

| | スナップショットのページ | 6 日分 |
|---|---|---|
| 罫線（pdftocairo と） | 全ページ一致（0.1 pt 未満の丸め差を除く） | — |
| 字（語の切れ目を無視して数える） | ハイフン・下線の符号の違い（上記）以外は 1 字差 | 同左＋poppler が捨てる縦書き用の「：」 |
| 語（文字列・枠まで一致） | 96.4% | 97.1% |
| パーサの本文が poppler と同じページ | 279 / 313 | 884 / 1004 |
| 読めない・panic | 0 | 0 |

本文の違いの多くは語の切れ目と、パーサが語の入力順に依存していることによる。調べるには
`cargo run --release -p kanpo-parser --example poppler_diff -- <PDF のディレクトリ>`（poppler が要る）。

## データとライセンス

`data/aj1-unicode.bin`（Adobe-Japan1 の CID → Unicode 表）は `examples/gen_aj1.rs` で次のデータから作る。

- Adobe [cmap-resources](https://github.com/adobe-type-tools/cmap-resources) の
  `Adobe-Japan1-7/cid2code.txt` — BSD-3-Clause（`data/LICENSE-Adobe-cmap-resources.md`）
- Unicode の [IVD](https://www.unicode.org/ivd/) `IVD_Sequences.txt` と `UnicodeData.txt` — Unicode License v3
  （`data/LICENSE-Unicode.txt`）

```bash
cargo run -p kanpo-pdf --example gen_aj1 -- cid2code.txt IVD_Sequences.txt UnicodeData.txt crates/kanpo-pdf/data/aj1-unicode.bin
```

依存 crate はいずれも MIT / Apache-2.0 系（`aes`・`cbc`・`md-5`・`sha2`・`miniz_oxide`・`serde`）。

## 調べるための example

```bash
cargo run -p kanpo-pdf --example fonts -- page.pdf      # フォントの一覧
cargo run -p kanpo-pdf --example glyphs -- page.pdf     # 1 文字ずつの座標
cargo run -p kanpo-pdf --example content -- page.pdf    # 復号・展開したコンテンツストリーム
# poppler の出力（あれば）との突き合わせ
cargo run -p kanpo-pdf --example compare_bbox -- page.pdf page.bbox.html -v
cargo run -p kanpo-pdf --example compare_rules -- page.pdf page.rules.json -v
# 隣り合う字の空きと poppler の語の切れ目の集計（<pdf>.poppler.html を読む）
cargo run --release -p kanpo-pdf --example gap_stats -- <PDF のディレクトリ>
```

[`Page::glyphs`]: src/page.rs
[`Page::paths`]: src/page.rs
[`Glyph::bbox`]: src/page.rs
[`page_rules`]: src/rules.rs
[`bbox::bbox_layout`]: src/bbox.rs
