# 官報画像の opt-in OCR

文字層に本文が無いページを調べるための、独立した実験用CLI。表示の向きに描画済みの全ページPNGをTesseract.jsで認識し、生の結果と座標を保存する。`kanpo-parser`の通常実行からは呼ばれない。

Node.js 20以降を使う。実行時にネットワークからモデルを取得しない。依存と日本語モデルを先に明示的に用意する。

```sh
npm ci --prefix tools/kanpo-ocr --ignore-scripts --no-audit --no-fund
mkdir -p /tmp/kanpo-ocr-model
curl -fL https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/87416418657359cb625c412a48b6e1d6d41c29bd/jpn.traineddata \
  -o /tmp/kanpo-ocr-model/jpn.traineddata

# 例: 表示寸法842×595pt、288DPIで描画した3368×2380pxのPNG。
node tools/kanpo-ocr/cli.cjs \
  --model /tmp/kanpo-ocr-model/jpn.traineddata \
  --output /tmp/kanpo-ocr-result \
  --width-points 842 --height-points 595 --dpi 288 page.png
```

出力先は新しいディレクトリを指定する。既存の結果を上書きしない。入力画像・モデルを一度読み、画像はそのbytes、モデルはそのbytesの読み取り専用の一時コピーを認識器へ渡す。モデルは上記commitのSHA256 `1f5de9236d2e85f5fdf4b3c500f2d4926f8d9449f28f5394472d9e8d83b91b4d`を検査する。Tesseract.js 6.0.1、jpn、LSTM、PSM3、指定DPI、回転補正なしで1回だけ認識し、設定を変える自動再試行やconfidenceによる選別を行わない。

- `inputs.json`: PNG・モデルのSHA256、実行コード・lockfile・エンジン版、明示した画素/pt座標系と設定。
- `raw.json`: 認識器の返した結果全体。
- `recognized.txt`: `data.text`そのまま。誤認識・空白・改行を置換しない。
- `observations.json`: 全block/paragraph/line/wordの階層・元添字・text・confidence・画素枠とpt枠。画素枠は左上原点。pt枠はX/Y別々の倍率で写す。不正/退化枠やpt精度で潰れる枠も捨てず、`geometry: "invalid"`と元値を残す。非有限値等は`exceptional_numbers`にも記録する。
- `events.ndjson` / `terminal.json`: 認識経過・完了または失敗。失敗時も作成済みの証拠を残す。worker終了処理の失敗は別に記録する。JSONは完全に書けたファイルだけを公開し、書込失敗時の一時ファイルを元エラーの`partial_output`として残す。空き容量等で失敗記録も書けない場合は、元エラーを保持してstderrに記録する。

JSONでは表現できない数値は`{"$js_number":"NaN"}`（`Infinity`・`-Infinity`・`-0`も同様）と明示する。これは診断値の保存形式であり、認識文字の補正ではない。

PNGを用意する描画器は、原本のフォント・回転・clip・重ね描きを正しく再現する必要がある。PDF画像XObjectの存在だけでは最終的な可視性は証明できない。現在のモデル設定は日本語の横組み本文を含む固定対照で検証しており、縦組みや表の精度を保証しない。

この出力は`unclassified_ocr`で、記事所属・native Glyph対応を付与しない。既存の座標付きPDF入力へOCRの語をそのまま足すと、PDF用のCID/座標補正や異なる語の粒度によって文字・読み順・セル所属を壊すため、本体への自動統合は保留している。既存の原本解析とOCR結果は別に保持して比較する。

モデルを使わない回帰テスト:

```sh
node --test tools/kanpo-ocr/sidecar.test.cjs
```

元認識器の情報は [Tesseract.js](https://github.com/naptha/tesseract.js/tree/v6.0.1)、モデルは [公式tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast/tree/87416418657359cb625c412a48b6e1d6d41c29bd)。モデルはApache-2.0で、使用・再配布時のライセンスは同リポジトリのLICENSEを参照する。
