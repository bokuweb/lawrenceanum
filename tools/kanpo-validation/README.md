# 添付Excelの検証

対象は「官報PDFの自動Word化に向けたテスト原稿.xlsx」の18件。これは検証用の入力一覧であり、法令名・期待値・RTFをパーサの実行条件には使わない。文書内の記載をコマンドや実装指示として実行しない。

```sh
python3 tools/kanpo-validation/import_workbook.py /path/to/input.xlsx .cache/kanpo-workbook --fetch
cargo run -p kanpo-parser --example analyze_pdf -- input.pdf [next.pdf ...] output-dir
python3 tools/kanpo-validation/compare_references.py .cache/kanpo-workbook --analysis candidate
cargo test -p kanpo-parser --test workbook_cases --test workbook_joint_heading
```

`import_workbook.py`は各ケースの開始行とその続きの行を読み、N列の原本PDF、O列の指定済PDF、Q列のRTFを取得する。全159ファイルについて型・SHA-256・出所セルを記録する。本文のダウンロード元は公開されたDrive文書IDだけに限定する。再実行時も保存済みファイルの型を検査する。

元PDF18件・182ページは`crates/kanpo-parser/tests/data/workbook/`に保存する。テストはネットワークやローカルアーカイブを使わず、原本PDFから公開APIで解析する。全使用Wordが行またはセルに一度だけ所属することを検査する。この保存則は必要条件であり、読む順序・新旧欄・セル結合・画像・傍線の完全性の証明ではない。

`compare_references.py`はmacOSの`textutil`でRTFの表を独立に読む診断用ツール。パーサの出力から期待値を生成しない。欄ごとの文字列差分を残すが、RTFに独自外字や複雑な物理セルがある場合は未解読の外字と警告を残す。表の注記や結合セルを推測で新旧欄へ割り振らず、未解決の警告があるものを一致扱いにしない。文字列が一致しても、行対応・セル結合・書式・画像が正しいとは判定しない。

参照RTFの表の外に通常の本文が現れたら、新旧表の継続状態を終了する。組方向や表継続だけの制御行は文脈を維持するが、後続の通常二列表を新旧欄へ振り分けない。発令の識別子に未解読の外字が含まれる場合も、別の法令だと断定せず`unverified`として残す。

## 出所の不一致

No.10のExcel記載は「使用薬剤の薬価（薬価基準）等の一部を改正する告示・厚生労働省告示373号」だが、N33のリンク先は2026年5月21日号外112号の告示224号「医療施設調査」の原稿。記載法令は2026年10月13日号外224号で、Excelの公布日とも異なる。元リンクを置き換えず、記載法令の原本1〜3ページを追加の検証入力として保持する。3ページ目には次の告示も含まれるため、記事の所属も検証対象となる。

両方を検証する。薬価のRTFを医療施設調査の出力と比較しない。`compare_references.py`の`--case`・`--analysis`で比較先を明示し、複数記事では`--article`で解析済み記事の添字を指定できる。比較報告には解析JSONのSHA-256と選択した記事の添字を残す。これは診断側の選択で、パーサが法令名やケース番号で分岐する機能ではない。RTFが追加表と新旧表を別の作りで記載している場合は、二列の新旧表だけの比較を原稿全体の比較と扱わない。

## 完了条件

今回の受入範囲は、ユーザー指定によりOCRを除外する。本文・新旧欄・条項・罫表・結合セル・割注・ルビ・数式の配置・傍線を確認し、画像内の文字や表は原画像として保持する。文字コードとの対応が無く字形認識を必要とする部分も、出所を残した未解読字形として扱う。

OCRを起動せず一括実行する場合は、画像OCRのレポートを付けず、未知字形の認識も明示的に無効にする。

```sh
python3 tools/kanpo-validation/parse_pdf.py original.pdf \
  --parser target/debug/examples/analyze_media_pdf \
  --native-extractor target/debug/examples/native_pages \
  --image-extractor target/debug/examples/image_sources \
  --native-glyph-engine none --output /tmp/new-non-ocr-document
```

添付18件とNo.10記載法令の追加3ページは、計185ページをこの設定で再解析した。本文のWord所属、表の重複・結合、文書への変換、ルビ・囲み・指数・二重傍線の文字位置、元の画像データと独立デコードを確認する。No.6の認識を必要とする2字は未解読として保持する。画像12点は本文内に配置し、追加原本のページ装飾4点も元データ・ページ・配置とともに保持する。

- 対象原本の全ページを確認し、本文・見出し・署名・附則の文字と読む順序が一致する。
- 新旧欄と条項の行対応、入れ子の表・空欄・結合セルが原本と一致する。
- 割注・ルビ・数式・縦横混在・画像・傍線の扱いを確認する。未対応箇所を文字抽出の成功へ含めない。
- 原本に基づく期待値、内容や配置を変えた対照例、欠損出典などの反例を検証する。
- 既存テストを維持し、差分を原本で確認する。一括の期待値更新で成功させない。

画像内の文字・表の編集可能化を含む認識全体は引き続き未完了で、今回のOCR除外の受入とは分けて扱う。

## 文字層で表現されない描画の監査

```sh
cargo run -p kanpo-pdf --example native_pages -- original.pdf > native-pages.json
python3 tools/kanpo-validation/audit_paint_gaps.py native-pages.json paint-gaps.json
python3 -m unittest discover -s tools/kanpo-validation -p '*_test.py'
```

`audit_paint_gaps.py`は、同じ書体・大きさ・送り方向のグリフの間に、文字の枠と重ならない描画があるか調べる。本文や期待値による探索は行わない。原本グリフとパスの添字、送りの隙間、描画の外接枠を残す。文字・記事への自動割当てはしない。直線の頂点や線幅から求めた外接枠は可視性の証明ではなく、罫線なども候補になり得る。

No.6では、文字コードではなく細かなパスとして描かれた括弧付き番号8か所を検出した。既存の使用Wordの保存則だけでは、この欠落は見つからない。No.13の細い罫線3か所も監査候補になった。候補が無いことを全描画の網羅性の証明にはしない。

元の画像XObjectについても別に監査した。主原本ではNo.10に1、No.15に2、No.16に8、No.18に1の配置がある。配置の捕捉と、画像内の文字・表・最終的な可視性の復元は別の確認事項として残す。

`native_pages`のschema 2は、読み込んだPDFのSHA-256・バイト数と、各ページの画像描画数・未捕捉画像の数も保存する。監査ツールは古い配列形式も読めるが、その場合は`source: null`として原本との対応を未確認のまま残す。曲線は元APIが制御点を保持しないため端点から字面の外接枠を推定せず、一般の線結合とともに`unbounded_paint_paths`へ記録する。

## 同じPDF内の字形を使う図形文字の認識

```sh
python3 -m pip install -r tools/kanpo-validation/requirements-source-font.txt
cargo build --locked -p kanpo-pdf --example native_pages
python3 tools/kanpo-validation/read_painted_compounds.py original.pdf \
  --extractor target/debug/examples/native_pages --output /tmp/new-compound-observations
python3 -m unittest discover -s tools/kanpo-validation -p '*_test.py'
```

この明示実行する認識器は、入力PDFを一度読み、その同じバイト列からネイティブ観測とPDFiumによる描画を取得する。schema・原本ハッシュ・サイズを照合し、映像の切り出し位置、画像ハッシュ、グリフ・パスの添字、実行コードとライブラリの版を保存する。元PDFの字形とUnicodeの対応がある孤立した漢字を手本に使い、未知のIdentity CID、不可視文字、他の文字やパスに重なる手本、画像のあるページを学習へ混ぜない。

両側の向かい合う括弧の形と、連結した描画を切らない一意の等間隔の段から字を分ける。全文を一字と読む候補も比較し、「二」「三」のような単独文字を無条件に複数字へ分解しない。連結成分の数と、対称なChamfer距離で全手本を比較する。法令名、対象番号の候補リスト、参照RTF、期待値ファイルは実行時に読まない。

No.6の原本8か所は括弧も含めて転記と一致し、No.13の細い罫線3か所は文字として認識しない。原本に基づく開発用転記は`tests/data/workbook/painted-compound-review.json`にあり、これは盲検評価ではない。合成した任意の段数・字形・ラベル、単独文字、罫線、未知の字体、誤った原本ハッシュなども検証する。

出力は`source_font_compound_observations`で、文字候補は`candidate_unverified`。距離は認識確率ではない。別のネイティブ文字220クラスを手本と分けた開発調査では3クラスを誤認識し、小さく潰した文字では誤りがさらに増えた。本文の一般的なOCRとしての正確性は認定しない。原本の使用Wordを保存する検証と、文字層にない描画の認識を区別して扱う。

## 認識結果を原本の行へ明示的に取り込む

```sh
cargo run --locked -p kanpo-parser --example analyze_painted_pdf -- \
  original.pdf /tmp/new-compound-observations/observations.json /tmp/new-painted-analysis
cargo test --locked -p kanpo-parser --test painted_text
```

公開APIは`kanpo_parser::painted_text::analyze_pdf(pdf, observation_report)`。通常の解析では実行しない。PDFのハッシュ・バイト数、原本グリフの送りから計算した空隙、そこにある全パスの添字、文字や画像との重複を照合する。手前のWordの末尾と次のWordの先頭が同一の元の行へ一度だけ隣接して所属し、行の文字列全体がWordの並びに一致するときだけ、そのUTF-8境界へ認識文字を挿入する。本文の文字列検索、期待値による位置選択、合成グリフの追加は行わない。

`native`は通常の解析そのもの、`observed`は認識文字を加えて記事とDocumentも再構成した結果。元のWord・raw・座標・所属は両方で保持する。`bindings`に元の行とWordの境界、`unresolved`に適用できなかった各観測、`observation_report`に認識器の元データを残す。原本の同じ境界へ複数の候補がある場合は、信頼度や入力順で一つを選ばず両方を未解決へ残す。

このAPIが検証するのは原本との対応と挿入位置。認識文字の正しさや描画全体の可視性は別であり、`observed`を検証済みの原文と扱わない。No.6の8か所は原本転記と一致し、原本Word・行の所属を変えずに本文とDocumentへ取り込めることを検査した。No.6以外の画像・書式・表の完全性は引き続き未認定。

## 原本の上付き・下付きの配置を残す

`media.baseline_shifts.bindings`は、前後の大きい文字の間にある小さい元Wordを最終文書へ対応づける。原本のグリフ、文字方向、字送り、字体サイズ、基準位置だけを使い、数式の文字列や指数の値から補わない。前後の文字が同じ元の行・セルと方向に属し、小さい字の位置が本文の字送りで埋まっておらず、全字形が前後の基準線より上または下にある場合だけを対象にする。ルビのように本文の字送りと重なる場合や、前後の親が一意でない場合は判定しない。

`source.shift`の`raised`・`lowered`は元の文字座標での配置を表す。`source.bounds`はページ上の物理範囲で、`baseline`・`parent_baselines`・`parent_bounds`は`writing_basis.side`と`flow`へ投影した座標。元の字体サイズ、前後のグリフ添字、原本SHAも保持する。最終文書の`document`・`field`・`offset`・`length`は非空白Unicodeスカラー位置で、本文は変更しない。欄全体の原本・最終文書の文字列が一致し、元Wordが一つの記事に収まる場合だけ対応づける。式全体の意味構造や、終端で前後の証明が揃わない指数まで認定する機能ではない。

傍線も、同じ配置証明から本文行の領域で元の塗り範囲を調べる。上付き文字自身の方向・配置も保持し、隣の文字に傍線があるという理由では付けない。原本に連続した傍線があるNo.9の二つの指数と、傍線のない改正前の指数を別々に検証する。現在の対象は`strict_bracketed_source_baseline_shifts`で、`coverage_verified`はfalse。

## 元のCFF字形から傍線を復元する

`underlines.line_projections`は、割注の結合や語順の整形がある行・セルの出所を記録する。既存レンダラーの実際の処理を観測し、元Wordの非空白スカラーを追跡する。後段で復元される文字も、原本入力から同じ処理を再実行して確認する。縦横混在のセルは元グリフの文字方向・基準位置・重ならない字送りから順序を決め、行またはセルの全体と欄全体を照合する。

各記録の`page`・`region`・`line`・任意の`cell`は元の解析上の位置。`field`・`offset`は観測を反映した全記事の同じ欄を連結した非空白Unicodeスカラー位置で、`text`と`tokens`を対応づける。`tokens.origin.kind: word`は元Wordとそのスカラー位置、`parallel_join`は割注の結合処理が作る区切り、`relocated_word`は元の印字を別の語へ付けた処理を表す。区切りの「・」にはグリフを作らず、負数記号「△」には実際の元Wordを対応づける。`target_word`は元の移動処理が選んだ語で、記号の本文位置は記録されたスカラー位置を使う。

No.12・13の割注、No.17の共同省令の題名・項番号・表の見出し・負数記号をこの対応で検証する。通常の`native`のWord・状態・本文・表は保持する。不明な文字、重複した出所、既に別の行・セルへ所属する記号、範囲の重なりや全体の不一致は認定しない。

```sh
cargo run --locked -p kanpo-parser --example analyze_underlined_pdf -- original.pdf /tmp/new-styled-analysis
cargo test --locked -p kanpo-parser --lib source_underlines
cargo test --locked -p kanpo-parser --test source_underlines
```

`kanpo_parser::source_underlines::analyze_pdf(pdf)`は、元のCFF字形が実際に塗る長方形から傍線を復元する明示実行API。フォント名やCID番号の表は使わない。元の文字変換行列の軸と字送りから、回転・鏡映を含む直交方向を同じ座標系へ変換する。進行方向だけから傍線の側を推測しない。元の長方形、最も近い文字列の位置、一つの行またはセルへの所属を使う。罫線への接続、別の方向の文字を含む重ね組み、不明な描画や画像との重なりを調べる。約物の字間詰めにより複数の字が同じ位置になる場合も、元の同じ行に所属するときだけ同じ傍線へ対応づける。

原本の文字と対応したRunの文字列を、散文・改正後・改正前の欄ごとに**全体で照合**する。全体が一致する欄だけRunを分割して`underline`を付ける。欄の一部分を本文検索して合わせない。文字列、表の形と結合、記事・行・セル・Wordの所属は変えない。`native`に通常の解析、`styled`に書式を付けた解析、`glyph_bindings`に元の文字と傍線のグリフ添字を返す。

書体が変わり文字の原点がずれていても、同じ原本Wordの別の文字が共通の基準位置を裏付け、字形の横断方向の範囲が重なる場合は、同じ行の傍線へ対応づける。セルが同じという理由だけで異なる基準位置を結ばない。フォント名・文字列・ずれの固定許容値は使わず、原本Wordの対応、範囲の重なり、行・セルの一致から判定する。

連続する傍線字形が一つのbbox Wordにまとめられる場合も、元のCFFが塗る矩形を直接参照する。本文として使用するWordのグリフを傍線候補へ転用せず、Wordへの分類だけを理由に図形を見落とさない。`Aj1PageSources::glyph_painted_rectangle`は元の描画情報だけを返し、それを傍線と認定する処理は行・セル・他の図形との関係から別に行う。従来のPage・Glyph・Sceneの出力形式は変更しない。

`Aj1PageSources::glyph_separate_painted_rectangles`は、一つの元グリフに複数の矩形輪郭があり、互いに正の隙間を持つとき、その個別の矩形を返す。穴・重なり・接触・未知の命令・一部だけのクリップは個別矩形の証明に使わない。縦組みの元の原点、回転・鏡映も保持する。`cargo run --locked -p kanpo-pdf --example painted_rectangles -- original.pdf`で、元PDFのSHAとグリフ添字付きの観測を取得できる。これだけで書式を決めず、使用中の本文グリフを図形へ転用しない。「三」などの文字に複数の矩形輪郭があっても、本文のまま保持する。

`glyph_bindings.decoration_components`は、元のグリフ・輪郭添字・物理座標を残す。一つのグリフが塗る二つの線を、一つのIDへまとめて失わない。文字位置に対応する平行な描画範囲を数え、同じ位置に重なる線分は一本として扱う。二本以上と認定する場合は、各線の進行方向の描画範囲も確認する。短い印、異なる方向の重ね組み、枠に接する線、他の描画で遮られる範囲を二重線として補わない。

元の解析に対応する`parallel_underlines`は、欄全体の非空白Unicodeスカラー位置と本数を表す。`source_underlines::field_text(analysis, field)`で同じ位置の文字列を取得できる。一括CLIと`media`の出力では`underlines.parallel_bindings`に、最終文書の`document`・`field`・`offset`・`length`・`parallel_strokes`・原本SHA・元Wordを返す。本数2は二重傍線で、従来の`Run.underline`も保持する。二重線の再現にはこの本数情報を使う。表も欄の本文順に数え、画像とルビは文字位置を消費しない。

最終文書への対応では、元の欄と最終文書の欄を全体で照合し直し、記事ごとの長さから位置を計算する。古い記事添字や本文検索で位置を合わせない。欄の末尾など離れた箇所の変更でも不一致なら適用せず、競合する範囲は双方を`unresolved_parallel`へ残す。原字から正規化したWordへの書式の対応が確定しない箇所は`unresolved_parallel_words`へ残す。No.4の「カ」「ヨ」、No.11の項番号、No.16の「二」は元PDFの二本の輪郭と文書位置を照合した回帰例であり、全原稿の書式を認定するものではない。

表のセルでは、原本のWord順と表示順が異なることがある。表の描画と同じ座標順でWordを並べ、元のIDの集合と個数、セルの文字列全体を照合してから書式を渡す。RTFや本文の検索から並びを決めない。

No.1・2・3・9・11・15・16は新旧両欄の文字列全体を対応づけて適用できる。No.1の独立RTF診断では一致文字の傍線も一致した。欄全体への文字の対応を証明することと、各傍線の正確性・網羅性を認定することは別である。元の文字・表・結合・記事・Wordを保存する対照も別に検証する。

横向きのWordが縦の本文列に埋め込まれている場合、同じ元の行・セルにある前後の字送りと、空いた位置から親の列を決める。文字の内容や周囲の傍線の有無では選ばない。一つの親列だけが物理的に対応するとき、その列の方向で元のCFF長方形との関係も調べる。Word自身の文字方向で得た傍線も保持する。長い傍線の対応先は、その位置を実際に占める文字で遮蔽を調べ、幅広い縦中横が列全体の対応を奪わないようにする。別の行、二つの親列、既に本文のある位置、欠けた前後の文字、元グリフの不完全な対応は採用しない。回転・鏡映の8方向と縮尺・平行移動、任意の字数でも同じ条件を検証する。

現在の対象は`scope: cardinal_cff_rectangle_decorations`。`coverage_verified`はfalseで、親列が一意に対応しない縦中横、斜めの文字変換、パスで描いた傍線、空白の書式を網羅したとはしない。項番号など、参照RTFとの書式差も未認定のまま残す。原本の対応が確定しない欄や文字が補われた欄は全体を変更せず、`unresolved_fields`・`source_issues`に残す。

## 画像を含む文書と一括実行

```sh
cargo build --locked -p kanpo-parser --example analyze_media_pdf
cargo build --locked -p kanpo-pdf --example native_pages --example image_sources
python3 -m pip install -r tools/kanpo-validation/requirements-source-font.txt -r tools/kanpo-ocr/requirements.txt
python3 tools/kanpo-validation/parse_pdf.py original.pdf \
  --parser target/debug/examples/analyze_media_pdf \
  --native-extractor target/debug/examples/native_pages \
  --image-extractor target/debug/examples/image_sources --output /tmp/new-kanpo-document
cargo test --locked -p kanpo-parser --test media
```

`document.json`の`documents`は、文字・傍線・表に加えて`kind: image`を持つ文書。画像は`assets`の原本ストリーム、宣言辞書、配置行列、クリップ、原本ハッシュへ対応する。通常の解析は`native`に残す。全ての元の文字・表の形・結合を保持することを検証する。Rust APIは`kanpo_parser::media::analyze_pdf`、図形文字の観測も与える場合は`analyze_pdf_with_observations`。

挿入先は法令名や画像の内容から選ばない。欄全体の原本・Documentの文字列が一致し、元の行との幾何的な前後関係が一意な境界へ入れる。新旧欄は元のf64の罫線で閉じた枠と元の行の役割を確認する。表の中では、一つの空のセルに画像全体が入り、Documentの表の形・全文・結合が一意に対応する場合だけ入れる。同じ境界やセルの複数画像は原本の頁と位置から順序を決め、描画番号順で選ばない。

主原本ではNo.10の1、No.15の2、No.16の8、No.18の1、計12画像を配置できる。No.16の最後の2様式は新旧それぞれの欄へ、No.18の貨幣画像は元の空の結合セルへ入る。ストリームの元データと寸法は、pypdf/Pillowの別の読み取りとも照合し、`media-source-review.json`に検証入力を残す。

一括実行はPDFと実行ファイルを捕捉し、図形文字の観測、Rustの文書構成、別の画像デコーダによるPNG・配置receiptを照合する。PNGは`images/`、原本は`source.pdf`、生の解析は`parse/`、図形文字の観測は`paint/`に保存する。`decoded_images`で各assetとPNGを対応づける。PNGは画像自体の画素で、原本のクリップ・重ね描き・最終可視性を焼き付けた合成ではない。

上記の実行は原画像を保持する。画像内の編集可能な文字・セル候補を追加する場合は、下記の観測レポートを明示する。参照RTFで外部の小組として呼ばれている様式も、原本の画像として保つ。重なる画像・文字、複数記事への所属、一意に対応しない表や境界は`unresolved`に残す。書式・画像・図形文字の各監査が残るため、一括実行の終了を原稿全体の完全性の認定にはしない。

### OCRの文字・セル候補を元画像部品へ追加する

```sh
python3 tools/kanpo-ocr/raster_layout.py /tmp/source-derived-raster-ocr \
  --output /tmp/new-raster-layout.json
python3 tools/kanpo-validation/parse_pdf.py original.pdf \
  --parser target/debug/examples/analyze_media_pdf \
  --native-extractor target/debug/examples/native_pages \
  --image-extractor target/debug/examples/image_sources \
  --raster-layout /tmp/new-raster-layout.json --output /tmp/new-editable-raster-document
```

`--raster-layout`は画像描画ごとに繰り返せる。入力レポートのbytesを`raster-inputs/`へ捕捉し、最初の解析と未知字形観測後の再解析に同じレポートを使う。Rust側では原本SHA・画像ストリームSHA・page/draw・原画像の寸法・配置・CropBox/Rotateと、文字・セルの参照関係を検査してから付ける。独立した画像デコーダでも元PNGとRGBAのハッシュ・元配置・画素寸法を再確認する。

出力は元の`assets[].raster_layout`。`documents`内の画像参照から同じassetへ到達できる。`words`は編集可能な文字候補、`alternatives`は元の観測への参照、`cells`と`tables.slots`は物理的なセル・行列と結合範囲である。囲みの入れ子、表の穴、重なるセル、境界をまたぐ文字、確定できない読む順序を明示し、空欄を推測で埋めない。未配置の画像部品にも観測を保持できるが、それによって記事の所属を確定しない。

`terminal.json`には追加した画像数、文字候補数、未解決件数と認識失敗数を別に記録する。認識の部分失敗があれば`partial`を返す。既存の本文・書式・元の画像を保持し、画像内の文字を`Document::field_text`へ連結しない。認識文字・セルの意味・全体の読む順序・最終可視性は未認定で、`coverage_verified`はfalse。実装が検査する対応関係と文字の正しさを区別する。

`raster_layout.py --character-report`で一文字ごとの観測を付けたレポートも同じ手順で取り込める。`character_layer`に、全モデル候補・Unicodeスカラー範囲・元画像の四隅・支持画素とセル/語への参照を保持する。Rust側でも全候補・全文字・全範囲の一対一の対応、文字列とconfidence、原本の座標変換、物理セルの包含、未解決情報を照合する。独立デコーダではこの層の原画像・画素寸法・認識用コピーの所属も検査する。

この場合の`terminal.json`は`raster_character_observations`・`raster_character_fragments`・`unresolved_raster_characters`も記録する。文字数は返された候補すべての観測数であり、重複のない原文の字数や正解数ではない。語と文字の異なる読みをconfidenceで選んだり、参照RTFから補ったりしない。

### 原本画像の罫線・閉領域と切り出し配置

一括実行に`--raster-grid`を付けると、デコードした元画素の罫線・交点・閉領域を追加で観測する。実行前に観測コードのbytesも捕捉し、`implementation/raster_grid.py`を使う。本文、画像、ネイティブの表や書式は変更しない。

```sh
python3 tools/kanpo-validation/parse_pdf.py original.pdf \
  --parser target/debug/examples/analyze_media_pdf \
  --native-extractor target/debug/examples/native_pages \
  --image-extractor target/debug/examples/image_sources \
  --raster-grid --output /tmp/new-kanpo-grid-document
```

`document.json`の`raster_grids`は全assetの観測一覧。`raster-grids/asset-NNNN/observation.json`には元画素の支持区間・交点・辺・閉領域を保存する。長方形の内側は元PNGから切り出し、`pdf-binding.json`で原本SHA・元画像ストリームSHA・asset/page/draw・デコードreceipt・配置・RGBAのハッシュへ結び付ける。各切り出しの配置行列は、元の画素から画像への行列に切り出し原点を合成したもの。四半回転・鏡映・せん断を保持し、切り出し画像をページ全体へ引き伸ばさない。

観測前後の原本対応を再検査し、PNG・receipt・payloadが変わった場合は成功した一覧を公開しない。作成済みの原像や観測は保持する。デコードできない画像は失敗情報として残す。これらの観測は`coverage_verified: false`で、記事所属・native Glyph対応を付けない。ロゴの枠も幾何的な候補になり得るため、`rectangular_cells`を本文の表へ自動変換しない。破線・途切れた罫線を補って全セルが取れたと判定する機能や、画像内文字のOCRは含まない。

認識用コピーの作成とNDLOCRは[画像OCR CLI](../kanpo-ocr/README.md)で明示実行する。罫線観測の枠は入れ子にもなるため、認識用の画素区画を作る処理では内側と外枠の残りを分ける。閉領域を全て重複のないセルとして扱わない。

### 元画像から転記した文字の位置別評価

```sh
python3 tools/kanpo-validation/compare_raster_review.py \
  crates/kanpo-parser/tests/data/workbook/raster-source-review.json \
  /tmp/source-derived-raster-ocr --output /tmp/new-raster-review-comparison.json
```

`raster-source-review.json`は、No.10の元画像を拡大して転記した診療科名43項目だけの評価用データ。マスクは転記を確認する領域で、復元した物理セルの境界ではない。元PDF・画像ストリーム・デコードPNG/RGBAのハッシュを固定し、パーサや認識器の実行時には読まない。様式全体、他の画像、官報全体を認定する正解データではない。

`raster-form-source-review.json`は同じ原像の見出し、欄名、病床数・従事者の名称、脚注も別途転記し、既存の43項目と合わせた111の文字領域を含む。元画像の切り出し範囲とUnicode転記の限界を`review_basis`に残す。これも評価専用で、チェック欄・ロゴの図形、全セルの意味や書式、読む順序を網羅した認定ではない。比較CLIの第一引数へこのファイルを指定して検証できる。

比較は元画像座標の中心で対応づける。正しい文字列が別の位置にある場合や、周囲の文脈だけの観測は、その項目の文字を埋めるために使わない。重複した認識結果も全て残して編集距離へ数え、正解に近い候補だけを選ばない。光学的な診断に限ってNFKCと空白除去を明示適用し、生テキストも併記する。文字の一致と評価マスク内の幾何を別に報告する。未一致なら終了コード2で、`coverage_verified`は一致時もfalse。この43項目の成功を全画像の文字・表・読む順序の完全性へ広げない。

「観測なし」は評価マスク内に認識領域の中心が無いことを示し、文字そのものを認識できなかったという判定ではない。広い認識領域に正しい文字があっても、その位置から語の範囲を確認できなければ自動で移し替えない。対応しなかった領域がどの評価マスクと交わるかも記録する。この位置別の編集距離は、OCR本文全体の文字誤り率として扱わない。

## 未対応のネイティブ文字

未知のIdentity CIDが隣接していても、既知の二つの半角字形からなる番号は独立して復号する。未知のCIDを数値の同じUnicodeへ置き換えたり、改め文の意味から文字を埋めたりしない。

一括CLIは全OSで認識器を無効にする。macOSで`--native-glyph-engine vision`を明示した場合だけローカルのApple Visionを使い、原本上で他の描画と重ならない未知グリフを個別に認識する。通常のRust解析は認識器を起動しない。現在の光学的な対応範囲は、原像で向き合う二つの弧が確認でき、その内側を一つの仮名として認識できた字形。文字の向きは元の文字変換行列から求める。法令名、CID表、期待文、個別の文字への補正辞書は使わない。

`native_glyphs`には元PDFのSHA、グリフ・Word・raw内の位置、元の矩形、認識結果と未解決の出所が残る。元のWordと行の文字列全体が一致するときだけ未知部分を置き換え、既知の番号や他の箇所は変更しない。図形文字の挿入と同じ行で共存する場合も、元のWordの順と挿入記録から位置を計算する。元の解析は`native`、認識前の解析は`parse-native/`、生の認識結果・原像・向きを戻したPNGは`native-glyphs/`に保持する。

`native_glyphs.source_issues`と`terminal.json`の`unresolved_text_scalars`は、認識候補にならなかった箇所も含む残りの置換文字を数える。原像との対応を確認することは、認識されたUnicodeの正確性を保証することではない。認識の状態は`candidate_unverified`とし、全文の完全性の認定には使わない。添付No.6の二つの字形は原本画像でも別途確認している。

## 枠と文字が別々に描かれた囲み文字

`frames.bindings`は、原本の中空の枠と、その内側にある本文を対応づける。CFFの二つの矩形輪郭が厳密に内外へ分かれ、向きが逆で、全体の描画・クリップが確認できる場合だけ枠として扱う。元の外枠・内枠・四辺の塗り範囲は`Aj1PageSources::glyph_rectangle_frame`から取得する。フォント名、CID、内側の文字の候補表は使わない。回転・鏡映を変えた原本入力や、数字以外の本文でも同じ条件を検査する。

本文が枠の内側に入り、元の一つの行・セルと欄全体の文字列へ対応する場合、枠を表していたグリフを本文のもう一文字として出さず、本文と枠の書式に分ける。No.4の二つの囲み「0」はこの方法で復元する。通常の`native`は変更せず、`removed_text`・`removed_offset`・`removed_length`で、囲み復元直前の欄から消費した文字範囲も記録する。既に本文外だった枠グリフからは文字を消費しない。

最終文書の`document`・`field`・`offset`・`length`は、ルビ・傍線と同じ非空白Unicodeスカラー位置を使う。`frame`に外側と内側の物理座標、`parent_glyphs`・`parent_words`に中の文字の出所、`frame_glyph`・`frame_word`に枠の出所、原本SHAも保持する。本文の文字数が変わるため傍線を元のWordから計算し直し、二重傍線の位置も更新する。表の行列・結合・元Word・グリフは維持する。

文字が枠を横切る、別の描画と重なる、行・セルや欄全体の対応が不明、囲みの範囲が競合する場合は文字を消費せず`frames.unresolved`へ残す。現在の対象は`scope: strict_cff_text_enclosures`で、パスで描いた枠、破線枠、複数の入れ子の囲みなどを網羅したとはしない。`coverage_verified`はfalseのまま返す。

## ルビと非矩形のセル

`source_media_analysis.ruby.bindings`は、本文へ読みを混ぜず、元のルビと親字の対応を表す。`document`・`field`・`offset`・`length`で、文書の欄全体の非空白Unicodeスカラー位置を指定する。`Document::field_text(field)`で同じ文字列を取得できる。`reading`と元のページ・Word・グリフ添字も保持する。表の文字列も元の順で含み、画像は本文の位置を消費しない。

元のWordとグリフの完全な対応、文字行列の方向、字の大小と位置関係から、一つの親字範囲を決める。元の行・セルと欄全体の文字列が一致するときだけ出力し、同じ親字に競合するルビは両方を未解決にする。読みや親字の候補表、期待文からの位置検索は使わない。No.11の元PDFでは新旧両欄の「輻」に「ふく」が対応する。本文・表・結合は変更しない。

罫線の途切れでL字につながる領域は、外接矩形へ広げると別セルと重なり、Document化で内容を上書きする場合があった。縦組みの論理セルが同じ開始位置を持ち、内容が衝突する場合に、元の最小セルへの幾何的な所属を保つ矩形へ分解する。同面積で競合する所属や、別セルに完全に覆われるセルがあれば分解を認定しない。横組みや折り返し行はそれぞれの格子解釈を保つ。全18件で論理セルの非重複と、元のセルからDocumentの表への全文保存を検査する。

## 過去の期待値の扱い

保存済みのAnalysis JSONは一括で作り直さない。原本を描画して確認した連名見出しの使用Wordと、PDFのMediaBox・CropBox・Rotateから確認した表示寸法だけを、`tests/data/workbook/historical-*.json`の宣言に基づいて検証時に補正する。本文、行順、表、記事などの残りの値は従来どおり完全比較する。静的な出典とアンカーを照合し、実行結果から期待値を生成しない。


## OCRを使わない添付全体の再検証

18件の主PDFとNo.10の記載法令の原本3ページ、計185ページを検証する。
No.10はリンク先と記載法令の両方が対象。原本のSHAを確認してから解析し、
原稿のRTFや期待文字列を実行時の入力には使わない。

```sh
python -m pip install -r tools/kanpo-validation/requirements-non-ocr.txt
cargo build --locked -p kanpo-parser --example analyze_media_pdf -p kanpo-pdf --example native_pages --example image_sources
python tools/kanpo-validation/check_workbook.py --output .cache/workbook-non-ocr
```

`verification.json`に各原本のSHA、文字・画像の保存検証と図形の描画比較を記録する。
`verify_documents.py`は全記事・全欄・入れ子のセルを走査し、元の文書からの変更を
囲みの削除記録と図形文字の原本観測だけで前向きに再生する。本文への無記録の補充・欠落を拒否する。
元のWordが重複せず一つの行／セルに属すること、結合セル・傍線・ルビ・上下付きの対象範囲、
本文画像とページ画像の参照、符号化前の画像の出所も確認する。

`verify_native_paint.py`は別のPDF読取器で原本から図形の描画だけを取り出し、
パースした演算子・破線・クリップ・元の座標変換の列から作ったPDFと独立に描画比較する。
2倍の解像度でRGBA画素が一つでも違えば失敗する。全体画像を背景と仮定して省略しない。
Formを独立に展開できない原本は、この検証器が未対応のまま合格にせずエラーにする。
文字の意味や読み順の正しさは、原本を確認したRust回帰テストと別に扱う。

通常の解析はモデルOCRを起動しない。画像内の編集可能な文字と、Unicode対応の無い字形の認識は
この検証の対象外で、元画像・元の字形と未対応文字の記録を保持する。
`coverage_verified`は任意の官報や合成描画全般を認定するフラグとして変更しない。

添付原本を用いた2026-11-06の確認では、21ファイル・185ページ、本文に使うWord 18,618個、
最終文書の非空白文字380,522字の保存を通過した。図形4,105配置の原本と再構成画像は、
2倍解像度で全RGBA画素が一致した。文字に対応しない空白上の傍線を含め、元フォントの確認済み矩形
27,150配置も保持している。元の画像16配置は本文12・ページ要素4に分けて保持し、元の符号化データと
独立デコードした画素を照合した。No.6のUnicode対応の無い2字と画像内の文字認識はOCR対象として除外し、
元の字形・画像を残す。これらの保存検証を、任意のPDFに対する意味理解やOCR精度の認定へ広げない。
