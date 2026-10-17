//! 行の書き直しの候補。どれも行の字（空白を除く）を入れ替えるだけで、字を足したり消したりはしない。

/// 書き直しの候補。
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub text: String,
    /// 書き直しの種類。
    pub rule: &'static str,
}

/// 縦中横・横組みの欄で枠がずれて出やすい単位（kanpo-parser の縦中横の単位と同じ）。
const UNITS: [&str; 22] =
    ["Hz", "kHz", "MHz", "GHz", "THz", "mW", "kW", "dB", "dBm", "dBi", "mA", "kV", "nm", "mm", "cm", "km", "kg", "mg", "mL", "ml", "ppm", "μm"];

/// 欄の空白（全角の空白）で区切った片の数の上限（それより多い行は表の行とみなして並べ替えない）。
const MAX_SEGMENTS: usize = 6;

/// 行の書き直しの候補を作る（元の行と同じものは含めない）。
///
/// - `unit-gap`: 単位をいったん全部抜き、数の直後に残った空白（単位が抜けた跡）を前から単位で埋める
///   （「四三三・六七　を超え四三四・一七MHz　以下…MHz」→「四三三・六七MHzを超え四三四・一七MHz以下…」）
/// - `unit-move`: 単位 1 つを、数の直後の別の位置へ移す
/// - `segments`: 全角の空白で区切った片を回す・逆に並べる・隣どうしを入れ替える
pub fn candidates(line: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut push = |text: String, rule: &'static str| {
        if text != line && !out.iter().any(|c| c.text == text) {
            out.push(Candidate { text, rule });
        }
    };
    for (text, rule) in unit_candidates(line) {
        push(text, rule);
    }
    let segments: Vec<&str> = line.split('\u{3000}').filter(|s| !s.is_empty()).collect();
    if (2..=MAX_SEGMENTS).contains(&segments.len()) {
        let n = segments.len();
        for r in 1..n {
            let rotated: Vec<&str> = segments[r..].iter().chain(&segments[..r]).copied().collect();
            push(rotated.join("\u{3000}"), "segments");
        }
        let reversed: Vec<&str> = segments.iter().rev().copied().collect();
        push(reversed.join("\u{3000}"), "segments");
        for i in 0..n - 1 {
            let mut swapped = segments.clone();
            swapped.swap(i, i + 1);
            push(swapped.join("\u{3000}"), "segments");
        }
    }
    out
}

/// 単位が後ろに続きうる字（数・括弧閉じ・記号の文字）。
fn before_unit(c: char) -> bool {
    c.is_ascii_alphanumeric() || ('０'..='９').contains(&c) || ('ａ'..='ｚ').contains(&c) || "〇一二三四五六七八九十百千・．.）)".contains(c)
}

/// 行の中の単位の (バイト位置, 単位)。長い単位から見る（「MHz」を「Hz」と取らない）。前後が英字の所
/// （「Hzの」は単位、「Hzone」は単位でない）は除く。
fn units_in(line: &str) -> Vec<(usize, &'static str)> {
    let mut sorted = UNITS;
    sorted.sort_by_key(|u| std::cmp::Reverse(u.len()));
    let mut found: Vec<(usize, &'static str)> = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        let prev_alpha = line[..i].chars().next_back().is_some_and(|c| c.is_ascii_alphabetic());
        let hit = sorted.iter().find(|u| {
            rest.starts_with(**u) && !prev_alpha && !rest[u.len()..].chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        });
        match hit {
            Some(u) => {
                found.push((i, u));
                i += u.len();
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    found
}

fn unit_candidates(line: &str) -> Vec<(String, &'static str)> {
    let units = units_in(line);
    let Some(&(_, unit)) = units.first() else { return Vec::new() };
    // 同じ単位だけの行（「MHz」と「kHz」が混ざる行は動かさない）。
    if units.iter().any(|&(_, u)| u != unit) {
        return Vec::new();
    }
    let mut out = Vec::new();
    // 単位を抜いた行。
    let mut stripped = String::new();
    let mut at = 0;
    for &(i, u) in &units {
        stripped.push_str(&line[at..i]);
        at = i + u.len();
    }
    stripped.push_str(&line[at..]);
    // unit-gap: 数の直後の空白を前から単位で埋める（空白の数と単位の数が同じとき）。
    let gaps: Vec<usize> = stripped
        .char_indices()
        .filter(|&(i, c)| c == '\u{3000}' && stripped[..i].chars().next_back().is_some_and(before_unit))
        .map(|(i, _)| i)
        .collect();
    // 空白が単位より多い（行頭の項目番号の後ろの空白「４　第一項…」も数の直後）ときは、単位の数だけの空白の
    // 組をどれも試す（組の数が多すぎる行は試さない）。
    for chosen in combinations(gaps.len(), units.len()).into_iter().take(MAX_GAP_SETS) {
        let mut filled = String::new();
        let mut at = 0;
        for &k in &chosen {
            let g = gaps[k];
            filled.push_str(&stripped[at..g]);
            filled.push_str(unit);
            at = g + '\u{3000}'.len_utf8();
        }
        filled.push_str(&stripped[at..]);
        out.push((filled, "unit-gap"));
    }
    // unit-move: 単位 1 つを、数の直後（次の字が数でも単位でもない所）へ移す。
    for &(i, u) in &units {
        let without = format!("{}{}", &line[..i], &line[i + u.len()..]);
        for (j, c) in without.char_indices() {
            let end = j + c.len_utf8();
            let next = without[end..].chars().next();
            if before_unit(c) && !c.is_ascii_alphabetic() && !next.is_some_and(before_unit) && end != i {
                out.push((format!("{}{}{}", &without[..end], u, &without[end..]), "unit-move"));
            }
        }
    }
    out
}

/// `unit-gap` で試す空白の組の数の上限。
const MAX_GAP_SETS: usize = 20;

/// 0..n から k 個を選ぶ組（小さい順）。k が n より大きければ無し。
fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    if k == 0 || k > n {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.clone());
        if out.len() > MAX_GAP_SETS {
            return out;
        }
        let Some(i) = (0..k).rev().find(|&i| idx[i] < n - k + i) else { return out };
        idx[i] += 1;
        for j in i + 1..k {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_gap_restores_shifted_units() {
        let bad = "ロ　四三三・六七　を超え四三四・一七MHz　以下の周波数の電波を使用する国際輸送用データMHz";
        let good = "ロ　四三三・六七MHzを超え四三四・一七MHz以下の周波数の電波を使用する国際輸送用データ";
        assert!(candidates(bad).iter().any(|c| c.text == good && c.rule == "unit-gap"));
        let bad = "（一、〇〇〇×ｎ）　離れた周波数の（±）（五〇〇×ｎ）kHz　の帯域内に輻射される平均電力）kHz";
        let good = "（一、〇〇〇×ｎ）kHz離れた周波数の（±）（五〇〇×ｎ）kHzの帯域内に輻射される平均電力）";
        assert!(candidates(bad).iter().any(|c| c.text == good));
    }

    #[test]
    fn unit_gap_chooses_among_extra_gaps() {
        let bad = "４　第一項の無線設備の許容値が一・四　のものMHz";
        let good = "４　第一項の無線設備の許容値が一・四MHzのもの";
        assert!(candidates(bad).iter().any(|c| c.text == good && c.rule == "unit-gap"));
        assert_eq!(combinations(3, 2), vec![vec![0, 1], vec![0, 2], vec![1, 2]]);
        assert!(combinations(1, 2).is_empty());
    }

    #[test]
    fn units_are_whole_tokens() {
        assert_eq!(units_in("一MHzと二Hz"), vec![(3, "MHz"), (12, "Hz")]);
        assert!(units_in("Style　Trust").is_empty());
    }

    #[test]
    fn segments_are_permuted() {
        let c = candidates("甲　乙　丙");
        let texts: Vec<&str> = c.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"乙　丙　甲"));
        assert!(texts.contains(&"丙　乙　甲"));
        assert!(texts.contains(&"乙　甲　丙"));
        assert!(!texts.contains(&"甲　乙　丙"));
    }

    #[test]
    fn long_rows_are_not_permuted() {
        assert!(candidates("一　二　三　四　五　六　七").is_empty());
    }
}
