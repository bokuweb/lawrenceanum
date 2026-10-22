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
node --test tools/kanpo-ocr/*.test.cjs
```

元認識器の情報は [Tesseract.js](https://github.com/naptha/tesseract.js/tree/v6.0.1)、モデルは [公式tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast/tree/87416418657359cb625c412a48b6e1d6d41c29bd)。モデルはApache-2.0で、使用・再配布時のライセンスは同リポジトリのLICENSEを参照する。

## 画像領域の座標をPDFページへ写す

画像XObjectの一部領域を認識したときは、画像全体をページ幅・高さへ引き伸ばすと位置が合わない。配置行列とCropBox・回転が別途分かる場合、既存の認識結果を再推論せずページ座標へ写せる。

```sh
node tools/kanpo-ocr/project-cli.cjs \
  --placement placement.json --output new-projection.json observations.json
```

`placement.json` は次の形式で、行列はPDFと同じ `[a,b,c,d,e,f]`、点の変換は `(a*x+c*y+e, b*x+d*y+f)`。

```json
{
  "pixel_width": 100,
  "pixel_height": 200,
  "image_width": 100,
  "image_height": 200,
  "recognition_to_image": [1, 0, 0, 1, 0, 0],
  "image_to_user": [50, 0, 0, 100, 10, 20],
  "crop_box": [0, 0, 300, 400],
  "rotate": 0
}
```

- `pixel_width/height`: 認識に使ったPNGの画素寸法。`observations.canvas`との一致を検査する。
- `image_width/height`: 元の画像XObjectの画素寸法。`recognition_to_image`は、認識PNGの左上原点の画素辺座標を、元画像の左上原点の画素辺座標へ写す。認識前の回転・切り出し・縮尺をここに明示する。自動推定しない。
- `image_to_user`: 画像の単位正方形からPDFユーザー座標への配置行列。呼出元Formを含む有効なCTMを指定する。画素の下向きYは画像単位座標の上向きYへ変換する。
- `crop_box/rotate`: 継承を解決したPDFのCropBoxと正規化済みの回転0/90/180/270。出力は表示の向きで左上原点、pt単位。

全block/paragraph/line/wordへ `projection` を付加し、斜め・反転・せん断でも四隅 `quad_points` と外接矩形 `bounds_points` を区別して保存する。元の `bbox`、text、confidence、出現順と添字は保持する。元の `bbox.points` は認識時に指定した旧座標系の記録であり、今回の変換結果は `projection` を参照する。退化・非有限・精度で潰れる枠は捨てずinvalidにする。元画像またはページ外の点も切り詰めず、inside/partial/touching/outsideを幾何的に記録する。

このコマンドは配置メタデータをPDFから抽出したり、PNGとPDFの関係を証明したりしない。結果は `unclassified_ocr_projection`、`source_binding: "declared_unverified"`、`visibility: "unverified"`。クリップ・soft mask・透明度・後からの重ね描きは、この座標計算だけでは評価できない。ページ内にあることを可視性や記事所属の根拠にはしない。任意の補足メタデータは宣言として保持する。

入力JSON2件は一度読み、その正確なbytesのSHA256と使用コードのSHA256を記録する。出力は新しいファイルだけに書き、元の認識結果を変更しない。認識誤りの補正・confidenceによる選別・native Glyph対応・記事割当は行わない。既定のパーサへの自動統合には、PDFとの対応・可視性・既存文字との重複・読み順を別途検証する必要がある。

## 原本PDFの画像描画からOCR・配置まで実行する

`pdf-cli.cjs`は、原本の読み取り専用コピーを作り、同じbytesに対するネイティブの画像観測と、pypdf/Pillowによる画像デコードを照合してから認識する。新しい出力ディレクトリを指定する。

```sh
cargo build --release --locked -p kanpo-pdf --example image_sources
python3 -m venv /tmp/kanpo-image-decoder
/tmp/kanpo-image-decoder/bin/python -m pip install -r tools/kanpo-ocr/requirements.txt
# Node依存と固定jpnモデルは上記の手順で用意する。
node tools/kanpo-ocr/pdf-cli.cjs \
  --extractor "$PWD/target/release/examples/image_sources" \
  --python /tmp/kanpo-image-decoder/bin/python \
  --model /tmp/kanpo-ocr-model/jpn.traineddata \
  --output /tmp/kanpo-pdf-ocr-result original.pdf
```

PDF全体のSHA256、実際に呼ばれた画像の参照・encoded stream SHA/サイズ、画素解釈に使う宣言辞書項目、デコード寸法を照合する。同じ画像が複数回描かれる場合も各描画を保持する。画像軸が表示の水平・垂直軸に正確に一致する場合は、反転・四半回転のみで向きを合わせ、画素を補間しない。せん断・斜めの画像は元の画素軸を保持し、配置行列で位置を写す。DPIは元の垂直サンプリングから整数へ丸めて一度だけ指定し、自動再試行しない。現APIが表現しない`UserUnit != 1`は失敗として記録する。

- `decoded/`: 原本PDFと抽出実行ファイルのコピー、ネイティブの`sources.json`、デコード済みPNG・配置・出所receipt。receiptには元辞書/clip/gapと使用コード・デコーダ版・各ハッシュを残す。
- `pageN-drawN-ocr/`: 既存CLIと同じ生認識結果・変更しない文字・認識経過。各非白画像を固定jpn/PSM3で一度認識する。不透明な全画素が厳密に白い画像はその事実を記録して認識を省く。
- `pageN-drawN-projected.json`: 認識器が使ったPNGのSHA/サイズ・canvasがデコードしたPNGと一致することを検査した配置結果。元の文字・confidence・枠・順序を保持する。
- `index.json` / `terminal.json`: 全描画と未取得の情報、成功/部分失敗を保持する。描画ごとのデコードや認識に失敗した場合も空の本文へ置換しない。部分失敗は終了コード2、全体の失敗は1。

`producer_hashes_checked`は保存された生産物の照合状態で、最終表示の証明ではない。元の座標変換結果の`source_binding`は引き続き`declared_unverified`、可視性は`unverified`。画像のclip・mask・透明度・重ね描きとページ合成は再現しない。原本の画像全体を認識するため、切り取られた部分やnative文字と重なる文字も入り得る。gapがなくても全本文の取得を保証せず、既存パーサへ自動統合しない。文字修復・confidence選別・native Glyph対応・記事所属も付与しない。

デコードだけを調べる場合:

```sh
/tmp/kanpo-image-decoder/bin/python tools/kanpo-ocr/decode_sources.py \
  --extractor "$PWD/target/release/examples/image_sources" \
  --output /tmp/kanpo-decoded-images original.pdf
```

固定依存はpypdf6.10.0・Pillow12.3.0・cryptography50.0.2。デコードは[pypdfの画像抽出API](https://pypdf.readthedocs.io/en/6.10.0/user/extract-images.html)を使い、encoded stream照合の内部属性は当該版へ固定する。モデル不要のNode回帰に加え、実ネイティブ観測・暗号化原本・画像フィルタ・反転/回転・取得失敗を検査するPython回帰を実行する。CIでも実行し、依存/実行ファイルが無い場合にskipしない。

```sh
node --test tools/kanpo-ocr/*.test.cjs
KANPO_IMAGE_SOURCES="$PWD/target/release/examples/image_sources" \
  /tmp/kanpo-image-decoder/bin/python tools/kanpo-ocr/decode_sources_test.py
```
