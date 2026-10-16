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
            // 公布の件名が長く折り返すと（「…に関する政」＋「令の一部を改正する政令をここに公布する。」）、
            // 件名の頭の行（文末でない長い行）は前の記事の末尾にある。新しい記事へ移す。
            let mut carried: Vec<&str> = Vec::new();
            if line.contains("をここに公布する") && !is_heading_mark(line.trim()) {
                while carried.len() < 4 {
                    let Some(last) = cur.last().map(|l| l.trim()) else { break };
                    if last.chars().count() < 25 || last.ends_with(['。', '」', '）']) || is_article_boundary(last) || last.contains("をここに公布する") {
                        break;
                    }
                    carried.insert(0, cur.pop().unwrap());
                }
            }
            if !cur.is_empty() {
                articles.push(cur.join("\n").trim().to_string());
            }
            cur = carried;
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

/// 区分見出し・欄見出しの語か（「その他告示」「法　規　的　告　示」）。
pub(crate) fn is_section_label(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    SECTION_HEADERS.contains(&compact.as_str()) || FIELD_HEADERS.contains(&compact.as_str())
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

/// 行が記事の `〇` 見出し（「〇総務省令第七十七号」）で始まるか。見出しは「〇」の直後に機関名
/// （漢字）が続く。漢数字の零（「〇・五ミリリットル」「〇　五〇」、改め文の「…〇」を「…」に」の
/// 折り返し）や単独の印の「〇」は見出しではない。機関名が漢数字で始まる（「〇九州地方整備局告示
/// 第九十八号」「〇四国地方整備局…」）ときは、行が「第…号」で終わる見出しの形のときだけ見出しとみなす。
/// 番号の折り返し（「第二八三六・五〇」＋「号、…」で行頭に来た「〇号、」「〇円」）も見出しではない。
pub(crate) fn is_heading_mark(t: &str) -> bool {
    const NUMERALS: &str = "〇一二三四五六七八九十百千";
    let kanji = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
    let mut chars = t.chars();
    if chars.next() != Some('〇') {
        return false;
    }
    match chars.next() {
        Some(c) if "号条項".contains(c) => false,
        // 助数詞の後に漢字が続かない（「〇円、」「〇年」）のは数。「〇人事院規則」は見出し。
        Some(c) if "年月日円人件個度倍分時点種級番歳回台".contains(c) => chars.next().is_some_and(kanji),
        Some(c) if kanji(c) && !NUMERALS.contains(c) => true,
        Some(c) if NUMERALS.contains(c) => {
            let t = t.trim_end();
            chars.next().is_some_and(|c| kanji(c) && !NUMERALS.contains(c))
                && t.ends_with('号')
                && t.contains('第')
                && t.chars().count() <= 40
                && !t.contains(['、', '。', '「', '」'])
        }
        _ => false,
    }
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
        let text = "〇厚生労働省令第百号\n接種量は、\n〇・五ミリリットルとする方法\n〇　五〇　鳥取県\n〇」を「六〇一、七〇〇」に改める。";
        assert_eq!(segment_articles(text).len(), 1);
        assert!(is_heading_mark("〇財務省、厚生労働省、令第七号"));
        assert!(!is_heading_mark("〇"));
        assert!(!is_heading_mark("〇号、第二八三六・五〇号"));
        assert!(!is_heading_mark("〇円とする。"));
        assert!(is_heading_mark("〇人事院規則一―七二"));
    }

    #[test]
    fn agency_starting_with_numeral_is_heading() {
        // 九州・四国の地方整備局は機関名が漢数字で始まる。前の告示と 1 つの記事にしない。
        let text = "〇東北地方整備局告示第百一号\n道路の区域を変更した。\n〇九州地方整備局告示第九十八号\n都市計画事業を認可した。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[1].starts_with("〇九州地方整備局告示第九十八号"));
        assert!(is_heading_mark("〇四国地方整備局告示第十二号"));
        // 漢数字の行（「〇九〇〇から」「〇五、二四〇メートル」）は見出しではない。
        assert!(!is_heading_mark("〇九〇〇から一七〇〇まで"));
        assert!(!is_heading_mark("〇五分の地点を中心とする半径一号"));
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

    #[test]
    fn carries_the_wrapped_head_of_a_promulgated_title() {
        // 件名が折り返し、頭の行は前の記事の末尾にある。署名（短い行）は前の記事に残す。
        let text = "この政令は、令和八年九月一日から施行する。\n内閣総理大臣高市早苗\n令和八年熊本地震による災害についての激甚災害及びこれに対し適用すべき措置の指定に関する政\n令の一部を改正する政令をここに公布する。\n御名御璽";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2, "{arts:?}");
        assert!(arts[0].ends_with("内閣総理大臣高市早苗"));
        assert!(arts[1].starts_with("令和八年熊本地震による災害"));
    }
}
