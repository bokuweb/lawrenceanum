//! 回転ページ（`Page rot: 270`、本文が横組み）の読み順復元を、実官報 PDF の
//! `pdftotext -bbox-layout` 出力を固定化したフィクスチャで検証する。
//!
//! フィクスチャ生成: `pdftotext -bbox-layout -enc UTF-8 <pdf> <out>.bbox.html`

use kanpo_parser::{reconstruct_vertical, segment_articles};

/// `kanpo-fetch` の標題突合と同じく、標題（対象の公示名）を含む記事セグメントを返す。
/// ちょうど 1 つでなければ失敗させる。
fn segment_with<'a>(segments: &'a [String], title: &str) -> &'a str {
    let hits: Vec<&String> = segments.iter().filter(|s| s.contains(title)).collect();
    assert_eq!(hits.len(), 1, "「{title}」を含む記事がちょうど 1 つ: {segments:#?}");
    hits[0]
}

/// 柱（左端帯の「官」「報」「令和 年 月 日」「第 号」「○曜日」）が本文へ漏れていないこと。
fn assert_no_running_header(out: &str) {
    for noise in ["号第報", "月年令和", "曜日日", "官\n", "\n報\n", "第\n号"] {
        assert!(!out.contains(noise), "柱 {noise:?} が混入: {out}");
    }
    for weekday in ["月曜日", "火曜日", "水曜日", "木曜日", "金曜日"] {
        assert!(!out.lines().any(|l| l.trim() == weekday), "柱 {weekday} が混入: {out}");
    }
}

#[test]
fn saga_minimum_wage_single_column_page() {
    // 令和8年10月2日 本紙第1800号 p8（最低賃金の改正決定に関する公示（佐賀労働局最低賃金公示一））。
    // 上に前ページから続く道路占用の表、左の列に「労働」欄の最低賃金公示、その右に「公告」
    // 「諸事項」の見出し、右端に家裁公告の標題。
    let out = reconstruct_vertical(include_str!("fixtures/saga_saitei_chingin_p8.bbox.html"));
    assert_no_running_header(&out);

    // 1 字ずつ字間を空けた見出しは 1 行にまとまり、記事境界になる。
    let lines: Vec<&str> = out.lines().collect();
    for heading in ["労働", "公告", "諸事項", "附則"] {
        assert!(lines.contains(&heading), "見出し {heading} が 1 行: {out}");
    }
    // 1 字語の逆順連結（「由理るす限制を用占」）が起きない。
    assert!(
        out.contains("占用を制限する理由　緊急輸送道路の占用を制限する"),
        "{out}"
    );
    // 表の項目名と内容は同じ行に並ぶ。
    assert!(out.contains("占用の制限の開始の期日　令和八年十月三日"), "{out}");

    let segments = segment_articles(&out);
    let seg = segment_with(&segments, "最低賃金の改正決定に関する公示");
    assert_eq!(
        seg,
        "労働\n\
         最低賃金の改正決定に関する公示\n\
         佐賀労働局最低賃金公示第１号\n\
         最低賃金法（昭和34年法律第137号）第12条の\n\
         規定に基づき、佐賀県最低賃金（昭和55年佐賀労\n\
         働基準局最低賃金公示第１号）の一部を次のよう\n\
         に改正する決定をしたので、同法第14条第１項の\n\
         規定により公示する。\n\
         令和８年10月２日\n\
         佐賀労働局長　田之上英治\n\
         第４号中「１時間1,030円」を「１時間1,095円」\n\
         に改める。\n\
         附則\n\
         この決定は、令和８年11月15日から効力を生ず\n\
         る。"
    );
}

#[test]
fn hyogo_minimum_wage_multi_column_page() {
    // 令和8年10月8日 本紙第1804号 p8（最低賃金の改正決定に関する公示（兵庫労働局最低賃金公示二））。
    // 列が x≈61–241（労働）/ 245–421（公告・諸事項）/ 429–789（割賦販売法の公示）に並び、
    // 左 2 列の下に 2 列抜きの公示がある。pdftotext の素の順序は列をまたいで行が交互に並ぶ。
    let out = reconstruct_vertical(include_str!("fixtures/hyogo_saitei_chingin_p8.bbox.html"));
    assert_no_running_header(&out);

    let segments = segment_articles(&out);
    let seg = segment_with(&segments, "最低賃金の改正決定に関する公示");
    assert!(
        seg.starts_with("労働\n最低賃金の改正決定に関する公示\n兵庫労働局最低賃金公示第２号\n"),
        "{seg}"
    );
    assert!(
        seg.contains("兵庫県計量器・測定器・分\n析機器・試験機・測量機械器具製造業最低賃金"),
        "{seg}"
    );
    assert!(
        seg.contains("第４号中「１時間1,117円」を「１時間1,173円」\nに改める。"),
        "{seg}"
    );
    assert!(
        seg.ends_with("この決定は、令和８年12月１日から効力を生ず\nる。"),
        "{seg}"
    );
    // 隣の列（公告）や下の全幅の公示が混ざらない。
    for other in ["広島法務局", "割賦販売法", "有権者申出方", "相続財産"] {
        assert!(!seg.contains(other), "最低賃金公示に {other} が混入: {seg}");
    }

    // 隣の列と下の全幅記事も、それぞれ列単位でまとまって読める。
    let pos = |s: &str| out.find(s).unwrap_or_else(|| panic!("{s} が無い: {out}"));
    assert!(pos("る。\n公告\n諸事項\n有権者申出方\n元当局所属公証人小原浩司の身元保証金還付に\n") > 0);
    assert!(pos("令和８年10月８日　広島法務局") < pos("割賦販売法及び割賦販売法施行令に基づく債権の申出に関する"));
    assert!(out.contains("名称　本店の所在地　営業廃止年月日"), "{out}");
    // 右半分（2 列抜きの割賦販売法の公示）は左半分の後に読む。
    assert!(pos("（備考）用紙の大きさは") < pos("割賦販売法に基づく同法第35条の３の61の許可を受けた者の営"));
}

#[test]
fn notice_title_splits_notices_under_same_field_header() {
    // 令和8年10月1日 本紙第1799号 p10。叙勲名簿の続き → 官庁報告「官庁事項」に船舶局無線従事者
    // 証明の訓練の公示と、指定保安検査機関の廃止に関する公示（九州産業保安監督部）が続く。
    let out = reconstruct_vertical(include_str!("fixtures/hoan_kensa_haishi_p10.bbox.html"));
    assert_no_running_header(&out);

    let segments = segment_articles(&out);
    let seg = segment_with(&segments, "指定保安検査機関の廃止に関する公示");
    assert!(
        seg.starts_with("指定保安検査機関の廃止に関する公示\n高圧ガス保安法（昭和26年法律第204号）第58\n"),
        "{seg}"
    );
    assert!(
        seg.contains("九州産業保安監督部長　樫福　錠治\n１．名称　福岡酸素株式会社"),
        "{seg}"
    );
    assert!(seg.ends_with("４．廃止年月日　令和８年９月30日"), "{seg}");
    assert!(!seg.contains("無線従事者"), "前の公示が混入: {seg}");

    // 前の公示（表を含む）も行単位で読める。
    let kunren = segment_with(&segments, "船舶局無線従事者証明の申請者に対する訓練の実");
    assert!(
        // 郵便番号の「－」は私用領域の U+E61C（全角の「－」）。
        kunren.contains("九州総合通信局　〒860－8795\n熊本県熊本市西区春日"),
        "{kunren}"
    );
}

#[test]
fn heading_only_page_keeps_heading_and_drops_running_header() {
    // 令和8年10月5日 本紙第1801号 p7。テキスト層にあるのは柱と家裁公告の標題だけ。
    let out = reconstruct_vertical(include_str!("fixtures/souzoku_heading_only_p7.bbox.html"));
    assert_eq!(out, "相続財産清算人の選任及び相\n続権主張の催告");
}
