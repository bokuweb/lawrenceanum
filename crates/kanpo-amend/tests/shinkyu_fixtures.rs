//! 実官報 PDF を `pdftotext -bbox-layout` に通した XHTML を固定化したフィクスチャで、
//! 縦書き新旧対照表の抽出を end-to-end（pdftotext 非依存）で検証する。
//!
//! フィクスチャ生成: `pdftotext -bbox-layout -enc UTF-8 <pdf> <out>.bbox.html`

use kanpo_amend::{detect_format_of, reconstruct_vertical, segment_articles, Block, Document};

/// 出力を「改正後」「改正前」の欄見出し行で 3 分割する（前文 / 改正後 / 改正前）。
/// 複数の表段がある場合は最初の段を対象にする。縦書きの列折り返しで語が改行分割される
/// ため（例: 「う\nばざめ」）、各ブロックは改行を除いた連結文字列で返す。
fn split_shinkyu(out: &str) -> (String, String) {
    let lines: Vec<&str> = out.lines().collect();
    let go = lines.iter().position(|l| l.trim() == "改正後").expect("改正後 見出し");
    let mae = lines[go..]
        .iter()
        .position(|l| l.trim() == "改正前")
        .map(|i| i + go)
        .expect("改正前 見出し");
    // 次の段の「改正後」が来るまでを改正前ブロックとする。
    let next_go = lines[mae + 1..]
        .iter()
        .position(|l| l.trim() == "改正後")
        .map(|i| i + mae + 1)
        .unwrap_or(lines.len());
    let after = lines[go + 1..mae].concat();
    let before = lines[mae + 1..next_go].concat();
    (after, before)
}

#[test]
fn gyogyo_shinkyu_splits_after_before_correctly() {
    // 漁業の許可及び取締り等に関する省令(338M50010000005)ページ2。表だけのページで、
    // 改正後/改正前がクリーンに分離されることを確認する。
    let xhtml = include_str!("fixtures/gyogyo_p2.bbox.html");
    let out = reconstruct_vertical(xhtml);

    assert_eq!(detect_format_of(&out).as_deref(), Some("shinkyu"), "out=\n{out}");
    let (after, before) = split_shinkyu(&out);

    // 改正後(新規追加側): 海域の列挙順が「中西部太平洋…東部太平洋及びインド洋協定海域」、
    // かつ「うばざめ」「ほほじろざめ」の禁止規定が新設される。
    assert!(
        after.contains("中西部太平洋条約海域、東部太平洋"),
        "改正後に新海域列挙: {after}"
    );
    assert!(after.contains("うばざめ") && after.contains("ほほじろざめ"), "改正後に新設禁止: {after}");

    // 改正前(旧側): 海域列挙が「インド洋協定海域…中西部太平洋」順で、冷凍保存の除外規定、
    // そして新設箇所が「（新設）」で示される。
    assert!(
        before.contains("インド洋協定海域においては"),
        "改正前に旧規定: {before}"
    );
    assert!(before.contains("（新設）"), "改正前に（新設）マーカ: {before}");

    // 取り違えていないこと（うばざめ/ほほじろざめは改正後のみ）。
    assert!(!before.contains("うばざめ"), "改正前に新設語が混入していない: {before}");
}

#[test]
fn yubin_shinkyu_table_body_is_extracted() {
    // 郵便法施行規則(総務五八)ページ2。小さな新旧対照表に型式認定一覧が同居する難ケース。
    // 既知の限界として表外の告示が末尾に残りうるが、新旧対照表「本体」は抽出できる。
    let xhtml = include_str!("fixtures/yubin_p2.bbox.html");
    let out = reconstruct_vertical(xhtml);

    assert_eq!(detect_format_of(&out).as_deref(), Some("shinkyu"), "out=\n{out}");
    let (after, before) = split_shinkyu(&out);

    // 表本体（特別送達の認証方法）の見出し・条文が両側に現れる。
    assert!(after.contains("特別送達の取扱いに係る認証の方法"), "改正後に表本体: {after}");
    assert!(before.contains("特別送達の取扱いに係る認証の方法"), "改正前に表本体: {before}");
    // 改正後は新しい条文番号(第百条第一項)、改正前は旧(第百九条)を含む。
    assert!(after.contains("第百条第一項"), "改正後に新条番号: {after}");
    assert!(before.contains("第百九条"), "改正前に旧条番号: {before}");

    // 表の下に同居する型式認定告示（道路交通法施行規則・交Ｎ…）は「告示」独立見出しを
    // 境界に別記事へ分割され、新旧対照表の記事には混ざらない（best_segment が落とせる）。
    let segments = segment_articles(&out);
    let table_seg = segments
        .iter()
        .find(|s| s.contains("特別送達の取扱いに係る認証の方法"))
        .expect("表本体を含む記事");
    assert!(!table_seg.contains("型式認定"), "表記事に型式認定が混入しない: {table_seg}");
    assert!(!table_seg.contains("交Ｎ"), "表記事に型式認定番号が混入しない: {table_seg}");
    assert!(
        segments.iter().any(|s| s.contains("型式認定")),
        "型式認定は別記事として分離される"
    );
}

#[test]
fn multipage_continuation_page_gets_after_before_labels() {
    // 雇用保険法施行規則の新旧対照表の継続ページ(p66)。先頭ページでないため「改正後/改正前」の
    // 欄見出しが無いが、上下半がほぼ同一内容なので継続ページと判定し、欄見出しを補って復元する。
    let xhtml = include_str!("fixtures/koyou_cont_p66.bbox.html");
    let out = reconstruct_vertical(xhtml);

    assert_eq!(detect_format_of(&out).as_deref(), Some("shinkyu"), "out=\n{}", &out[..out.len().min(400)]);
    assert!(out.lines().any(|l| l.trim() == "改正後"), "継続ページに改正後ラベル");
    assert!(out.lines().any(|l| l.trim() == "改正前"), "継続ページに改正前ラベル");

    let (after, before) = split_shinkyu(&out);
    // 改正後/改正前の差分: 改正後「次の第一号から第四号まで」 vs 改正前「次の各号のいずれにも」。
    assert!(after.contains("第一号から第四号まで"), "改正後に新文言: {after}");
    assert!(before.contains("各号のいずれにも"), "改正前に旧文言: {before}");
}

#[test]
fn gyogyo_structures_into_document_with_shinkyu_block() {
    // 構造化出力: 漁業省令ページ2 → 前文の段落 + 改正後/改正前の新旧対照表ブロック。
    let xhtml = include_str!("fixtures/gyogyo_p2.bbox.html");
    let doc = Document::from_text(&reconstruct_vertical(xhtml));

    assert_eq!(doc.format, "shinkyu");
    // 先頭は前文(段落)、続いて新旧対照表ブロック。
    assert!(matches!(doc.blocks.first(), Some(Block::Paragraph { .. })));
    let shinkyu = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Shinkyu { after, before } => Some((after, before)),
            _ => None,
        })
        .expect("新旧対照表ブロック");
    let after_text: String = shinkyu.0.iter().map(|r| r.text.as_str()).collect();
    let before_text: String = shinkyu.1.iter().map(|r| r.text.as_str()).collect();
    assert!(after_text.contains("中西部太平洋条約海域、東部太平洋"), "改正後セル");
    assert!(before_text.contains("インド洋協定海域においては"), "改正前セル");
    // 傍線フィールドは現状すべて false。
    assert!(shinkyu.0.iter().all(|r| !r.underline));

    // JSON 化できる（HTML 等への変換用）。
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("\"kind\":\"shinkyu\""));
}

#[test]
fn personnel_list_page_is_not_misdetected_as_shinkyu() {
    // 上下2段組だが「上下で別内容」の人事異動ページ。新旧対照表ではないので、
    // 継続ページ検出（内容類似ガード）に誤って引っかからず、欄見出しを付けない。
    let xhtml = include_str!("fixtures/jinji_p4.bbox.html");
    let out = reconstruct_vertical(xhtml);
    assert!(!out.lines().any(|l| l.trim() == "改正後"), "人事ページに改正後ラベルを付けない: {out}");
    assert!(!out.lines().any(|l| l.trim() == "改正前"), "人事ページに改正前ラベルを付けない");
}
