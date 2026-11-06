# 官報画像の opt-in OCR

文字層に本文が無いページを調べるための、独立した opt-in CLI。OCRツールの既定は [ndlocr-lite-rs 0.0.15](https://github.com/bokuweb/ndlocr-lite-rs/releases/tag/0.0.15)（Rust + ONNX）。`kanpo-parser` の通常実行からは呼ばれない。Tesseract.js は `--engine tesseract` で明示選択できる。

## NDLOCR 0.0.15 の固定ビルド

上流のタグ `0.0.15` はコミット `f97e930b8c3e9512f7788f258ef685da26416483` に対応する。Cargo の package version はこのタグでも `0.1.0` のため、`--version` ではリリースを識別できない。固定コミットのクリーンな checkout を用意し、次のコマンドで `--release --features onnx --locked` ビルドと receipt の作成を行う。ビルド前後のコミット・変更有無を検査し、成功したバイナリの SHA256 と Cargo.lock の SHA256 を記録する。モデルは上流の手順で別途用意する。

```sh
git clone --branch 0.0.15 https://github.com/bokuweb/ndlocr-lite-rs.git /path/ndlocr-lite-rs
node tools/kanpo-ocr/ndlocr-build.cjs --source /path/ndlocr-lite-rs
node tools/kanpo-ocr/cli.cjs \
  --ndlocr-bin /path/ndlocr-lite-rs/target/release/ndlocr-lite-rs \
  --model /path/ndlocr-lite-rs/models \
  --output /tmp/kanpo-ndlocr-result \
  --width-points 595 --height-points 842 --dpi 300 page.png
```

receipt の既定位置は実行ファイルの隣の `<binary>.kanpo-build.json`。別の場所に保存する場合はビルド時の `--receipt NEW_FILE` と認識時の `--ndlocr-receipt FILE`（または `NDLOCR_RECEIPT`）を指定する。既存 receipt は上書きしない。これはローカルビルドの再現記録で、配布元の署名ではない。

`NDLOCR_BIN` または PATH でも実行ファイルを指定できる。認識前に receipt のリリース・コミット・ONNX/release設定と実行ファイルの SHA256 を照合する。不明なビルド・別バージョン・変更されたバイナリは認識しない。検査したバイナリとモデルの bytes を読み取り専用の一時コピーにして実行する。認識時のダウンロードや Tesseract への自動再試行は行わない。

`--model` は models ディレクトリ。既定では TXT を `layout_available: false` として保存する。PDF CLIも同じ既定・receipt指定に対応し、座標を要求しない場合は `projection` を作らず `geometry_status: unavailable_from_engine` を記録する。原文、入力・モデル・binary・receiptのハッシュ、stdout/stderrを保存する。

structure-rulesを無効にし、辞書を渡さない。検出・行confidenceは上流既定の0.25・0.3を使う。`--det-conf-threshold` / `--min-line-confidence` による明示変更は可能だが、過去の開発cropで良かった検出0.9をページ全体の既定には採用しない。

[0.0.15 の固定入力検証記録](audits/ndlocr-0.0.15-20261023.json): 既存の5画像を各3回実行し、生テキストが全試行で一致。転記済みの開発用crop2件は355文字中1文字の誤り（評価時だけNFKC・空白除去）。残る全ページ画像3件には全ページ正解がなく、精度・網羅性を保証する結果ではない。PDF CLIの既定選択・receipt照合・座標なしの結果保存も実機で確認した。

### NDLOCRの認識領域を明示的に保存する

PNG CLI・PDF CLIへ `--ndlocr-regions` を追加すると、同じ1回の認識から生の `trace.json` と `recognition_regions` を保存する。領域には元の順番・text・confidence・縦組みかどうか・画素矩形を保持する。内部改行も変更しない。block/paragraph/wordの階層や文字ごとの枠は生成しない。

traceのschema、canvas、未補正の実行設定・閾値、有限のconfidenceと画素矩形を検査する。trace本文・領域textを改行で連結した本文・生TXTがすべて完全一致する場合だけ `observations.json` を出す。不一致や不正な矩形では失敗し、作成済みの生TXT・trace・実行ログを保存する。設定変更による再認識は行わない。

PDF CLIでは、原本・デコードPNG・配置receiptを従来どおり照合し、各認識領域へ `projection` を追加する。領域は認識器の粒度で、単語・字・段落の境界や本文の網羅性を表すものではない。可視性・記事所属・native Glyph対応も未検証のまま保持する。

[認識領域の検証記録](audits/ndlocr-0.0.15-regions-20261023.json): Node117件・Python45件成功。実機5画像は従来の生TXTと完全一致し、PDF CLIの185領域の元属性・座標投影も検査した。原本の重ね表示は選択した5領域の非盲検開発確認で、全領域の可視性や本文の精度を保証しない。

[旧ビルドとの比較仕様・実測結果](../../docs/ocr-engine-comparison.md)は当時のバイナリに対する結果であり、0.0.15 の測定値ではない。以下は明示選択する Tesseract の手順。

Node.js 20以降を使う。実行時にネットワークからモデルを取得しない。依存と日本語モデルを先に明示的に用意する。

```sh
npm ci --prefix tools/kanpo-ocr --ignore-scripts --no-audit --no-fund
mkdir -p /tmp/kanpo-ocr-model
curl -fL https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/87416418657359cb625c412a48b6e1d6d41c29bd/jpn.traineddata \
  -o /tmp/kanpo-ocr-model/jpn.traineddata

# 例: 表示寸法842×595pt、288DPIで描画した3368×2380pxのPNG。
node tools/kanpo-ocr/cli.cjs --engine tesseract \
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

Tesseractの全block/paragraph/line/wordと、存在するNDLOCRの各認識領域へ `projection` を付加し、斜め・反転・せん断でも四隅 `quad_points` と外接矩形 `bounds_points` を区別して保存する。元の `bbox`、text、confidence、出現順と添字は保持する。元の `bbox.points` は認識時に指定した旧座標系の記録であり、今回の変換結果は `projection` を参照する。退化・非有限・精度で潰れる枠は捨てずinvalidにする。元画像またはページ外の点も切り詰めず、inside/partial/touching/outsideを幾何的に記録する。

このコマンドは配置メタデータをPDFから抽出したり、PNGとPDFの関係を証明したりしない。結果は `unclassified_ocr_projection`、`source_binding: "declared_unverified"`、`visibility: "unverified"`。クリップ・soft mask・透明度・後からの重ね描きは、この座標計算だけでは評価できない。ページ内にあることを可視性や記事所属の根拠にはしない。任意の補足メタデータは宣言として保持する。

入力JSON2件は一度読み、その正確なbytesのSHA256と使用コードのSHA256を記録する。出力は新しいファイルだけに書き、元の認識結果を変更しない。認識誤りの補正・confidenceによる選別・native Glyph対応・記事割当は行わない。既定のパーサへの自動統合には、PDFとの対応・可視性・既存文字との重複・読み順を別途検証する必要がある。

## 原本PDFの画像描画からOCR・配置まで実行する

`pdf-cli.cjs`は、原本の読み取り専用コピーを作り、同じbytesに対するネイティブの画像観測と、pypdf/Pillowによる画像デコードを照合してから認識する。新しい出力ディレクトリを指定する。

```sh
cargo build --release --locked -p kanpo-pdf --example image_sources
python3 -m venv /tmp/kanpo-image-decoder
/tmp/kanpo-image-decoder/bin/python -m pip install -r tools/kanpo-ocr/requirements.txt
# このTesseract例ではNode依存と固定jpnモデルを上記の手順で用意する。NDLOCRでは --engine を省略し、binary/receiptとmodelsディレクトリを指定する。
node tools/kanpo-ocr/pdf-cli.cjs --engine tesseract \
  --extractor "$PWD/target/release/examples/image_sources" \
  --python /tmp/kanpo-image-decoder/bin/python \
  --model /tmp/kanpo-ocr-model/jpn.traineddata \
  --output /tmp/kanpo-pdf-ocr-result original.pdf
```

PDF全体のSHA256、実際に呼ばれた画像の参照・encoded stream SHA/サイズ、画素解釈に使う宣言辞書項目、デコード寸法を照合する。同じ画像が複数回描かれる場合も各描画を保持する。画像軸が表示の水平・垂直軸に正確に一致する場合は、反転・四半回転のみで向きを合わせ、画素を補間しない。せん断・斜めの画像は元の画素軸を保持し、配置行列で位置を写す。DPIは元の垂直サンプリングから整数へ丸めて一度だけ指定し、自動再試行しない。現APIが表現しない`UserUnit != 1`は失敗として記録する。

- `decoded/`: 原本PDFと抽出実行ファイルのコピー、ネイティブの`sources.json`、デコード済みPNG・配置・出所receipt。receiptには元辞書/clip/gapと使用コード・デコーダ版・各ハッシュを残す。
- `pageN-drawN-ocr/`: 既存CLIと同じ生認識結果・変更しない文字・認識経過。既定では各非白画像をNDLOCR 0.0.15で一度認識する。明示したTesseractでは固定jpn/PSM3を使う。不透明な全画素が厳密に白い画像はその事実を記録して認識を省く。
- `pageN-drawN-projected.json`: 認識器が使ったPNGのSHA/サイズ・canvasがデコードしたPNGと一致することを検査した配置結果。元の文字・confidence・枠・順序を保持する。
- `index.json` / `terminal.json`: 全描画と未取得の情報、成功/部分失敗を保持する。描画ごとのデコードや認識に失敗した場合も空の本文へ置換しない。部分失敗は終了コード2、全体の失敗は1。

`producer_hashes_checked`は保存された生産物の照合状態で、最終表示の証明ではない。元の座標変換結果の`source_binding`は引き続き`declared_unverified`、可視性は`unverified`。画像のclip・mask・透明度・重ね描きとページ合成は再現しない。原本の画像全体を認識するため、切り取られた部分やnative文字と重なる文字も入り得る。gapがなくても全本文の取得を保証せず、既存パーサへ自動統合しない。文字修復・confidence選別・native Glyph対応・記事所属も付与しない。

## 原画像の罫線と閉領域を観測する

```sh
python3 tools/kanpo-ocr/raster_grid.py source.png --output /tmp/new-raster-grid
python3 -m unittest discover -s tools/kanpo-ocr -p 'raster_grid_test.py'
```

`raster_grid.py`は認識前の元RGBA画素だけを使う。完全に不透明な黒画素の水平・垂直の最大連続区間を調べ、長さが画像短辺の1/20以上、長さと厚さの比が10以上の細い支持成分を候補にする。隣接する走査線で実際に黒画素が重なる区間だけを結び、候補ごとにすべての支持区間を残す。`--minimum-run`と`--minimum-aspect`で条件を明示でき、選択した値を記録する。文字列、法令名、認識結果、期待する行列数は使わない。

元の黒画素で交差が確認できる点と、支持が途切れない辺から平面グラフを作る。閉じた面のうち単純な長方形だけを`rectangular_cells`へ記録し、罫線帯の内側の画素を補間せず切り出す。元PNGのbytes・RGBAハッシュ、処理コード・Pillowの版、切り出し枠・PNGハッシュ・親画像への整数平行移動も保存する。破線や1画素の穴を閉じず、非長方形の閉領域は`unresolved`へ残す。原画像は読み取り専用コピーで保持し、認識用のノイズ除去や文字の補正はこの処理では行わない。

これは罫線の候補と閉領域の観測で、表という意味の分類ではない。ロゴの横線や囲みも候補になり得る。細かい罫線・破線・灰色や不透明でない線は対象外で、候補が0件でも表が無いとは判定しない。`unresolved`もすべての見落としの一覧ではない。本文・セル結合・読み順・OCRの正確性・可視性・全表の網羅性は未認定のまま残す。

元PDFとの対応と、回転・鏡映を含む切り出し後の配置receiptも必要な場合は、一括パーサの`--raster-grid`を使う。[検証CLIの説明](../kanpo-validation/README.md)を参照。通常の本文と画像の配置は保持し、元PDF・画像ストリーム・デコード画素との照合結果を別の監査情報にする。

### 元画素を残した認識用コピーと全領域のOCR

```sh
# 一括パーサ --raster-grid が保存した一つのassetを指定する。
python3 tools/kanpo-ocr/prepare_raster.py /tmp/grid-document/raster-grids/asset-0000 \
  --pdf /tmp/grid-document/source.pdf --profile median3-ruling-bands \
  --output /tmp/new-raster-prepared
node tools/kanpo-ocr/raster-cli.cjs --prepared /tmp/new-raster-prepared \
  --model /path/models --ndlocr-bin /path/ndlocr-lite-rs \
  --ndlocr-receipt /path/build.json --output /tmp/new-raster-ocr
```

`prepare_raster.py`は元PDF・PNG・RGBA・罫線観測・配置receiptのハッシュを検査し、元画素から罫線の観測を再計算する。任意の領域を消す宣言を、ハッシュの一致だけでは受け入れない。元PDF・PNG・receipt・観測と実行したPythonコードを保存する。`--profile identity`は画素を変更しない。明示選択する`median3-ruling-bands`は別の認識用コピーに3×3の中央値フィルターをかけ、元の細い罫線帯と長さの範囲を半径1画素だけ広げて白くする。元の画素は変更しない。この処理は細かい文字の一部を消すこともあり、認識の正確性を保証しない。

閉領域の候補だけでなく、その外側も区画に分ける。候補が0件でも元画像全体を対象にする。枠が入れ子なら、内側の候補と外枠の残りの領域を分ける。包含関係のない重なりや同じ枠の複数候補は`overlapping_ruling_faces`として全IDを残し、一つを選ばない。各元画素がちょうど一つの`part`と`core`へ入ることを検査する。これは画素区画の検証であり、表・セル結合・OCRの網羅性の認定ではない。

各区画は既定768画素のcoreと128画素の周囲を持つ小画像へ分け、認識用コピーの非白部分に余白1画素を付けて切り出す。現在の周囲は同じ区画内に限る。`preparation.json`に区画、全core、window、切り出し位置、PNG・RGBA・元PDFへの配置行列のハッシュを保存する。認識用コピーが空になった領域も`recognition_copy_empty_unverified`として保持し、原本に本文が無いとは判定しない。原本と認識用PNGには異なるハッシュを持たせる。

`raster-cli.cjs`は保存されたハッシュ・元画像の所属・区画の重複と穴・切り出し行列を再検査し、読んだ入力bytesを認識実行のディレクトリへ捕捉する。各非空コピーを固定したNDLOCRのpageプロファイルで一回認識する。複数行を含む候補も単一行と仮定しない。原文・confidence・trace・領域枠は変更せず、全領域を元PDFへ投影する。各認識領域には`source_core`で元画素座標、中心がcore内か周囲にあるか、core境界をまたぐかを残す。これは文字の正誤や読む順序を決める情報ではない。

`prepare_raster.py --record-ink-support`は認識用コピーの非白画素を8近傍の成分と全走査区間で記録する。`recognition-ink.json`を親PNG・原本RGBAへ結び付け、認識CLIでもハッシュ・画素範囲・成分の面積と外接枠・重複を検査する。各認識領域の`source_ink`に、枠内の実画素の数と外接枠、core内の画素数、その外接枠の中心による所属を追加する。元の認識枠と`source_core`は維持する。

認識枠の中心が空のcoreへ入り、実際のインクが周囲にしか無い場合を、この追加情報で区別できる。中空の字形では、インクの無い位置へ文字の中心が来ることもあるため、core内の画素数が0という理由だけで文字を消さない。ここで記録するのは前処理後の画素の支持であり、原本の空白、認識された文字の正しさ、文字単位の字面を認定するものではない。認識候補の自動削除や書き換えは行わない。

横倒しになった字形の向きを調べる場合は、macOS上で次を明示実行する。

```sh
python3 tools/kanpo-ocr/raster_orientation.py /tmp/raster-prepared \
  --output /tmp/new-source-orientations
# 元のpreparedと同じ原像・画素プロファイル・区画のオプションを使う。
python3 tools/kanpo-ocr/prepare_raster.py /tmp/grid-document/raster-grids/asset-0000 \
  --pdf /tmp/grid-document/source.pdf --profile median3-ruling-bands \
  --orientation-report /tmp/new-source-orientations/orientation-0000.json \
  --output /tmp/new-oriented-raster-prepared
```

観測CLIは入力・Swift/Python実装・コンパイルした実行ファイルを捕捉し、Vision revision 3、日本語、言語補正なしの設定で各認識用コピーを一度観測する。生の上位3候補と、取得できた各文字範囲の四隅を保存する。最上位候補の字形の右方向が、実際のコピー画素と対応する2領域以上で一様な場合にだけ、4方向の回転候補を記録する。文字列の内容やconfidenceで向きを選ばない。異なる向きが混在する場合は多数決せず未解決にする。縦書きの読む方向と、字形自体の向きは別に扱う。

`--orientation-report`では、原本と認識用コピーのハッシュを照合し、四隅から向きの観測を再計算する。元画像・認識用の親画像・区画・coreは維持し、モデルへ渡す小画像だけを90度単位で転置する。補間は行わない。`recognition_to_source`で回転後の画素を元の画像座標へ戻し、元の画像配置とも合成する。認識CLIは小画像の寸法・配置・元座標への変換を検査し、coreとインクの支持は回転前の元座標で記録する。これは認識器による向きの候補であり、文字の正確性や観測されなかった部分までの向きを保証しない。

実験的な`--profile median3-ruling-patterns`は、上記の認識用コピーへ追加の成分処理を行う。元の不透明な黒画素を走査し、実際の直交罫線との交点で挟まれた区間に、反復する短い線があるか調べる。先頭・末尾を除いて4本以上の支持があり、長さとピッチの変動が元の線幅以内に収まる場合だけを候補にする。観測した各黒区間・交点の元画素・線幅を記録し、文字列や正解の転記を判定に使わない。

一つのセル内の支持が少ない場合は、隣接する3セル以上で合計6本以上の完全な支持を観測し、長さとピッチが同じ線幅の許容範囲に収まる連続区間を候補にする。セルごとの位相のリセットは認めるが、説明できないセルを飛び越えない。長い区間に不規則な支持が混じる場合は、その両端の交点へ既に到達している候補が同じ長さ・間隔を裏付ける場合に限り、4本以上が連続する局所区間を記録する。合わない支持は`unmodeled_support`に残す。この推定から別の推定を再帰的に作らない。各候補の`pattern_kind`、セル・局所区間、両側の候補IDを記録する。

認識用コピーでは8近傍の連結成分を調べ、候補の帯内に収まり、観測した破線の長さを大きく超えない成分だけを白くする。帯の外へ接続する字形や縦に長い成分は保護する。孤立した1画素の成分もこの明示プロファイルでは除く。`ruling-patterns.json`に全候補、元画素の支持、消費した成分の全画素区間と理由を残す。差分はこの記録から再現でき、元画像は維持する。1画素の点も意図した印字である可能性があるため、原本との比較が必要である。

認識画像の寸法を前処理から独立させる実験には、`prepare_raster.py`へ`--retain-window`を付ける。これは非白部分への再切り出しを行わず、元の区画とcoreから決めたwindow全体をそのまま使う。元画素の縮小・拡大・余白の合成は行わない。`settings.recognition_crop: source_window`と切り出し行列を記録し、認識CLIでもwindowとの完全一致を検査する。未指定時は従来どおり非白部分へ切り出す。

`--periodic-partition`は区画分けにも点線の候補を使う実験オプションで、画素の前処理とは独立している。交点には元の黒画素を必要とし、候補の点線区間に沿う辺だけに`inferred_periodic_span`を記録する。局所的な反復しか裏付けられない`flanked_interval`はセル全体の境界に使わない。実線と点線の所属が重なる場合や、閉領域が単純な長方形でない場合は未解決の根拠を残す。元の罫線観測を維持し、追加のグラフ・セル候補・点線の支持を`partition-observation.json`へ保存する。この宣言と実装のハッシュを認識CLIでも検査し、元の実線候補と追加候補のIDを混同しない。区画とcoreは追加候補の内外を含めて元画素全体を覆う。

併せて`--complete-ruling-bands`を明示すると、既に観測した細い罫線帯の中で、長さの下限に満たない黒区間も調べる。長い元の支持を含み、隣の走査行と実際の黒画素を共有する4近傍の成分だけを取り込む。帯の外へ広げず、白い隙間も埋めない。元の罫線のID、帯、取り込んだ実画素の全区間を`source_band_completion`として残す。元の画像・認識用コピーの画素を変更するオプションではない。これにより長い区間だけを先に抽出した際の見かけの切れ目を区別できるが、表の意味構造の認定は行わない。

複数の帯が重なっていても、つながった成分が元の支持より太くなる場合は補完を採用しない。元の罫線と、採用できなかった支持を未解決の記録へ残す。斜めに接する画素だけの場合や、元からある白い切れ目も連続した罫線とは扱わない。

前処理の効果は項目ごとに異なる。元の字形が同じでも認識用の切り出し寸法によって結果が変わり、点線を文字と誤認することもある。これらのプロファイルを正確な本文復元の既定と認定せず、自動の切り替え・正解に近い候補の選別・文字の修復は行わない。独立に転記した原像と、文字・位置・重複を比較する必要がある。

`index.json`と各`tile-NNNNN-result.json`に処理状況を残す。一部の認識が失敗しても他の生データを保ち、再試行や不完全な本文の連結は行わない。`terminal.json`の`complete`は依頼した認識処理の完了で、原文の完全性ではない。罫線やチェック欄を文字として誤認識する問題、重複した領域、coreをまたぐ文字列、全本文の読む順序・編集可能な表への統合は未解決。出力は`unclassified_source_derived_image_ocr`、記事・native Glyphへの所属はnull、`coverage_verified`はfalseのまま保持する。

`raster_layout.py`は、この全観測から編集可能な文字・セル候補を作る。正解転記やRTFは読まない。

```sh
python3 tools/kanpo-ocr/raster_layout.py /tmp/source-derived-raster-ocr \
  --output /tmp/new-raster-layout.json
```

元PDF・PNG/RGBA・配置・認識用親画像を照合し、モデル入力PNGを逆転置すると元の切り出し画素へ完全一致すること、四隅が同じ元画像位置へ写ることを検査する。認識器のtraceから全領域の文字・confidence・枠・順序を再照合する。周囲の文脈だけにある認識や、画素の支持が無い観測も消さずに残す。

文字候補の所属は、認識枠内の実際のコピー画素の外接枠と、半開区間のcoreから決める。同じ位置の全画素区間が完全一致し、かつ生文字列も同じ場合だけ、複数観測を一つの候補の参照にまとめる。同じ文字列が別の位置にある場合はまとめない。支持画素が同じでも文字が食い違う場合は`text: null`と全候補を保持し、confidenceで選ばない。一部だけ重なる画素は未解決にし、部分文字列の一致から継ぎ合わせない。

セルは元の罫線と区画の幾何を使い、文字の支持画素全体を含む内側のセルが一意な場合だけ対応づける。セルの辺から行列と結合範囲を作り、穴・重なりを保存する。一つのセル内でも、共通の行の範囲と重ならない字送りを持つ横書き候補だけを順序づける。縦横混在や高い枠で複数行がつながる場合は順序未確定にする。区画の一覧順を本文の読む順序にはしない。

これは原画像を残した、位置付きの候補表現である。コピーの支持画素は文字の正確性を保証せず、セルの意味や書式、画像全体の読む順序も未認定。原画像部品へ追加し、独立デコーダで照合する手順は[検証ツール](../kanpo-validation/README.md)を参照。

一文字ごとの位置を観測したレポートがある場合は、同じ原画像・認識用コピーのレポートを明示する。

```sh
python3 tools/kanpo-ocr/raster_layout.py /tmp/source-derived-raster-ocr \
  --character-report /tmp/new-source-orientations/orientation-0000.json \
  --output /tmp/new-character-raster-layout.json
```

`character_layer`に、Visionが返した全候補の文字範囲・confidence・四隅とレポート全体を保持する。原画像・認識用コピーを照合し、元画素から向きの観測を再計算する。上位候補だけでなく、返された全候補についてUnicodeスカラーの範囲が文字列全体を復元することを確認する。結合文字や非BMP文字をUTF-16の位置と混同せず、文字列の正規化や候補の選別は行わない。

各文字の四隅から元画像の画素範囲を求め、実際のコピー画素の数・外接枠・全走査区間のハッシュを記録する。支持画素全体が入る一意の内側のセルへ対応づけ、NDLOCRの語の支持範囲へ入る文字も参照で結ぶ。一部だけ重なる語は別の参照にし、同じ位置という理由で文字列を置き換えない。モデルが位置を返さない文字、支持画素のない文字、競合するセルは未解決のまま残す。

連続した元文字範囲を、物理セル・字形の右方向・幾何の状態が変わる位置で分け、`fragments`へ残す。全範囲を順に連結すると元候補の文字列に完全一致する。空白も保持し、両側の支持が同じ場合だけ同じ範囲に含める。欄をまたぐ空白や先頭・末尾の空白は、位置を作らず別に保持する。これはモデルの候補内の順序であり、異なる候補や画像全体の読む順序の認定ではない。元のNDLOCRの語・生データは維持する。

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
  /tmp/kanpo-image-decoder/bin/python -m unittest discover -s tools/kanpo-ocr -p '*_test.py'
```

## 罫線と元画素で列を分けて認識する実験モード

横書きを列の左から右へ読む前提を明示できる場合は、`--partition native-gutters-ltr`を追加する。原本由来の画像を列で分けてから認識し、全体認識で列を横断する混線を減らすためのopt-inである。

```sh
node tools/kanpo-ocr/pdf-cli.cjs --engine tesseract \
  --extractor "$PWD/target/release/examples/image_sources" \
  --python /tmp/kanpo-image-decoder/bin/python \
  --model /tmp/kanpo-ocr-model/jpn.traineddata \
  --partition native-gutters-ltr \
  --output /tmp/kanpo-pdf-column-result original.pdf
```

現在の`image_sources` exampleが出す`native_paths`を使う。通常のPageのパス観測で、clipは従来の外接矩形であり、色・透明度・Formによるclip・最終表示を完全に証明するものではない。古いexampleにこの項目がない場合は明示的に失敗する。

分割規則はOCR結果を参照しない。RGBのいずれかが白と異なりalphaが正の画素について8近傍の連結成分を求め、すべての成分の縦の範囲に交わる直線の縦罫を候補にする。その罫の記録された幅が、画像全高にわたる白/透明な内側の隙間に入る場合だけ、隙間の整数の中央で全画像を切り分ける。列数・会社名・文言・文字数・行数・固定ptで分岐しない。罫線のない隙間だけでは分けず、罫が届かない見出し/脚注の画素、曲線や塗り/記録済みclipも対照にする。元画像にclipやForm呼出経路がある場合、不明な幾何、斜めの画像などは全体のまま保持する。

各分割画像を元の位置へ並べて全RGBA画素に完全一致することを検査する。画像の補間・画素の除去は行わない。元PNGと配置/receiptも`partitioned/`へ保持し、分割画像のreceiptには親PNG・親receipt・切り出し枠・規則/座標計算のハッシュを残す。

分割した全列には、[Tesseract公式の1列用モード](https://tesseract-ocr.github.io/tessdoc/ImproveQuality.html#page-segmentation-method)（PSM4、可変の文字サイズを持つ単一列）を一律に使う。分割しない全画像には従来のPSM3を使う。どちらも1回だけで、認識後の文言・confidence・正解との比較によって設定を切り替えたり再試行したりしない。

各描画の`partitions`に列ごとの生認識とPDFへの配置を残す。すべて成功した場合だけ`pageN-drawN-partitioned-text.txt`を作り、列順の生テキストを改行でつなぐ。元の認識文字・空白・confidence・枠・順序は各結果に保持する。1列でも失敗すれば、成功した列の結果と失敗を保持し、不完全な連結本文を成功扱いしない。

`source_path`は認識結果内の添字なので、文書SHA・page・draw・partitionと組にして参照する。分割の添字と元画素の範囲はreceiptにも記録する。

これは画像の分割と明示した読み方向の実験であり、記事所属・native文字との統合・最終可視性や全字の正しさは未確定である。固定原図の164文字対照では全体認識9件から固定範囲2件へ減ったが、その1件は正しく認識した「第」の枠が評価範囲を越える問題。別の191文字対照では15件から1件になり、「免→先」が残る。ほかにも「99→9」など、全体認識より悪化する文字がある。結果を文言で修復せず、既定パーサへの自動統合は保留する。詳細は第63次監査JSONを参照。


## native本文が一部欠ける混在ページの確認

[固定0.0.15による追加原本の試行](audits/issue227-ndlocr-hybrid-source100.json)では、nativeが読める解散公告・決算と、読めない株式・相続公告が同居するページを、閾値を変えず1回認識した。96の縦組み領域から本文を得たが、独立した公告の読み順が混ざり、「受遺者→受遣者」などの誤りも残った。以前「係る→伴う」と誤認識扱いした箇所は、原本を拡大して確認すると印字が「伴う」で、NDLOCRの読みが正しかった。[原本転記の訂正記録](audits/issue227-ndlocr-source-heading-correction105.json)に旧参照の誤りを記録し、元の固定参照とraw出力は保持した。同じ見出しが複数あるため、一箇所に文字列が現れることを見出しの再現率にしない。

座標の投影や元属性の保持は本文の正確さ・所属・最終可視性を保証しない。縦組みの流れを原本の幾何と画素で分ける検証が必要であり、横書き用native-gutters-ltrを縦組みへそのまま適用しない。この試行は通常パーサーの本文や記事へ自動統合していない。
