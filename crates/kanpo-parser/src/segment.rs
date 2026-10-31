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
    let lines: Vec<&str> = text.lines().collect();
    // 官庁報告の区分の中か（区分見出し「官庁報告」から、次の区分見出しか `〇` の見出しまで）。
    let mut report = false;
    for (i, &line) in lines.iter().enumerate() {
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if compact == "官庁報告" {
            report = true;
        } else if SECTION_HEADERS.contains(&compact.as_str()) || is_heading_mark(line.trim()) {
            report = false;
        }
        // 公布文が「…政令をここ」「に公布する。」と句の途中で折り返すと、どちらの行も句を含まない。
        // 句が始まる行を公布文の行とする（号外第 178 号 6 ページの政令第二百五十二号）。
        let promulgation = line.contains(PROMULGATION) || lines.get(i + 1).is_some_and(|next| wraps_promulgation(line, next));
        let prev = cur.iter().rev().find(|l| !l.trim().is_empty()).copied();
        // 区分見出しに直結する丸印の小見出しも本文に含める。
        // 「人事異動 / 内閣 / 〇財務大臣臨時代理」「叙位・叙勲 / 〇叙勲」を分断しない。
        let heading_prefix = is_heading_mark(line.trim())
            && cur.iter().find(|l| !l.trim().is_empty()).is_some_and(|l| matches!(l.trim(), "人事異動" | "叙位・叙勲"))
            && cur.iter().all(|l| l.trim().is_empty() || is_section_label(l) || l.trim() == "内閣");
        // 人事異動・叙位・叙勲の欄の中の丸印の小見出し（「〇外務大臣臨時代理」「〇監査委員選任」）は欄の続き。
        // 目次はこの欄を 1 項目（「内閣 財務省 最高裁判所 福岡県」）に載せる。号数の見出し（「第…号」）は分ける。
        let section_entry = is_heading_mark(line.trim())
            && !is_numbered_heading(line.trim())
            && cur.iter().find(|l| !l.trim().is_empty()).is_some_and(|l| matches!(l.trim(), "人事異動" | "叙位・叙勲"));
        if (is_article_boundary(line) || promulgation || is_notice_title(line, prev) || is_agency_notice_title(line, prev) || (report && starts_enactment(&lines, i, prev)))
            && !cur.is_empty() && !heading_prefix && !section_entry {
            // 公布の件名が長く折り返すと（「…に関する政」＋「令の一部を改正する政令をここに公布する。」）、
            // 件名の頭の行（文末でない長い行）は前の記事の末尾にある。新しい記事へ移す。
            let mut carried: Vec<&str> = Vec::new();
            if promulgation && !is_heading_mark(line.trim()) {
                while carried.len() < 4 {
                    let Some(last) = cur.last().map(|l| l.trim()) else { break };
                    if last.chars().count() < 25 || last.ends_with(['。', '」', '）']) || is_article_boundary(last) || last.contains(PROMULGATION) {
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

/// 号数の見出し（「〇総務省令第七十七号」）か。
fn is_numbered_heading(t: &str) -> bool {
    t.ends_with('号') && t.contains('第')
}

/// 法律・政令の公布文の句。
const PROMULGATION: &str = "をここに公布する";

/// `line` から始まる公布文の句が、`next` の行へ折り返しているか（「…政令をここ」「に公布する。」）。
fn wraps_promulgation(line: &str, next: &str) -> bool {
    let (line, next) = (line.trim(), next.trim());
    if line.contains(PROMULGATION) || next.contains(PROMULGATION) || line.ends_with('。') {
        return false;
    }
    let joined = format!("{line}{next}");
    joined.find(PROMULGATION).is_some_and(|at| at < line.len())
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
            SECTION_HEADERS.contains(&compact.as_str())
                || FIELD_HEADERS.contains(&compact.as_str())
                || TEXT_FIELD_HEADERS.contains(&compact.as_str())
                // 「資料」は本文の単独行（「⒜…情報及び / 資料 / ⒝…」）にもなるので、字間を空けて組んだ見出し
                // （「資　料」）だけを区分見出しとみなす。
                || (compact == "資料" && t.contains(char::is_whitespace))
        }
}

/// 区分見出し・欄見出しの語か（「その他告示」「法　規　的　告　示」）。
pub(crate) fn is_section_label(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    SECTION_HEADERS.contains(&compact.as_str()) || FIELD_HEADERS.contains(&compact.as_str())
}

/// 官庁報告の中の欄見出しか（「労働」「国家試験」）。欄見出しのうち「人事異動」は官庁報告より
/// 前に載る区分（その下に褒章・皇室事項が続く）、「諸事項」は公告の欄。「地方自治事項」は官庁報告の後に
/// 区分として載り、組版の標題で分ける欄ではない。
pub(crate) fn is_report_field(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    FIELD_HEADERS.contains(&compact.as_str()) && !matches!(compact.as_str(), "人事異動" | "諸事項" | "地方自治事項")
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

/// `lines[i]` から、`〇` の見出しの無い規則の制定文（「内閣官房組織令（…）第十二条の規定に基づき、…規則を次のように
/// 定める。」）が始まるか。
///
/// 官庁報告の内閣官房の規則は見出しも号数もなく、前の規則の附則（「この規則は、…から施行する。」）のすぐ後に
/// 次の規則の制定文が続く。制定文は日付と署名の行（「令和八年七月二十九日　内閣総理大臣高市早苗」）のすぐ上で
/// 終わる。日付だけの行（最低賃金の公示の「令和８年９月29日」）は、目次が 1 項目にまとめる公示の続きなので見ない。
/// 定款の変更（「…の一部を次のように変更する。」）のように、目次が 1 項目にまとめる公告の続きも見ない。
/// 官庁報告の区分の中だけで見る（告示・省令は `〇` の見出しで分かれ、その中の 2 文目の制定文は記事の続き）。
fn starts_enactment(lines: &[&str], i: usize, prev: Option<&str>) -> bool {
    let ends = |l: &str| l.trim_end().ends_with('。') || l.trim_end().ends_with("。）");
    if !prev.is_some_and(ends) {
        return false;
    }
    let Some(end) = (i..lines.len().min(i + 6)).find(|&k| ends(lines[k])) else { return false };
    if lines[i..=end].iter().any(|l| is_section_label(l) || is_heading_mark(l.trim())) {
        return false;
    }
    let sentence: String = lines[i..=end].iter().map(|l| l.trim()).collect();
    let enacts = sentence.contains("次のように定める") || sentence.contains("次のように改正") || sentence.ends_with("を制定する。");
    let signed = lines.get(end + 1).is_some_and(|l| {
        let mut parts = l.trim().split(char::is_whitespace).filter(|p| !p.is_empty());
        parts.next().is_some_and(crate::notice_boundaries::date) && parts.next().is_some()
    });
    enacts && signed && !is_heading_mark(lines[i].trim())
}

/// 機関名だけの公示の標題の行（「北陸地方整備局公示」）か。
///
/// 号外の官庁事項では各地方整備局の公示が `〇` も番号もない標題の行で続き、前の公示の表の行や
/// 「図面縦覧場所…」の行のすぐ後に来るため、直前の行の文末では分けられない。漢字だけの行は
/// 文の折り返し（「…規定に基づき公示」＋「する。」）にはならない。欄見出し（「官庁事項」）の直後は
/// 欄見出しが既に記事を始めているので分けない。
fn is_agency_notice_title(line: &str, prev: Option<&str>) -> bool {
    let t = line.trim();
    t.chars().count() <= 30
        && t.strip_suffix("公示").is_some_and(|agency| agency.chars().count() >= 2 && agency.chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || c == '・'))
        && prev.is_some_and(|p| !is_section_label(p))
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
        // 機関名がかなで始まる見出し（「〇こども家庭庁告示第十三号」「〇デジタル庁告示第八号」）は、
        // 見出しの形（「第…号」で終わる 1 行）のときだけ。
        Some(c) if ('\u{3041}'..='\u{3096}').contains(&c) || ('\u{30A1}'..='\u{30FA}').contains(&c) => {
            let t = t.trim_end();
            t.ends_with('号') && t.contains('第') && t.chars().count() <= 40 && !t.contains(['、', '。', '「', '」'])
        }
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
    // 人事院規則は「〇」の見出しを持たず、区分見出し「規則」の後に制定文から始まる。
    "規則",
    "国会事項",
    "官庁報告",
    "皇室事項",
    "叙位・叙勲",
    "褒賞",
];

/// 官庁報告・公告の中の欄見出し（目次の小見出し）。独立行で現れたら別記事の開始とみなす。
/// 「資料」のように本文の単独行と紛れやすい語は入れない（原本の組版で確かめる）。
/// 「産業」は全頁の単独行がすべて欄見出しで、目次にも「労働」と同じ小見出しとして載る。
const FIELD_HEADERS: &[&str] = &["官庁事項", "人事異動", "労働", "産業", "法務", "国家試験", "公聴会", "諸事項", "地方自治事項"];

/// 本文の字だけで記事を分ける欄見出し。「会社その他の公告」は公告の中で諸事項などの後に組まれる欄で、
/// 目次は「会社その他」の 1 項目に載せる。原本の組版の欄見出し（`is_field_label`・`is_native_section_label`）には
/// 入れない（入れると見出しの後の公告の読み順と書式が変わる）。
const TEXT_FIELD_HEADERS: &[&str] = &["会社その他の公告"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_section_label_starts_an_article_without_circle_heading() {
        let text = "〇内閣官房令第七号\n…内閣官房令を次のように定める。\n附則\nこの内閣官房令は、公布の日から施行する。\n規則\n人事院は、国家公務員法に基づき、次の人事院規則を制定する。\n人事院規則一七－〇－一五三";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].ends_with("公布の日から施行する。"));
        assert!(arts[1].starts_with("規則\n人事院は、"));
    }

    #[test]
    fn industry_field_label_starts_an_article() {
        let text = "官庁事項\n東北地方整備局公示\n…次のとおり公示する。\n（七）図面縦覧場所東北地方整備局\n産業\n日本産業規格\n…規定に基づき公示する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].ends_with("図面縦覧場所東北地方整備局"));
        assert!(arts[1].starts_with("産業\n日本産業規格"));
    }

    #[test]
    fn segments_articles_on_circle_heading() {
        let text = "〇総務省令第七十七号\n電波法の一部を改正する省令\n〇農林水産省告示第七百六十四号\n規格を廃止する件";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("電波法"));
        assert!(arts[1].contains("規格を廃止"));
    }

    #[test]
    fn a_katakana_agency_heading_starts_its_own_article() {
        let text = "〇デジタル庁告示第八号\n給付を定める。\n附則\nこの告示は、公布の日から適用する。\n〇デジタル庁・総務省告示第三十号\n事務を定める。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[1].starts_with("〇デジタル庁・総務省告示第三十号"));
        assert!(is_heading_mark("〇カジノ管理委員会規則・国土交通省令第二号"));
        // The heading form is still required for a katakana start.
        for t in ["〇メートル以上のもの", "〇キロワット第一号、第二号", "〇センチメートル（第三号）"] {
            assert!(!is_heading_mark(t), "{t}");
        }
    }

    #[test]
    fn personnel_heading_stays_with_its_body() {
        let text = "国会事項\n質問書提出\n本文。\n人事異動\n内閣\n〇財務大臣臨時代理\n指定する。\n叙位・叙勲\n〇叙勲\n章を授ける。\n皇室事項\n行幸啓";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 4);
        assert!(arts[1].starts_with("人事異動\n内閣\n〇財務"));
        assert!(arts[2].starts_with("叙位・叙勲\n〇叙勲"));
        // 告示は号数による照合のため、従来どおり丸印の見出しから分ける。
        assert_eq!(segment_articles("告示\n〇総務省告示第一号\n本文")[1], "〇総務省告示第一号\n本文");
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
    fn regional_bureau_notice_titles_start_their_own_articles() {
        // 号外第 195 号 29 ページ: 前の公示の表の行・期日の行の後に、標題だけの行で次の公示が続く。
        let text = "官庁事項\n東北地方整備局公示\n車両制限令…定める。\n一般国道112号　寒河江市\n方向の車線に限る）\n北陸地方整備局公示\n車両制限令…定める。\n２　指定する期日　令和８年９月１日\n中部地方整備局公示\n車両制限令…指定する。\n次のとおり公示\nする。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 3, "{arts:?}");
        assert!(arts[0].starts_with("官庁事項\n東北地方整備局公示"));
        assert!(arts[1].starts_with("北陸地方整備局公示"));
        assert!(arts[2].starts_with("中部地方整備局公示") && arts[2].ends_with("する。"));
    }

    #[test]
    fn segments_on_a_promulgation_phrase_wrapped_mid_phrase() {
        // 公布文が「…政令をここ」「に公布する。」と句の途中で折り返す（号外第 178 号 6 ページ）。
        let text = "この政令は、公布の日から施行する。\n内閣総理大臣高市早苗\n更生保護制度の充実を図るための保護司法等の一部を改正する法律の施行期日を定める政令をここ\nに公布する。\n御名御璽\n政令第二百五十二号";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2, "{arts:?}");
        assert!(arts[0].ends_with("内閣総理大臣高市早苗"));
        assert!(arts[1].starts_with("更生保護制度の充実") && arts[1].ends_with("政令第二百五十二号"));
        // 句が次の行だけにあれば、その行が公布文の行。文末の行は公布文の頭ではない。
        assert!(!wraps_promulgation("内閣総理大臣高市早苗", "地方税法施行令の一部を改正する政令をここに公布する。"));
        assert!(!wraps_promulgation("この政令は、公布の日から施行する。", "をここに公布する。"));
    }

    #[test]
    fn local_government_section_starts_an_article_but_is_not_a_report_field() {
        // 「地方自治事項」は前の記事の様式の丸印の欄（号外第 1802 号の「〇日本区域の使用者を代表する地方調整委員」）に続けない。
        let text = "厚生労働大臣　上野賢一郎\n〇日本区域の使用者を代表する地方調整委員\n氏名　年齢　現職\n地方自治事項\n香川県\n選挙\n当選した。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 3, "{arts:?}");
        assert!(arts[2].starts_with("地方自治事項\n香川県"));
        assert!(!is_report_field("地方自治事項") && is_report_field("労働"));
    }

    #[test]
    fn company_notices_section_starts_an_article_after_the_previous_notice() {
        // 本紙第1799号28ページ: 企業年金基金の公告の後に「会社その他の公告」の欄が続く。
        let text = "公告\n企業年金基金変更公告\n…変更したので公告する。\n理事長山口敬\n会社その他の公告\n合併公告\n左記会社は合併して…";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2, "{arts:?}");
        assert!(arts[0].ends_with("理事長山口敬"));
        assert!(arts[1].starts_with("会社その他の公告\n合併公告"));
        assert!(!is_report_field("会社その他の公告"));
    }

    #[test]
    fn spaced_materials_heading_starts_an_article_but_a_bare_line_does_not() {
        // 本紙第1792号9ページ: 最低賃金の公示の後に、字間を空けた「資　料」の区分で国際収支状況が続く。
        let text = "労働\n最低賃金の改正決定に関する公示\n…決定したので公示する。\n資　料\n令和８年７月中国際収支状況（速報）\n項目　７月";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2, "{arts:?}");
        assert!(arts[1].starts_with("資　料\n令和８年７月中国際収支状況"));
        assert_eq!(segment_articles("⒜計画の実施の進捗状況についての情報及び\n資料\n⒝計画に関連するその他の情報").len(), 1);
    }

    #[test]
    fn an_unheaded_rule_starts_at_its_enactment_right_above_its_signed_date() {
        // 号外第169号184ページ: 内閣官房の規則が、前の規則の附則のすぐ後に制定文から続く。
        let text = "官庁報告\n官庁事項\n内閣官房組織令（…）第十二条の規定に基づき、国家情報局に調査官\nを置く規則を次のように定める。\n令和八年七月二十九日　内閣総理大臣高市早苗\n国家情報局に調査官を置く規則\n附則\n１この規則は、令和八年七月三十一日から施行する。\n内閣官房組織令（…）第十二条の規定に基づき、内閣衛星情報センター\n組織規則の一部を改正する規則を次のように定める。\n令和八年七月二十九日　内閣総理大臣高市早苗\n内閣衛星情報センター組織規則の一部を改正する規則";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 3, "{arts:?}");
        assert!(arts[1].starts_with("官庁事項\n内閣官房組織令") && arts[1].ends_with("から施行する。"));
        assert!(arts[2].starts_with("内閣官房組織令（…）第十二条の規定に基づき、内閣衛星情報センター"));
        // 日付だけの行の上の公示（目次が 1 項目にまとめる最低賃金の公示）と、定款の変更は分けない。
        let grouped = "…「１時間1095円」に改める。\n山梨労働局最低賃金公示第２号\n最低賃金法…の規定により、次のように改正する。\n令和８年９月29日\n…に改める。\n…定款の一部を次のように変更する。\n令和８年７月30日　国土交通省共済組合代表者";
        assert_eq!(segment_articles(grouped).len(), 1);
    }

    #[test]
    fn keeps_the_entries_of_a_personnel_section_in_one_article() {
        // 人事異動の欄の丸印の小見出しは欄の続き（号外第 1753 号 5 ページ）。目次はこの欄を 1 項目に載せる。
        let text = "人事異動\n内閣\n〇防衛大臣臨時代理\n指定する（七月十九日）\n〇外務大臣臨時代理\n指定する\n福岡県\n〇監査委員選任\n選任された。\n褒賞\n紺綬褒章\n〇総務省告示第百号\n告示する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 3, "{arts:?}");
        assert!(arts[0].starts_with("人事異動") && arts[0].ends_with("選任された。"));
        assert!(arts[1].starts_with("褒賞"));
        assert!(arts[2].starts_with("〇総務省告示第百号"));
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

pub(crate) fn is_field_label(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    FIELD_HEADERS.contains(&compact.as_str())
}

// These ordinary words need native masthead evidence; they are not raw lexical
// boundaries. The existing numbered-root and plain-text segmentation stay fixed.
const NATIVE_SECTION_HEADERS: &[&str] = &["資料"];
pub(crate) fn is_native_section_label(text: &str) -> bool {
    let value: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    SECTION_HEADERS.iter().chain(FIELD_HEADERS).chain(NATIVE_SECTION_HEADERS).any(|&label| label == value)
}
pub(crate) fn is_native_section_prefix(text: &str) -> bool {
    let value: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    SECTION_HEADERS.iter().chain(FIELD_HEADERS).chain(NATIVE_SECTION_HEADERS).any(|&label| label.starts_with(&value))
}
