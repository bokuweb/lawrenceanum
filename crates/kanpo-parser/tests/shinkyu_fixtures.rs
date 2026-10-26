//! 実官報 PDF を `pdftotext -bbox-layout` に通した XHTML を固定化したフィクスチャで、
//! 縦書き新旧対照表の抽出を end-to-end（pdftotext 非依存）で検証する。
//!
//! フィクスチャ生成: `pdftotext -bbox-layout -enc UTF-8 <pdf> <out>.bbox.html`

use kanpo_parser::{detect_format_of, reconstruct_vertical, segment_articles, Block, Document};

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
    let rows = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Shinkyu { rows ,..} => Some(rows),
            _ => None,
        })
        .expect("新旧対照表ブロック");
    // 条・別表ごとに行分割され、改正後/改正前が対応する。
    assert!(rows.len() >= 3, "（さめ…）/別表第四/別表第八 で複数行: {}", rows.len());
    let all_after: String = rows.iter().flat_map(|r| &r.after).map(|r| r.text.as_str()).collect();
    let all_before: String = rows.iter().flat_map(|r| &r.before).map(|r| r.text.as_str()).collect();
    assert!(all_after.contains("中西部太平洋条約海域、東部太平洋"), "改正後セル");
    assert!(all_before.contains("インド洋協定海域においては"), "改正前セル");
    // 先頭行（さめの…）は改正後/改正前 両方を持つ。
    let first = &rows[0];
    assert!(!first.after.is_empty() && !first.before.is_empty(), "先頭行は両側");
    // 傍線フィールドは現状すべて false。
    assert!(rows.iter().flat_map(|r| &r.after).all(|r| !r.underline));

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

#[test]
fn kaiyo_table_continues_into_lower_tiers_on_same_page() {
    // 環境省告示（海洋汚染防止法施行令 別表第一の物質）ページ2。上半分（見出し付き）から
    // 下半分の段へ欄見出し無しで表が続き、下段の左には別記事（農林水産省告示）が同居する。
    // 表中の (1)(15) 等は ToUnicode 無しの私用 CID で組まれている。
    let xhtml = include_str!("fixtures/kaiyo_busshitsu_p2.bbox.html");
    let out = reconstruct_vertical(xhtml);
    assert_eq!(detect_format_of(&out).as_deref(), Some("shinkyu"), "out=\n{out}");

    // 段の先頭列（bbox が上余白にかかる）も落とさない。
    assert!(out.contains("〇環境省告示第四十三号"), "記事見出し: {out}");
    assert!(out.contains("公布の日から適用する。"), "前文: {out}");
    // 私用 CID の括弧付き数字・ハイフンを復元し、化けた漢字を残さない。
    assert!(!out.contains(['挎', '昝', '昞', '慺']), "化けたグリフ: {out}");

    let lines: Vec<&str> = out.lines().collect();
    let go = lines.iter().position(|l| *l == "改正後").unwrap();
    let mae = lines.iter().position(|l| *l == "改正前").unwrap();
    let (after, before) = (&lines[go + 1..mae], &lines[mae + 1..]);
    // 折り返した物質名をセルごとに組み、係数は物質名の後ろに来る。
    assert!(after.contains(
        &"（２）\u{3000}Ｎ－アルキル－Ｎ・Ｎ・Ｎ－トリメチルアンモニウムクロリド（アルキル基の炭素数が十二から十六までのもの及びその混合物に限る。）\u{3000}一、〇〇〇"
    ), "{after:#?}");
    // 下半分の段の表（改正後の続き）は改正後へ、改正前の続きは改正前へ入る。
    assert!(after.contains(&"（２７）\u{3000}テトラキス（ヒドロキシメチル）ホスホニウム硫酸塩\u{3000}一〇〇"), "{after:#?}");
    assert!(after.contains(&"（１６）\u{3000}ポリアスパラギン酸アミン\u{3000}〇"), "{after:#?}");
    assert!(before.contains(&"（３３）〜（５０）\u{3000}（略）\u{3000}（略）"), "{before:#?}");
    assert!(!before.iter().any(|l| l.contains("テトラキス")), "改正前に改正後の行が混入しない");

    // 同居する農林水産省告示は表の後ろに出て、記事分割で表とは別記事になる。
    let segments = segment_articles(&out);
    let table = segments.iter().find(|s| s.contains("〇環境省告示")).unwrap();
    assert!(!table.contains("保安林") && !table.contains("その他告示"), "{table}");
    assert!(segments.iter().any(|s| s.starts_with("〇農林水産省告示第千五百二十三号") && s.contains("保安林")));

    // 条見出しの無い表は号（一・二・三）ごとに改正後/改正前を揃える。
    let doc = Document::from_text(table);
    let rows = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Shinkyu { rows ,..} => Some(rows),
            _ => None,
        })
        .expect("新旧対照表ブロック");
    assert_eq!(rows.len(), 3);
    for (row, go) in rows.iter().zip(["一", "二", "三"]) {
        assert!(row.after[0].text.starts_with(go) && row.before[0].text.starts_with(go), "{row:?}");
    }
}

#[test]
fn analysis_traces_every_word_and_matches_text() {
    // 可視化用の解析結果は本文と一致し、使われた語はちょうど 1 つの行に属する。
    for xhtml in [
        include_str!("fixtures/kaiyo_busshitsu_p2.bbox.html"),
        include_str!("fixtures/gyogyo_p2.bbox.html"),
        include_str!("fixtures/koyou_cont_p66.bbox.html"),
    ] {
        let a = kanpo_parser::analyze(xhtml);
        assert_eq!(a.text, reconstruct_vertical(xhtml));
        for page in &a.pages {
            let mut seen = std::collections::HashSet::new();
            for line in page.regions.iter().flat_map(|r| &r.lines) {
                let in_table = line.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter()));
                for id in line.words.iter().chain(in_table) {
                    assert!(seen.insert(*id), "語 {id} が複数の行に属する");
                    assert_eq!(page.words[*id].status, kanpo_parser::trace::WordStatus::Used);
                }
            }
            let used = page.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used).count();
            assert_eq!(used, seen.len());
        }
    }
    // 新旧対照表の欄見出しのラベルと、上余白の柱は捨てた語として残る。
    let a = kanpo_parser::analyze(include_str!("fixtures/kaiyo_busshitsu_p2.bbox.html"));
    let status = |s| a.pages[0].words.iter().filter(|w| w.status == s).count();
    assert_eq!(status(kanpo_parser::trace::WordStatus::HeaderLabel), 6);
    assert!(status(kanpo_parser::trace::WordStatus::Margin) > 0);
    assert!(a.articles.iter().any(|art| art.text.starts_with("〇農林水産省告示")));
}
