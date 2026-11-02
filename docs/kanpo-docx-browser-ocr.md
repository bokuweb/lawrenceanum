# 官報 PDF → DOCX のブラウザ OCR

`pnpm --dir figma build:wasm` で PDF/DOCX 用 WASM、
`pnpm --dir figma build:ocr` で ndlocr のブラウザパッケージを作る。
OCR のソースは `figma/scripts/build-ndlocr-wasm.sh` のコミットに固定し、
大きなベンチマークアーカイブを除く sparse checkout で取得する。
開発中は `NDLOCR_SOURCE=/absolute/path/to/ndlocr-lite-rs` で手元のソースを使える。

生成先は `figma/src/wasm/ndlocr/`。JS、2つの Worker、Rust WASM、
onnxruntime-web 1.24.3 の対応する MJS/WASM、型定義、ライセンスを含む。
実行時に CDN は使わない。Vite の `base: './'` でサブパスへ配信できる。
`update-law-data.yml` は UI ビルド前に OCR パッケージも作る。

`VITE_NDLOCR_MODEL_BASE` は次の5ファイルを置くディレクトリのURL。
未設定または空文字なら、同じサイトの `ndlocr-models/` を使う。
別オリジンから配る場合は、サイトからの CORS を許可する。

- `deim-s-1024x1024.onnx`
- `parseq-ndl-24x256-30-tiny-189epoch-tegaki3-r8data-202604.onnx`
- `parseq-ndl-24x384-50-tiny-300epoch-tegaki3-r8data-202604.onnx`
- `parseq-ndl-24x768-100-tiny-153epoch-tegaki3-r8data-202604.onnx`
- `NDLmoji.yaml`

既存の `publish-ndlocr-models.yml` で R2 に配信する。
OCR はサイトの service worker が設定する COOP/COEP と SharedArrayBuffer を使う。
OCR パッケージ自体は service worker を追加しない。

## 検証

2026-11-02、Apple M6 / 32 GiB、Chromium 147、ORT Web 2 threads。
本番 Vite ビルドを `/sub/` 配下で配信し、画像だけの官報 PDF（本紙1749号13ページ）
を選択して DOCX をダウンロードした。画像1枚の OCR と DOCX 作成で35.0秒。
ZIP 内 `word/document.xml` に2,864文字の本文が入り、破産管財人などの語と
先頭の「株式会社楽一産業」を確認した。原本に対する精度の保証ではない。

NDLOCR 側の別の官報1ページ（3509×2480）は185行・3,027文字。
最終の連続2回で24.49秒 / 21.28秒、本文はネイティブ CLI と一致した。
localhost の測定であり、R2 の初回モデル転送時間を表さない。

同じ検証を CI でも行う。固定ソースからモデルをハッシュ検証して取得し、
本番バンドルを `/ocr-test/` 配下で配信する独立サーバで PDF → OCR → DOCX を実行する。

```sh
pnpm --dir figma build:wasm
NDLOCR_FETCH_MODELS=1 pnpm --dir figma build:ocr
pnpm --dir figma build
pnpm --dir figma test:kanpo-docx-ocr
```

手元にモデルがある場合、最終コマンドには `NDLOCR_MODEL_DIR=/absolute/path/to/models` を渡せる。
関連: ndlocr-lite-rs PR #26（ライブラリAPI）、#28（バックエンド）、#29（ブラウザ版）。

取得したOCRソースは独立したCargoワークスペースとして扱う。親リポジトリの
`target/` が通常ディレクトリのCIでも、親のworkspace.membersには追加しない。
