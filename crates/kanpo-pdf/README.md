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
  つないだり切ったりするので完全には一致しない。poppler と違い、次の 2 つは字の組み方で決める
  - 字間（Tc）だけで空けた字（字間を空けて組んだ見出し「振　替　法」、決算公告の欄の名前「利　益　剰　余　金」
    など）は、横書きでも 0.55 em まで同じ語にする
  - 字と字の間に罫線（字送りに直交する線）があれば、空きが小さくても語を切る（表のセルの境目）
- poppler が読めずに捨てるフォント（対応表の無い CID の文字集合で ToUnicode も無いもの。官報では傍線などの
  字形の `Adobe-Npb1`）の字は、poppler と同じく語に入れない（[`Page::glyphs`] には入る）
- ToUnicode の無い `Identity` の CID フォント（官報では項目記号の (1)・①・㈠、目次のページ番号の数字、
  人名・地名の異体字などを組む `DFHSMinchoRPro6N-W3-Identity-V`）の字は、`bbox` では私用領域の
  U+F0000 + CID にする（`bbox::IDENTITY_CID_BASE`）。poppler は CID をそのまま符号位置にして無関係な
  漢字（`掠`・`振`）を出すので、本物の漢字と区別がつかない。kanpo-parser が字形の番号から字を決める。
  [`Page::glyphs`] の `text` は poppler と同じ
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
| 語（文字列・枠まで一致） | 92.4% | 86.2% |
| パーサの本文が poppler と同じページ | 266 / 313 | 826 / 1004 |
| 読めない・panic | 0 | 0 |

語の一致が低いのは、字間（Tc）を空けた語（「利　益　剰　余　金」など）や罫線をまたぐ字の扱いを、poppler に
合わせるより正しい語にする方へ寄せているため（poppler はこれらを 1 字ずつに割ったり、表のセルをまたいで
つないだりする）。本文の違いはこの語の切れ目と、パーサが語の入力順に依存していることによる。調べるには
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

## 画像XObjectの出所を調べる opt-in API

`extract_pages_with_xobject_sources` は、文字抽出と同じ入力スナップショットから、通常の元ページ・既存の元フォント証拠・観測した画像描画を一つの所有されたbundleに保存する。既定の文字抽出には画像認識を追加しない。

- `page()` / `native_sources()`: 読み取り専用の元ページと既存のAJ1フォント証拠。複製したページの変更はbundleに影響しない。
- `document_sha256()` / `media_box()` / `crop_box()`: 入力PDFのbytesのSHA256と、継承を解決したページ枠。
- `images()`: 描画ごとの添字、資源名の元bytes、間接参照、Form呼出経路、画素寸法、画像単位座標からPDFユーザー座標への有効な行列。同じ画像を別の場所で使った場合も別の描画として残す。
- `ImageDraw::encoded_bytes()` / `encoded_sha256()` / `declared_dictionary()`: 復号済みでフィルタを保持した画像ストリームと元の辞書。SHAはストリームbytesのもので、辞書や色解釈を代用しない。間接辞書項目は宣言のまま残し、デコード済み画素とは扱わない。
- `clips`: 明示したW/W*の規則と元パス、およびFormのBBoxを有効な行列・ページ回転で写した四隅。曲線の制御点が得られない場合や、変換値が表現できない場合は不完全と記録する。`page_clip_bounds`は従来実装の外接矩形であり、正確なclip形状の代わりにはしない。
- `capture_gaps()`: 未取得のインライン画像、見つからない資源、Formの再帰・従来の深さ制限・展開失敗、不正な行列や枠・寸法などを記録する。取得できない画像を「画像なし」に読み替えない。

FormのMatrixとBBoxの扱い、任意項目のnullを未指定とみなす扱いは、[Adobe PDF Referenceの4.9節・3.2.8節](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.5_v6.pdf)に基づく。文言・会社名・頁ID・行数・文字数で配置を決めない。

```bash
cargo run --release -p kanpo-pdf --example image_sources -- original.pdf > sources.json
```

このexampleは配置・clip・資源参照・元ストリームのSHA256・宣言辞書をJSONで出力する。名前と文字列の元bytesはhexで保持し、非有限の数値は元f64のbitsとして残す。画像のデコードやOCRは実行しない。

`native_paths`には同じ元Pageのパス観測を追加している。従来の左上原点のページ座標・stroke/fill・線幅・clipの外接矩形・元頂点・曲線/閉路の印を保持する。曲線の制御点や完全な合成/可視性・Formのclip所属を証明するものではない。従来の画像/元ページAPIや文字抽出の出力は変更しない。

観測対象はページと展開できたForm内の名前付きImage XObjectであり、パターン・透明グループ等を含む全描画機構や本文全体の網羅性は証明しない。空のgap一覧や`source_geometry_complete`は、最終表示や全画素の正しさを保証しない。後者は記録した変換・clip表現について既知の問題がないことを示す。`visibility_verified()`は常にfalseで、透明度・mask・後からの重ね描き・OCRの正解率・native Glyph対応・記事所属を推定しない。通常の本文へ統合するには、実際の画素デコード、表示範囲、既存文字との重複、認識結果と読み順の検証が別途必要である。
