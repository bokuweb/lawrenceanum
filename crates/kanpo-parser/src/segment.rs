//! 1 ページ内の複数記事（法令/告示）の分割。

/// 官報1ページ分の本文を「記事」単位に分割する。
///
/// 1ページに複数の法令/告示が詰まることがあるので、記事の先頭マーカで区切る。
/// - 省令・告示・規則: `〇` 見出し（例: 「〇総務省令第七十七号」）。
/// - 法律・政令の公布: 「{件名}をここに公布する。」で始まるブロック。
///   （これが無いと複数の政令が1記事に混ざり、呼び出し側の標題突合が誤った巨大ブロックを返す。）
pub fn segment_articles(text: &str) -> Vec<String> {
    let mut articles: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.lines() {
        let prev = cur.iter().rev().find(|l| !l.trim().is_empty()).copied();
        if (is_article_boundary(line) || is_notice_title(line, prev)) && !cur.is_empty() {
            articles.push(cur.join("\n").trim().to_string());
            cur = Vec::new();
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        let s = cur.join("\n").trim().to_string();
        if !s.is_empty() {
            articles.push(s);
        }
    }
    articles
}

/// その行が新しい記事の先頭か。
///
/// - 省令・告示・規則の `〇` 見出し（例: 「〇総務省令第七十七号」）
/// - 法律・政令の公布行（「…をここに公布する」）
/// - 官報の区分見出しの独立行（新旧対照表ページに同居する別記事の境界）。
///   「告示」「公告」「公示」「法規的告示」「条約」「庁令」「訓令」「国会事項」「官庁報告」等。
///   例: 省令の改め文の直後に並ぶ「法規的告示」の別件、型式認定告示など。
/// - 官庁報告・公告の欄見出しの独立行（「官庁事項」「労働」「諸事項」等）。横組みページでは
///   最低賃金公示などがこの見出しの下に並ぶ。
fn is_article_boundary(line: &str) -> bool {
    let t = line.trim();
    is_heading_mark(t)
        || t.contains("をここに公布する")
        || {
            // 区分見出しは字間を空けて組まれる（「法　規　的　告　示」）。
            let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
            SECTION_HEADERS.contains(&compact.as_str()) || FIELD_HEADERS.contains(&compact.as_str())
        }
}

/// 官庁報告・公告の欄の中で、記事の標題（「指定保安検査機関の廃止に関する公示」）が始まる行か。
///
/// 同じ欄見出しの下に複数の公示が続くため、欄見出しだけでは記事を分けられない。
/// 文の途中の折り返し行（「…規定により公示」＋「する。」）と取り違えないよう、
/// 句読点を含まない短い 1 行で、直前の行が文末（`。`）で終わる場合に限る。
fn is_notice_title(line: &str, prev: Option<&str>) -> bool {
    let t = line.trim();
    (t.ends_with("に関する公示") || t.ends_with("に関する公告"))
        && t.chars().count() <= 40
        && !t.contains(['。', '、'])
        && prev.is_some_and(|p| p.trim_end().ends_with('。'))
}

/// 行が記事の `〇` 見出し（「〇総務省令第七十七号」）で始まるか。漢数字の零（「〇・五ミリ
/// リットル」「〇　五〇」）や単独の印の「〇」は見出しではない。
pub(crate) fn is_heading_mark(t: &str) -> bool {
    let mut chars = t.chars();
    chars.next() == Some('〇')
        && chars
            .next()
            .is_some_and(|c| !c.is_whitespace() && !c.is_ascii_digit() && !"・．.，、〇一二三四五六七八九十百千０１２３４５６７８９".contains(c))
}

/// 官報本文に現れる区分（記事カテゴリ）見出し。これらが独立行で現れたら別記事の開始とみなす。
/// 本文中に単独行で現れることはまずないものに限定し、誤分割を避ける。
const SECTION_HEADERS: &[&str] = &[
    "告示",
    "公告",
    "公示",
    "法規的告示",
    "その他告示",
    "条約",
    "庁令",
    "訓令",
    "国会事項",
    "官庁報告",
    "皇室事項",
];

/// 官庁報告・公告の中の欄見出し（目次の小見出し）。独立行で現れたら別記事の開始とみなす。
/// 「資料」「産業」のように本文の単独行と紛れやすい語は入れない。
const FIELD_HEADERS: &[&str] = &["官庁事項", "人事異動", "労働", "法務", "国家試験", "公聴会", "諸事項"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_articles_on_circle_heading() {
        let text = "〇総務省令第七十七号\n電波法の一部を改正する省令\n〇農林水産省告示第七百六十四号\n規格を廃止する件";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("電波法"));
        assert!(arts[1].contains("規格を廃止"));
    }

    #[test]
    fn zero_numeral_is_not_heading() {
        // 「〇・五ミリリットル」の漢数字の零で始まる行で記事を切らない。
        let text = "〇厚生労働省令第百号\n接種量は、\n〇・五ミリリットルとする方法\n〇　五〇　鳥取県";
        assert_eq!(segment_articles(text).len(), 1);
        assert!(is_heading_mark("〇財務省、厚生労働省、令第七号"));
        assert!(!is_heading_mark("〇"));
    }

    #[test]
    fn segments_articles_on_kokuji_heading() {
        // 新旧対照表の下に同居する別記事(告示)を「告示」独立見出しで分離する。
        let text = "改正前\n第十六条〔同上〕\nこの省令は…から施行する。\n告示\n型式認定番号…\n交Ｎ2626…";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("第十六条") && !arts[0].contains("型式認定"));
        assert!(arts[1].starts_with("告示") && arts[1].contains("型式認定"));
    }

    #[test]
    fn segments_articles_on_section_header() {
        // 省令の改め文の直後に同居する「法規的告示」の別件を境界で切り離す。
        let text = "〇農林水産省令第三十二号\n…の一部を改正する省令\n附則\nこの省令は…から施行する。\n法規的告示\n…第六十二条第二号ただし書の規定に基づき…\n…の措置を講じている場合";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("省令") && !arts[0].contains("ただし書"));
        assert!(arts[1].starts_with("法規的告示"));
    }

    #[test]
    fn segments_articles_on_field_header_and_notice_title() {
        // 横組みページ: 「労働」欄の最低賃金公示、「公告」以下の別件。
        let text = "掺図面縦覧場所　北陸地方整備局\n労働\n最低賃金の改正決定に関する公示\n第４号中「１時間1030円」を「１時間1095円」\nに改める。\n公告\n諸事項\n相続財産清算人の選任及び相";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 4, "{arts:#?}");
        assert!(arts[1].starts_with("労働\n最低賃金") && arts[1].ends_with("に改める。"));
        assert_eq!(arts[3], "諸事項\n相続財産清算人の選任及び相");

        // 同じ「官庁事項」欄の中で、前の公示の文末に続く標題行で分ける。
        let text = "官庁事項\n船舶局無線従事者証明の申請者に対する訓練の実施\nする納付書により納付すること。）。\n指定保安検査機関の廃止に関する公示\n高圧ガス保安法…公示する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2, "{arts:#?}");
        assert!(arts[1].starts_with("指定保安検査機関の廃止に関する公示"));

        // 文の途中の折り返し（直前の行が文末でない）では分けない。
        let text = "次の事項について\n最低賃金の改正決定に関する公示\nを行う。";
        assert_eq!(segment_articles(text).len(), 1);
    }

    #[test]
    fn segments_articles_on_promulgation_boundary() {
        // 同一ページに複数の政令公布が並ぶケース。「をここに公布する」で記事を分ける。
        let text = "労働組合法施行令の一部を改正する政令をここに公布する。\n御名御璽\n労働組合法施行令の一部を次のように改正する。\n美容師法施行令の一部を改正する政令をここに公布する。\n御名御璽\n美容師法施行令の一部を次のように改正する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("労働組合法施行令") && !arts[0].contains("美容師"));
        assert!(arts[1].contains("美容師法施行令") && !arts[1].contains("労働組合"));
    }
}
