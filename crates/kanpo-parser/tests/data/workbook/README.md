# 添付一覧の原本

`index.json`は添付Excelから取り出した18件の一覧と出所セル・公開リンク・ファイルのSHA-256。`case-01.pdf`〜`case-18.pdf`はN列のリンク先そのもの。期待出力から作り直したPDFではない。

No.10のリンク先は一覧記載の薬価告示と異なる。元ファイルは保持し、`case-10-listed-1.pdf`〜`case-10-listed-3.pdf`に、ローカル官報アーカイブにある2026年10月13日号外224号1〜3ページの原本も保存する。不一致の詳細は`index.json`に記録する。

ダウンロード・独立RTF診断と完了条件は[`tools/kanpo-validation`](../../../../../tools/kanpo-validation/README.md)を参照。本文の保存則と個々の回帰テストの成功を、18件すべての完全な解析とは数えない。

`case-06.paint-observations.json`は、同じPDFのネイティブ字形を手本に認識した未補正の観測入力。本文期待値ではなく、明示実行APIの原本照合・挿入位置の検証に使う。`painted-compound-review.json`は元PDFの図形文字8か所を確認した開発用転記で、認識器は実行時にこれを読まない。本文全体や汎用OCRの盲検精度を保証するものではない。

`media-source-review.json`は元の画像12配置のストリーム・寸法などを別のpypdf/Pillowによる読み取りでも照合した記録。画像の中身をOCRした期待テキストではない。元データ、配置先、本文と表の保存を別々に検証する。

`case-06.native-glyph-observations.json`は、孤立した未知のネイティブ字形を原本から描画し、Apple Visionが返した観測入力。モデルの文字列やconfidenceは補正していない。元の両側の弧を画像から確認できたときだけ括弧を補う。個別のCIDを文字へ変換する実行時の表ではない。開発時には原本の字形も確認したが、観測の状態は`candidate_unverified`のまま保ち、異なる出所や既知の番号へ適用できないことを対照試験する。
