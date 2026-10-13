//! 横組みページの読み順復元。
//!
//! 官報には `/Rotate 270` で横向きに置かれ、本文を横書きで組んだページがある（官庁報告
//! 「労働」欄の最低賃金公示、公告「諸事項」など）。`pdftotext -bbox-layout` はこのページを
//! 回転後の座標（横長、x≈0–842 / y≈0–595）で出すが、`<page>` の幅・高さは回転前の
//! 595×842 のままなので、ページ寸法では判別できない。柱の位置で判定する（[`is_left_strip_header`]）。
//!
//! 縦書き本紙の段が、回転により左から右へ並ぶ「列」になる（x≈61–241, 245–421, 429–789…）。
//! 2 段抜きの記事は列の下に全幅で置かれ、柱（「官」「報」「令和 年 月 日」「第 号」）は
//! 左端の帯 (x≈42–55) に縦に並ぶ。`pdftotext` 自身の読み順（`<line>` の順）は 1 列なら正しいが、
//! 複数列だと列をまたいで行単位に交互に並ぶため、列を見つけて左→右・各列上→下に組み直す。
//!
//! 号外の公告・決算公告など、回転せず縦長のページに横書きで組んだページも同じように読む。

use crate::trace;
use crate::vertical::Col;

/// 回転ページの左端の柱の帯（回転前のページ上余白）。語全体がこの x より左にあれば柱として
/// 捨てる。本文の左端は x≈61、表の丸数字は x≈53 から始まるので、語の右端で判定する。
pub(crate) const LEFT_STRIP: f32 = 56.0;
/// 列の間の溝とみなす最小の空き(pt)。列間は 4〜8pt。
const MIN_GUTTER: f32 = 3.0;
/// 溝に語が食い込むのを許す幅(pt)。両端をこれだけ縮めるので、`2 * GUTTER_OVERLAP - MIN_GUTTER`
/// (=5pt) までの重なりは溝として扱う（行末のぶら下げ約物は半角 4pt ほど）。段落の字下げ（1em=8pt）
/// だけずれた短い行（「る。」）は列と区別する。1 字語（8pt）は溝の判定から外れ、中心で振り分ける。
const GUTTER_OVERLAP: f32 = 4.0;
/// 同じ行とみなす行中心 y の差(pt)（表の行揃えの判定に使う）。
const ALIGN_TOL: f32 = 1.0;
/// 本文の段とみなす最小の列幅(pt)。横組みの段の幅は約 176–180pt、表の項目名の列は 100pt 前後。
const MIN_TEXT_COLUMN_WIDTH: f32 = 150.0;

/// 回転ページの柱（左端の帯にある「官」「報」）か。
///
/// 回転ページでは柱が上端ではなく左端の帯に縦に並び、上端の帯には本文が来る。テキスト層に
/// 柱しか無い回転ページもある（本文が画像・アウトライン）ので、語の形ではなく柱の位置で見る。
pub(crate) fn is_left_strip_header(x1: f32, text: &str) -> bool {
    x1 <= LEFT_STRIP && matches!(text.trim(), "官" | "報")
}

/// 横組みの語（柱は除去済み）を読み順の行に組む。
///
/// `is_margin_noise` は使わない。横組みの日付行「令和８年10月２日」は数字を除くと柱の
/// 「令和年月日」と同じ骨格になり、本文が消える。
pub(crate) fn horizontal_lines(cols: Vec<Col>) -> Vec<trace::Line> {
    let mut lines: Vec<trace::Line> = Vec::new();
    order_region(cols, &mut lines);
    lines
}

/// 横書きの 1 行（y が重なる語の並び、x 昇順）。
struct Line {
    y0: f32,
    y1: f32,
    words: Vec<Col>,
}

impl Line {
    fn center(&self) -> f32 {
        (self.y0 + self.y1) / 2.0
    }
}

fn y_center(w: &Col) -> f32 {
    (w.y + w.y1) / 2.0
}

/// 語を行に束ねる。語の中心 y が直前の行の y 範囲に入れば同じ行とする。
fn group_lines(words: &[Col]) -> Vec<Line> {
    let mut sorted: Vec<&Col> = words.iter().collect();
    sorted.sort_by(|a, b| y_center(a).total_cmp(&y_center(b)));
    let mut lines: Vec<Line> = Vec::new();
    for w in sorted {
        let c = y_center(w);
        match lines.last_mut() {
            Some(l) if c > l.y0 && c < l.y1 => {
                l.y0 = l.y0.min(w.y);
                l.y1 = l.y1.max(w.y1);
                l.words.push(w.clone());
            }
            _ => lines.push(Line {
                y0: w.y,
                y1: w.y1,
                words: vec![w.clone()],
            }),
        }
    }
    for l in &mut lines {
        l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    lines
}

/// 領域の語を読み順に並べ、行文字列として `out` へ積む（再帰的な XY-cut）。
///
/// 1. 縦の溝（列間）で左右に切れるなら、左→右の順に再帰する。
/// 2. 切れなければ、横に切って上下どちらかが列組みになる位置を探し、上→下に再帰する。
///    行間は 1〜2pt しかないため、あらゆる行間で切ると多段組の行が交互に並んでしまう。
///    「列組みになる部分」が最も大きくなる 1 か所だけで切る（全幅記事と段組の境目）。
/// 3. どちらも無ければ 1 本の流れとして行を上→下に出す。
fn order_region(words: Vec<Col>, out: &mut Vec<trace::Line>) {
    if words.is_empty() {
        return;
    }
    let lines = group_lines(&words);
    if lines.len() >= 2 {
        if let Some(cut) = column_cut(&words) {
            let (left, right): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| x_center(w) < cut);
            order_region(left, out);
            order_region(right, out);
            return;
        }
        if let Some(cut) = band_cut(&lines) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            order_region(top, out);
            order_region(bottom, out);
            return;
        }
    }
    out.extend(
        lines
            .iter()
            .map(|l| trace::Line::text(render_line(&l.words), l.words.iter().map(|w| w.id).collect())),
    );
}

/// 領域を左右の列に分ける縦の溝の x。左から順に、列分割として妥当な最初の溝を返す。
///
/// 溝は、各語を両端から [`GUTTER_OVERLAP`] ずつ縮めた区間の切れ目として探す。行末のぶら下げ
/// （句読点・括弧）や、回転した縦書き部分の字下げで、隣の列と数 pt 重なることがあるため。
/// 語は中心 x で左右に振り分ける（[`x_center`]）。
fn column_cut(words: &[Col]) -> Option<f32> {
    // 字間空けの見出し（「中　間　決　算　公　告」）は 1 字語の並びだが、1 語として溝をふさぐ。
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for line in group_lines(words) {
        for (w, joined) in line.words.iter().zip(letter_spaced(&line.words)) {
            match spans.last_mut() {
                Some(span) if joined => span.1 = w.x1,
                _ => spans.push((w.x, w.x1)),
            }
        }
    }
    let mut spans: Vec<(f32, f32)> = spans
        .into_iter()
        .map(|(x, x1)| (x + GUTTER_OVERLAP, x1 - GUTTER_OVERLAP))
        .filter(|(a, b)| a < b)
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut right_edge = f32::NEG_INFINITY;
    for (a, b) in spans {
        if right_edge.is_finite() && a - right_edge >= MIN_GUTTER {
            let cut = (right_edge + a) / 2.0;
            let (left, right): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| x_center(w) < cut);
            if is_column_split(&left, &right) {
                return Some(cut);
            }
        }
        right_edge = right_edge.max(b);
    }
    None
}

fn x_center(w: &Col) -> f32 {
    (w.x + w.x1) / 2.0
}

/// 溝の左右が別の列（左→右に順に読む）か、表の列（行ごとに横に読む）か。
///
/// - 表（項目名｜内容）は、行の少ない側の行がほぼすべて反対側の行と同じ高さに並ぶ。段組の
///   列どうしは行送りが独立している。たまたま同じ行送りで組まれた段組と区別するため、
///   両側とも段の幅に近い本文の列なら、行が揃っていても段組とみなす。
/// - 中央寄せの見出しが上下に並ぶだけ（「公　告」「諸事項」「有権者申出方」）でも溝はできる。
///   少なくとも一方に本文の段の幅の行がなければ列とはみなさない。
fn is_column_split(left: &[Col], right: &[Col]) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let (ll, rl) = (group_lines(left), group_lines(right));
    let (few, many) = if ll.len() <= rl.len() { (&ll, &rl) } else { (&rl, &ll) };
    let aligned = few
        .iter()
        .filter(|a| many.iter().any(|b| (a.center() - b.center()).abs() <= ALIGN_TOL))
        .count();
    let width = |ws: &[Col]| {
        let x0 = ws.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
        let x1 = ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
        x1 - x0
    };
    let (wl, wr) = (width(left), width(right));
    let table_like = aligned as f32 >= few.len() as f32 * 0.75 && wl.min(wr) < MIN_TEXT_COLUMN_WIDTH;
    // 本文の段なら、段の幅いっぱいの行がある。見出しだけの側（「官　庁　事　項」と
    // 「人事院公示第26号」が離れて並ぶ等）は、全体の幅が広くても 1 行ずつは短い。
    let longest_line = |ls: &[Line]| ls.iter().map(|l| width(&l.words)).fold(0.0, f32::max);
    !table_like && longest_line(&ll).max(longest_line(&rl)) >= MIN_TEXT_COLUMN_WIDTH
}

/// 上下に切る y。切った上側か下側が列組み（[`column_cut`] で切れる 2 行以上）になる
/// 行間のうち、列組み側の語数が最大になるものを選ぶ。
fn band_cut(lines: &[Line]) -> Option<f32> {
    let mut by_top: Vec<&Line> = lines.iter().collect();
    by_top.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let mut best: Option<(usize, f32)> = None;
    let mut bottom_edge = f32::NEG_INFINITY;
    for (i, l) in by_top.iter().enumerate() {
        if i > 0 && l.y0 > bottom_edge {
            let cut = (bottom_edge + l.y0) / 2.0;
            let (top, bottom) = by_top.split_at(i);
            let score = [top, bottom]
                .iter()
                .filter(|part| part.len() >= 2)
                .filter_map(|part| {
                    let ws: Vec<Col> = part.iter().flat_map(|l| l.words.iter().cloned()).collect();
                    column_cut(&ws).map(|_| ws.len())
                })
                .max();
            if let Some(score) = score {
                if best.map(|(s, _)| score > s).unwrap_or(true) {
                    best = Some((score, cut));
                }
            }
        }
        bottom_edge = bottom_edge.max(l.y1);
    }
    best.map(|(_, cut)| cut)
}

/// 1 行の語（x 昇順）を連結する。
///
/// - 1em の 3/4 以上空いていれば全角空白を挟む（「佐賀労働局長　田之上英治」、表のセル境界）。
/// - 1 字ずつの語が並ぶ字間空けの見出し（「労　働」「公　告」「附　則」「名　　称」、表の欄見出し
///   「改　　正　　後」は 7em 空き）は詰めて 1 語にする。ただし字間が急に広がったところ
///   （表の次のセル）では区切る。
fn render_line(words: &[Col]) -> String {
    let mut s = String::new();
    for (i, (w, joined)) in words.iter().zip(letter_spaced(words)).enumerate() {
        if i > 0 && !joined {
            let p = &words[i - 1];
            let em = (p.y1 - p.y).min(w.y1 - w.y).max(1.0);
            if w.x - p.x1 >= em * 0.75 {
                s.push('\u{3000}');
            }
        }
        s.push_str(&w.text);
    }
    s
}

/// 1 行の語（x 昇順）のそれぞれが、直前の語と字間空けの見出しとしてつながるか（先頭は false）。
/// 1 字語どうしで、字間が 8em 以内かつ見出しの中でほぼ一定なら、つながる。
fn letter_spaced(words: &[Col]) -> Vec<bool> {
    let mut out = vec![false; words.len()];
    // 字間空けの見出しの字間（直前の 1 字語どうしの空き）。
    let mut spaced_gap: Option<f32> = None;
    for i in 1..words.len() {
        let (p, w) = (&words[i - 1], &words[i]);
        let em = (p.y1 - p.y).min(w.y1 - w.y).max(1.0);
        let gap = w.x - p.x1;
        let single = p.text.chars().count() == 1 && w.text.chars().count() == 1;
        out[i] = single
            && match spaced_gap {
                None => gap <= em * 8.0,
                Some(g) => (gap - g).abs() <= em,
            };
        spaced_gap = out[i].then_some(gap);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(x: f32, x1: f32, y: f32, text: &str) -> Col {
        Col {
            id: 0,
            x,
            x1,
            y,
            y1: y + 8.0,
            text: text.to_string(),
        }
    }

    fn read(words: Vec<Col>) -> String {
        horizontal_lines(words)
            .into_iter()
            .map(|l| l.text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn detects_rotated_page_header_in_left_strip() {
        // 回転ページの柱は左端の帯（x≈42–55）にある。縦書きページの柱（上端の帯）や本文の
        // 「官」とは区別する。
        assert!(is_left_strip_header(54.5, "官"));
        assert!(is_left_strip_header(54.5, "報"));
        assert!(!is_left_strip_header(279.5, "官"));
        assert!(!is_left_strip_header(54.3, "金曜日"));
    }

    #[test]
    fn reads_columns_left_to_right_then_full_width_below() {
        // 2 列（行送りが異なる）+ 下に全幅の記事。pdftotext は列をまたいで行を交互に出すが、
        // 列ごとに上→下、その後に全幅記事を読む。
        let words = vec![
            w(61.0, 241.0, 60.0, "左一"),
            w(245.0, 421.0, 62.0, "右一"),
            w(61.0, 241.0, 70.0, "左二"),
            w(245.0, 421.0, 75.0, "右二"),
            w(61.0, 241.0, 80.0, "左三"),
            w(61.0, 421.0, 100.0, "全幅一"),
            w(61.0, 421.0, 110.0, "全幅二"),
        ];
        assert_eq!(read(words), "左一\n左二\n左三\n右一\n右二\n全幅一\n全幅二");
    }

    #[test]
    fn reads_label_value_table_row_by_row() {
        // 項目名｜内容 の表は列に分けず、行ごとに読む。
        let words = vec![
            w(68.0, 164.0, 140.0, "項目一"),
            w(172.0, 412.0, 140.0, "内容一の一行目"),
            w(172.0, 412.0, 151.0, "内容一の二行目"),
            w(68.0, 164.0, 162.0, "項目二"),
            w(172.0, 412.0, 162.0, "内容二"),
        ];
        assert_eq!(
            read(words),
            "項目一\u{3000}内容一の一行目\n内容一の二行目\n項目二\u{3000}内容二"
        );
    }

    #[test]
    fn keeps_word_ids_of_each_line() {
        let mut a = w(61.0, 241.0, 100.0, "佐賀労働局長");
        a.id = 7;
        let mut b = w(249.0, 289.0, 100.0, "田之上英治");
        b.id = 9;
        let lines = horizontal_lines(vec![b, a]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].words, vec![7, 9]);
    }

    #[test]
    fn joins_letter_spaced_headings() {
        let line = vec![w(78.0, 87.0, 46.0, "労"), w(114.0, 123.0, 46.0, "働")];
        assert_eq!(render_line(&line), "労働");
        // 字間空けの見出しが続く表の見出し行は、字間が急に広がったところで区切る。
        let header = vec![
            w(97.0, 105.0, 349.0, "名"),
            w(145.0, 153.0, 349.0, "称"),
            w(216.5, 224.5, 349.0, "本"),
            w(229.3, 237.3, 349.0, "店"),
            w(340.5, 396.5, 349.0, "営業廃止年月日"),
        ];
        assert_eq!(render_line(&header), "名称\u{3000}本店\u{3000}営業廃止年月日");
        // 新旧対照表の欄見出し（字間 7em）。
        let kaisei = vec![
            w(171.1, 179.1, 92.4, "改"),
            w(235.1, 243.1, 92.4, "正"),
            w(299.1, 307.1, 92.4, "後"),
            w(526.6, 534.6, 92.4, "改"),
            w(590.6, 598.6, 92.4, "正"),
            w(654.6, 662.6, 92.4, "前"),
        ];
        assert_eq!(render_line(&kaisei), "改正後\u{3000}改正前");
    }
}
