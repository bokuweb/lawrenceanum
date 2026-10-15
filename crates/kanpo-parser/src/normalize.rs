//! 縦書き約物の正規化・私用領域(PUA)グリフ除去・ページ柱ノイズ判定。

/// 縦書き約物の正規化 + ページ柱ノイズ除去をテキスト全体に適用する。
///
/// 行単位に約物正規化・PUA 除去・柱の剥がしを行い、連続する空行を 1 行に畳む。
pub fn normalize_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line: String = line.chars().filter_map(text_char).collect();
        let stripped = strip_margin_lead(&line);
        if is_margin_noise(&stripped) {
            continue;
        }
        lines.push(stripped.trim_end().to_string());
    }
    // 連続する空行を 1 行に畳む。
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for l in lines {
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        out.push(l);
    }
    out.join("\n").trim().to_string()
}

/// 私用領域(PUA)の文字か。官報 PDF は傍線・下線などをフォント固有グリフ(U+E000–
/// U+F8FF)で埋め込むことがあり、テキストとしては意味を持たないため除去する。
pub(crate) fn is_private_use(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

/// 語（`<word>`）の文字列を本文の文字列にする。[`text_char`] と同じだが、「－」（U+E61C）だけの
/// 語は落とす。表の金額欄の「－」（金額が無い印）のように単独で置かれたものを字にすると、横に
/// 並んだ表の間の溝をふさいで列の切り分けが変わるため、語の中で他の字に続くものだけを戻す。
/// 小数点・桁区切り（U+EA74・U+EA75）は単独の語でも戻す（前の数字に重ねて置かれ、語が分かれる）。
pub(crate) fn word_text(raw: &str) -> String {
    if !raw.is_empty() && raw.chars().all(|c| c == '\u{E61C}' || (is_private_use(c) && private_use_char(c).is_none())) {
        return String::new();
    }
    let mut out = String::new();
    for c in raw.chars() {
        match identity_cid(c) {
            Some(cid) => match cid_glyph(&[cid_char(cid)], 1.0, 1.0, true) {
                Some(g) => out.push_str(&g.text),
                None => out.extend(text_char(cid_char(cid))),
            },
            None => out.extend(text_char(c)),
        }
    }
    out
}

/// kanpo-pdf が ToUnicode の無い `Identity` の CID フォントの字を置く私用領域（U+F0000 + CID）の字なら、その CID。
fn identity_cid(c: char) -> Option<u32> {
    (c as u32).checked_sub(kanpo_pdf::bbox::IDENTITY_CID_BASE).filter(|&cid| cid <= 0xFFFD)
}

/// CID をそのまま符号位置とみなした字（poppler と同じ。[`cid_glyph`] の表はこの符号で引く）。
fn cid_char(cid: u32) -> char {
    char::from_u32(cid).unwrap_or('\u{FFFD}')
}

/// 語の 1 文字を本文の文字にする。約物を正規化し、私用領域の文字は意味の分かるもの
/// （[`private_use_char`]）だけを戻して、ほかは落とす。
pub(crate) fn text_char(c: char) -> Option<char> {
    if is_private_use(c) {
        return private_use_char(c);
    }
    Some(normalize_char(c))
}

/// ToUnicode で私用領域に写された、意味の分かる文字。横書きの数字の小数点・桁区切りが
/// DF 平成明朝（`DFHSMinchoRPro6N-W3`、CID 26064・26065）で U+EA74・U+EA75 に写されている
/// （「433.795MHz」「2,760,000円」）。蓄積した官報の 1,400 字余りがすべて数字の間に出る。
/// U+E61C（CID 24952）は全角の「－」（型式「５ＢＡ－ＢＡ１」、項番号「３－１－５」）。
/// 原本の字形で確かめた。英数字の間に出る。
fn private_use_char(c: char) -> Option<char> {
    match c {
        '\u{EA74}' => Some('.'),
        '\u{EA75}' => Some(','),
        '\u{E61C}' => Some('－'),
        _ => None,
    }
}

/// ToUnicode を持たない私用 CID（`Adobe-Npb1` 等）のグリフを本来の文字へ戻した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CidGlyph {
    /// 連番の系統（同じページに同じ系統の記号が出ているかの判定に使う）。
    pub family: u8,
    /// 常用漢字と同じ符号か。字間を空けて組んだ名前・見出しの 1 字（「張　振昊」の
    /// `振`、「係　数」の `数`）と区別がつかないので、呼び出し側が文脈で判断する。
    pub ambiguous: bool,
    pub text: String,
}

/// 系統の番号。
const FAM_PAREN_NUM: u8 = 1;
const FAM_CIRCLED: u8 = 2;
const FAM_ROMAN: u8 = 3;
const FAM_PAREN_KANJI: u8 = 4;
const FAM_PAREN_KANA: u8 = 5;
const FAM_SINGLE: u8 = 6;

/// 連番の範囲の中で常用漢字と同じ符号（文脈が無ければ写像しない）。
const AMBIGUOUS: &str = "挑振挺挾挿掘掛掌故效敍敏敕敗敘教敝敞敢散敦敬";

/// ToUnicode を持たない私用 CID のグリフを本来の文字へ戻す。
///
/// 官報の表や条文中の項目記号（(1)・①・(i)・㈠・(ア)）、化学名のハイフン、縦組み数字の
/// 小数点、人名・地名の異体字は、ToUnicode 無しの別フォントで組まれることがあり、poppler は
/// CID をそのまま符号位置とみなして無関係な漢字を出す（例: (1)→`挎`、(15)→`昝昫`、
/// ①→`挭`、㈠→`掴`、(ア)→`敁`、‐→`慺`）。フォント切替で独立した 1 語になり、1em 四方に
/// 収まる（2 桁は半角 2 グリフで 1em）ことを条件にする。続けて組まれて 1 語になったもの
/// （「（３９）(i)」）は、記号の数だけの高さまで許す。
///
/// 対応は蓄積した官報の原本画像と突き合わせて確かめたもの。連番の範囲は確認できた
/// 符号の並びから端まで延ばしている。
///
/// kanpo-pdf の出力では、ToUnicode の無い `Identity` のフォントの字が私用領域（U+F0000 + CID）に
/// 分けてあるので、字の大きさや前後の語を見ずに写像し、常用漢字と同じ符号や、目次のページ番号の
/// 数字（`掠`→一、`採`→二 …）・異体字も写像する（[`identity_only_glyph`]）。poppler の出力では
/// 本物の漢字と区別できないので、これらは扱わない（目次のページ番号はページの中の並びで見る
/// [`toc_page_numbers`]）。
pub(crate) fn private_cid_glyph(text: &str, width: f32, height: f32) -> Option<CidGlyph> {
    let chars: Vec<char> = text.trim().chars().collect();
    // kanpo-pdf の出力では、ToUnicode の無いフォントの字が私用領域に分けてあるので紛れが無い。
    if !chars.is_empty() && chars.iter().all(|&c| identity_cid(c).is_some()) {
        let chars: Vec<char> = chars.iter().map(|&c| identity_cid(c).map_or(c, cid_char)).collect();
        return cid_glyph(&chars, width, height, true);
    }
    cid_glyph(&chars, width, height, false)
}

/// [`private_cid_glyph`] の本体。`sure` なら字が ToUnicode の無いフォントのものと分かっている
/// （常用漢字と同じ符号も写像し、字の大きさも問わない）。
fn cid_glyph(chars: &[char], width: f32, height: f32, sure: bool) -> Option<CidGlyph> {
    if !sure && chars.len() < 3 && height > width * 1.3 {
        return None;
    }
    let fw = |n: u32| -> String { n.to_string().chars().map(|d| char::from_u32('０' as u32 + d.to_digit(10).unwrap()).unwrap()).collect() };
    let nth = |s: &str, i: u32| s.chars().nth(i as usize);
    let (family, mapped) = match chars {
        // 目次のページ番号の数字（〇〜九。半角の字を縦に重ねて 1 em に 2 桁）
        _ if sure && chars.iter().all(|c| ('\u{639F}'..='\u{63A8}').contains(c)) => {
            (FAM_SINGLE, chars.iter().filter_map(|&c| nth("〇一二三四五六七八九", c as u32 - 0x639F)).collect())
        }
        [c] if sure && identity_only_glyph(*c as u32).is_some() => identity_only_glyph(*c as u32)?,
        [c] => {
            let code = *c as u32;
            match code {
                // (1)〜(10)
                0x630E..=0x6317 => (FAM_PAREN_NUM, format!("（{}）", fw(code - 0x630D))),
                // ①〜⑳
                0x632D..=0x6340 => (FAM_CIRCLED, char::from_u32(0x2460 + code - 0x632D)?.to_string()),
                // (i)〜(x)
                0x6395..=0x639E => {
                    const ROMAN: [&str; 10] = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];
                    (FAM_ROMAN, format!("（{}）", ROMAN[(code - 0x6395) as usize]))
                }
                // ㈠〜㈩
                0x63B4..=0x63BD => (FAM_PAREN_KANJI, format!("（{}）", nth("一二三四五六七八九十", code - 0x63B4)?)),
                // (ア)〜(ン)（五十音順）
                0x6541..=0x656E => (
                    FAM_PAREN_KANA,
                    format!(
                        "（{}）",
                        nth("アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン", code - 0x6541)?
                    ),
                ),
                // 縦組み数字の小数点
                0x65E0 => (FAM_SINGLE, "・".to_string()),
                // 化学名などのハイフン
                0x617A => (FAM_SINGLE, "－".to_string()),
                // ㊟（「注４を注５とし」の注）
                0x629D => (FAM_SINGLE, "（注）".to_string()),
                // (Ⅰ)・(Ⅱ)（「在宅患者訪問診療料（Ⅰ）及び（Ⅱ）」）
                0x638B => (FAM_SINGLE, "（Ⅰ）".to_string()),
                0x638C => (FAM_SINGLE, "（Ⅱ）".to_string()),
                // 異体字（一点しんにょうの「辻」・「釜」、地名・人名・医療用語の字）
                0x5A79 => (FAM_SINGLE, "辻".to_string()),
                0x4F42 => (FAM_SINGLE, "釜".to_string()),
                0x4F2E => (FAM_SINGLE, "灸".to_string()),
                0x4F37 => (FAM_SINGLE, "梗".to_string()),
                0x4F3D => (FAM_SINGLE, "竈".to_string()),
                0x4F41 => (FAM_SINGLE, "斧".to_string()),
                0x4F45 => (FAM_SINGLE, "籾".to_string()),
                0x5A31 => (FAM_SINGLE, "葛".to_string()),
                0x5A52 => (FAM_SINGLE, "薩".to_string()),
                0x5A7B => (FAM_SINGLE, "鄭".to_string()),
                0x5A91 => (FAM_SINGLE, "樋".to_string()),
                0x5B3D => (FAM_SINGLE, "德".to_string()),
                _ => return None,
            }
        }
        // (11)〜(99): 「(十の位」U+661D..=U+6625 +「一の位)」U+6627..=U+6630（1..9,0 の順）
        [t, o] if ('\u{661D}'..='\u{6625}').contains(t) && ('\u{6627}'..='\u{6630}').contains(o) => {
            let tens = *t as u32 - 0x661C;
            let ones = (*o as u32 - 0x6626) % 10;
            (FAM_PAREN_NUM, format!("（{}）", fw(tens * 10 + ones)))
        }
        // 続けて組まれて 1 語になったもの（「（３９）(i)」）。常用漢字と同じ符号だけの語は除く。
        _ if chars.len() >= 3 => {
            let (mut out, mut known, mut i, mut tokens) = (String::new(), sure, 0, 0);
            while i < chars.len() {
                let pair = chars.get(i..i + 2).map(|p| p.iter().collect::<String>());
                let pair: Option<Vec<char>> = pair.map(|p| p.chars().collect());
                let (g, n) = match pair.and_then(|p| cid_glyph(&p, width, width, sure)) {
                    Some(g) => (g, 2),
                    None => (cid_glyph(&chars[i..i + 1], width, width, sure)?, 1),
                };
                known |= !g.ambiguous;
                out.push_str(&g.text);
                i += n;
                tokens += 1;
            }
            if !known || (!sure && height > width * 1.3 * tokens as f32) {
                return None;
            }
            (FAM_PAREN_NUM, out)
        }
        _ => return None,
    };
    Some(CidGlyph {
        family,
        ambiguous: !sure && chars.len() == 1 && AMBIGUOUS.contains(chars[0]),
        text: mapped,
    })
}

/// ToUnicode の無いフォントの字と分かっているときだけ写像するもの（常用漢字と同じ符号を含む）。
/// 原本の字形で確かめた。
fn identity_only_glyph(code: u32) -> Option<(u8, String)> {
    let nth = |s: &str, i: u32| s.chars().nth(i as usize).map(String::from);
    let text = match code {
        // (あ)〜(わ)（五十音順。(と)・(ぬ)・(る) で確かめた）
        0x650E..=0x6538 => {
            let kana = nth("あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわ", code - 0x650E)?;
            return Some((FAM_PAREN_KANA, format!("（{kana}）")));
        }
        // ㈩ に続く (十一)〜(十五)
        0x63BF..=0x63C3 => return Some((FAM_PAREN_KANJI, format!("（十{}）", nth("一二三四五", code - 0x63BF)?))),
        // 化学名のプライム（「N・N′－」）
        0x65C9 => "′",
        // 2 行にまたがる大きな括弧の上半分・下半分（「（自令和七年四月一日／至令和八年三月三十一日）」）
        0x6645 | 0x6646 => "（",
        0x6647 | 0x6648 => "）",
        // 複数行にまたがる大きな括弧の部品・飾り罫（字ではない）
        0x62FC..=0x62FF | 0x6699..=0x669F => "",
        // 決算公告の負の数の印（縦書き用の字形で、縦組みでは横倒しの ▷ に見える）
        0x624D => "△",
        // 人名・地名の異体字・簡体字など
        0x41BE => "婷",
        0x41FC => "暻",
        0x4306 => "鄔",
        0x4322 => "雯",
        0x4473 => "宁",
        0x4B17 => "吴",
        0x4F34 => "鍵",
        0x4F37 => "梗",
        0x4F38 => "鮫",
        0x4F3A => "杖",
        0x4F45 => "籾",
        0x52C7 => "刘",
        0x5305 => "启",
        0x55AD => "煒",
        0x5A37 => "祇",
        0x5A53 => "鯖",
        0x5A70 => "蛸",
        0x5A7B => "鄭",
        0x5B12 | 0x5B3D => "德",
        _ => return None,
    };
    Some((FAM_SINGLE, text.to_string()))
}

/// 目次のページ番号の数字の私用 CID（U+639F `掟`→〇、U+63A0 `掠`→一 … U+63A8 `推`→九）。
const TOC_DIGIT_ZERO: u32 = 0x639F;

/// 目次のページ番号（ToUnicode の無い数字のフォント）の語を見つけ、漢数字（「一三」）にした文字列を返す。
/// `words` は語の `(x0, y0, x1, y1, 字)`。戻り値は語ごと（ページ番号でなければ `None`）。
///
/// ページ番号の数字は、私用 CID で連番の漢字（`掠`・`採`・`探`・`掣`・`掤`・`接`・`掦`・`控`・`推`・`掟`）に
/// 化ける。どれも常用漢字なので、字だけでは本文の字と区別できない。目次ではページ番号を
/// 項目の列の下端にそろえて縦に組み、2 桁以上は 1 字 0.75em 以下に詰める。そこで、この 10 字だけの語の
/// うち、2 字以上を 1 字 0.85em 未満に詰めたもの、または同じ大きさの同じ 10 字だけの語と下端がそろうもの
/// （ほかの項目のページ番号）を、ページ番号とする（蓄積した官報 59 日分の目次で、項目の掲載ページと
/// 突き合わせて確かめた。項目が 1 つだけの目次の「一」は並ぶ語が無いので写像しない）。
/// kanpo-pdf の出力（私用領域に分けた字）は [`private_cid_glyph`] が紛れなく写像するので、これは
/// poppler・以前の抽出器の出力（R2 に蓄積した座標付きテキスト）のためのもの。
pub(crate) fn toc_page_numbers(words: &[(f32, f32, f32, f32, String)]) -> Vec<Option<String>> {
    let digit = |c: char| (TOC_DIGIT_ZERO..=TOC_DIGIT_ZERO + 9).contains(&(c as u32));
    let digits: Vec<usize> = (0..words.len())
        .filter(|&i| {
            let t = words[i].4.trim();
            !t.is_empty() && t.chars().all(digit)
        })
        .collect();
    let mut out = vec![None; words.len()];
    for &i in &digits {
        let (x, y, x1, y1, t) = &words[i];
        let (w, h, n) = (x1 - x, y1 - y, t.trim().chars().count());
        let packed = n >= 2 && h < n as f32 * w * 0.85;
        let aligned = digits.iter().any(|&j| {
            let (u, _, u1, v1, _) = &words[j];
            j != i && (v1 - y1).abs() < 1.0 && ((u1 - u) - w).abs() < 0.5
        });
        if packed || aligned {
            out[i] = Some(t.trim().chars().map(|c| "〇一二三四五六七八九".chars().nth((c as u32 - TOC_DIGIT_ZERO) as usize).unwrap()).collect());
        }
    }
    out
}

/// 文脈なしで写像してよいものだけを写像する（常用漢字と同じ符号は `None`）。
#[cfg(test)]
fn fix_private_cid_glyphs(text: &str, width: f32, height: f32) -> Option<String> {
    private_cid_glyph(text, width, height).filter(|g| !g.ambiguous).map(|g| g.text)
}

/// 縦書き presentation form を通常の全角約物に写像する。
pub(crate) fn normalize_char(c: char) -> char {
    match c {
        '\u{FE10}' => '，',
        '\u{FE11}' => '、',
        '\u{FE12}' => '。',
        '\u{FE13}' => '：',
        '\u{FE14}' => '；',
        '\u{FE15}' => '！',
        '\u{FE16}' => '？',
        '\u{FE17}' => '〖',
        '\u{FE18}' => '〗',
        '\u{FE19}' => '…',
        '\u{FE31}' => '—',
        '\u{FE32}' => '–',
        '\u{FE33}' | '\u{FE34}' => '｜',
        '\u{FE35}' => '（',
        '\u{FE36}' => '）',
        '\u{FE37}' => '｛',
        '\u{FE38}' => '｝',
        '\u{FE39}' => '〔',
        '\u{FE3A}' => '〕',
        '\u{FE3B}' => '【',
        '\u{FE3C}' => '】',
        '\u{FE3D}' => '《',
        '\u{FE3E}' => '》',
        '\u{FE3F}' => '〈',
        '\u{FE40}' => '〉',
        '\u{FE41}' => '「',
        '\u{FE42}' => '」',
        '\u{FE43}' => '『',
        '\u{FE44}' => '』',
        '\u{FE47}' => '〔',
        '\u{FE48}' => '〕',
        other => identity_cid(other).map_or(other, cid_char),
    }
}

/// 行頭にぽつんと現れるページ柱の 1 文字（「官」「報」）を、後続が大きな空白で
/// 区切られている場合に限り除去する。
fn strip_margin_lead(line: &str) -> String {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        if matches!(first, '官' | '報') {
            let rest = chars.as_str();
            // 1 文字 + 連続空白(2 個以上) + 本文、という柱パターンのみ剥がす。
            if rest.starts_with("  ") {
                return rest.trim_start().to_string();
            }
            if rest.trim().is_empty() {
                return String::new();
            }
        }
    }
    line.to_string()
}

/// ページ柱（発行日・曜日・号数の余白テキスト）と思われる行か。
pub(crate) fn is_margin_noise(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return false; // 空行は畳み処理に任せる。
    }
    // 数字（半角・全角）を除いた骨格で判定（「令和  ８  年…」等の数字ゆらぎを吸収）。
    let skeleton: String = compact
        .chars()
        .filter(|c| !c.is_ascii_digit() && !('０'..='９').contains(c))
        .collect();
    const NOISE: &[&str] = &[
        "官",
        "報",
        "令和年月日",
        "平成年月日",
        "号外第号",
        "（号外第号）",
        "(号外第号)",
        "月曜日",
        "火曜日",
        "水曜日",
        "木曜日",
        "金曜日",
        "土曜日",
        "日曜日",
    ];
    NOISE.contains(&skeleton.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_variant_glyphs_checked_against_the_originals() {
        // 原本の画像と突き合わせた異体字（宮城県塩竈市・心筋梗塞・鍼灸・奈良県葛城市・鹿児島県薩摩郡・樋口）。
        for (cid, want) in [("伽", "竈"), ("伷", "梗"), ("伮", "灸"), ("佁", "斧"), ("佅", "籾"), ("娱", "葛"), ("婒", "薩"), ("婻", "鄭"), ("媑", "樋"), ("嬽", "德")] {
            assert_eq!(fix_private_cid_glyphs(cid, 9.0, 9.0).as_deref(), Some(want), "{cid}");
        }
        assert_eq!(fix_private_cid_glyphs("掋", 9.0, 9.0).as_deref(), Some("（Ⅰ）"));
        // 「掌」は常用漢字なので文脈なしでは写像しない。
        assert_eq!(fix_private_cid_glyphs("掌", 9.0, 9.0), None);
        assert_eq!(private_cid_glyph("掌", 9.0, 9.0).map(|g| g.text).as_deref(), Some("（Ⅱ）"));
    }

    #[test]
    fn maps_toc_page_numbers_by_their_placement() {
        // 号外第 220 号の目次（項目の掲載ページ 1・13・14・66・72）。下端がそろい、2 桁は 1 字 0.75em。
        let w = |x: f32, y: f32, y1: f32, t: &str| (x, y, x + 9.0, y1, t.to_string());
        let words = vec![
            w(327.7, 221.1, 230.1, "掠"),
            w(191.1, 216.6, 230.1, "掠探"),
            w(85.7, 216.6, 230.1, "接接"),
            w(50.5, 216.6, 230.1, "掦採"),
            // 本文の字（下端がそろわない 1 字）はそのまま。
            w(300.0, 400.0, 409.0, "接"),
        ];
        let got = toc_page_numbers(&words);
        assert_eq!(got, vec![Some("一".into()), Some("一三".into()), Some("六六".into()), Some("七二".into()), None]);
        // 1 字の語が 1 つだけなら写像しない（字間を空けた見出しの「接」と区別できない）。
        assert_eq!(toc_page_numbers(&[w(327.7, 221.1, 230.1, "掠")]), vec![None]);
        // 3 桁（1 字 0.67em）は 1 つでも写像する。掟は〇。
        assert_eq!(toc_page_numbers(&[w(489.5, 212.1, 230.1, "掣掠掟")]), vec![Some("四一〇".into())]);
    }

    #[test]
    fn maps_private_use_decimal_point_and_thousands_separator() {
        // 数字の小数点・桁区切りは私用領域に写されている。ほかの私用領域の字（傍線など）は落とす。
        assert_eq!(normalize_text("433\u{EA74}795MHz"), "433.795MHz");
        assert_eq!(normalize_text("５ＢＡ\u{E61C}ＢＡ１"), "５ＢＡ－ＢＡ１");
        assert_eq!(word_text("５ＢＡ\u{E61C}ＢＡ１"), "５ＢＡ－ＢＡ１");
        // 単独で置かれた私用領域の字（表の金額欄の「－」など）の語は落とす。
        assert_eq!(word_text("\u{E61C}"), "");
        assert_eq!(word_text("\u{EA75}"), ",");
        assert_eq!(normalize_text("2\u{EA75}760\u{EA75}000円"), "2,760,000円");
        assert_eq!(normalize_text("第\u{E005}\u{E005}条"), "第条");
    }

    #[test]
    fn normalizes_vertical_punctuation() {
        let raw = "\u{FE35}施行\u{FE36}\u{FE12}";
        assert_eq!(normalize_text(raw), "（施行）。");
    }

    #[test]
    fn drops_margin_noise_lines() {
        let raw = "本則を次のように改める\n官\n令和 ８ 年 ６ 月 15 日\n第六条";
        let out = normalize_text(raw);
        assert!(out.contains("本則を次のように改める"));
        assert!(out.contains("第六条"));
        assert!(!out.contains("令和"));
    }

    #[test]
    fn restores_private_cid_numbered_items() {
        // 1em 四方の独立語だけを写像する。
        assert_eq!(fix_private_cid_glyphs("挎", 8.0, 8.0).as_deref(), Some("（１）"));
        assert_eq!(fix_private_cid_glyphs("挗", 8.0, 8.0).as_deref(), Some("（１０）"));
        assert_eq!(fix_private_cid_glyphs("昝昫", 8.0, 8.0).as_deref(), Some("（１５）"));
        assert_eq!(fix_private_cid_glyphs("昡昰", 8.0, 8.0).as_deref(), Some("（５０）"));
        assert_eq!(fix_private_cid_glyphs("慺", 8.0, 8.0).as_deref(), Some("－"));
        assert_eq!(fix_private_cid_glyphs("挭", 8.0, 8.0).as_deref(), Some("①"));
        // 続けて組まれた「（３９）(i)」「（４０）(vii)」（(vii) は常用漢字「掛」と同じ符号）。
        assert_eq!(fix_private_cid_glyphs("星是掕", 8.0, 16.0).as_deref(), Some("（３９）（i）"));
        assert_eq!(fix_private_cid_glyphs("映昰掛", 8.0, 16.0).as_deref(), Some("（４０）（vii）"));
        assert_eq!(fix_private_cid_glyphs("掛挑振", 8.0, 24.0), None);
        assert_eq!(fix_private_cid_glyphs("星是掕", 8.0, 40.0), None);
        // 常用漢字と同じ符号は文脈が無ければ写像しない（呼び出し側が判断する）。
        assert_eq!(fix_private_cid_glyphs("振", 8.0, 8.0), None);
        assert_eq!(private_cid_glyph("振", 8.0, 8.0).map(|g| g.text).as_deref(), Some("③"));
        assert_eq!(fix_private_cid_glyphs("敫", 8.0, 8.0).as_deref(), Some("（ロ）"));
        assert_eq!(fix_private_cid_glyphs("掝", 8.0, 8.0).as_deref(), Some("（ix）"));
        assert_eq!(fix_private_cid_glyphs("掴", 8.0, 8.0).as_deref(), Some("（一）"));
        assert_eq!(fix_private_cid_glyphs("敁", 8.0, 8.0).as_deref(), Some("（ア）"));
        assert_eq!(fix_private_cid_glyphs("无", 8.0, 8.0).as_deref(), Some("・"));
        // 語の中の「振」（通常の本文）は触らない。
        assert_eq!(fix_private_cid_glyphs("振替", 8.0, 16.0), None);
        // 通常の本文（2 文字で 2em の高さ・常用漢字の語）は触らない。
        assert_eq!(fix_private_cid_glyphs("星昭", 8.0, 16.0), None);
        assert_eq!(fix_private_cid_glyphs("昭和", 8.0, 8.0), None);
    }

    /// kanpo-pdf が ToUnicode の無い Identity のフォントの字を置く私用領域の字。
    fn tagged(cid: u32) -> String {
        cid_char(kanpo_pdf::bbox::IDENTITY_CID_BASE + cid).to_string()
    }

    #[test]
    fn decodes_glyphs_tagged_as_unmapped_identity_font() {
        let word = |cids: &[u32]| cids.iter().map(|&c| tagged(c)).collect::<String>();
        let glyph = |cids: &[u32], w: f32, h: f32| private_cid_glyph(&word(cids), w, h).map(|g| g.text);
        // 目次のページ番号の数字（半角の字を縦に重ねた「一〇」は 1 em より高い）。
        assert_eq!(glyph(&[25505], 9.0, 9.0).as_deref(), Some("二"));
        assert_eq!(glyph(&[25504, 25503], 9.0, 13.5).as_deref(), Some("一〇"));
        // 常用漢字と同じ符号（`振`・`故`）も、タグが付いていれば紛れない。
        assert_eq!(glyph(&[25391], 8.0, 8.0).as_deref(), Some("③"));
        assert_eq!(glyph(&[25925], 8.0, 8.0).as_deref(), Some("（オ）"));
        assert_eq!(glyph(&[25889], 8.0, 8.0).as_deref(), Some("（と）"));
        assert_eq!(glyph(&[25539], 8.0, 8.0).as_deref(), Some("（十五）"));
        // 異体字（CID 20280 は poppler では「伸」になる）。
        assert_eq!(word_text(&word(&[20280])), "鮫");
        assert_eq!(word_text(&word(&[24954])), "－");
        // 飾り罫は落とす。対応の分からない字は poppler と同じ字にする。
        assert_eq!(word_text(&word(&[26265, 26267])), "");
        assert_eq!(word_text(&word(&[0x6682])), "\u{6682}");
        // poppler の出力で原本と突き合わせた異体字は、タグが付いていても同じ字にする（鍼灸）。
        assert_eq!(word_text(&word(&[20270])), "灸");
        // タグの無い（poppler の）入力では、常用漢字と同じ符号は写像しない。
        assert_eq!(fix_private_cid_glyphs("伸", 8.0, 8.0), None);
        assert_eq!(fix_private_cid_glyphs("採", 9.0, 9.0), None);
        assert_eq!(word_text("伸"), "伸");
    }

    #[test]
    fn strips_private_use_glyphs() {
        // U+E0A8 等の PUA(傍線グリフ)は除去される。
        let raw = "本文\u{E0A8}\u{E0A8}\u{E0A8}\n令和八年";
        let out = normalize_text(raw);
        assert!(!out.contains('\u{E0A8}'));
        assert!(out.contains("本文"));
        assert!(out.contains("令和八年"));
    }
}
