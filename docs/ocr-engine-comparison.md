# OCR エンジン選択と比較

2026-10-23 の追加指示により、現在のOCR既定は NDLOCR **0.0.15**。固定ビルド・receiptの手順は [OCR README](../tools/kanpo-ocr/README.md) を参照。[0.0.15 のローカル検証記録](../tools/kanpo-ocr/audits/ndlocr-0.0.15-20261023.json)には5画像×3回の原文・出所・設定を保持している。既存の開発crop2件は355文字中1文字の誤りだが、独立評価ではない。

以下の数値と比較は **0.0.15 への切替前** のビルドに対する履歴であり、そのまま新バージョンの精度を表さない。

当時の既定の実験用 OCR は Tesseract.js 6.0.1 / 固定 tessdata_fast jpn。
bokuweb/ndlocr-lite-rs の release + onnx CLI は `--engine ndlocr` で選択する。
`--model` は NDLOCR では models ディレクトリ、Tesseract では jpn.traineddata。
`--ndlocr-bin`（または NDLOCR_BIN / PATH）で実行ファイルを指定する。
エンジン失敗時に別エンジンへ切り替えない。

**閾値調整後の結果:** 同じ官報2例で、NDLOCRの検出閾値を0.25→0.9へ変えると
文字誤り率が56.06%→**0.28%（1/355文字）**に改善した。
行confidenceは既定の0.3。Tesseractは4.51%（16/355文字）。
初期設定だけの比較では判断できず、この2例では検出閾値の影響が大きかった。
ただし、これは同じ開発データで選んだ閾値で、独立評価ではない。
**同じ閾値0.9の全ページ入力では本文が大きく欠落するため、一律採用しない。**
当時は既定をTesseractとし、NDLOCRの閾値を比較用に明示指定していた。
以下のコマンドは評価用で、全ページへの推奨設定ではない。

```bash
node tools/kanpo-ocr/cli.cjs --engine ndlocr \
  --ndlocr-bin /path/ndlocr-lite-rs/target/release/ndlocr-lite-rs \
  --model /path/ndlocr-lite-rs/models \
  --det-conf-threshold 0.9 --min-line-confidence 0.3 \
  --output /tmp/kanpo-ndlocr-tuned \
  --width-points 595 --height-points 842 --dpi 300 page.png
```

NDLOCR の recognize-page は現状 TXT のみを公開するため、座標を捏造しない。
`layout_available: false`、`blocks: []` を記録し、PDF 投影を作らない。
原文、stdout/stderr、実行条件、実行ファイルとモデルの SHA256 を保存する。
入力画像とモデルは private snapshot を使う。structure-rules
を無効化し、辞書を渡さない。行confidence閾値は上流既定の0.3とする。NDLOCR 内部の汎用的な認識・読み順処理は残る。

比較は同一画像、同一マシン、逐次実行で行う。既存の官報の正解転記 ROI を
両者共通の入力 crop にして CER（Levenshtein / 正解文字数）を測る。
NFKC + 空白除去は評価時だけ行い、生の出力を保持する。過去の出力と異なり、
この試験は ROI の境界内 bbox 選別ではなく crop 全体のテキスト比較である。
既存転記は非盲検の開発用データで、官報全体の精度を保証しない。
全ページの速度も別途測る。プロセス起動・モデル読込・出力保存を含む時間を
報告し、初回と反復を区別する。ビルド・モデル取得は計測外。

クレジット: [ndlocr-lite-rs](https://github.com/bokuweb/ndlocr-lite-rs)、
[NDLOCR-Lite](https://github.com/ndl-lab/ndlocr-lite)（CC BY 4.0）。
本リポジトリでは外部 CLI を呼び出し、元実装は変更しない。

## 2026-10-23 の初期設定の精度実測

Apple M6 / RAM 32 GiB / macOS arm64、NDLOCR commit
`0ba1c0007c12c91f2527d40a9033ca7815d18795`（作業ツリー変更なし）を
`cargo build --release --features onnx --locked` でビルドした。
CPU 推論。NDLOCR は DEIM + PARSeq 30/50/100 cascade、行confidence 0.3、
quality-boost は既定の無効、structure-rules は無効、外部辞書なし。
Tesseract はこのリポジトリの固定 jpn fast / LSTM / PSM3。
いずれもモデル選択の自動やり直しなし。

| 同一入力の官報告知 crop | 正解文字数 | Tesseract edits / CER | NDLOCR edits / CER |
|---|---:|---:|---:|
| 20261007g002210006 の最初の告知 | 164 | 1 / 0.61% | 75 / 45.73% |
| 20260723h017530015 の番号付き告知 | 191 | 15 / 7.85% | 124 / 64.92% |
| 合計 | 355 | 16 / **4.51%** | 199 / **56.06%** |

各3回で生テキストは一致した。これはテキスト出力全体の誤り率であり、
文字認識器だけの誤り率ではない。NDLOCR は、例えば
`債務者上野優磨` の後に `上野 優磨` / `上野` / `債務者` / `優磨`
を重ねて出力する。複数行にわたって同様の重複が見られ、挿入誤りが多い。
個別文字の誤認識や余分な数字も残る。重複の発生段階が検出・領域選択・
context recovery のどこかは、このTXT比較だけでは断定しない。
先に試した行confidence 0 の出力と、この2 crop の結果は同じだった。
閾値を既定値へ戻すだけではこの症状は改善しなかった。

今回の官報2例の初期設定では Tesseract が優勢だったが、検出閾値調整後は逆転した。
NDLOCR を官報本文の正規データへ採用するには、
独立した正解セットでの再評価が必要。この2例から一般文書・縦書き・表の
優劣は結論しない。通常の `kanpo-parser` の処理は変更していない。

## 初期設定の処理時間とメモリ

同じマシンで逐次実行し、エンジンの実行順を交互に変え、画像ごとに各3回測定。
毎回新プロセスで起動し、画像・モデルのsnapshot、モデルロード、出力保存を含む。
OSキャッシュは消去していない。ビルド、PDFの画像抽出は含まない。
下表は各画像の3回の中央値。全ページ3画像はすべて3509×2480px。

| 入力 | Tesseract 秒 | NDLOCR 秒 | NDLOCR / Tesseract |
|---|---:|---:|---:|
| 最初の告知 crop 750×520px | 0.502 | 3.384 | 6.74倍 |
| 番号付き告知 crop 750×582px | 0.572 | 3.400 | 5.94倍 |
| 20261007g002210006 全画像 | 6.231 | 5.883 | 0.94倍 |
| 20260723h017530015 全画像 | 7.382 | 5.997 | 0.81倍 |
| 20260724h017540022 全画像 | 6.170 | 5.972 | 0.97倍 |

全画像ではNDLOCRが約3〜19%速い。一方、小さい画像を1枚ずつ処理する場合は
モデルの初期化負担が大きい。常駐モデルのスループットは未計測。
全画像の `/usr/bin/time -l` 最大RSSはTesseract約222〜224 MiB、
NDLOCR約659〜687 MiB。これはOSの最大RSS測定値で、同時に動く
全プロセスのメモリを合算した値ではない。

初回と2〜3回目の時間、各試行、モデル/binary/sourceのハッシュ、生の予測文は
[機械可読レポート](../tools/kanpo-ocr/audits/engine-comparison-20261023.json)を参照。
今回の最終計測は `.cache/ocr-comparison-20261023/run3/`。
先行の `run` / `run2` は接続確認・confidence 0 の調査用で最終統計に含めない。

## 再実行

`benchmark.py` は `samples` 配列を持つJSON manifestを受け取る。
各項目に `id`, `image`, `sha256`, `width_points`, `height_points`, `dpi`、
精度も測る場合は `truth` を指定する。画像は必ず同じbytesを両者に渡す。
今回のmanifestは上記レポートの `manifest`、元PDF2例のROIと転記は
`tools/kanpo-ocr/fixtures/issue227-{first,numbered}-notice63.json` に保存されている。
全ページ3枚目のPDFは既存ローカルcacheの原本。各画像のハッシュを検査してから走らせる。

```bash
python3 tools/kanpo-ocr/benchmark.py \
  --manifest .cache/ocr-comparison-20261023/manifest.json \
  --ndlocr-bin /path/ndlocr-lite-rs/target/release/ndlocr-lite-rs \
  --ndlocr-models /path/ndlocr-lite-rs/models \
  --tesseract-model /path/jpn.traineddata \
  --output /tmp/ocr-comparison-new --repeats 3
```

確認: Node回帰テスト101件成功。原本PDFから画像抽出→NDLOCR→TXT保存も
実行し、同じ画像の直接入力とテキストが一致すること、座標なしを明記して
PDF投影を作らないことを確認した。

## 閾値だけの追加比較

モデル、画像、画像の分割方法、辞書、読み順処理を変えず、検出閾値5種類と
行confidence閾値3種類の15組を同じ告知2 crop に適用した（計30実行）。
表は合計355正解文字に対するCER。上流実装は検出がなくなると別の行抽出へ
fallbackするため、閾値の変更は採用行だけでなく内部経路も変え得る。
追加の読取専用プローブで、検出0.9/0.95では5画像すべてDEIMの行候補が0件になり、
fallback条件へ入ることを確認した。詳細は末尾の原因調査を参照。

| 検出閾値 | 行0.3 | 行0.7 | 行0.9 |
|---|---:|---:|---:|
| 0.25 | 56.06% | 56.06% | 51.27% |
| 0.50 | 15.77% | 15.77% | 15.77% |
| 0.75 | 61.41% | 61.41% | 61.41% |
| 0.90 | **0.28%** | **0.28%** | **0.28%** |
| 0.95 | **0.28%** | **0.28%** | **0.28%** |

0.9 / 0.3では最初の告知0/164 edits、番号付き1/191 edits（葛→蔦）。
各3回再実行して一致。cropの時間中央値は3.390秒 / 3.486秒。

ただし、全ページ3画像へ同じ0.9 / 0.3を適用すると、原文出力は次のように激減した。
この文字数は改行・空白を含む生テキストの長さで、精度やrecallの指標ではない。
原本上にある多数の告知が出力されておらず、全ページの改善とは判定しない。

| 全ページ入力 | 検出0.25の出力文字数 | 検出0.9の出力文字数 | 0.9の時間中央値 |
|---|---:|---:|---:|
| 20261007g002210006 | 3,127 | 129 | 5.558秒 |
| 20260723h017530015 | 3,027 | 109 | 4.323秒 |
| 20260724h017540022 | 2,760 | 372 | 7.034秒 |

3回ずつの全ページ出力も一致した。出力を大きく落として得られた時間を、
同じ本文量を処理した性能改善として扱わない。従って、既定はTesseractを維持し、
NDLOCRの検出閾値0.9を全ページへ自動適用しない。画像の粒度ごとに独立した
正解セットを用意するまでは、調整後のcrop精度を全体精度として使わない。

[閾値の全試行と再計測レポート](../tools/kanpo-ocr/audits/threshold-comparison-20261023.json)。
再実行は次のとおり（出力先は新規ディレクトリ）。

```bash
python3 tools/kanpo-ocr/threshold-sweep.py \
  --manifest .cache/ocr-comparison-20261023/manifest.json \
  --ndlocr-bin /path/ndlocr-lite-rs/target/release/ndlocr-lite-rs \
  --models /path/ndlocr-lite-rs/models \
  --output /tmp/ocr-threshold-sweep-new
```

既定TesseractのPDF CLIも原本から再実行し、Tesseract使用・成功・PDF投影保存を確認した。


## 欠落の原因調査（同日追加）

既存releaseライブラリへ読取専用プローブをリンクし、モデル・実装を変えずに
DEIMの行候補と、CLIで使う補助行抽出のbboxを取得した。
[生のトレースとプローブソース](../tools/kanpo-ocr/audits/detection-trace-20261023.json)。

| 入力 | 検出0.25のDEIM行候補 | 検出0.9のDEIM行候補 | 補助行抽出の領域数 |
|---|---:|---:|---:|
| 最初の告知crop | 30 | 0 | 11 |
| 番号付き告知crop | 47 | 0 | 12 |
| 20261007g002210006全画像 | 316 | 0 | 11 |
| 20260723h017530015全画像 | 210 | 0 | 9 |
| 20260724h017540022全画像 | 344 | 0 | 17 |

`src/app.rs` はDEIM行候補が0件の場合に補助のインク密度による行抽出へ切り替える。
0.9で良くなった2 crop は、実際にはDEIMが良くなったのではなく、単一段の
補助行抽出が良く働いたものだった。

全画像は4段組み。補助処理は画像幅全体でY方向のインクを数えてから帯を切り出し、
各帯へX方向の外接範囲を1つだけ付けるため、段の異なる行が同じ領域に入る。
例えば20261007の領域 `[273,409,3209,1717]` は幅2936px・高さ1308pxで、
多数の行・複数段をまとめて含む。認識前の行抽出がこの時点で崩れている。
単一段cropの同じ処理では、高さ28〜32px程度の通常の行に分かれる。

画像が100万画素以上ならfast版、未満ならnaive版を選ぶが、両者とも
幅全体の行方向投影から帯を作る構造である。fast/naiveの違いだけを原因とは断定しない。
また、bbox記録は後段のフィルタ・文脈整理前なので、最終129文字まで減る各段階の
寄与を分けるには、実CLIの段階ごとのtraceを追加する必要がある。

改善では、単に閾値を上げるのではなく、DEIMの重複候補整理、段をまたがない
補助行抽出、候補が本文をカバーしているかの検証を分けて評価する。
既存2 crop を調整用とし、別ページの固定正解でTesseractとの優劣を判定する。
