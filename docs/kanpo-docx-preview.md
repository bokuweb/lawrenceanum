# 官報 PDF / DOCX 比較ページ

隠しURL `#/kanpo-docx-preview` を直接開く。通常のナビゲーションには追加しない。

入力は、既存の官報アーカイブ（日付・号・ページをSelectで指定）、収録済みの官報サンプル、
またはローカルPDF。選択後は左側のPDF.jsで原本を確認でき、変換ボタンで生成した
DOCXを右側のJADE Coreエディタのプレビューとして表示する。両側に独立したページ送りと
表示倍率を用意する。DOCXのページ数と原本ページ数は一致する前提にしない。
DOCXをダウンロードでき、全処理はブラウザ内で行う。

JADEは `jade-rs/jade-core` の固定コミットのRust/WASM `OffscreenDocumentView` を
Reactコンポーネントでホストする。本文・表・ページレイアウトはJADE自身が描画し、
HTMLへの簡易変換やPDF画像の流用でDOCXプレビューを代用しない。
描画は専用Worker、PDFはPDF.js Worker、画像OCRは既存のNDLOCR Workerを使う。
日本語フォントは同梱してWorkerに登録する。JADEとフォントのライセンスも配布する。

OCRの初回準備で自動再読み込みする間は入力操作を待たせ、選択ファイルの消失を防ぐ。
入力変更時は前の取得を中止し、前のDOCXとエラーを消す。遅れて終わった取得・OCR・
描画結果を新しい入力に適用しない。PDF.jsにはバッファのコピーを渡し、変換入力をdetach
させない。PDFエラー、変換エラー、JADE読み込みエラーをそれぞれ表示する。
Worker・ImageBitmap・Object URLは入力変更とアンマウントで解放する。
画像PDFはOCRが必要で、OCRを無効にして文字だけで変換する場合は明示的に選ぶ。

検証: Selectから既存官報を変換し、PDF / JADE双方のcanvasに描画されること、
生成DOCX本文・表、持ち込みPDF、入力の差し替え、無効PDF、ページ送り・倍率、
サブパスでのWASM/Worker読込と通常メニューから隠されていることを実ブラウザで確認する。


## ビルドと検証

```sh
pnpm --dir figma install --frozen-lockfile
pnpm --dir figma build:wasm
pnpm --dir figma build:jade
pnpm --dir figma build:ocr  # 画像PDFのOCRを使う場合
pnpm --dir figma build
```

JADE Coreは `a0647cc6542cc14a922a5d67c9997a588efb77b9` に固定する。
ビルドスクリプトはRust 1.93.1・対応するwasm-bindgenを用意し、WASMのC依存も
LLVMのarchiverで構築する。生成物はGitに含めず、Pagesの配信ビルドと官報E2Eで生成する。
OCR / JADEとも、固定コミットの取得前に疎なチェックアウトのパターンを設定し、
空のリポジトリでも配信環境のGitで取得できるようにする。
初回はJADEのコンパイルに数分かかる。JADEのWASMとフォントは比較ページで遅延ロードする。

```sh
pnpm --dir figma exec playwright test --config playwright.kanpo.config.ts
NDLOCR_MODEL_DIR=/path/to/ndlocr/models pnpm --dir figma test:kanpo-docx-ocr
```

後者は本番ビルドをサブパスで配信し、画像官報のOCR本文、PDF.js / JADE双方の実描画、
原本1ページからDOCX複数ページになる場合の独立したページ送りを確認する。
モデルは既存の `build:ocr` / 官報OCRテストの手順で用意する。
