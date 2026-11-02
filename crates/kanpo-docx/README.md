# kanpo-docx

官報の PDF を DOCX（Word）にする。PDF の字はそのまま読み、ページの画像（スキャンした本文）は OCR（[ndlocr-lite-rs](https://github.com/bokuweb/ndlocr-lite-rs)）の結果から読む。ライブラリと CLI（`kanpo-docx`）として使え、I/O を持たないので WASM でも動く（[`kanpo-docx-wasm`](../kanpo-docx-wasm)、`/kanpo-docx` のページ）。

## 仕組み

1. `page_images(pdf)`: ページに置かれた画像を取り出し、8 bit グレースケールに復号する（FlateDecode の 8 bit・1 bit、CCITTFaxDecode G4、DCTDecode。原本 3000 冊分の画像の形式を調べた）。ページの表示の向き（`/Rotate`）に起こして、字が正立した画像にする。どのページにもある、ほぼ白い背景の画像（黒い画素が 0.05% 未満）は除く。
2. 呼出元が画像を OCR する（ndlocr-lite-rs。CLI はネイティブ、ブラウザは WASM）。認識領域の枠は画像の画素。`ocr_page` が、表示したページ全体の画素の枠（`kanpo_parser::OcrPage`）に写す。
3. `convert(pdf, ocr)`: PDF の字と OCR の結果を合わせて解析し（`kanpo_parser::analyze_item_with_ocr`。ネイティブの字と重なる OCR 領域は除く）、記事ごとに段落・本文の中の表（結合セル付き）・新旧対照表（2 列の表）を DOCX に書く（[docx-rs](https://github.com/bokuweb/docx-rs)）。記事の最初の行は太字にする。

## ライブラリ

```rust
// OCR の結果を渡す（OCR しないなら空のスライス）。
let docx: Vec<u8> = kanpo_docx::convert(&pdf, &ocr_pages)?;

// OCR の手段を渡して、画像の認識から通して行う。
struct MyOcr;
impl kanpo_docx::OcrEngine for MyOcr {
    fn recognize(&mut self, image: &kanpo_docx::PageImage) -> anyhow::Result<Vec<kanpo_docx::OcrRegion>> { /* … */ }
}
let docx = kanpo_docx::convert_with(&pdf, &mut MyOcr)?;
```

## CLI

```bash
cargo run --release -p kanpo-docx -- input.pdf -o output.docx \
  --ndlocr /path/to/ndlocr-lite-rs --models /path/to/ndlocr-lite-rs/models
```

- 既定で、画像を ndlocr-lite-rs（`--features onnx` でビルドしたもの）の `recognize-page --trace-json` で認識する（`--structure-rules false`。官報の census で使ったのと同じ設定）。`--ndlocr`・`--models` は環境変数 `NDLOCR_BIN`・`NDLOCR_MODELS` でも渡せる。
- `--no-ocr`: PDF の字だけで書く。
- `--ocr-dir DIR`: 認識し直さずに trace JSON を読む（1 ページの PDF は `DIR/{PDF の名前}.json`、複数ページは `DIR/{PDF の名前}-{ページ}.json`）。
- `--dump-images DIR`: 取り出した画像を PNG で書き出す（確認用）。

本紙第1749号 13 ページ（破産の公告、`/Rotate 270` の画像だけのページ）は、ネイティブの ndlocr-lite-rs で約 6 秒。ページの柱の字（PDF の字）は画像に含まれないので、ページ全体を描いて認識した trace にあった柱の誤読（「彗星」）は出ない。

## WASM

`kanpo-docx-wasm` の `pageImages(pdf)` がページの画像（`{page, width, height, left, top, scale, pageWidth, pageHeight, pixels}`）を返し、`convert(pdf, tracesJson)` が同じ並びの trace（OCR しない画像は `null`）から DOCX を返す。`figma/scripts/build-kanpo-wasm.sh kanpo-docx-wasm kanpo-docx` で `figma/src/wasm/kanpo-docx/` に作り、`/kanpo-docx` のページが読み込む。OCR はブラウザで動く ndlocr-lite-rs の WASM 版（`figma/src/wasm/ndlocr/`）で、モデル（約 150MB）は `VITE_NDLOCR_MODEL_BASE`（既定は同 origin の `ndlocr-models/`）から読む。
