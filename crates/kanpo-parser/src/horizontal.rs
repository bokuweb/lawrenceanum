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

use crate::grid::{Grid, GridCell};
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
    horizontal_lines_around(cols, &[], &[], &[], &[])
}

/// 語を取り除いた表の外枠 `(x0, y0, x1, y1)`。
type Block = (f32, f32, f32, f32);

/// [`horizontal_lines`] で、表として先に取り除いた語の外枠 `blocks` を、列の溝・上下の切れ目を
/// ふさぐものとして扱う版。2 段抜きの表を抜いた跡が列の溝に見え、表の上の行（日付と署名が
/// 1 行に並ぶ「令和八年十月十五日　九州運輸局長　日向　弘基」）が下の段組の列に分かれないようにする。
///
/// `vrules`（縦の罫線 `(x, y0, y1)`）のうち、領域の高さの大半に掛かって語をまたがない線は、ほかの溝より先に
/// 列の境にする（[`rule_cut`]）。
///
/// `tables` は先に取り出した表の外枠（囲みの段の中で、表と表の間の溝を囲みの境とみなす。[`order_region`]）。
fn horizontal_lines_around(cols: Vec<Col>, blocks: &[Block], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)], tables: &[Block]) -> Vec<trace::Line> {
    let mut lines: Vec<trace::Line> = Vec::new();
    let ctx = Context { vrules, hrules, tables };
    order_region(cols, blocks, &ctx, false, false, &mut lines);
    lines
}

/// 横組みの語を、列に分けずに上から 1 行ずつ組む（縦書きのページの中の小さな横組みの欄用。
/// 欄は段の幅より狭く、段組みは無い。表の行は左のセルから順に読む）。
pub(crate) fn row_lines(cols: Vec<Col>) -> Vec<trace::Line> {
    period_heading(group_lines(&cols))
        .iter()
        .map(|l| trace::Line::text(render_line(&l.words), l.words.iter().map(|w| w.id).collect()))
        .collect()
}

/// セルの右端からこの空き（em 比）までに終わる行は、次の行へ折り返している（セルの内側の余白を含む）。
const WRAP_EM: f32 = 2.0;

/// セルの中で `next` が `prev` の折り返しの続きか。`prev` が文（4 字以上で数字が半分未満）で、セルの
/// 右端 `right` から [`WRAP_EM`] までに終わり、`next` が項目名（「所在地：」）で始まらない。右揃えの
/// 金額の行は右端に届くが折り返しではない。
fn wrapped(prev: &Line, next: &Line, right: f32) -> bool {
    let text: String = prev.words.iter().map(|w| w.text.as_str()).collect();
    let numeric = text.chars().filter(|c| c.is_ascii_digit()).count() * 2 >= text.chars().count();
    let end = prev.words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let next: String = next.words.iter().map(|w| w.text.as_str()).collect();
    // 行末の数が欄の間ほど空けて置かれた行（「その団体の数値　1.00」。右揃えの数値の欄）は折り返しではない。
    let em = (prev.y1 - prev.y0).max(1.0);
    let trailing_number = prev.words.len() >= 2 && {
        let (p, w) = (&prev.words[prev.words.len() - 2], &prev.words[prev.words.len() - 1]);
        // 読点・桁区切りで終わる数（住所の「Langeweg　79,」）は続きがある。
        w.x - p.x1 >= em * 0.75
            && w.text.ends_with(|c: char| c.is_ascii_digit())
            && w.text.chars().all(|c| c.is_ascii_digit() || ".,−－-△".contains(c))
    };
    // 数で終わり、次の行が単位の英字で始まる（「426.0875」「MHz」、「8.5」「kHz」）なら、数の行でも折り返し。
    const UNITS: [&str; 12] = ["GHz", "MHz", "kHz", "Hz", "dB", "kW", "mW", "W", "mm", "cm", "km", "mL"];
    let unit_split = text.ends_with(|c: char| c.is_ascii_digit()) && UNITS.iter().any(|u| next.starts_with(u));
    (unit_split || (!numeric && !trailing_number)) && !labeled(&next) && text.chars().count() >= 4 && right - end <= em * WRAP_EM
}

/// 幅 4 字分までの狭い欄で、`prev` が 2〜3 字の行で欄の右端の 1 字分以内まで組まれ、数を含まない（様式の表の欄見出し「その／他有／
/// 価証／券評／価差／額金」を 2 字ずつ折り返したもの）。[`wrapped`] は 4 字以上の行しか折り返しとみなさない。
fn narrow_wrapped(prev: &Line, left: f32, right: f32) -> bool {
    let em = (prev.y1 - prev.y0).max(1.0);
    let end = prev.words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let chars = prev.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).count();
    // 2〜3 字（狭い科目の欄の「営業外収益」「営業外費用」は右端まで組まれても別の科目。1 字ずつ縦に積んだ欄の名前
    // 「負／債／及／び」は [`stacked`] で読む）。
    // 1 語（2 列に縦に積んだ欄の名前「負債及び／純資産の部」の「負」「純」は 1 字ずつの語）。
    (2..=3).contains(&chars)
        && prev.words.len() == 1
        && right - left <= em * 4.0
        && right - end <= em
        && !prev.words.iter().flat_map(|w| w.text.chars()).any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || c == '×')
}

/// 同じ行の同じセルの中で、欄に収まらずに折り返した金額の続きか（「113,191,23」「1」）。
fn amount_tail(prev: &Line, next: &Line) -> bool {
    let text: String = prev.words.iter().map(|w| w.text.as_str()).collect();
    let next: String = next.words.iter().map(|w| w.text.as_str()).collect();
    text.chars().filter(|c| c.is_ascii_digit()).count() * 2 >= text.chars().count()
        && text.ends_with(|c: char| c.is_ascii_digit())
        // 桁区切りの最後の組が 3 桁に足りない（「23」で切れている）。
        && text.contains(',')
        && text.rsplit(',').next().is_some_and(|g| g.len() < 3)
        && (1..=3).contains(&next.chars().count())
        && next.chars().all(|c| c.is_ascii_digit())
}

/// 「所在地：」「電話番号：」のような項目名で始まる行か（前の行がセルの右端まで組まれていても
/// 折り返しの続きではない）。
fn labeled(text: &str) -> bool {
    text.chars().take(8).any(|c| c == '：' || c == ':')
}

/// 罫線の表（他の枠を含まない格子で、中の語が横組みのもの）を表として組み、残りの語は
/// [`horizontal_lines`] で行に組んで、表を読み順の位置（表の下か右にある最初の行の手前）に差し込む。
/// 表の欄の間の溝を列の溝と取り違えて、表が欄ごとに読まれないようにするため。
pub(crate) fn horizontal_lines_with_tables(cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Vec<trace::Line> {
    lines_with_tables(cols, grids, vrules, hrules, false)
}

/// [`horizontal_lines_with_tables`] の、段組みの無い 1 段（横書きの新旧対照表の片側の欄）を読む版。表の外の
/// 語は段に分けずに上から 1 行ずつ組む（左寄せの短い行「目次」「［略］」と中央寄せの表題の間を段の溝と
/// 取り違えて、左の行を先に読んでいた）。
pub(crate) fn one_column_lines_with_tables(cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Vec<trace::Line> {
    lines_with_tables(cols, grids, vrules, hrules, true)
}

/// 左右に離して並べた 2 つの罫線の表で、上端と下端が揃い、右の表のどの行の上端も左の表のいずれかの行の上端と揃うもの
/// （たばこの小売定価の表の、品名｜寸法｜本数の表と、離れて組んだ定価の 2 欄の表）は、右の表の欄を左の表の行の後ろに
/// つなぐ（品名の行がすべて読まれた後に、定価だけが並んでいた）。
fn merge_side_by_side(tables: &mut Vec<trace::Table>) {
    let tops = |t: &trace::Table| -> Vec<f32> {
        (0..t.rows).map(|r| t.cells.iter().filter(|c| c.row == r).map(|c| c.y0).fold(f32::INFINITY, f32::min)).collect()
    };
    // 上端が y に揃うセルの行（行をまたぐセルは下の行に置かれることがあるので、セルごとに見る）。
    let row_at = |t: &trace::Table, y: f32| t.cells.iter().filter(|c| (c.y0 - y).abs() <= 2.0).map(|c| c.row).min();
    let mut i = 0;
    while i < tables.len() {
        let partner = (0..tables.len()).find(|&j| {
            let (a, b) = (&tables[i], &tables[j]);
            j != i
                && b.x0 > a.x1
                && b.x0 - a.x1 < a.x1 - a.x0
                && (a.y0 - b.y0).abs() <= 3.0
                && (a.y1 - b.y1).abs() <= 3.0
                && b.rows >= 3
                && a.rows >= b.rows
                // 同じ欄の並びの表（2 つに分けて組んだ座標の一覧の続き）はつながない。
                && a.cols != b.cols
                // 右の表は金額だけ（科目の欄のある表どうし（並べて組んだ決算公告の囲み）はつながない）。
                && b.cells.iter().all(|c| {
                    let t: String = c.text.chars().filter(|ch| !ch.is_whitespace()).collect();
                    t.is_empty() || (t.chars().any(|ch| ch.is_ascii_digit()) && t.chars().all(|ch| ch.is_ascii_digit() || ",.△▲－-円%％()（）".contains(ch)))
                })
                && {
                    tops(b).iter().all(|y| y.is_finite() && row_at(a, *y).is_some())
                }
        });
        let Some(j) = partner else {
            i += 1;
            continue;
        };
        let b = tables.remove(j);
        let i2 = if j < i { i - 1 } else { i };
        let a = &mut tables[i2];
        let tb = tops(&b);
        for mut c in b.cells {
            let y = tb[c.row];
            c.row = row_at(a, y).unwrap_or(c.row);
            c.col += a.cols;
            a.cells.push(c);
        }
        a.cols += b.cols;
        a.x1 = b.x1;
        i = i2 + 1;
    }
}

/// `vrules`・`hrules` は縦・横の罫線（`(x, y0, y1)`・`(y, x0, x1)`）。
fn lines_with_tables(cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)], one_column: bool) -> Vec<trace::Line> {
    let geom: std::collections::HashMap<usize, (f32, f32, f32, f32)> = cols.iter().map(|c| (c.id, (c.x, c.x1, c.y, c.y1))).collect();
    let texts: std::collections::HashMap<usize, String> = cols.iter().map(|c| (c.id, c.text.clone())).collect();
    let mut lines = lines_with_tables_inner(cols, grids, vrules, hrules, one_column);
    rejoin_split_names(&mut lines, &geom, &texts);
    lines
}

/// 署名の氏の後ろに同じ高さで続く名（「取締役　北川」の右の「久芳」）が、表と表の間の溝で別の列に分かれて離れた行に
/// 読まれていたら、氏の行の後ろへ戻す（囲みの中の貸借対照表と損益計算書の間の溝が、署名の氏と名の間にも通っていた）。
/// 名だけの行（1〜3 字の漢字・仮名の 1 語）で、前の行のうち、役職（取締役・理事・社長・清算人・代表者・執行役・社員）を
/// 含み 1〜3 字の氏で終わる行の、氏の語と上下が揃い 1.5 字分以内の右から始まるものだけ。
fn rejoin_split_names(lines: &mut Vec<trace::Line>, geom: &std::collections::HashMap<usize, (f32, f32, f32, f32)>, texts: &std::collections::HashMap<usize, String>) {
    const TITLES: [&str; 7] = ["取締役", "理事", "社長", "清算人", "代表者", "執行役", "社員"];
    let name = |t: &str| {
        let t = t.trim();
        let n = t.chars().count();
        (1..=3).contains(&n) && t.chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3040}'..='\u{30FF}').contains(&c))
    };
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        // 名は漢字を含む（社名の折り返し「ション」「ングス」は名ではない）。
        let single = l.table.is_none()
            && l.words.len() == 1
            && texts.get(&l.words[0]).is_some_and(|t| name(t) && t.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
        let Some(&(nx, _, ny, ny1)) = single.then(|| geom.get(&l.words[0])).flatten() else {
            i += 1;
            continue;
        };
        let em = (ny1 - ny).max(1.0);
        let host = (0..i).rev().take(60).find(|&j| {
            let m = &lines[j];
            let Some(&last) = m.words.last() else { return false };
            let Some(&(_, lx1, ly, _)) = geom.get(&last) else { return false };
            // 氏の語のすぐ前が役職の語（名まで組まれた行「代表取締役　康本　和正」には付けない）。
            let titled = m.words.len() >= 2 && texts.get(&m.words[m.words.len() - 2]).is_some_and(|t| TITLES.iter().any(|x| t.contains(x)));
            m.table.is_none()
                && titled
                && texts.get(&last).is_some_and(|t| name(t))
                && (ly - ny).abs() < 1.0
                && nx >= lx1 - 0.5
                && nx - lx1 <= em * 1.5
        });
        if let Some(j) = host {
            let l = lines.remove(i);
            let m = &mut lines[j];
            m.text.push('\u{3000}');
            m.text.push_str(l.text.trim());
            m.words.extend(l.words);
            continue;
        }
        i += 1;
    }
}

fn lines_with_tables_inner(cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)], one_column: bool) -> Vec<trace::Line> {
    let mut rest = cols;
    let mut tables: Vec<trace::Table> = Vec::new();
    for g in grids.iter().filter(|g| g.xs.len() >= 3 && g.ys.len() >= 3 && !grids.iter().any(|o| g.encloses(o))) {
        let (mine, others): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(|c| g.contains(x_center(c), y_center(c), 0.5));
        rest = others;
        match grid_table(g, &mine, 0) {
            Some(t) => tables.push(t),
            None => rest.extend(mine),
        }
    }
    merge_side_by_side(&mut tables);
    // 縦の罫線だけの表は、決算公告の囲みの中の表と同じく本文の段の溝をふさがない（上の記事の段が 1 段につながる）。
    let framed = tables.len();
    let (ruled, others) = rule_column_tables(rest, vrules, hrules);
    rest = others;
    let ruled_n = ruled.len();
    tables.extend(ruled);
    let (keyed, others) = keyed_tables(rest, vrules);
    rest = others;
    tables.extend(keyed);
    // 縦組みの欄（号外の決算公告のページの囲みの中の合併公告・資本金の額の減少公告）は、表と同じく先に取り出して
    // 右の列から縦に読む。横の行に組むと、列が左から順に 1 行につながっていた。
    let mut verticals: Vec<(Block, Vec<trace::Line>)> = Vec::new();
    if !one_column {
        for (block, ids) in vertical_regions(&rest) {
            let (mine, others): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(|c| ids.contains(&c.id));
            rest = others;
            verticals.push((block, vertical_lines(mine)));
        }
    }
    if tables.is_empty() && verticals.is_empty() {
        return if one_column { row_lines(rest) } else { horizontal_lines_around(rest, &[], vrules, hrules, &[]) };
    }
    let geom: std::collections::HashMap<usize, (f32, f32, f32, f32)> = rest.iter().map(|c| (c.id, (c.x, c.x1, c.y, c.y1))).collect();
    // 2 段抜きの表（本文の段 2 つ分の幅）だけ。決算公告の囲みの中の表は囲みの列ごとに並び、
    // 囲みの列の溝は本文の段の溝と揃わないので、溝をふさぐと囲みの読み順が崩れる。縦組みの欄は、取り出す前の
    // 列の語と同じく溝・切れ目をふさぐ。
    let blocks: Vec<Block> = tables
        .iter()
        .enumerate()
        .filter(|(i, t)| !(framed..framed + ruled_n).contains(i) && t.x1 - t.x0 >= MIN_TEXT_COLUMN_WIDTH * 2.0)
        .map(|(_, t)| t)
        .map(|t| (t.x0, t.y0, t.x1, t.y1))
        .chain(verticals.iter().map(|v| v.0))
        .collect();
    let rects: Vec<Block> = tables.iter().map(|t| (t.x0, t.y0, t.x1, t.y1)).collect();
    let mut lines = if one_column { row_lines(rest) } else { horizontal_lines_around(rest, &blocks, vrules, hrules, &rects) };
    // 縦組みの欄は、欄の左に同じ高さで並ぶ最後の行の後ろ（同じ囲みの決算公告の後ろ。囲みの枠は罫線として
    // 取り出せないことが多い）。
    // ただし、欄のすぐ下（30pt 以内）から同じ列で始まる横組みの行があり、間の横の罫線が欄より左へ延びていなければ、
    // その行の前（同じ列の囲みの中の縦組みの合併公告の下に組んだ決算公告。左の囲みとの間には段の境の線がある）。
    // 間の線が左の囲みまで延びていれば、欄は左の囲みと同じ囲みの中（「左記の」決算公告の右の合併公告）。
    // 字の並びを比べるために空白を除き、全角の英数字を半角にする（「ＳＦ」と「SF」）。
    let plain = |t: &str| -> String {
        t.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '０'..='９' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                _ => c,
            })
            .collect()
    };
    // 列の終わりで折り返した「左／記」も。
    let refers_left: Vec<(Block, String)> = verticals
        .iter()
        .map(|v| (v.0, plain(&v.1.iter().map(|l| l.text.as_str()).collect::<String>())))
        // 「貸借対照表の要旨は次のとおり」も（欄のすぐ下に続く貸借対照表は先に `below` で見る）。
        .filter(|(_, t)| t.contains("左記") || t.contains("要旨は次のとおり"))
        .collect();
    let beside = |b: &Block| -> Option<usize> {
        let below = lines.iter().position(|l| {
            let g: Vec<(f32, f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
            let x0 = g.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let x1 = g.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            let y0 = g.iter().map(|p| p.2).fold(f32::INFINITY, f32::min);
            !g.is_empty()
                && x0 >= b.0 - 10.0
                && x1 <= b.2 + 10.0
                && y0 >= b.3 - 2.0
                && y0 - b.3 < 30.0
                && !hrules.iter().any(|&(y, hx0, hx1)| y > b.3 - 2.0 && y < y0 + 1.0 && hx0 < b.0 - 20.0 && hx1 > b.0)
        });
        let left_of = |l: &trace::Line| {
            let g: Vec<(f32, f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
            !g.is_empty() && g.iter().all(|p| p.1 <= b.0 + 1.0 && (p.2 + p.3) / 2.0 >= b.1 && (p.2 + p.3) / 2.0 <= b.3)
        };
        below.or_else(|| {
            let last = lines.iter().rposition(left_of)?;
            // 「左記の」貸借対照表を指す欄（決算公告の囲みの右の縦組みの合併公告）は、欄の左の囲みの中の行より先（「…貸借対照表の
            // 要旨は左記のとおりです。」の後に左の貸借対照表が続く）。欄の左に続く行のうち、欄との間に欄の高さほどの縦の罫線
            // （隣の囲みの境）の無いものの頭へ。
            // 欄の左の行を列（左右の範囲の重なる行の並び）に分け、右の列から、欄の文に名前の出てこない会社・法人の名前の
            // 行のある列（同じ高さに並ぶ別の囲みの決算公告）の手前まで。
            if let Some((_, notice)) = refers_left.iter().find(|(r, _)| r == b) {
                let fenced = |l: &trace::Line| {
                    let x1 = l.words.iter().filter_map(|w| geom.get(w)).map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                    vrules.iter().any(|&(x, y0, y1)| x > x1 - 1.0 && x < b.0 - 1.0 && y0 <= b.1 + 10.0 && y1 >= b.3 - 10.0)
                };
                let mut first = last;
                while first > 0 && left_of(&lines[first - 1]) && !fenced(&lines[first - 1]) {
                    first -= 1;
                }
                let span = |l: &trace::Line| {
                    let g: Vec<(f32, f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
                    (g.iter().map(|p| p.0).fold(f32::INFINITY, f32::min), g.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max))
                };
                let mut chunks: Vec<(usize, f32, f32)> = Vec::new();
                for k in first..=last {
                    let (x0, x1) = span(&lines[k]);
                    match chunks.last_mut() {
                        Some(c) if x0 < c.2 && x1 > c.1 => {
                            c.1 = c.1.min(x0);
                            c.2 = c.2.max(x1);
                        }
                        _ => chunks.push((k, x0, x1)),
                    }
                }
                let named = |from: usize, to: usize| {
                    let names: Vec<String> = lines[from..to]
                        .iter()
                        .map(|l| plain(&l.text))
                        // 名前の途中で折り返した行（「日鉄物産名古屋コイルセンター株式」「会社」）も見る。4 字未満の切れ端（「会社」）は見ない。
                        .filter(|t| {
                            (["会社", "法人", "組合"].iter().any(|n| t.contains(n)) || t.ends_with("株式") || t.ends_with("有限") || t.ends_with("合同"))
                                && !t.contains("公告")
                                && (4..=40).contains(&t.chars().count())
                        })
                        .map(|t| t.trim_start_matches(|c: char| "（）()甲乙丙丁".contains(c)).to_string())
                        .collect();
                    names.is_empty() || names.iter().any(|n| notice.contains(n.as_str()))
                };
                let mut at = None;
                for k in (0..chunks.len()).rev() {
                    let end = chunks.get(k + 1).map(|c| c.0).unwrap_or(last + 1);
                    if !named(chunks[k].0, end) {
                        break;
                    }
                    at = Some(chunks[k].0);
                }
                if let Some(at) = at.filter(|&i| !fenced(&lines[i])) {
                    return Some(at);
                }
            }
            Some(last + 1)
        })
    };
    let items: Vec<(Block, Vec<trace::Line>, Option<usize>)> = tables
        .into_iter()
        .map(|t| ((t.x0, t.y0, t.x1, t.y1), vec![trace::Line { text: t.render(), words: Vec::new(), table: Some(t) }], None))
        .chain(verticals.into_iter().map(|(b, ls)| (b, ls, beside(&b))))
        .collect();
    let mut placed: Vec<(usize, Block, Vec<trace::Line>)> = items
        .into_iter()
        .map(|(t, ls, at)| {
            // 表の右か、表の下（行の高さの中ほどが表の下端より下。字の大きい太字の見出しは上端が表の
            // 最後の行に少し掛かる）にある行。
            let after = |l: &trace::Line| {
                let g: Vec<(f32, f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
                let x0 = g.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let x1 = g.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                let y0 = g.iter().map(|p| p.2).fold(f32::INFINITY, f32::min);
                let y1 = g.iter().map(|p| p.3).fold(f32::NEG_INFINITY, f32::max);
                !g.is_empty() && (x0 >= t.2 - 2.0 || (x0 < t.2 && x1 > t.0 && (y0 >= t.3 - 2.0 || (y0 + y1) / 2.0 >= t.3)))
            };
            // 表のすぐ上の行（表の幅に収まり、表の上端より上 30pt 以内にある行のうち一番下のもの。表の見出し「貸借対照表の要旨」）の
            // 後ろに置く（囲みごとに読むと、表の右か下にある最初の行は隣の囲みの行のことがある）。無ければ表の右か下にある
            // 最初の行の前。
            let heading = lines
                .iter()
                .enumerate()
                .filter_map(|(i, l)| {
                    let g: Vec<(f32, f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
                    let x0 = g.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                    let x1 = g.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                    let y1 = g.iter().map(|p| p.3).fold(f32::NEG_INFINITY, f32::max);
                    // 表の幅に収まる行だけ（隣の損益計算書の見出しと 1 行に組まれた行は見出しにしない）。
                    (!g.is_empty() && y1 <= t.1 + 2.0 && t.1 - y1 < 30.0 && x0 >= t.0 - 20.0 && x1 <= t.2 + 20.0).then_some((i, y1))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .map(|(i, _)| i + 1);
            (at.or(heading).unwrap_or_else(|| lines.iter().position(after).unwrap_or(lines.len())), t, ls)
        })
        .collect();
    // 後ろの位置から差し込む。同じ位置には、読み順で後の表（下・右）から差し込む。縦組みの欄（行の無い表）は
    // 上下に並ぶので、下の欄から。
    let vertical = |ls: &[trace::Line]| ls.first().is_some_and(|l| l.table.is_none());
    let key = |p: &(usize, Block, Vec<trace::Line>)| if vertical(&p.2) { p.1 .1 } else { f32::NEG_INFINITY };
    // 左右は 10pt 刻みで比べる（同じ欄の上下に並ぶ表の左端は 1pt 未満ずれることがあり、上下の順より先に効いて
    // 表が上下ばらばらの順に差し込まれていた）。
    let column = |p: &(usize, Block, Vec<trace::Line>)| (p.1 .0 / 10.0).round();
    placed.sort_by(|a, b| {
        b.0.cmp(&a.0).then(key(b).total_cmp(&key(a))).then(column(b).total_cmp(&column(a))).then(b.1 .1.total_cmp(&a.1 .1))
    });
    for (at, _, ls) in placed {
        lines.splice(at..at, ls);
    }
    lines
}

/// 縦組みの語（縦長で 2 字以上）か。
fn upright(c: &Col) -> bool {
    c.text.trim().chars().count() >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5
}

/// 横組みのページの中の縦組みの欄の外枠と、欄の語の ID。縦組みの語（1 列が 1 語）が、列の幅の 2 倍までの
/// 間隔で左右に並び、上下に重なるものをつなぎ、3 列以上・30 字以上のものだけ（表の欄の縦書きの見出し
/// 「資産の部」などは欄にしない）。欄の外枠の中の語（1 字の列も）は欄の語。
fn vertical_regions(words: &[Col]) -> Vec<(Block, Vec<usize>)> {
    let tall: Vec<&Col> = words.iter().filter(|c| upright(c)).collect();
    let mut group: Vec<usize> = (0..tall.len()).collect();
    fn root(g: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..tall.len() {
        for j in i + 1..tall.len() {
            let (a, b) = (tall[i], tall[j]);
            // 2 列分の間隔（間の短い列「二号」が上下に重ならずに挟まる署名の列「株式会社誠和」も。字の箱の端数で 2 字分を
            // わずかに越える）。
            let pitch = (a.x1 - a.x).max(b.x1 - b.x) * 2.0 + 0.5;
            let gap = (a.x.max(b.x) - a.x1.min(b.x1)).max(0.0);
            let overlap = a.y1.min(b.y1) - a.y.max(b.y);
            // 前の列の終わりの高さから始まる列（字下げして組んだ署名「代表取締役　中澤　晴彦」）も同じ欄。
            if gap <= pitch && overlap > -0.5 {
                let (ra, rb) = (root(&mut group, i), root(&mut group, j));
                group[ra] = rb;
            }
        }
    }
    let mut clusters: std::collections::BTreeMap<usize, Vec<&Col>> = std::collections::BTreeMap::new();
    for (i, c) in tall.iter().enumerate() {
        let r = root(&mut group, i);
        clusters.entry(r).or_default().push(c);
    }
    let mut out: Vec<(Block, Vec<usize>)> = Vec::new();
    for cs in clusters.values() {
        let chars: usize = cs.iter().map(|c| c.text.trim().chars().count()).sum();
        // 列の数（x の重なる語は同じ列）。
        let mut xs: Vec<(f32, f32)> = cs.iter().map(|c| (c.x, c.x1)).collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let columns = xs.iter().fold(Vec::<(f32, f32)>::new(), |mut acc, &(x0, x1)| {
            match acc.last_mut() {
                Some(last) if x0 < last.1 - 0.5 => last.1 = last.1.max(x1),
                _ => acc.push((x0, x1)),
            }
            acc
        });
        if columns.len() < 3 || chars < 30 {
            continue;
        }
        let block = (
            cs.iter().map(|c| c.x).fold(f32::INFINITY, f32::min),
            cs.iter().map(|c| c.y).fold(f32::INFINITY, f32::min),
            cs.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max),
            cs.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max),
        );
        let inside = |c: &Col| {
            let (cx, cy) = (x_center(c), y_center(c));
            cx >= block.0 - 0.5 && cx <= block.2 + 0.5 && cy >= block.1 - 0.5 && cy <= block.3 + 0.5
        };
        // 欄の中に横組みの行（2 字以上の横長の語）があれば、欄ではなく表などの一部とみなす。
        if words.iter().any(|c| inside(c) && c.text.trim().chars().count() >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5) {
            continue;
        }
        // 縦組みでない語は外枠に収まるものだけ（号の 1 ページ目の題字の下の縦組みの「（原稿作成　国立印刷局）」のすぐ右の、
        // 目次の項目の頭の「〇」を欄に入れていた）。
        // 字間を空けて横に組んだ 1 字ずつの見出し（欄のすぐ下の囲みの「決　算　公　告」）は欄に入れない。
        // 縦の列の中の 1 字の語（「代表取締役」「巽」「省一」の「巽」。隣の列の同じ高さにも 1 字の語が並ぶ）は、すぐ上か下に
        // 同じ x の縦長の語があれば列の字。
        let in_column = |c: &Col| {
            let em = (c.x1 - c.x).max(1.0);
            cs.iter().any(|o| o.id != c.id && (o.x - c.x).abs() < em * 0.3 && ((c.y - o.y1).abs() < em * 1.5 || (o.y - c.y1).abs() < em * 1.5))
        };
        let spaced_row = |c: &Col| {
            let em = (c.y1 - c.y).max(1.0);
            !in_column(c)
                && c.text.trim().chars().count() == 1
                && words.iter().filter(|o| o.id != c.id && o.text.trim().chars().count() == 1 && (y_center(o) - y_center(c)).abs() < 1.0 && (x_center(o) - x_center(c)).abs() < em * 4.0).count() >= 1
                && words.iter().filter(|o| o.text.trim().chars().count() == 1 && (y_center(o) - y_center(c)).abs() < 1.0).count() >= 3
        };
        let within = |c: &Col| upright(c) || (c.x >= block.0 - 0.5 && c.x1 <= block.2 + 0.5 && !spaced_row(c));
        out.push((block, words.iter().filter(|c| inside(c) && within(c)).map(|c| c.id).collect()));
    }
    out
}

/// 縦組みの欄の語を、右の列から順に、各列を上から読んで 1 列 1 行に組む。同じ列で 1 字分以上空いた語の間は
/// 全角の空白（「代表取締役　田口　貴之」）。
fn vertical_lines(words: Vec<Col>) -> Vec<trace::Line> {
    let mut words = words;
    // 縦書きの列の中で横倒しに組んだ欧文の語（「PlayStation」「Firewall Ultra」）は横組み用の書体で、縦書き用の書体の列の語
    // （幅がちょうど整数 pt）より半 em ほど左に出る。すぐ右の、上か下に続く縦書きの語の列に揃える（欧文の語だけが 1 列に
    // なり、列の後ろに出ていた）。
    let anchors: Vec<(f32, f32, f32, f32)> = words
        .iter()
        .filter(|w| !w.text.is_ascii() && ((w.x1 - w.x) - (w.x1 - w.x).round()).abs() < 0.01)
        .map(|w| (w.x, w.x1, w.y, w.y1))
        .collect();
    let latin = |w: &Col| {
        let width = w.x1 - w.x;
        // 和字に英字の混じった語（「株式会社Lacicu」）も同じく左に出る（上の「（乙）」と同じ列にならず、左の列の
        // 「代表取締役」とつながっていた）。
        let mixed = w.text.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3;
        w.text.chars().count() >= 2
            && (mixed || w.text.chars().all(|c| c.is_ascii_alphanumeric() || " -'&.,/:@".contains(c)))
            && w.y1 - w.y >= width * 1.8
            && (width - width.round()).abs() >= 0.01
    };
    // 同じ x に上下に続く欧文の語（「Japan」「Longevity」「Trust」）はまとめて寄せる。
    let mut idx: Vec<usize> = (0..words.len()).filter(|&i| latin(&words[i])).collect();
    idx.sort_by(|&a, &b| words[a].x.total_cmp(&words[b].x).then(words[a].y.total_cmp(&words[b].y)));
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for i in idx {
        match runs.last_mut() {
            Some(r) if {
                let p = &words[*r.last().unwrap()];
                (p.x - words[i].x).abs() < 0.5 && words[i].y - p.y1 < (p.x1 - p.x) * 2.0
            } => r.push(i),
            _ => runs.push(vec![i]),
        }
    }
    for run in runs {
        let (x, x1) = (words[run[0]].x, words[run[0]].x1);
        let width = x1 - x;
        let (top, bottom) = (words[run[0]].y, words[*run.last().unwrap()].y1);
        // 列の続き（縦書きの語のすぐ上か下）だけ。
        let adjacent = |&&(ax, _, ay, ay1): &&(f32, f32, f32, f32)| {
            ax - x >= width * 0.3 && ax - x <= width * 0.8 && ((top - ay1 >= -1.0 && top - ay1 < width * 2.0) || (ay - bottom >= -1.0 && ay - bottom < width * 2.0))
        };
        if let Some(&(ax, ax1, _, _)) = anchors.iter().filter(adjacent).min_by(|a, b| a.0.total_cmp(&b.0)) {
            let dx = (ax + ax1) / 2.0 - (x + x1) / 2.0;
            for &i in &run {
                words[i].x += dx;
                words[i].x1 += dx;
            }
        }
    }
    words.sort_by(|a, b| x_center(b).total_cmp(&x_center(a)));
    let mut columns: Vec<(f32, f32, Vec<Col>)> = Vec::new();
    for w in words {
        // 1 つの列の語は上下に 1.5 字分以上は重ならない（英字の混じった語「株式会社Diversity」は字の箱が右にずれ、
        // 左隣の列の「代表取締役　山畔　清明」と同じ列になって交互に混ざっていた）。欧文の語の箱は 1 字分ほどはみ出す。
        let stacked = |ws: &[Col]| ws.iter().any(|o| o.y1.min(w.y1) - o.y.max(w.y) > (o.x1 - o.x).min(w.x1 - w.x) * 1.5);
        // 重なる列が 2 つあれば（英字の語でずれた列と、その左の列）、x の重なりの大きい方。
        let overlap = |x0: f32, x1: f32| w.x1.min(x1) - w.x.max(x0);
        match columns
            .iter_mut()
            .filter(|(x0, x1, ws)| overlap(*x0, *x1) > (w.x1 - w.x).min(*x1 - *x0) * 0.5 && !stacked(ws))
            .max_by(|a, b| overlap(a.0, a.1).total_cmp(&overlap(b.0, b.1)))
        {
            Some((x0, x1, ws)) => {
                *x0 = x0.min(w.x);
                *x1 = x1.max(w.x1);
                ws.push(w);
            }
            None => columns.push((w.x, w.x1, vec![w])),
        }
    }
    columns.sort_by(|a, b| (b.0 + b.1).total_cmp(&(a.0 + a.1)));
    columns
        .into_iter()
        .map(|(x0, x1, mut ws)| {
            ws.sort_by(|a, b| a.y.total_cmp(&b.y));
            let em = (x1 - x0).max(1.0);
            let mut text = String::new();
            for (i, w) in ws.iter().enumerate() {
                if i > 0 && w.y - ws[i - 1].y1 >= em * 0.5 {
                    text.push('\u{3000}');
                } else if i > 0 && ws[i - 1].text.trim().ends_with(|c: char| c.is_ascii_alphanumeric()) && w.text.trim().starts_with(|c: char| c.is_ascii_alphanumeric()) {
                    // 横倒しの欧文の語の間（「PlayStation」「VR2」）。
                    text.push(' ');
                }
                text.push_str(w.text.trim());
            }
            trace::Line::text(text, ws.iter().map(|w| w.id).collect())
        })
        .collect()
}

/// 同じ x で端の接する縦の罫線の線分（行ごと・交点ごとに切れて描かれた線）をつなぐ。
fn join_vrules(vrules: &[(f32, f32, f32)]) -> Vec<(f32, f32, f32)> {
    let mut rules: Vec<(f32, f32, f32)> = vrules.iter().copied().filter(|r| r.0.is_finite()).collect();
    rules.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut joined: Vec<(f32, f32, f32)> = Vec::new();
    for r in rules {
        match joined.last_mut() {
            Some(l) if (l.0 - r.0).abs() <= 1.0 && r.1 <= l.2 + 3.0 => l.2 = l.2.max(r.2),
            _ => joined.push(r),
        }
    }
    joined
}

/// 縦の罫線だけで欄を区切った表（横倒しのページの財務諸表の損益計算書。行を区切る横線も枠の上下の線も無く、
/// 格子にならない）を表にする。`(表, 表に入らなかった語)`。
///
/// 上端と下端の揃う縦の罫線（途中で切れて描かれた線はつなぐ）を、間に別の縦の罫線（隣の表の線）が通らない
/// 範囲でまとめ、その間を欄とする。両端の字の無い欄は除く。2 欄以上で、罫線をまたぐ語も文（「。」）も無く、
/// 2 列目より右に金額の欄（行の 8 割以上が金額だけの行）があるものだけ（段の境の線と紙面の枠の線を欄と
/// 取り違えない）。行は、どの欄の行も（前の行に行間を空けずに続く折り返しでなければ）行の頭にする。
fn rule_column_tables(words: Vec<Col>, vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> (Vec<trace::Table>, Vec<Col>) {
    let mut joined = join_vrules(vrules);
    joined.retain(|r| r.2 - r.1 >= 60.0);
    let mut words = words;
    let mut tables = Vec::new();
    let mut seen: Vec<(f32, f32)> = Vec::new();
    for seed in joined.clone() {
        if seen.iter().any(|&(y0, y1)| (y0 - seed.1).abs() <= 4.0 && (y1 - seed.2).abs() <= 4.0) {
            continue;
        }
        seen.push((seed.1, seed.2));
        let (top, bottom) = (seed.1, seed.2);
        let mut xs: Vec<f32> = joined.iter().filter(|r| (r.1 - top).abs() <= 4.0 && (r.2 - bottom).abs() <= 4.0).map(|r| r.0).collect();
        xs.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        let inside = |w: &Col, x0: f32, x1: f32| {
            let (cx, cy) = (x_center(w), y_center(w));
            x0 <= cx && cx < x1 && top <= cy && cy < bottom
        };
        // 間に高さの違う縦の罫線（別の表の線）が通るところと、字の無い欄（左右に並ぶ 2 つの表の間の二重線・三重線）で
        // 切る（左右に離れて並ぶ 2 つの表の線を 1 つの表にしない）。
        // 欄の端に沿う線（15pt 以内。二重線の内側の線や、途中までしか引かれていない欄の枠）は別の表の線ではない。
        let between = |a: f32, b: f32| joined.iter().any(|r| r.0 > a + 15.0 && r.0 < b - 15.0 && r.2.min(bottom) - r.1.max(top) >= (bottom - top) * 0.5);
        let mut runs: Vec<Vec<f32>> = vec![Vec::new()];
        let n = xs.len();
        for (k, x) in xs.into_iter().enumerate() {
            let run = runs.last_mut().unwrap();
            // 字の無い広い欄の後ろに 1 欄だけ残る（右端の「法人単位」の欄）なら、字の無い欄（「—」が図形の金額の欄）も表の欄
            // （切ると右端の欄が表から外れ、金額だけが表の後ろに読まれていた）。
            let last_one = k + 2 == n;
            match run.last() {
                Some(&a) if between(a, x) || (!words.iter().any(|w| inside(w, a, x)) && !(last_one && x - a >= 20.0)) => runs.push(vec![x]),
                _ => run.push(x),
            }
        }
        for xs in runs {
            if xs.len() < 3 {
                continue;
            }
            let (left, right) = (xs[0], *xs.last().unwrap());
            let crossing = words.iter().any(|w| {
                let cy = y_center(w);
                top <= cy && cy < bottom && w.x1 > left && w.x < right && xs.iter().any(|&x| w.x < x - 2.0 && w.x1 > x + 2.0)
            });
            // 罫線のすぐ外（12pt 以内）に狭い欄の語（幅 40pt 以下）が 2 つ以上あれば、表は罫線の外へ続いている（外枠の
            // 線が取れなかった表。番号の欄が外に残る）。段の境の線の隣の記事の行（幅が広い）は数えない。
            let beyond = words
                .iter()
                .filter(|w| {
                    let cy = y_center(w);
                    top <= cy && cy < bottom && w.x1 - w.x <= 40.0 && ((w.x1 <= left + 1.0 && w.x1 > left - 12.0) || (w.x >= right - 1.0 && w.x < right + 12.0))
                })
                .count()
                >= 2;
            // 文の入った欄（段の境の線に挟まれた記事）は表ではない。
            let sentences = words.iter().any(|w| inside(w, left, right) && w.text.contains('。'));
            // 金額の欄: 行の 8 割以上が金額だけの行（語は桁区切りで切れていることがあるので、行の字で見る）。
            let numeric = xs.windows(2).skip(1).any(|p| {
                let ws: Vec<Col> = words.iter().filter(|w| inside(w, p[0], p[1])).cloned().collect();
                let lines = group_cell_lines(&ws);
                let amounts = lines.iter().filter(|l| amount_line(l)).count();
                amounts >= 2 && amounts * 10 >= lines.len() * 8
            });
            // 金額の欄が無くても、表の幅いっぱいの横の罫線で行を区切った表（地方交付税の補正係数の表の「十三」の行。
            // 上下の外枠は前後のページへ続くので無い）は、罫線で囲んだ升ごとに 1 つのセルにする。
            let rows: Vec<f32> = hrules
                .iter()
                .filter(|r| r.0 > top + 5.0 && r.0 < bottom - 5.0 && r.1 <= left + 3.0 && r.2 >= right - 3.0)
                .map(|r| r.0)
                .collect();
            let ys: Vec<f32> = std::iter::once(top).chain(rows.iter().copied()).chain(std::iter::once(bottom)).collect();
            // 升ごとに読む表は、最後の欄のほかはどの升も 1 項目（折り返して続く 1 つの文）で、2 欄以上に字のある行がある
            // （科目の並ぶ財務諸表や「〃」の並ぶ表は行ごとに読む）。新旧対照表（欄見出し「改正後」「改正前」）は除く。
            // 欄見出しは字間を空けて 1 字ずつの語に組まれる（「改　正　後」）ので、一番上の行の字をつないで見る。
            let shinkyu = {
                let mut head: Vec<&Col> = words.iter().filter(|w| inside(w, left, right) && rows.first().is_some_and(|&y| y_center(w) < y)).collect();
                head.sort_by(|a, b| a.x.total_cmp(&b.x));
                let t: String = head.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
                ["改正後", "改正前", "訂正後", "訂正前"].iter().any(|k| t.contains(k))
            };
            let blocks = !rows.is_empty() && !shinkyu && {
                let cell = |r: usize, c: usize| -> Vec<Line> {
                    let ws: Vec<Col> = words.iter().filter(|w| inside(w, xs[c], xs[c + 1]) && ys[r] <= y_center(w) && y_center(w) < ys[r + 1]).cloned().collect();
                    group_cell_lines(&ws)
                };
                (0..ys.len() - 1).all(|r| (0..xs.len() - 2).all(|c| cell(r, c).windows(2).all(|p| wrapped(&p[0], &p[1], xs[c + 1] + 0.5))))
                    && (0..ys.len() - 1).any(|r| (0..xs.len() - 1).filter(|&c| !cell(r, c).is_empty()).count() >= 2)
            };
            if crossing || beyond || sentences || (!numeric && !blocks) {
                continue;
            }
            if !numeric {
                let (mine, others): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| inside(w, left, right));
                words = others;
                tables.push(block_table(&xs, &ys, &mine));
                continue;
            }
            let g = Grid { xs: xs.clone(), ys: vec![top, bottom], cells: (0..xs.len() - 1).map(|i| GridCell { x0: i, x1: i + 1, y0: 0, y1: 1 }).collect() };
            let (mine, others): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| inside(w, left, right));
            words = others;
            match grid_table(&g, &mine, usize::MAX) {
                Some(t) => tables.push(t),
                None => words.extend(mine),
            }
        }
    }
    (tables, words)
}

/// 表の行の見出しの番号（「第2821号」「第165回」「第Ｐ－280号」）か、英字と数字の混じった記号
/// （自動車の認定番号「2026－ＧＥ0002」、船舶の信号符字「ＪＤ5605」「7ＫＶＰ」）か。
/// 記号は英字を含むものだけ（数字だけの欄（船舶番号・金額）を行の見出しと取り違えない）。
pub(crate) fn row_key(text: &str) -> bool {
    let t = text.trim();
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    let alpha = |c: char| c.is_ascii_alphabetic() || ('Ａ'..='Ｚ').contains(&c);
    let code_char = |c: char| digit(c) || alpha(c) || "－-".contains(c);
    let inner = t.strip_prefix('第').and_then(|t| t.strip_suffix(['号', '回']));
    let numbered = inner.is_some_and(|i| (1..=8).contains(&i.chars().count()) && i.chars().any(digit) && i.chars().all(code_char));
    // 「－」で始まるもの（折り返した名称の続き「BCM」「－204MD」）は記号ではない。
    let code = (3..=16).contains(&t.chars().count())
        && t.chars().all(code_char)
        && t.chars().any(digit)
        && t.chars().any(alpha)
        && !t.starts_with(['－', '-'])
        // 数と小文字の単位（「１ppm」「５mg」）は値で記号ではない（残留基準値の表の値が行の見出しになっていた）。
        && !(t.chars().filter(|c| alpha(*c)).all(|c| c.is_ascii_lowercase()) && t.trim_start_matches(digit).chars().all(|c| c.is_ascii_lowercase()));
    numbered || code || kanji_code(t)
}

/// 漢字の付いた登録番号（宅地建物取引業の免許番号「令08全保1384」、肥料の登録番号「生第100209号」「輸第4774号」、
/// 型式認定番号「交Ｎ26－67」）。漢字 1〜3 字・英数字・漢字 1〜3 字・数字・「号」の並び（前後の漢字と途中の
/// 漢字は省けるが、漢字は 2 か所まで）で、数字が 4 字以上（住所の「香川55－７」は除く）。日付・金額（年・月・日・円）
/// と住所（番地・丁目・町など。漢字の後が数字と「－」だけの「中島５－８－29」も）も除く。
fn kanji_code(t: &str) -> bool {
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    let alnum = |c: char| digit(c) || c.is_ascii_alphabetic() || ('Ａ'..='Ｚ').contains(&c) || "－-".contains(c);
    let kanji = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c) && !"年月日円号番地丁目条町村市区郡県".contains(c);
    let chars: Vec<char> = t.strip_suffix('号').unwrap_or(t).chars().collect();
    if !(4..=16).contains(&chars.len()) || chars.iter().filter(|&&c| digit(c)).count() < 4 || !chars.iter().any(|&c| kanji(c)) {
        return false;
    }
    // 漢字の並びと英数字の並びに分ける。
    let mut runs: Vec<(bool, usize)> = Vec::new();
    for &c in &chars {
        let k = if kanji(c) {
            true
        } else if alnum(c) {
            false
        } else {
            return false;
        };
        match runs.last_mut() {
            Some(r) if r.0 == k => r.1 += 1,
            _ => runs.push((k, 1)),
        }
    }
    let kanji_runs: Vec<usize> = runs.iter().filter(|r| r.0).map(|r| r.1).collect();
    // 漢字の後が数字と「－」だけで「－」を含み、「号」で終わらないものは住所の続き（「中島５－８－29」「大石原200－３」。
    // 縦組みの表の住所の欄の折り返し）。
    let tail: Vec<char> = chars.iter().rev().take_while(|&&c| !kanji(c)).copied().collect();
    let address = !t.ends_with('号') && tail.iter().any(|c| "－-".contains(*c)) && tail.iter().all(|&c| digit(c) || "－-".contains(c));
    // 漢字 1 字と数字だけ（「名２1121」「寺２１12」。「－」の字形が字の層に無い住所の折り返し）は記号ではない。記号は漢字が
    // 2 字以上（「生第」「令…全保」）か英字を含む（「交Ｎ26－67」）。
    let letters = chars.iter().any(|c| c.is_ascii_alphabetic() || ('Ａ'..='Ｚ').contains(c));
    let short = kanji_runs.iter().sum::<usize>() < 2 && !letters;
    // 英数字で終わり、漢字の並びは 2 つまで・各 3 字まで。
    !address && !short && runs.last().is_some_and(|r| !r.0) && kanji_runs.len() <= 2 && kanji_runs.iter().all(|&n| n <= 3)
}

/// 罫線の無い横組みの表で、左端の列に行の見出しの番号（[`row_key`]）が 3 つ以上縦に並ぶものを表に
/// する。`(表, 表に入らなかった語)`。縦組みの表は語を 90 度回して渡す（[`crate::vertical`]）。
///
/// - 行: 番号の上端から次の番号の上端まで。最後の行は、番号の列に字の無い行が続く限り（折り返しの続き）。
///   番号の列の字でも、行の間隔より近く続くものは最後の行の折り返し（認定番号の「－23」）
/// - 欄: 番号の行で番号の右に並ぶ語の左端のうち、2 つ以上の行で揃うもの（1 つの行だけの語の切れ目
///   （「キャデラック　７ＢＵ－」）を欄にしない）。右端は、番号の行の高さに掛かる縦の罫線（段の境の線）が
///   あればそこまで、無ければ番号から右へ 5 em 未満の空きで続く語まで（隣の段の行を入れない）
/// - セルの中の行は折り返しの続きとしてつなぐ（「第一種膨脹式救」「命いかだ（投下」「用）」）
pub(crate) fn keyed_tables(words: Vec<Col>, vrules: &[(f32, f32, f32)]) -> (Vec<trace::Table>, Vec<Col>) {
    let mut words = words;
    let mut tables = Vec::new();
    // 表にしなかった番号の列の番号。
    let mut rejected: std::collections::HashSet<usize> = std::collections::HashSet::new();
    loop {
        let mut keys: Vec<Col> = words.iter().filter(|w| row_key(&w.text) && !rejected.contains(&w.id)).cloned().collect();
        keys.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        // 左端が 2pt 以内の漢字の付いた登録番号は同じ列として上から並べる（左端が 1pt ずれた番号「輸第100238号」が、x の順で
        // 列の後ろに回って別の短い列になり、表に入らなかった）。
        {
            // 頭に漢字の付いた登録番号（「生第…号」「令08全保…」。「第…号」だけのものは除く）。
            let prefixed = |t: &str| kanji_code(t.trim()) && !t.trim().starts_with('第');
            let mut groups: Vec<Vec<Col>> = Vec::new();
            for k in keys {
                match groups.last_mut() {
                    Some(g) if (k.x - g[0].x).abs() < 2.0 && prefixed(&k.text) && prefixed(&g[0].text) => g.push(k),
                    _ => groups.push(vec![k]),
                }
            }
            for g in &mut groups {
                g.sort_by(|a, b| a.y.total_cmp(&b.y));
            }
            keys = groups.into_iter().flatten().collect();
        }
        // 番号 l と次の番号 k の間が、番号の列に字の無い行（折り返しの続き）で 2 em 未満の空きで続くか。
        let continuous = |l: &Col, k: &Col| {
            let em = (k.y1 - k.y).max(1.0);
            let mut tops: Vec<f32> = words
                .iter()
                .filter(|w| w.y > l.y + em * 0.5 && w.y < k.y - em * 0.5 && w.x1 > l.x && w.x < l.x + em * 40.0)
                .map(|w| {
                    if w.x < l.x1 + 1.0 {
                        f32::NAN
                    } else {
                        w.y
                    }
                })
                .collect();
            if tops.iter().any(|t| t.is_nan()) {
                return false;
            }
            tops.push(l.y);
            tops.push(k.y);
            tops.sort_by(f32::total_cmp);
            tops.windows(2).all(|p| p[1] - p[0] < em * 2.0)
        };
        // 左端の揃う番号の列（行の高さの 6 倍を超えて離れたら、間が折り返しの続きでなければ別の表）。
        let mut clusters: Vec<Vec<Col>> = Vec::new();
        for k in keys {
            let em = (k.y1 - k.y).max(1.0);
            // 間に、番号の列の左端から番号の列を越えて延びる、番号でない 5 字以上の語（表の区切りの見出し「有効期間が
            // 令和14年２月９日となったもの」）があれば別の表。
            let divided = |l: &Col, k: &Col| {
                words.iter().any(|w| {
                    let em = (l.y1 - l.y).max(1.0);
                    w.y > l.y1 && w.y1 < k.y && w.x <= l.x + em && w.x1 > l.x1 + em * 2.0 && w.text.chars().count() >= 5 && !row_key(&w.text)
                })
            };
            let joins = clusters.last().is_some_and(|c| {
                let l = c.last().unwrap();
                (k.x - c[0].x).abs() < 2.0 && k.y > l.y && (k.y - l.y < em * 6.0 || continuous(l, &k)) && !divided(l, &k)
            });
            match clusters.last_mut() {
                Some(c) if joins => c.push(k),
                _ => clusters.push(vec![k]),
            }
        }
        // 番号が行の頭にあること: 番号のすぐ左（2 em 未満）に同じ行の語が続く番号が半分を超える列は、表の途中の
        // 欄（型式「ＫＸ085－５」）か、別の語に続く記号の後ろ半分（「交　Ａ26－48」）なので行の見出しにしない。
        let leads_rows = |c: &Vec<Col>| {
            let led = c
                .iter()
                .filter(|k| {
                    let em = (k.y1 - k.y).max(1.0);
                    !words.iter().any(|w| {
                        w.id != k.id && (y_center(w) - y_center(k)).abs() <= em * 0.5 && w.x1 <= k.x + 0.5 && k.x - w.x1 < em * 2.0
                    })
                })
                .count();
            led * 2 >= c.len()
        };
        // 2 行だけの列も、すぐ上（2 em 以内）に数を含まない 3 語以上の欄見出しの行（「登録番号　肥料の種類　肥料の名称…」）
        // があれば表（1 つの区分に 2 件だけの登録の一覧が、セルの中の折り返しで行が割れていた）。
        let headed = |c: &Vec<Col>| {
            let k = &c[0];
            let em = (k.y1 - k.y).max(1.0);
            let above: Vec<&Col> = words
                .iter()
                .filter(|w| w.y1 <= k.y + 0.5 && w.y1 >= k.y - em * 2.0 && w.x >= k.x - em * 2.0 && !w.text.chars().any(|ch| ch.is_ascii_digit() || ('０'..='９').contains(&ch)))
                .collect();
            above.len() >= 3 && above.iter().any(|w| (w.x - k.x).abs() <= em * 2.0)
        };
        let Some(mut cluster) = clusters.into_iter().find(|c| (c.len() >= 3 || (c.len() == 2 && kanji_code(c[0].text.trim()) && !c[0].text.trim().starts_with('第') && headed(c))) && leads_rows(c)) else {
            break;
        };
        // 数字を含まない記号（信号符字「ＪＩＪＴ」「ＪＫＲＮ」）も、番号の列に左端が揃えば行の見出し。
        {
            let x = cluster[0].x;
            let (top, end) = (cluster.iter().map(|k| k.y).fold(f32::INFINITY, f32::min), cluster.iter().map(|k| k.y).fold(f32::NEG_INFINITY, f32::max));
            let span = (end - top) / (cluster.len() - 1).max(1) as f32;
            let letters = |t: &str| {
                let n = t.chars().count();
                (3..=8).contains(&n) && t.chars().all(|c| c.is_ascii_uppercase() || ('Ａ'..='Ｚ').contains(&c))
            };
            let extra: Vec<Col> = words
                .iter()
                .filter(|w| (w.x - x).abs() < 1.0 && letters(&w.text) && w.y > top - span && w.y < end + span && !cluster.iter().any(|k| k.id == w.id))
                .cloned()
                .collect();
            cluster.extend(extra);
        }
        cluster.sort_by(|a, b| a.y.total_cmp(&b.y));
        let em = cluster.iter().map(|k| k.y1 - k.y).fold(0.0, f32::max).max(1.0);
        let left = cluster[0].x - 1.0;
        let (first, final_y) = (cluster[0].y, cluster.last().unwrap().y1);
        // 別の表の行の見出し（右に並ぶ表の番号の列。この表の番号と同じ種類（「第…号」か記号か）で、同じ左端の
        // 番号がほかに 2 つ以上ある）。種類の違う記号の列（「第2821号」の表の型式「ＦＲＮ－ＳＡ－25」）は表の欄。
        let numbered = |t: &str| t.trim().starts_with('第');
        let kind = numbered(&cluster[0].text);
        let same_key = |w: &Col| row_key(&w.text) && numbered(&w.text) == kind;
        let other_key = |w: &Col| {
            !cluster.iter().any(|k| k.id == w.id)
                && same_key(w)
                && words.iter().filter(|o| o.id != w.id && same_key(o) && (o.x - w.x).abs() < 2.0).count() >= 2
        };
        // 表の右端の柵: 番号の行の高さに掛かる縦の罫線（段の境の線）か、右に並ぶ別の表の番号の列の左端。
        let rule_fence = vrules
            .iter()
            .filter(|&&(x, y0, y1)| x > cluster[0].x1 && y0 <= first && final_y <= y1)
            .map(|&(x, _, _)| x)
            .reduce(f32::min);
        let key_fence = words
            .iter()
            .filter(|w| w.x > cluster[0].x1 && w.y1 >= first - em && w.y <= final_y + em && other_key(w))
            .map(|w| w.x - 0.5)
            .reduce(f32::min);
        // 隣の段の本文の行の頭: 表の高さの範囲で、番号の行に揃わない語（隣の段の本文の行）が 3 つ以上、表の行より
        // 多く同じ左端から始まり、そこから始まる表の行が 3 分の 1 未満の x（罫線も溝も無く、日付の欄のすぐ右に
        // 組まれた段の本文）。欄の左端（セルの中の折り返し「ＮＥＰＴＵＮＥ／ＬＥＡＤＥＲ」「進水装置用膨脹／
        // 式救命いかだ」が多く始まる）は、表の行の多くもそこから始まる。
        let on_row = |w: &Col| cluster.iter().any(|k| (y_center(w) - y_center(k)).abs() <= em * 0.35);
        let mut flow_starts: Vec<f32> = words
            .iter()
            .filter(|w| w.x > cluster[0].x1 + em && w.y >= first - em && w.y1 <= final_y + em && !on_row(w))
            .map(|w| w.x)
            .collect();
        flow_starts.sort_by(f32::total_cmp);
        let flow_fence = flow_starts
            .iter()
            .copied()
            .filter(|&x| {
                let flow = flow_starts.iter().filter(|&&f| (f - x).abs() < 2.0).count();
                // 漢字の付いた登録番号の表は、欄の 1 行目が x に掛かる行も数える（折り返しの「17937」が、括弧の字「（６）」の分右の
                // x から始まり、1 行目の「埼玉県知事」はその左から始まる）。
                let kanji = kanji_code(cluster[0].text.trim());
                let rows = cluster
                    .iter()
                    .filter(|k| {
                        words.iter().any(|w| {
                            on_row(w) && (y_center(w) - y_center(k)).abs() <= em * 0.35 && ((w.x - x).abs() < 2.0 || (kanji && w.x < x && w.x1 > x + 1.0))
                        })
                    })
                    .count();
                flow >= 3 && flow > rows && rows * 3 < cluster.len()
            })
            .map(|x| x - 0.5)
            .reduce(f32::min);
        let fence = [rule_fence, key_fence, flow_fence].into_iter().flatten().reduce(f32::min);
        // 番号の行で番号から右へ続く語。
        let row_words = |k: &Col| -> Vec<&Col> {
            let mut row: Vec<&Col> = words.iter().filter(|w| (y_center(w) - y_center(k)).abs() <= em * 0.35 && w.x >= k.x1 - 0.5).collect();
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            row
        };
        // 表の右端: 番号の行で番号から右へ続く語の右端の最大。〃 だけの行のように欄の間が大きく空く行が
        // あっても、ほかの行で欄が続いていれば表の中。
        let mut right = cluster.iter().map(|k| k.x1).fold(f32::NEG_INFINITY, f32::max);
        for k in &cluster {
            let mut end = k.x1;
            for w in row_words(k) {
                let outside = match fence {
                    // 柵にわずかに掛かるだけの語（隣の段の頭の 0.1pt 右まで組んだ住所の行）は表の中。
                    Some(f) => w.x1 > f + em * 0.5,
                    None => w.x - end >= em * 5.0,
                };
                if outside || other_key(w) {
                    break;
                }
                end = end.max(w.x1);
            }
            right = right.max(end);
        }
        // 欄の左端（番号の行で前の語から空いて始まる語の左端と、その行の番号）。
        let mut starts: Vec<(f32, usize)> = Vec::new();
        for (ri, k) in cluster.iter().enumerate() {
            let mut end = k.x1;
            for w in row_words(k) {
                if w.x1 > right + 2.0 || other_key(w) {
                    break;
                }
                if w.x - end >= em * 0.75 {
                    starts.push((w.x, ri));
                }
                end = end.max(w.x1);
            }
        }
        starts.sort_by(|a, b| a.0.total_cmp(&b.0));
        // 左端の近い（1 em 以内）語の切れ目をまとめ、2 つ以上かつ行の 3 分の 1 以上の行で揃うものを欄にする
        // （字間を空けた欧文の船名の切れ目「ＭＩＴＳＵＩ　Ｏ」「ＣＯＳＭＯＳ　Ｌ」を欄にしない）。
        let mut runs: Vec<Vec<(f32, usize)>> = Vec::new();
        for s in starts {
            match runs.last_mut() {
                Some(run) if s.0 - run[0].0 <= em => run.push(s),
                _ => runs.push(vec![s]),
            }
        }
        let mut columns: Vec<f32> = vec![left];
        for run in runs {
            let rows: std::collections::HashSet<usize> = run.iter().map(|s| s.1).collect();
            let x = run[0].0;
            if rows.len() >= 2 && rows.len() * 3 >= cluster.len() && columns.last().is_some_and(|&c| x - c > em) {
                columns.push(x - 1.0);
            }
        }
        // 欄が 1 つ（記号が縦に並ぶだけ。表の途中の型式の欄など）なら表にしない。
        if columns.len() < 2 {
            rejected.extend(cluster.iter().map(|k| k.id));
            continue;
        }
        // 表の語: 番号の列の左端から右端まで、最初の番号の上端から。最後の行は、番号の列に字の無い行が
        // 1 em 未満の空きで続く限り。
        let top = cluster[0].y - 0.5;
        let last = cluster.last().unwrap().clone();
        let in_x = |w: &Col| w.x >= left && w.x1 <= right + 2.0;
        let mut bottom = last.y1;
        let key_end = columns.get(1).copied().unwrap_or(right) - 1.0;
        // 行の間隔（番号の上端の差の中央値）。
        let mut pitches: Vec<f32> = cluster.windows(2).map(|p| p[1].y - p[0].y).collect();
        pitches.sort_by(f32::total_cmp);
        let pitch = pitches[pitches.len() / 2];
        loop {
            // 次の行（いまの下端から 1 em 以内に始まる語）。番号の列に字があれば表の外（行の間隔より
            // 近ければ最後の行の番号の折り返し）。
            let next: Vec<&Col> = words
                .iter()
                .filter(|w| in_x(w) && w.y >= bottom - em * 0.5 && w.y < bottom + em && w.y > last.y + em * 0.5)
                .collect();
            // 番号の列から次の欄の先まで延びる、番号でない 5 字以上の語（表の区切りの見出し「有効期間が令和14年６月９日と
            // なったもの」）も表の外（最後の行の番号に付いていた）。
            let heading = |w: &Col| w.x < key_end && w.x1 > key_end + em * 2.0 && w.text.chars().count() >= 5 && !row_key(&w.text);
            if next.is_empty() || next.iter().any(|w| (w.x < key_end && w.y >= last.y + pitch * 0.75) || other_key(w) || heading(w)) {
                break;
            }
            // 次の行の語がどれも今の下端より上で終わる（下端の少し上から始まる小さい字）なら、下端は伸びず
            // 次も同じ語になるので、そこで止める。
            let grown = next.iter().map(|w| w.y1).fold(bottom, f32::max);
            if grown <= bottom {
                break;
            }
            bottom = grown;
        }
        let col_of = |w: &Col| columns.iter().rposition(|&c| c <= w.x + 1.0).unwrap_or(0);
        // 見出しの行: 最初の番号のすぐ上（3.5 em 以内）に続く行を下から上へ、どの語も 1 つの欄に収まり、番号の
        // 記号を含まない限り（「認定番号　車名及び型式　車体の形状」、2 行に組んだ「船舶／番号」）。見出しの語の
        // ある欄が 2 つ以上のときだけ見出しの行にする。
        let fits = |w: &Col| {
            columns.iter().rposition(|&c| c <= w.x + em * 0.5).is_some_and(|i| {
                w.x >= left - em && w.x1 <= columns.get(i + 1).map_or(right + 2.0, |&c| c + em * 0.5)
            })
        };
        // 字の小さい語（日付の欄の上の「令和」）は見出しではない。
        let above: Vec<Col> = words
            .iter()
            .filter(|w| {
                w.y1 <= cluster[0].y + 0.5
                    && w.y >= cluster[0].y - em * 3.5
                    && w.x1 > left
                    && w.x < right.min(fence.unwrap_or(f32::INFINITY))
                    && w.y1 - w.y >= em * 0.8
            })
            .cloned()
            .collect();
        let mut head: Vec<Col> = Vec::new();
        let mut head_top = cluster[0].y;
        for l in group_cell_lines(&above).into_iter().rev() {
            // 表の右端から始まる語（隣の段の本文の行の頭）は見出しに入れない。
            let (keep, outside): (Vec<Col>, Vec<Col>) = l.words.into_iter().partition(|w| fits(w));
            // 最初の行は表の 1.5 em 上まで、2 行目からは前の行に接するもの（2 行に組んだ見出し）だけ。
            // 1 行空いた上の行（署名「外務大臣　茂木　敏充」）は見出しではない。
            let gap = if head.is_empty() { em * 1.5 } else { em * 0.5 };
            if l.y1 < head_top - gap
                || keep.is_empty()
                || keep.iter().any(|w| row_key(&w.text))
                || outside.iter().any(|w| w.x < right - em)
            {
                break;
            }
            head_top = head_top.min(l.y0);
            head.extend(keep);
        }
        if head.iter().map(col_of).collect::<std::collections::HashSet<_>>().len() < 2 {
            head.clear();
            head_top = cluster[0].y;
        }
        let head_ids: std::collections::HashSet<usize> = head.iter().map(|w| w.id).collect();
        let off = usize::from(!head.is_empty());
        let (mine, others): (Vec<Col>, Vec<Col>) =
            words.into_iter().partition(|w| head_ids.contains(&w.id) || (in_x(w) && w.y >= top && w.y1 <= bottom + 0.5));
        words = others;
        let row_of = |w: &Col| {
            if head_ids.contains(&w.id) {
                0
            } else {
                cluster.iter().rposition(|k| k.y - em * 0.35 <= w.y).unwrap_or(0) + off
            }
        };
        let mut cells: std::collections::BTreeMap<(usize, usize), Vec<Col>> = std::collections::BTreeMap::new();
        for w in &mine {
            cells.entry((row_of(w), col_of(w))).or_default().push(w.clone());
        }
        let nc = columns.len();
        // 行の上端・下端（見出しの行は見出しの上端から最初の番号まで）。
        let row_top = |r: usize| if r < off { head_top } else { cluster[r - off].y };
        let row_bottom = |r: usize| if r < off { cluster[0].y } else { cluster.get(r - off + 1).map_or(bottom, |k| k.y) };
        let cells: Vec<trace::TableCell> = cells
            .into_iter()
            .map(|((row, col), ws)| {
                let lines = group_cell_lines(&ws);
                trace::TableCell {
                    row,
                    col,
                    rowspan: 1,
                    colspan: 1,
                    x0: columns[col],
                    y0: row_top(row),
                    x1: columns.get(col + 1).copied().unwrap_or(right),
                    y1: row_bottom(row),
                    text: lines.iter().map(|l| render_line(&l.words)).collect::<String>(),
                    words: ws.iter().map(|w| w.id).collect(),
                }
            })
            .collect();
        tables.push(trace::Table { x0: left, y0: top.min(head_top - 0.5), x1: right, y1: bottom, rows: cluster.len() + off, cols: nc, cells });
    }
    (tables, words)
}

/// 格子のセルに語を割り当てて横組みの表にする。中の語が横組みでなければ（縦組みの表）、
/// 字が 1 つのセルにしか無ければ、文章の入ったセル（囲み記事）があれば `None`。
///
/// 行を区切る罫線の無い欄（1 つのセルに複数の項目が並ぶ）は、項目ごとの行に分ける。行の頭は、
/// ほかの欄の行と高さが揃う行（揃わない行はセルの中の折り返し）。
///
/// 左から `item_cols` 欄は、ほかの欄の行と揃わない行も（折り返しの続きでなければ）行の頭にする（行を区切る横線の
/// 無い表。金額の無い科目の行「財務費用」「経常収益」が前の科目の行につながらないように）。0 なら普通の格子の表。
fn grid_table(g: &Grid, words: &[Col], item_cols: usize) -> Option<trace::Table> {
    // 2 字以上の語の字数で、横長の語が縦長の語より多いこと。
    // 欄の多い（7 欄以上の）率の表（桁区切りの無い表）の数だけの語（小数の率「1.47」が「1」「.」「47」に割れたもの）は
    // 向きを見ない（2 桁の語は縦長）。
    let rates = g.xs.len() >= 8 && !words.iter().any(|w| w.text.contains(','));
    let (h_chars, v_chars) = words.iter().fold((0, 0), |(h, v), w| {
        let n = w.text.chars().count();
        if n < 2 || (rates && w.text.chars().all(|c| c.is_ascii_digit() || ".".contains(c))) {
            (h, v)
        } else if w.x1 - w.x > w.y1 - w.y {
            (h + n, v)
        } else {
            (h, v + n)
        }
    });
    if words.is_empty() || h_chars <= v_chars {
        return None;
    }
    let mut cell_lines: Vec<Vec<Line>> = vec![Vec::new(); g.cells.len()];
    {
        let mut cell_words: Vec<Vec<Col>> = vec![Vec::new(); g.cells.len()];
        for w in words {
            if let Some(i) = g.cell_at(x_center(w), y_center(w)) {
                cell_words[i].push(w.clone());
            }
        }
        for (i, ws) in cell_words.into_iter().enumerate() {
            cell_lines[i] = group_cell_lines(&ws);
        }
    }
    if cell_lines.iter().filter(|l| !l.is_empty()).count() < 2 {
        return None;
    }
    let text_of = |i: usize| -> String { cell_lines[i].iter().flat_map(|l| l.words.iter().map(|w| w.text.as_str())).collect() };
    // 縦の罫線だけの表の欄は、行を区切る線が無いので 1 つのセルに科目が全部入る（字数では見ない）。
    // 項目名｜内容の表の内容の欄は文なので、文の数でも見ない（縦の罫線だけの表は文の無いものだけ）。
    // 1 行目がどの欄も短い見出し（40 字以内）で、どのセルも罫線の 1 行に収まる 3 行以上の表は、一覧の欄なら長くても表
    // （地方公共団体の名前を並べた欄｜「告示別表第一の…の項の機能要件の欄に掲げる機能要件」の別表。表にしないと欄ごとに
    // 縦に読んでいた）。科目の欄が罫線の行をまたぐ財務諸表（キャッシュ・フロー計算書の科目の欄が 1 つのセル）は除く。
    let headed = g.ys.len() >= 4 && g.xs.len() >= 3 && g.cells.iter().all(|c| c.y1 - c.y0 == 1) && {
        let top: Vec<usize> = (0..g.cells.len()).filter(|&i| g.cells[i].y0 == 0).collect();
        top.len() == g.xs.len() - 1 && top.iter().all(|&i| g.cells[i].y1 == 1 && !cell_lines[i].is_empty() && text_of(i).chars().count() <= 40)
    };
    // 見出しの無い表（前のページから続く表）でも、句点の無い、読点で区切った短い名前の一覧（地方公共団体の名前「北海道北見市」を
    // 並べた欄）は文ではない。8 つ以上の名前の 8 割以上が 12 字まで。
    let name_list = |t: &str| {
        let items: Vec<&str> = t.split('、').collect();
        !t.contains('。') && items.len() >= 8 && items.iter().filter(|s| s.chars().count() <= 12).count() * 10 >= items.len() * 8
    };
    let prose_cell = |i: usize| {
        let t = text_of(i);
        // 見出し付きの表で長くてもよいのは、読点で区切った一覧の欄（間接法のキャッシュ・フロー計算書の様式の科目を並べた
        // 欄は行ごとに読む）。
        let list = (headed && t.matches('、').count() >= 5) || name_list(&t);
        // 5 欄以上の表の句点の無い長い欄は名前の一覧（出資者の「…投資事業有限責任組合」を何行も並べた欄）で文ではない。
        // 数の多い欄（率を並べた「普通態容補正」の欄）は除く。
        let names = g.xs.len() >= 6 && !t.contains('。') && t.chars().filter(|c| c.is_ascii_digit()).count() * 20 <= t.chars().count();
        // 番号を振った項目の並び（「１　ポック形成単位測定法を用いるとき。」「２　プラーク…とき。」）は文の欄ではない。
        let items = cell_lines[i].iter().filter(|l| crate::vertical::starts_item(&render_line(&l.words))).count() >= 2;
        item_cols == 0 && ((!list && !names && t.chars().count() > 200) || (t.matches('。').count() >= 2 && !items))
    };
    // 文は最後の欄にだけあり、その行の前の欄に短い項目名（40 字以内）があれば、項目名｜内容の表（遺伝子組換え生物等の
    // 承認の告示の「遺伝子組換え生物等の第一種使用等の方法」｜「…」）。
    // 項目名の下に欄をまたいで組んだ内容の文（項目名の無い行）も、項目名｜内容の行が 2 つ以上ある表なら同じ。
    let last = g.xs.len() - 1;
    let label_of = |i: usize| {
        let gc = g.cells[i];
        (0..g.cells.len()).any(|j| {
            let o = g.cells[j];
            o.x1 <= gc.x0 && o.y0 < gc.y1 && gc.y0 < o.y1 && !cell_lines[j].is_empty() && text_of(j).chars().count() <= 40
        })
    };
    let label_rows = (0..g.cells.len()).filter(|&i| g.cells[i].x1 == last && g.cells[i].x0 > 0 && !cell_lines[i].is_empty() && label_of(i)).count();
    let labeled = |i: usize| g.cells[i].x1 == last && g.cells[i].x0 > 0 && (label_of(i) || label_rows >= 2);
    let prose = (0..cell_lines.len()).any(|i| prose_cell(i) && !labeled(i));
    let long_cells = (0..cell_lines.len()).any(|i| {
        let t = text_of(i);
        t.chars().count() > 200 && ((headed && t.matches('、').count() >= 5) || name_list(&t))
    });
    if prose {
        return None;
    }
    // セル i の k 行目が、前の行の折り返しの続きか。前の行が文で、セルの右端まで組まれている
    // （右揃えの金額の行は右端に届くが、折り返しではない）。
    let wraps = |i: usize, k: usize| {
        k > 0
            && (wrapped(&cell_lines[i][k - 1], &cell_lines[i][k], g.xs[g.cells[i].x1])
                || narrow_wrapped(&cell_lines[i][k - 1], g.xs[g.cells[i].x0], g.xs[g.cells[i].x1]))
    };
    // 行の頭の高さ（格子の行ごと）。
    let ny = g.ys.len() - 1;
    // 表の頭の欄見出しの行（数を含まない罫線の行が 3 行まで続き、その下に数のある行がある）。欄見出しのセルの
    // 折り返し（「資本剰余金／合計」「前中期目標期／間繰越積立金」）は 1 つの見出しなので、行を分けずにつなぐ。
    let digits = |i: usize| text_of(i).chars().any(|c| c.is_ascii_digit());
    let digit_row = |r: usize| (0..g.cells.len()).any(|i| g.cells[i].y0 <= r && r < g.cells[i].y1 && digits(i));
    // 数のある行までまたぐセル（金額の「—」が字の層に無い行から下の科目をまとめた 1 つの科目の欄）の行も欄見出しではない。
    let header_rows = (0..ny)
        .take_while(|&r| {
            // 科目の名前の行が 2 つ以上あるセル（金額がどれも「—」の「流動資産」「固定資産」）の行も欄見出しではない。
            // 1 行目が「科目」の欄見出しの表だけ（純資産変動計算書の欄見出しの折り返し「前中期目標期間繰越積立金」は見ない）。
            let kamoku = (0..g.cells.len()).any(|j| g.cells[j].y0 == 0 && text_of(j).replace(['　', ' '], "").starts_with("科目"));
            let accounts = |i: usize| kamoku && g.cells[i].y0 > 0 && cell_lines[i].iter().filter(|l| account_end(l)).count() >= 2;
            !digit_row(r)
                && !(0..g.cells.len()).any(|i| g.cells[i].y0 <= r && r < g.cells[i].y1 && !cell_lines[i].is_empty() && ((r..g.cells[i].y1).any(digit_row) || accounts(i)))
        })
        .count();
    let header_rows = if header_rows <= 3 && header_rows < ny && item_cols == 0 { header_rows } else { 0 };
    let header_cell = |i: usize| g.cells[i].y1 <= header_rows;
    let mut anchors: Vec<f32> = Vec::new();
    // 科目の欄（行間で行を分けたセル）。
    let mut item_cells = vec![false; g.cells.len()];
    // 罫線で区切った行の中で、どの欄も 1 項目なら（1 項目を折り返して上下中央に置いた欄だけの行）、行をさらに分けない
    // （欄の 1 行目の高さがたまたま揃って行が割れていた）。
    // 項目の数: 前の行がセルの右端の 1em 以内まで組まれていない行は項目の頭（狭い欄の「流動負債」「固定負債」は
    // 右端の 2em 以内に届くので、折り返しの判定より厳しく見る）。
    let items = |i: usize| {
        let right = g.xs[g.cells[i].x1];
        (0..cell_lines[i].len())
            .filter(|&k| {
                k == 0 || {
                    let p = &cell_lines[i][k - 1];
                    let end = p.words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
                    !wraps(i, k) || right - end > (p.y1 - p.y0).max(1.0)
                }
            })
            .count()
    };
    // 1 つのセルの中で、項目名と数を空けて並べた行（交付税の補正係数の表の「２級地（旧４級地）　1.118　1.117」。値の欄が
    // 罫線で分かれていない）。数を含み、語の間に 1.5 字分以上の空きがある。
    let rowlike = |l: &Line| {
        let em = (l.y1 - l.y0).max(1.0);
        let mut ws: Vec<&Col> = l.words.iter().collect();
        ws.sort_by(|a, b| a.x.total_cmp(&b.x));
        ws.iter().any(|w| w.text.chars().any(|c| c.is_ascii_digit())) && ws.windows(2).any(|p| p[1].x - p[0].x1 >= em * 1.5)
    };
    let rowlike_cell = |i: usize| cell_lines[i].len() >= 3 && cell_lines[i].iter().filter(|l| rowlike(l)).count() * 2 >= cell_lines[i].len();
    for r in 0..ny {
        let band: Vec<usize> = (0..g.cells.len()).filter(|&i| g.cells[i].y0 <= r && r < g.cells[i].y1).collect();
        anchors.push(g.ys[r]);
        let (top, bottom) = (g.ys[r], g.ys[r + 1]);
        // 長い欄のある見出し付きの表（[`headed`]）は罫線の行で行を分ける（折り返した欄の行の高さが隣の欄の行と揃い、
        // 左右の欄の行が 1 行ずつ交互に組まれていた）。
        // 4 欄以上に字のある罫線の行で、項目の数がいちばん多い欄が 1 つだけなら、欄ごとに行数の違う 1 件の記録（品種登録の
        // 表の「第31951号／令和８年10月５日」｜…｜住所と名称を何行も並べた欄）なので、行をさらに分けない（欄の行の高さが
        // たまたま揃って、1 件が 2〜3 行に割れ、欄の中の字が行をまたいで混ざっていた）。金額だけの行のある行（財務諸表）は見ない。
        let record = {
            let counts: Vec<usize> = band.iter().filter(|&&i| !cell_lines[i].is_empty()).map(|&i| items(i)).collect();
            let max = counts.iter().copied().max().unwrap_or(0);
            let numeric = |l: &Line| {
                let t = render_line(&l.words);
                t.chars().any(|c| c.is_ascii_digit()) && t.chars().all(|c| c.is_ascii_digit() || "△▲,.()（）－-―—　 ".contains(c))
            };
            let amounts = band.iter().any(|&i| cell_lines[i].iter().any(numeric));
            counts.len() >= 4 && max >= 2 && counts.iter().filter(|&&c| c == max).count() < 2 && !amounts && !band.iter().any(|&i| rowlike_cell(i))
        };
        // 左端の欄の行が「第31826号」のような番号で始まる行と、その下に重ねた年月日（「令和８年８月24日」）の行なら、番号の
        // 行だけを行の頭にする（品種登録の表。1 件が 2 行に割れ、2 行目の欄の字（住所の続き・公表の年月日の続き）が別の行に
        // なっていた）。
        // 番号の欄は左端でなくてもよい（品種登録の取下げの表の「品種登録の番号」の欄）。番号の行のある一番左の欄。
        let key_line = |l: &Line| {
            let t = render_line(&l.words);
            let digits = t.chars().skip(1).take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c)).count();
            // 番号だけの行（文の折り返し「第183号）第４条に…」は除く）。左端の欄は後ろに字が続いてもよい。
            t.starts_with('第') && digits >= 3 && t.chars().nth(digits + 1) == Some('号') && (t.trim_end().chars().count() == digits + 2 || l.words.first().is_some_and(|w| w.text.trim().chars().count() == digits + 2))
        };
        let key_col = band.iter().filter(|&&i| cell_lines[i].iter().any(|l| key_line(l))).map(|&i| g.cells[i].x0).min();
        let key_rows: Vec<f32> = band
            .iter()
            .filter(|&&i| Some(g.cells[i].x0) == key_col)
            .flat_map(|&i| cell_lines[i].iter())
            .filter(|l| key_line(l))
            .map(|l| l.y0 - 0.5)
            .collect();
        let first_lines = band.iter().filter(|&&i| Some(g.cells[i].x0) == key_col).map(|&i| cell_lines[i].len()).sum::<usize>();
        // 左端でない欄では、ほかの欄に番号の行より多く行があるときだけ（1 件が何行にも割れているとき）。
        let split_record = key_col == Some(0) || band.iter().any(|&i| cell_lines[i].len() > key_rows.len());
        if r >= header_rows && !key_rows.is_empty() && (key_rows.len() < first_lines || (key_col != Some(0) && split_record)) {
            anchors.extend(key_rows.into_iter().filter(|&y| top < y && y < bottom));
            continue;
        }
        if item_cols == 0 && ny > 1 && (long_cells || r < header_rows || record || band.iter().all(|&i| items(i) <= 1)) && !band.iter().any(|&i| rowlike_cell(i)) {
            continue;
        }
        for &i in &band {
            let aligned: Vec<bool> = cell_lines[i]
                .iter()
                .enumerate()
                .map(|(k, l)| {
                    let c = l.center();
                    // 欄によって字の種類（数字と漢字）で字面の高さが少し違うので、行の高さの 4 割まで。
                    let tol = ((l.y1 - l.y0) * 0.4).max(ALIGN_TOL);
                    // 前の行に行間を空けずに続く行（科目の折り返し「資産見返物品受贈／額」）は、隣の欄の行と揃うときだけ
                    // 行の頭（左右に並べた貸借対照表の右の表の行と高さが揃い、折り返しが別の行になっていた）。
                    let snug = k > 0 && l.y0 - cell_lines[i][k - 1].y1 < (l.y1 - l.y0) * 0.25;
                    let near = |j: usize| !snug || g.cells[j].x0 == g.cells[i].x1 || g.cells[j].x1 == g.cells[i].x0;
                    // 揃う 2 行がどちらも折り返しの続きなら、行の頭ではない（2 つの欄が同時に折り返した）。
                    band.iter().filter(|&&j| g.cells[j].x0 != g.cells[i].x0 && near(j)).any(|&j| {
                        cell_lines[j].iter().enumerate().any(|(n, m)| !(wraps(i, k) && wraps(j, n)) && (m.center() - c).abs() <= tol)
                    })
                })
                .collect();
            // 左端の欄で 3 行以上がほかの欄の行と揃うなら、科目の欄（行を区切る横線の無い財務諸表）なので、揃わない
            // 行（金額の無い科目「財務収益」）も行の頭にする。
            let rows_of_items = g.cells[i].x0 < item_cols || (g.cells[i].x0 == 0 && aligned.iter().filter(|&&a| a).count() >= 3);
            // 行間が行の区切りの手掛かりになるのは、行間の空く所と詰まった所の両方があるセルだけ（どの行も行間を
            // 詰めて組んだセルは、行間では科目と折り返しを見分けられない）。
            let spaced = (1..cell_lines[i].len()).any(|k| cell_lines[i][k].y0 - cell_lines[i][k - 1].y1 >= (cell_lines[i][k].y1 - cell_lines[i][k].y0) * 0.25);
            item_cells[i] |= rows_of_items && spaced;
            // 科目の行の折り返しは前の行に行間を空けずに続く（科目と科目の間は行間が空く。「有形固定」「資産合計」
            // 「２無形固定資産」）。
            let tight = |k: usize| k > 0 && cell_lines[i][k].y0 - cell_lines[i][k - 1].y1 < (cell_lines[i][k].y1 - cell_lines[i][k].y0) * 0.25;
            // 前の行と次の行がどちらも科目の名前で終われば、行間を空けずに続いても次の科目（金額の「―」が字の層に無い
            // 科目「固定負債」が上の科目「流動負債」に続いていた）。
            // 前の行より字が大きければ次の科目（長い科目だけ小さい字で 2 行に組み、次の科目「法人税等調整額」が
            // 「法人税、住民税及び／事業税」に続いていた）。小さい字の科目の 1 行目は、金額が 2 行目の高さにあるので
            // 行の頭にしない（隣の表の小さい字の科目の折り返しが割れる）。数や項目番号を含む行（「１２．０／ｍ岸壁」
            // 「き／二　条鰭亜綱」）は、字の大きさが字の種類で違うだけなので見ない。
            let resized = |p: &Line, l: &Line| {
                let (a, b) = (render_line(&p.words), render_line(&l.words));
                let plain = |t: &str| !t.chars().any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
                (l.y1 - l.y0) > (p.y1 - p.y0) * 1.25
                    && a.chars().count() >= 2
                    && plain(&a)
                    && plain(&b)
                    && !crate::vertical::starts_item(&b)
                    // 括弧の閉じていない行の続き（「荷さばき施設（荷さ／ばき地）」）でもない。
                    && a.matches(['（', '(']).count() <= a.matches(['）', ')']).count()
            };
            let next_item = |k: usize| {
                k > 0
                    && ((account_end(&cell_lines[i][k - 1]) && account_end(&cell_lines[i][k]))
                        || section_heading(&cell_lines[i][k - 1])
                        || section_heading(&cell_lines[i][k])
                        || resized(&cell_lines[i][k - 1], &cell_lines[i][k]))
            };
            for (k, l) in cell_lines[i].iter().enumerate() {
                let c = l.center();
                // 項目番号で始まる行（「10　発行日」の次の「11　発行価格」）は、行間を空けずに続いても次の項目。
                let numbered = k > 0 && crate::vertical::starts_item(&render_line(&l.words));
                // 金額だけの欄の行は、行間を空けて続けば行の頭（同じ行のほかの欄が字の層に無い「—」だけで、揃う行が無い
                // 金額「△2」が前の行につながっていた）。
                // ほかの欄のどの行とも高さが重ならない行だけ（その行のほかの欄は字が無い）。
                let alone_in_row = |l: &Line| !band.iter().filter(|&&j| j != i).any(|&j| cell_lines[j].iter().any(|m| m.y0 < l.y1 && l.y0 < m.y1));
                let figures = !cell_lines[i].is_empty() && cell_lines[i].iter().all(|m| {
                    let t = render_line(&m.words);
                    t.chars().any(|ch| ch.is_ascii_digit()) && t.chars().all(|ch| ch.is_ascii_digit() || "△▲,.()（）－-　 ".contains(ch))
                });
                // 前の行も項目名と数の行なら、右端まで組まれていても（数は右揃え）折り返しではない。
                let own_row = rowlike_cell(i) && k > 0 && rowlike(l) && (rowlike(&cell_lines[i][k - 1]) || !wraps(i, k));
                if top < c && c < bottom && (own_row || aligned[k] || next_item(k) || (rows_of_items && (!tight(k) || numbered)) || (figures && k > 0 && !tight(k) && alone_in_row(l))) {
                    anchors.push(l.y0 - 0.5);
                }
            }
        }
    }
    anchors.sort_by(f32::total_cmp);
    anchors.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    let row_of = |y: f32| anchors.iter().rposition(|&a| a <= y).unwrap_or(0);
    // 左端の欄の、右の欄の行をいくつもまたぐ 1 項目のセル（行をまたいで上下中央に置いた「資産の部」「負債及び純資産の部」、
    // 1 字ずつ縦に積んだ欄の名前）は、またぐ行の 1 行目に置く（中ほどの行の科目「固定資産」に付いていた）。
    let group_label_row = |i: usize| -> Option<usize> {
        let gc = g.cells[i];
        let ls = &cell_lines[i];
        let all: Vec<&Col> = ls.iter().flat_map(|l| l.words.iter()).collect();
        if gc.x0 != 0 || ls.is_empty() || (ls.len() != 1 && !stacked(&all)) || header_cell(i) {
            return None;
        }
        let (top, bottom) = (g.ys[gc.y0], g.ys[gc.y1]);
        let right: Vec<&Line> = (0..g.cells.len())
            .filter(|&j| g.cells[j].x0 >= gc.x1)
            .flat_map(|j| cell_lines[j].iter())
            .filter(|l| top < l.center() && l.center() < bottom)
            .collect();
        let mut rows: Vec<usize> = right.iter().map(|l| row_of(l.center())).collect();
        rows.sort_unstable();
        rows.dedup();
        // 罫線の 1 行の中の 2 行（「(2,738,256)」と「103,045,208」の上下 2 段の金額）だけなら、下の行に付いたままにする。
        if !(rows.len() >= 3 || (rows.len() >= 2 && gc.y1 - gc.y0 >= 2)) {
            return None;
        }
        // 1 行目が括弧の中の数（内数）だけや、2 字までのルビの行なら、行の頭ではない。
        let first: Vec<String> = right.iter().filter(|l| row_of(l.center()) == rows[0]).map(|l| render_line(&l.words)).collect();
        // 表の頭の、数の無い行をまたぐ欄見出し（「大分類」「小分類」の 2 行と「細分類」の下の「番号」「項目名」）は動かさない
        // （上の欄見出しの下の欄見出しの並びの前に出て、欄の順が崩れる）。
        let heading = gc.y0 == 0 && !right.iter().any(|l| render_line(&l.words).chars().any(|c| c.is_ascii_digit()));
        if heading {
            return None;
        }
        let inner = first.iter().all(|t| t.starts_with(['(', '（']));
        let ruby = first.iter().map(|t| t.chars().count()).sum::<usize>() <= 2;
        (!inner && !ruby).then(|| rows[0])
    };
    let mut cells: Vec<trace::TableCell> = Vec::new();
    for (i, ls) in cell_lines.iter().enumerate() {
        let gc = g.cells[i];
        let mut by_row: std::collections::BTreeMap<usize, Vec<&Line>> = std::collections::BTreeMap::new();
        // 縦組みの欄の名前（1 字ずつ縦に積んだ字）は、行に分けずに一番上の行に置く。
        let all: Vec<&Col> = ls.iter().flat_map(|l| l.words.iter()).collect();
        // 欄の多い（6 欄以上の）表の、行を 2 つ以上またぐ左端以外のセルの、行間を詰めた短い 4 行までの字（たばこの小売定価の
        // 表の製造国「イタリア／ギリシャ／ルーマニア」、製品の区分「Ｆ45　20スティック／ハードパック　5.4ｇ」）も 1 つの
        // 値なので、またぐ行の一番上の行に置く（行の高さに合わせて 1 行ずつ別の品目の行に分かれていた）。
        let block = g.xs.len() >= 7
            && gc.x0 > 0
            && gc.y1 - gc.y0 >= 2
            && (2..=4).contains(&ls.len())
            && ls.iter().all(|l| render_line(&l.words).chars().count() <= 16)
            && (1..ls.len()).all(|k| ls[k].y0 - ls[k - 1].y1 < (ls[k].y1 - ls[k].y0) * 0.3);
        let whole = stacked(&all) || block;
        for l in ls {
            // 罫線の行を 2 つ以上またぐ狭い欄で、2 字ずつ折り返した 1 項目（様式の表の欄見出し「株式／引受／金」）も、一番上の
            // 行に置く。
            let narrow = (1..ls.len()).all(|k| narrow_wrapped(&ls[k - 1], g.xs[gc.x0], g.xs[gc.x1]));
            // 2 行以上の欄見出しで、見出しの最後の行まで掛かるセル（「純資産合計」「積立金」を上下中央に置いたもの）は、見出しの
            // 最後の行に置く（下の段の欄見出しが欄の順に 1 行に並ぶ。上下中央の高さの行に分かれていた）。
            let row = if header_rows >= 2 && header_cell(i) && gc.y1 == header_rows && gc.y1 - gc.y0 >= 2 {
                row_of(g.ys[header_rows - 1] + 0.5)
            } else if let Some(first) = group_label_row(i) {
                first
            } else if whole || (gc.y1 - gc.y0 > 1 && (narrow || header_cell(i))) {
                row_of(ls[0].center())
            } else {
                row_of(l.center())
            };
            by_row.entry(row).or_default().push(l);
        }
        for (row, ls) in by_row {
            let words: Vec<&Col> = ls.iter().flat_map(|l| l.words.iter()).collect();
            cells.push(trace::TableCell {
                row,
                col: gc.x0,
                rowspan: 1,
                colspan: gc.x1 - gc.x0,
                x0: g.xs[gc.x0],
                y0: g.ys[gc.y0],
                x1: g.xs[gc.x1],
                y1: g.ys[gc.y1],
                text: if stacked(&words) {
                    // 縦組みの欄の名前（「負債及び／純資産の部」を 1 字ずつ縦に並べたもの）は、横組みの表の
                    // 中なので左の列から上→下に読む。
                    let mut ws = words.clone();
                    ws.sort_by(|a, b| (x_center(a) / 4.0).round().total_cmp(&(x_center(b) / 4.0).round()).then(a.y.total_cmp(&b.y)));
                    ws.iter().map(|w| w.text.as_str()).collect()
                } else {
                    // セルの右端まで組んだ行の次の行は折り返しの続き（「神奈川県横浜市中」「区」）なのでつなぐ。
                    let mut text = String::new();
                    for (k, l) in ls.iter().enumerate() {
                        // 科目の欄の同じ行の 2 行目からは、行間を空けずに続く折り返し（右端まで組んでいない「掛金及過去
                        // 勤務掛金収」「入」も）。
                        let continued = item_cells[i] && l.y0 - ls[k.max(1) - 1].y1 < (l.y1 - l.y0) * 0.25;
                        // 2 行とも科目の名前で終われば別の科目（字間を空けて欄の幅いっぱいに組んだ「流　動　負　債」は右端に届く）。
                        let items = k > 0 && ((account_end(ls[k - 1]) && account_end(l)) || section_heading(ls[k - 1]) || section_heading(l));
                        // 行末の金額（「（旧無級地であつた地域）　1.012」）の次の行は折り返しではない（金額の欄の無い升に
                        // 項目と係数を並べた表）。
                        let after_amount = k > 0 && amount_end(ls[k - 1]) && !amount_tail(ls[k - 1], l);
                        let narrow = k > 0 && narrow_wrapped(ls[k - 1], g.xs[gc.x0], g.xs[gc.x1]);
                        // 長い欄のある見出し付きの表の欄は、名前や項の一覧を折り返したもの（行を中央に寄せて組み、右端に届かない）。
                        // 項目番号で始まる行（内容の欄の「（１）　令和９年９月15日から…」）は折り返しの続きではない。
                        let numbered = k > 0 && crate::vertical::starts_item(&render_line(&l.words));
                        if k > 0 && !long_cells && !(header_cell(i) && !numbered) && (items || after_amount || numbered || (!wrapped(ls[k - 1], l, g.xs[gc.x1]) && !narrow && !amount_tail(ls[k - 1], l) && !continued)) {
                            text.push('\n');
                        }
                        text.push_str(&render_line(&l.words));
                    }
                    text
                },
                words: ls.iter().flat_map(|l| l.words.iter().map(|w| w.id)).collect(),
            });
        }
    }
    // 科目の欄で折り返した科目（「（役員退任給与引当」「金）」）が、隣の欄の行の頭で 2 行に分かれ、金額が 2 行目に
    // しか無ければ、1 行目の字を 2 行目の頭へ移す（科目と金額を同じ行にする）。1 行目の科目が欄の右端まで組まれていて、
    // 2 行目の科目が 3 字以内のときだけ。
    let at = |cells: &[trace::TableCell], row: usize, col: usize| cells.iter().position(|c| c.row == row && c.col == col);
    let mut k = 0;
    while k < cells.len() {
        let (row, col) = (cells[k].row, cells[k].col);
        let next = at(&cells, row + 1, col);
        let amount_here = at(&cells, row, col + 1).is_some_and(|i| !cells[i].text.is_empty());
        let amount_next = at(&cells, row + 1, col + 1).is_some_and(|i| !cells[i].text.is_empty());
        let reaches = {
            let ws: Vec<&Col> = words.iter().filter(|w| cells[k].words.contains(&w.id)).collect();
            let end = ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
            let em = ws.iter().map(|w| w.y1 - w.y).fold(0.0, f32::max).max(1.0);
            cells[k].x1 - end <= em * WRAP_EM
        };
        // 科目は字で書き（金額の欄を動かさない）、続きは字と括弧だけ。見出しの行（1 行目）は動かさない。
        let kana_kanji = |c: char| ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c);
        // 1 行目は科目の途中で切れている（「販売費及び一般管理」「法人税・住民税及び」）。科目の名前や括弧で終わるもの
        // （「有形固定資産」「（負債合計）」。金額が離れた欄にある）は動かさない。
        const TERMS: [&str; 14] = ["負債", "資産", "資本", "利益", "損失", "収益", "費用", "引当金", "剰余金", "準備金", "積立金", "差額金", "合計", "株式"];
        let t = cells[k].text.trim_end();
        let label = t.chars().any(kana_kanji) && !t.chars().any(|c| c.is_ascii_digit()) && !TERMS.iter().any(|e| t.ends_with(e)) && !t.ends_with(['）', ')']);
        let tail = |t: &str| t.chars().all(|c| kana_kanji(c) || "（）()".contains(c));
        // 折り返しの続きは行間を空けずに続く（別の科目の行は行間が空く）。
        let bottom = |i: usize| words.iter().filter(|w| cells[i].words.contains(&w.id)).map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
        let top = |i: usize| words.iter().filter(|w| cells[i].words.contains(&w.id)).map(|w| w.y).fold(f32::INFINITY, f32::min);
        let height = words.iter().filter(|w| cells[k].words.contains(&w.id)).map(|w| w.y1 - w.y).fold(0.0, f32::max).max(1.0);
        if let Some(n) = next.filter(|&n| {
            row > 0
                && label
                && tail(&cells[n].text)
                && !amount_here
                && amount_next
                && reaches
                && cells[n].text.chars().count() <= 3
                && !cells[k].text.contains('\n')
                && top(n) - bottom(k) < height * 0.25
        }) {
            let (text, ids) = (cells[k].text.clone(), cells[k].words.clone());
            cells[n].text = text + &cells[n].text;
            cells[n].words.splice(0..0, ids);
            cells.remove(k);
            continue;
        }
        k += 1;
    }
    // 字の無い行は詰める。
    let mut used: Vec<usize> = cells.iter().map(|c| c.row).collect();
    used.sort_unstable();
    used.dedup();
    for c in &mut cells {
        c.row = used.iter().position(|&r| r == c.row).unwrap();
    }
    Some(trace::Table {
        x0: g.left(),
        y0: g.top(),
        x1: g.right(),
        y1: g.bottom(),
        rows: used.len(),
        cols: g.xs.len() - 1,
        cells,
    })
}

/// 行が小数点か桁区切りのある数（「1.012」「3,373,355」）で終わるか。金額は桁の区切りで語が割れる（「1」「.」「012」）
/// ので、行の字の末尾の数で見る。
fn amount_end(l: &Line) -> bool {
    let t = render_line(&l.words);
    let tail: String = t.chars().rev().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',')).collect();
    tail.starts_with(|c: char| c.is_ascii_digit()) && tail.contains(['.', ','])
}

/// 科目の欄の区分の見出しの行（「（業務外損益の部）」「(資産の部)」）か。前後の行とは別の行（金額の「—」が字の層に無いと、
/// 前の「業務損失」と次の「業務外収益」の行と 1 行になっていた）。
fn section_heading(l: &Line) -> bool {
    let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
    // 大きな区分の番号で始まる行（「Ⅲ　財務活動によるキャッシュ・フロー」）も。
    (t.starts_with(['（', '(']) && (t.ends_with("の部）") || t.ends_with("の部)")) && t.chars().count() <= 14)
        || (t.starts_with(|c: char| ('Ⅰ'..='Ⅻ').contains(&c))
            && !t.chars().any(|c| c.is_ascii_digit())
            && t.chars().count() <= 24
            // 名前の終わりまである行だけ（「Ⅱ資本剰余金の当期変」「動額」の折り返しは別の行にしない）。
            && ["フロー", "額", "益", "失", "用", "等", "計", "高", "金", "費", "産", "債", "本"].iter().any(|e| t.ends_with(e)))
}

/// 行が科目の名前（頭に 2 字以上ある）で終わるか（「流動負債」「固定資産」「営業外収益」「賞与引当金」「その他利益剰余金」）。
/// 続く 2 行がどちらも科目の名前で終わるなら、折り返し（「固定資産」「売却益」）ではなく別の科目。
fn account_end(l: &Line) -> bool {
    const TERMS: [&str; 17] = ["負債", "資産", "資本", "利益", "損失", "損益", "収益", "費用", "引当金", "剰余金", "準備金", "積立金", "差額金", "合計", "株式", "原価", "財産"];
    let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
    // 名前の頭が 2 字以上（「引当金」だけの行は「うち利息返還損失」「引当金」の折り返し）。損益計算書の 1 行目の「売上高」も
    // （金額の「—」が字の層に無いと、次の「売上原価」の行が「売上高」の行につながっていた）。
    t == "売上高" || t == "基金" || TERMS.iter().any(|e| t.ends_with(e) && t.chars().count() >= e.chars().count() + 2)
}

/// 語が縦に積まれているか（縦組みの欄の名前）。2 語・3 字以上で、どの語も 1〜2 字か縦長で、全体が
/// 横幅以上の高さ（2 列に組んだ「資産｜の部」は正方形に近い）。
fn stacked(words: &[&Col]) -> bool {
    let (x0, x1) = words.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), w| (a.min(w.x), b.max(w.x1)));
    let (y0, y1) = words.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), w| (a.min(w.y), b.max(w.y1)));
    let upright = |w: &&Col| (w.text.chars().count() <= 2 && w.x1 - w.x <= (w.y1 - w.y) * 1.5) || w.y1 - w.y > (w.x1 - w.x) * 1.5;
    // 同じ欄に縦に並んだ「〃」（行ごとの繰り返しの印）も欄の名前ではない。
    let text: String = words.iter().map(|w| w.text.as_str()).collect();
    // 横の行に 1〜2 字の語が 3 つ以上並ぶなら、字間を空けた横組みの項目名（「事　業　収　益」）。
    // 縦長の語（「及び純資産の部」）は行にまとめると何行にも掛かるので数えない。
    let owned: Vec<Col> = words.iter().filter(|w| w.text.chars().count() <= 2).map(|w| (*w).clone()).collect();
    words.len() >= 2
        && text.chars().count() >= 3
        && !text.chars().all(|c| c == '〃')
        // 1 桁の金額の並び（「0」「0」…「△」「—」）は欄の名前ではない。
        && !text.chars().all(|c| c.is_ascii_digit() || "△▲－—-―,.、\u{E61C}".contains(c))
        && words.iter().all(upright)
        && y1 - y0 >= x1 - x0
        && group_lines(&owned).iter().all(|l| l.words.len() <= 2)
}

/// 横書きの 1 行（y が重なる語の並び、x 昇順）。
#[derive(Clone)]
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

/// 上付き・下付きの小さな字だけの行（式「10⁻⁶」の「−６」。数と符号で、字の高さが隣の行の字の 7 割未満で 4 字まで）が、隣の行と上下に
/// 重なり、その行のどの語とも左右に重ならなければ、その行に入れる（号外第 217 号 11 ページ: 「（0.2×f×10−６＋15）Hz」の
/// 「−６」が行の前の別の行になっていた。e-Gov の本文との突き合わせで見つけた）。
fn attach_small_script_lines(lines: &mut Vec<Line>) {
    let chars = |l: &Line| l.words.iter().map(|w| w.text.chars().filter(|c| !c.is_whitespace()).count()).sum::<usize>();
    // 上付き・下付きの字（数と符号）。
    // 小さな字の注の印（科目の右肩の「（注）」「（注１）」）も。
    let script = |l: &Line| {
        let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
        let note = t.starts_with(['（', '(']) && t.ends_with(['）', ')']) && t.chars().nth(1) == Some('注');
        note || t.chars().all(|c| c.is_ascii_digit() || "−-＋+()（）.０１２３４５６７８９".contains(c))
    };
    // 語の字の高さ（行の高さは段の違う語が混ざると大きくなるので、語で比べる）。
    let glyph = |l: &Line| l.words.iter().map(|w| w.y1 - w.y).fold(f32::INFINITY, f32::min);
    let body = |l: &Line| {
        let mut hs: Vec<f32> = l.words.iter().filter(|w| !w.text.trim().is_empty()).map(|w| w.y1 - w.y).collect();
        hs.sort_by(f32::total_cmp);
        hs.get(hs.len() / 2).copied().unwrap_or(0.0)
    };
    let mut i = 0;
    while i < lines.len() {
        let small = chars(&lines[i]) <= 4 && chars(&lines[i]) > 0 && script(&lines[i]);
        let target = [i + 1, i.wrapping_sub(1)].into_iter().find(|&j| {
            lines.get(j).is_some_and(|m| {
                let l = &lines[i];
                let overlap = l.y1.min(m.y1) - l.y0.max(m.y0);
                small
                    && glyph(l) < body(m) * 0.7
                    && overlap >= (l.y1 - l.y0) * 0.3
                    && !l.words.iter().any(|w| m.words.iter().any(|v| v.x1.min(w.x1) - v.x.max(w.x) > 0.5))
            })
        });
        if let Some(j) = target {
            let l = lines.remove(i);
            let j = if j > i { j - 1 } else { j };
            let m = &mut lines[j];
            m.words.extend(l.words);
            m.words.sort_by(|a, b| a.x.total_cmp(&b.x));
            i = i.saturating_sub(1);
            continue;
        }
        i += 1;
    }
}

/// 横倒しの縦書きの行の縦中横の単位（「MHz」「kHz」。表示では幅 1 字・高さが字数分の縦長の枠）は、枠が 1 字分（と 1pt）
/// 右へずれて出るので、左へ戻す（号外第 217 号 11 ページ「一・四MHzのもの」が「一・四　のものMHz」になっていた。
/// e-Gov の本文との突き合わせで見つけた）。
fn unshift_upright_latin(words: &mut [Col]) {
    for w in words.iter_mut() {
        let (width, height) = (w.x1 - w.x, w.y1 - w.y);
        // 単位だけ（社名の「Style」なども縦長の枠になるが、縦中横ではない）。
        const UNITS: [&str; 22] =
            ["Hz", "kHz", "MHz", "GHz", "THz", "mW", "kW", "dB", "dBm", "dBi", "mA", "kV", "nm", "mm", "cm", "km", "kg", "mg", "mL", "ml", "ppm", "μm"];
        if UNITS.contains(&w.text.trim()) && width > 0.0 && height >= width * 1.6 {
            w.x -= width + 1.0;
            w.x1 -= width + 1.0;
        }
    }
}

/// 表のセルの語を行に束ねる。[`group_lines`] より厳しく、語の中心 y が行の最初の語の中心から行の
/// 高さの 35% 以内か、行と高さの半分以上重なって行のどの語とも左右に重ならない語だけを同じ行に
/// する（金額の欄は行送りが字の高さとほぼ同じで、半行ずれて真下に置かれた「0」が上の行の範囲に
/// 掛かり、「1,627,662」とつながっていた。字の種類で字面の高さが違う「3」「,298,613」はつなぐ）。
fn group_cell_lines(words: &[Col]) -> Vec<Line> {
    let mut sorted: Vec<&Col> = words.iter().collect();
    sorted.sort_by(|a, b| y_center(a).total_cmp(&y_center(b)));
    let mut lines: Vec<(f32, Line)> = Vec::new();
    for w in sorted {
        let c = y_center(w);
        match lines.last_mut() {
            Some((c0, l))
                if (c - *c0).abs() <= (w.y1 - w.y).max(1.0) * 0.35
                    || (l.y1.min(w.y1) - l.y0.max(w.y) >= (w.y1 - w.y) * 0.5 && !l.words.iter().any(|v| v.x < w.x1 && w.x < v.x1)) =>
            {
                l.y0 = l.y0.min(w.y);
                l.y1 = l.y1.max(w.y1);
                l.words.push(w.clone());
            }
            _ => lines.push((c, Line { y0: w.y, y1: w.y1, words: vec![w.clone()] })),
        }
    }
    let mut lines: Vec<Line> = lines
        .into_iter()
        .map(|(_, mut l)| {
            unshift_upright_latin(&mut l.words);
            l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
            l
        })
        .collect();
    attach_small_script_lines(&mut lines);
    lines
}

/// 語を行に束ねる。語の中心 y が直前の行の y 範囲に入れば同じ行とする。
fn group_lines(words: &[Col]) -> Vec<Line> {
    // 行の高さの 2.5 倍を超える縦長の語（表の左の縦組みの欄の名前「負債及び正味財産の部」）は、行の高さを広げて隣の
    // 行を 1 行にまとめてしまうので、行に入れずに 1 語の行にして、掛かる最初の行の前に置く。
    let mut heights: Vec<f32> = words.iter().map(|w| w.y1 - w.y).collect();
    heights.sort_by(f32::total_cmp);
    let median = heights.get(heights.len() / 2).copied().unwrap_or(0.0);
    let tall = |w: &Col| median > 0.0 && w.y1 - w.y > median * 2.5 && w.text.trim().chars().count() >= 2 && w.x1 - w.x < (w.y1 - w.y) * 0.5;
    let (talls, words): (Vec<&Col>, Vec<&Col>) = words.iter().partition(|w| tall(w));
    let mut lines = group_line_words(words);
    for t in talls {
        let at = lines.iter().position(|l| l.y1 > t.y).unwrap_or(lines.len());
        lines.insert(at, Line { y0: t.y, y1: t.y1, words: vec![t.clone()] });
    }
    lines
}

fn group_line_words(words: Vec<&Col>) -> Vec<Line> {
    let mut sorted = words;
    sorted.sort_by(|a, b| y_center(a).total_cmp(&y_center(b)));
    let mut lines: Vec<Line> = Vec::new();
    for w in sorted {
        let c = y_center(w);
        match lines.last_mut() {
            Some(l) if c > l.y0 && c < l.y1 => {
                // 括弧 1 字の語（負の金額の「(△785)」の「(」は、隣の「△」の字に合わせて下にずれて置かれる）は行の
                // 高さを広げない（広げると次の行の語の中心が行に入り、2 行が 1 行になる）。
                if !matches!(w.text.trim(), "(" | ")" | "（" | "）") {
                    l.y0 = l.y0.min(w.y);
                    l.y1 = l.y1.max(w.y1);
                }
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
        unshift_upright_latin(&mut l.words);
        l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    attach_small_script_lines(&mut lines);
    lines
}

/// 領域の語を読み順に並べ、行文字列として `out` へ積む（再帰的な XY-cut）。
///
/// 1. 縦の溝（列間）で左右に切れるなら、左→右の順に再帰する。
/// 2. 切れなければ、横に切って上下どちらかが列組みになる位置を探し、上→下に再帰する。
///    行間は 1〜2pt しかないため、あらゆる行間で切ると多段組の行が交互に並んでしまう。
///    「列組みになる部分」が最も大きくなる 1 か所だけで切る（全幅記事と段組の境目）。
/// 3. どちらも無ければ 1 本の流れとして行を上→下に出す。
///
/// `blocks`（取り除いた表の外枠）は語と同じく溝・切れ目をふさぎ、中心で左右・上下に振り分ける。
///
/// `hrules`（横の罫線 `(y, x0, x1)`）のうち、領域の幅いっぱいに掛かってさらに領域の外（左右の列）まで延びる線は、
/// 列の溝より先に上下の境にする（[`box_rule_cut`]）。
/// `boxed` は囲みの境の線で分けた段（[`is_column_split`]）。
/// `column` は列の溝で左右に分けた後の領域（囲みの境の線で切るのはその中だけ）。
fn order_region(mut words: Vec<Col>, blocks: &[Block], ctx: &Context, boxed: bool, column: bool, out: &mut Vec<trace::Line>) {
    let (vrules, hrules) = (ctx.vrules, ctx.hrules);
    if words.is_empty() {
        return;
    }
    let lines = group_lines(&words);
    // 語の左右の範囲の外にある表（紙面の隣の段いっぱいの表）は、語の領域の切れ目をふさがない。
    let (wx0, wx1) = (words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min), words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max));
    let near: Vec<Block> = blocks.iter().filter(|b| b.0 < wx1 && b.2 > wx0).copied().collect();
    let blocks = near.as_slice();
    // 横に並ぶ囲みの見出しどうしが 1 行にまとまった領域（[`multi_row`]）も、列の溝で左右に分ける。
    if lines.len() >= 2 || multi_row(&words) {
        // 紙面の幅いっぱいに 1 本で引いた横の罫線（囲みの段の境）で先に上下に切る（号外の決算公告のページで、横長の囲みの中に
        // 貸借対照表 2 つと縦組みの合併公告が並ぶ段が上下に重なるもの。列で先に切ると、どの段も左の貸借対照表が先に読まれた）。
        // 段の中は囲みとして読む（囲みの中の表と表の間の溝で左右に分ける）。
        if let Some(cut) = band_rule_cut(&words, blocks, hrules, ctx.tables) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, ctx, true, column, out);
            order_region(bottom, &bb, ctx, true, column, out);
            return;
        }
        if let Some((cut, top, bottom)) = paired_tables_cut(&words, ctx.tables) {
            let (above, rest): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < top);
            let (zone, below): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(|w| y_center(w) <= bottom);
            // 損益計算書の期間の「（自」「至」は表の左端より左に出ることがあるので、すぐ右（1.5em 以内）に右の欄の語が
            // 続く 2 字以内の語は右の欄に入れる。
            let label = |w: &Col| {
                let em = (w.y1 - w.y).max(1.0);
                w.text.trim().chars().filter(|c| !"（）()〔〕".contains(*c)).count() <= 2
                    && zone.iter().any(|r| x_center(r) >= cut && (y_center(r) - y_center(w)).abs() <= ALIGN_TOL && r.x >= w.x1 - 0.5 && r.x - w.x1 < em * 1.5)
            };
            let sides: Vec<bool> = zone.iter().map(|w| x_center(w) < cut && !label(w)).collect();
            let (mut left, mut right): (Vec<Col>, Vec<Col>) = (Vec::new(), Vec::new());
            for (w, l) in zone.into_iter().zip(sides) {
                if l { left.push(w) } else { right.push(w) }
            }
            let part = |ws: &[Col]| -> Vec<Block> {
                let y0 = ws.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
                let y1 = ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
                blocks.iter().filter(|b| (b.1 + b.3) / 2.0 >= y0 && (b.1 + b.3) / 2.0 <= y1).copied().collect()
            };
            for ws in [above, left, right, below] {
                let bs = part(&ws);
                order_region(ws, &bs, ctx, boxed, column, out);
            }
            return;
        }
        // 囲みの列の間の広い溝で先に列に分け、列の中を囲みの境の横の罫線で上下に分ける。
        // 囲みの段の中では、表と表の間（囲みの境、囲みの中の貸借対照表と損益計算書の境）の溝を囲みの境とみなす。
        // 表の枠の下端（左右の欄を分ける縦の罫線がそこで終わる、領域の幅いっぱいの横の罫線）より下の行（表の備考・
        // 附則）は、左右の欄より後に読む（左の欄の下に続く短い行が左の欄の続きとして読まれていた）。
        if let Some(cut) = table_bottom_cut(&words, blocks, hrules, vrules) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, ctx, boxed, column, out);
            order_region(bottom, &bb, ctx, boxed, column, out);
            return;
        }
        if let Some(cut) = divider_end_cut(&words, blocks, vrules) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, ctx, boxed, column, out);
            order_region(bottom, &bb, ctx, boxed, column, out);
            return;
        }
        let gutters = column_gutters(&words, blocks, if boxed { ctx.tables } else { &[] }, boxed);
        let gutter = gutters.as_ref().map(|g| g.0);
        // 広い溝（囲みの間）で左右とも 100pt 以上の列に分かれる溝が 2 つ以上並ぶなら、まとめて切る（横に 3 つ並ぶ決算公告の
        // 囲みの見出しの行。左の溝で切った残りは段の幅に満たず、右の溝が列の境とみなされなかった）。
        if let Some((_, cuts)) = gutters.as_ref().filter(|(g, cuts)| !boxed && cuts.len() >= 2 && cuts[0].0 == g.0 && cuts.iter().all(|c| c.1 >= WIDE_GUTTER * 2.0 && c.2 >= 100.0)) {
            if rule_cut(&words, blocks, vrules, hrules).is_none() && box_rule_cut(&words, blocks, hrules).is_none() {
                let xs: Vec<f32> = cuts.iter().map(|c| c.0).collect();
                let mut parts: Vec<(Vec<Col>, Vec<Block>)> = vec![(Vec::new(), Vec::new()); xs.len() + 1];
                for w in std::mem::take(&mut words) {
                    let k = xs.iter().filter(|&&x| x < x_center(&w)).count();
                    parts[k].0.push(w);
                }
                for b in blocks {
                    let k = xs.iter().filter(|&&x| x < (b.0 + b.2) / 2.0).count();
                    parts[k].1.push(*b);
                }
                // 囲みは同じくらいの幅（表の欄は幅がまちまち）。
                let widths: Vec<f32> = parts.iter().map(|(ws, _)| ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - ws.iter().map(|w| w.x).fold(f32::INFINITY, f32::min)).collect();
                let (narrow, wide) = (widths.iter().copied().fold(f32::INFINITY, f32::min), widths.iter().copied().fold(f32::NEG_INFINITY, f32::max));
                if narrow >= 100.0 && wide <= narrow * 1.3 {
                    for (ws, bs) in parts {
                        order_region(ws, &bs, ctx, false, true, out);
                    }
                    return;
                }
                words = parts.into_iter().flat_map(|(ws, _)| ws).collect();
            }
        }
        if let Some(cut) = box_rule_cut(&words, blocks, hrules).filter(|_| column && !gutter.is_some_and(|g| g.1 >= WIDE_GUTTER)) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, ctx, true, true, out);
            order_region(bottom, &bb, ctx, true, true, out);
            return;
        }
        if let Some(cut) = rule_cut(&words, blocks, vrules, hrules).or_else(|| gutter.map(|g| g.0)) {
            if let Some(y) = stray_cut(&words, blocks, hrules, cut) {
                let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < y);
                let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < y);
                order_region(top, &bt, ctx, boxed, column, out);
                order_region(bottom, &bb, ctx, boxed, column, out);
                return;
            }
            if let Some(t) = blocks.is_empty().then(|| header_table(&words)).flatten() {
                out.push(trace::Line { text: t.render(), words: Vec::new(), table: Some(t) });
                return;
            }
            let (left, mut right): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| x_center(w) < cut);
            // 囲みの中で、右の頭の 1〜2 行が左のどの語よりも上にあり、署名（「代表取締役　中島　勝」）なら、左右の欄より先に
            // 読む（会社名の下の右寄せの署名が、左の貸借対照表の後の、右の損益計算書の頭に読まれていた）。
            if boxed {
                let left_top = left.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
                let (head, rest): (Vec<Col>, Vec<Col>) = right.iter().cloned().partition(|w| w.y1 < left_top - 1.0);
                let head_lines = group_lines(&head);
                let signer = |l: &Line| {
                    let t: String = l.words.iter().map(|w| w.text.as_str()).collect();
                    ["代表取締役", "取締役", "代表社員", "代表理事", "理事長", "清算人", "代表清算人", "執行役"].iter().any(|k| t.starts_with(k))
                };
                if !head.is_empty() && !rest.is_empty() && head_lines.len() <= 2 && head_lines.first().is_some_and(signer) {
                    out.extend(head_lines.iter().map(|l| trace::Line::text(render_line(&l.words), l.words.iter().map(|w| w.id).collect())));
                    right = rest;
                }
            }
            let (bl, br): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.0 + b.2) / 2.0 < cut);
            // 囲みの段として切るのは段の中の最初の溝だけ（囲みの中は普通に読む）。紙面の幅いっぱいの罫線で切った段
            // （[`band_rule_cut`]。列の中ではない）は、左右の残りにも囲みが並ぶので（3 列の決算公告、合併公告の下に
            // 貸借対照表が 2 つ並ぶ囲み）、左右とも囲みの段として読む。
            order_region(left, &bl, ctx, boxed && !column, true, out);
            order_region(right, &br, ctx, boxed && !column, true, out);
            return;
        }
        if blocks.is_empty() {
            if let Some(t) = label_table(&words, &lines) {
                out.push(trace::Line { text: t.render(), words: Vec::new(), table: Some(t) });
                return;
            }
        } else {
            // 表の上の語が項目名｜内容の表なら、表の上端で切って先に組む（国債の発行条件の「20　払込期日」の下に
            // 「（別表）」の罫線の表が続くもの。行間で上下に切ると、「14　利子」の内容の折り返しと算式の下の「15　償還期限」
            // 〜「17」が項目名と内容に離れていた）。
            let top = blocks.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
            let (above, rest): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| w.y1 <= top + 1.0);
            let above_lines = group_lines(&above);
            if let Some(t) = label_table(&above, &above_lines) {
                out.push(trace::Line { text: t.render(), words: Vec::new(), table: Some(t) });
                order_region(rest, blocks, ctx, boxed, column, out);
                return;
            }
        }
        if let Some(cut) = band_cut(&lines, blocks) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, ctx, boxed, column, out);
            order_region(bottom, &bb, ctx, boxed, column, out);
            return;
        }
    }
    out.extend(
        period_heading(join_wrapped_labels(lines))
            .iter()
            .map(|l| trace::Line::text(render_line(&l.words), l.words.iter().map(|w| w.id).collect())),
    );
}

/// 罫線の無い財務諸表で、狭い科目の欄に折り返した科目の続きの行（「賞与引当」「金」、「鉄道施設建」「設見返債務」
/// 「（注）」）を前の行の科目につなぐ。続きの行は、前の行の頭の語と同じ x から始まり、行間を空けずに続き（科目と
/// 科目の間は行間が空く）、数を含まない短い字（8 字まで）だけで、前の行の 2 つ目の語（金額）より左に収まる。
/// 前の行は科目の後ろに金額のある行か、科目だけの行（「資産見返負」「債（注）」）。
fn join_wrapped_labels(lines: Vec<Line>) -> Vec<Line> {
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    // 金額の後ろに離して置いた単位の「円」（傍線の切れ目で「237,029,519」と「円」が別の語になる）も金額の語。
    let amount = |w: &Col| {
        w.text.chars().any(|c| c.is_ascii_digit()) || (!w.text.trim().is_empty() && w.text.trim().chars().all(|c| "△▲,.()（）－―-".contains(c))) || w.text.trim() == "円"
    };
    // つないだ続きの語は幅を無くして科目の語の後ろに置く（後ろの語との空きを変えない）。
    let joined = |w: &Col| w.x1 - w.x < 0.01;
    let mut out: Vec<Line> = Vec::with_capacity(lines.len());
    for l in lines {
        if let Some(p) = out.last_mut() {
            let em = p.words.iter().map(|w| w.y1 - w.y).fold(0.0, f32::max).max(1.0);
            let text: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
            // 続きの行の頭と同じ x から始まる科目の語（「Ⅲ　法令に基づく」「引当金等」は 2 つ目の語）。
            let head = l.words.first().and_then(|f| p.words.iter().position(|w| !joined(w) && (w.x - f.x).abs() <= 1.0));
            if let Some(h) = head {
                // 科目の語と、それにつないだ語の後ろ。
                let after = h + 1 + p.words[h + 1..].iter().take_while(|w| joined(w)).count();
                let limit = p.words.get(after).map(|w| w.x).unwrap_or(f32::INFINITY);
                let hw = &p.words[h];
                // 注の番号（「（注１）」「１）」）は数でも科目の続き。
                let note = {
                    let t = text.trim_start_matches(['（', '(']).trim_start_matches('注');
                    let t = t.trim_end_matches(['）', ')']);
                    !t.is_empty() && t.chars().all(digit) && (text.ends_with(['）', ')']))
                };
                // 科目の名前の途中で折り返した行（「教育・研究及び診療の質」「の向上及び組織運営改善」「積立金（目的積立金）」、
                // 「大学改革支援・学位」「授与機構債務負担金」）は 20 字まで。科目の名前で終わる行の次は別の科目のことがあるので
                // 8 字まで。
                let head_text: String = p.words[h..after].iter().map(|w| w.text.as_str()).collect();
                const ENDS: [&str; 22] = [
                    "金", "額", "費", "益", "失", "債務", "資産", "負債", "資本", "収入", "支出", "勘定", "等", "）", ")", "計", "高", "価", "料", "税", "当", "備",
                ];
                // 文の終わり（「。」「：」）の次や、見出しの括弧（「＜払戻しを行う…＞」「【…】」）で始まる行は続きではない。
                let unfinished = !ENDS.iter().any(|e| head_text.trim_end().ends_with(e)) && !head_text.trim_end().ends_with(['。', '：', ':']);
                if text.starts_with(['＜', '<', '【', '〈', '〔']) {
                    out.push(l);
                    continue;
                }
                let n = text.chars().count();
                // 後ろの注の印（「額（注１）」の「（注１）」）の数は見ない。
                let body = text.rfind(['（', '(']).filter(|&k| text[k..].trim_start_matches(['（', '(']).starts_with('注') && text.ends_with(['）', ')'])).map(|k| &text[..k]).unwrap_or(&text);
                let tail = (n >= 1 && (n <= 8 || (n <= 20 && unfinished)))
                    && (!body.chars().any(digit) || note)
                    // 続きの行は 1 つの欄の中（語の間が空かない。表の別の欄の「日」を科目の続きにしない）。
                    && l.words.windows(2).all(|q| q[1].x - q[0].x1 < em * 0.5)
                    && l.words.iter().all(|w| w.x1 <= limit)
                    && l.y0 - p.y1 < em * 0.2
                    && hw.text.chars().count() >= 3
                    // 数は 6 字以上の科目の 1 字まで（「１年以内回収予定長期貸」。住所「…２－10－14」・型式の記号は科目ではない）。
                    && match hw.text.chars().filter(|&c| digit(c)).count() {
                        0 => true,
                        // 金額のある行だけ（金額の無い住所の行「…二丁目２番地」の下の会社名をつながない）。
                        1 => {
                            hw.text.chars().count() >= 6
                                && p.words.len() > after
                                && p.words[after..].iter().all(|w| w.text.trim().chars().all(|c| c.is_ascii_digit() || ",.△▲()（）".contains(c)))
                        }
                        _ => false,
                    }
                    // 科目の後ろは金額だけ（文の行の折り返しではない）。
                    && p.words[after..].iter().all(|w| amount(w));
                if tail {
                    // 字は科目の語に足し、続きの語は字の無い語にして置く（語の組み先は残す。1 字の続きの「金」が後ろの金額の
                    // 頭の 1 字の語「2」と字間空けの見出しのようにつながらないように）。
                    let at = hw.x1;
                    let added: String = l.words.iter().map(|w| w.text.as_str()).collect();
                    p.words[after - 1].text.push_str(&added);
                    for (n, mut w) in l.words.into_iter().enumerate() {
                        w.x = at + n as f32 * 0.001;
                        w.x1 = w.x + 0.001;
                        w.text.clear();
                        p.words.insert(after + n, w);
                    }
                    p.y1 = p.y1.max(l.y1);
                    continue;
                }
            }
        }
        if let Some(p) = out.last_mut() {
            if join_wrapped_cells(p, &l) {
                continue;
            }
        }
        out.push(l);
    }
    out
}

/// 罫線の無い表の行で、欄の中で折り返した続きの行（「輸第105684号　化成肥料　ポリリン酸アンモニウ　セントラルグリーン」の
/// 下の「ム　株式会社　3418番地」）を、上の行のそれぞれの欄の語につなぐ。続きの行は、上の行の最初の欄（登録番号）に
/// 語が無く、行間を空けずに続き、どの語も上の行のどれかの欄の語の左右の範囲に収まる。上の行は 4 欄以上。
fn join_wrapped_cells(p: &mut Line, l: &Line) -> bool {
    if p.words.len() < 3 || l.words.is_empty() {
        return false;
    }
    let em = p.words.iter().map(|w| w.y1 - w.y).fold(0.0, f32::max).max(1.0);
    // 行の頭の欄が「第31826号」のような番号の欄なら、その欄に重ねた年月日（「令和８年８月24日」）から始まる続きの行も
    // つなぐ（品種登録の表の「品種登録の番号及び年月日」の欄）。次の行の番号から始まる行はつながない。
    let key = |t: &str| {
        let t = t.trim();
        t.starts_with('第') && t.ends_with('号') && t.chars().count() >= 4 && t.chars().skip(1).take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c)).count() >= 2
    };
    let keyed = key(&p.words[0].text) && !key(&l.words[0].text) && l.words[0].x < p.words[0].x1 && l.words[0].x1 > p.words[0].x;
    if l.y0 - p.y1 > em * 0.3 || (l.words[0].x < p.words[0].x1 + em && !keyed) {
        return false;
    }
    // 4 欄以上の行（1 字分以上空けて並ぶ語が 4 つ以上）だけ（決算公告の「日付　住所」の下の会社名は欄の続きではない）。
    let cells = 1 + p.words.windows(2).filter(|q| q[1].x - q[0].x1 >= em).count();
    if cells < 4 {
        return false;
    }
    // 上の行の欄（語）ごとに、続きの語の入る欄を探す（欄の左端から右端の 1 字先まで）。
    let slot = |w: &Col| p.words.iter().position(|c| w.x >= c.x - em * 0.5 && w.x1 <= c.x1 + em * 2.0 && w.x < c.x1);
    let slots: Vec<Option<usize>> = l.words.iter().map(slot).collect();
    if slots.iter().any(|s| s.is_none()) {
        return false;
    }
    // 同じ欄の語は前の行の語の後ろに続ける。上の行の語の後ろに字を足し、続きの語は字の無い語にして置く。
    let mut added: Vec<(usize, Col)> = Vec::new();
    for (w, s) in l.words.iter().zip(slots) {
        let k = s.unwrap();
        p.words[k].text.push_str(&w.text);
        let mut e = w.clone();
        e.text.clear();
        e.x = p.words[k].x1;
        e.x1 = e.x + 0.001;
        added.push((k, e));
    }
    for (k, e) in added.into_iter().rev() {
        p.words.insert(k + 1, e);
    }
    p.y1 = p.y1.max(l.y1);
    true
}

/// 損益計算書の見出しの右に 2 行に組んだ期間（「自　令和７年４月１日」「至　令和８年３月31日」）は、見出しと「至」の行が
/// 同じ高さで 1 行になり、「自」の行が見出しより先になる（「自　令和７年４月１日」「損益計算書の要旨至　令和８年３月31日」）。
/// 見出し・「自」の行・「至」の行の順にする。
fn period_heading(lines: Vec<Line>) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::with_capacity(lines.len());
    let mut it = lines.into_iter().peekable();
    while let Some(l) = it.next() {
        let from = l.words.first().is_some_and(|w| w.text.starts_with('自')) && l.words.len() <= 4;
        let split = it.peek().and_then(|n| {
            let at = n.words.iter().position(|w| w.text.starts_with('至'))?;
            let head: String = n.words[..at].iter().map(|w| w.text.as_str()).collect();
            (at > 0 && (head.ends_with("要旨") || head.ends_with("計算書")) && (n.words[at].x - l.words[0].x).abs() < 3.0).then_some(at)
        });
        match split.filter(|_| from) {
            Some(at) => {
                let n = it.next().unwrap();
                let (head, rest) = n.words.split_at(at);
                let line = |ws: &[Col]| Line {
                    y0: ws.iter().map(|w| w.y).fold(f32::INFINITY, f32::min),
                    y1: ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max),
                    words: ws.to_vec(),
                };
                out.push(line(head));
                out.push(l);
                out.push(line(rest));
            }
            None => out.push(l),
        }
    }
    out
}

/// 罫線の無い、見出しの行と内容の行 1 つの表（装置型式指定の告示の「型式指定番号｜指定装置の種類、名称及び型式｜
/// 指定装置を取り付けることができる自動車の範囲｜製作者等の氏名又は名称及び住所」）。欄は語の切れ目（3.5pt 以上）で
/// 3〜8 欄に分かれ、見出しはどの欄にもある数字の無い語の行（どの欄も同じ高さから始まる）で、その下の内容の行の間に、
/// すべての欄を通る行間は無い。内容の行が 3 欄以上で揃って並ぶのは 3 行まで（貸借対照表のような行ごとの表ではない）。
/// 欄ごとに読む（左右の列に分けると見出しと内容が欄の半分ずつに分かれ、行ごとに読むと欄が 1 行ずつ混ざる）。
fn header_table(words: &[Col]) -> Option<trace::Table> {
    let mut spans: Vec<(f32, f32)> = words.iter().map(|w| (w.x, w.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cols: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        match cols.last_mut() {
            Some(c) if a - c.1 < 3.5 => c.1 = c.1.max(b),
            _ => cols.push((a, b)),
        }
    }
    if !(3..=8).contains(&cols.len()) {
        return None;
    }
    let col_of = |w: &Col| cols.iter().position(|c| x_center(w) >= c.0 && x_center(w) <= c.1);
    // 行間（語の下端 y から下の語の上端まで 1pt 以上空き、そこをまたぐ語の無いもの）の中ほど。
    let mut edges: Vec<f32> = words.iter().map(|w| w.y1).collect();
    edges.sort_by(f32::total_cmp);
    let gap = |y: f32| -> Option<f32> {
        let next = words.iter().filter(|w| w.y1 > y + 0.5).map(|w| w.y).fold(f32::INFINITY, f32::min);
        (next.is_finite() && next - y >= 1.0).then_some((y + next) / 2.0)
    };
    let every = |pred: &dyn Fn(&Col) -> bool| (0..cols.len()).all(|i| words.iter().any(|w| col_of(w) == Some(i) && pred(w)));
    let cut = edges.iter().filter_map(|&y| gap(y)).find(|&c| every(&|w: &Col| y_center(w) < c) && every(&|w: &Col| y_center(w) > c))?;
    let (head, body): (Vec<&Col>, Vec<&Col>) = words.iter().partition(|w| y_center(w) < cut);
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    if head.iter().any(|w| w.text.chars().any(digit)) || head.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max) - head.iter().map(|w| w.y).fold(f32::INFINITY, f32::min) > 40.0 {
        return None;
    }
    // 見出しはどの欄も同じ高さから始まる（上の文の行の続き「欄に掲げる金額」を見出しにしない）。
    let em = head.iter().map(|w| w.y1 - w.y).fold(0.0, f32::max).max(1.0);
    let tops: Vec<f32> = (0..cols.len()).map(|i| head.iter().filter(|w| col_of(w) == Some(i)).map(|w| w.y).fold(f32::INFINITY, f32::min)).collect();
    if tops.iter().copied().fold(f32::NEG_INFINITY, f32::max) - tops.iter().copied().fold(f32::INFINITY, f32::min) > em * 1.2 {
        return None;
    }
    // 内容の行が 3 欄以上で高さを揃えて並ぶのは 3 行まで（行ごとに並ぶ表（貸借対照表）ではない）。
    let body_cols: Vec<Vec<Line>> = (0..cols.len())
        .map(|i| group_cell_lines(&body.iter().filter(|w| col_of(w) == Some(i)).map(|w| (*w).clone()).collect::<Vec<_>>()))
        .collect();
    let rows = body_cols
        .iter()
        .flatten()
        .filter(|l| body_cols.iter().filter(|ls| ls.iter().any(|m| (m.center() - l.center()).abs() <= ALIGN_TOL)).count() >= 3)
        .map(|l| (l.center() / 2.0).round() as i32)
        .collect::<std::collections::BTreeSet<_>>();
    // 欄の内容は行間を空けずに続く（下の告示の見出しを欄に入れない）。見出しの欄の行の中に 1em 以上の空きは無い
    // （隣の欄と溝が狭くて 1 欄になったもの）。
    let loose = body_cols.iter().any(|ls| ls.windows(2).any(|p| p[1].y0 - p[0].y1 > (p[1].y1 - p[1].y0) * 0.5));
    let head_split = (0..cols.len()).any(|i| {
        let ws: Vec<Col> = head.iter().filter(|w| col_of(w) == Some(i)).map(|w| (*w).clone()).collect();
        group_cell_lines(&ws).iter().any(|l| l.words.windows(2).any(|p| p[1].x - p[0].x1 >= (l.y1 - l.y0)))
    });
    if loose || head_split || rows.len() > 3 || body_cols.iter().any(|ls| ls.len() >= 2 && ls.iter().filter(|l| amount_line(l)).count() * 2 >= ls.len()) {
        return None;
    }
    // 内容は 1 行（すべての欄を通る行間が無い）。
    let body_y0 = body.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    if edges.iter().filter(|&&y| y > body_y0 + 0.5).filter_map(|&y| gap(y)).any(|c| words.iter().any(|w| y_center(w) > c) && body.iter().any(|w| y_center(w) < c)) {
        return None;
    }
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let y1 = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    // 欄の境は溝の中ほど。
    let xs: Vec<f32> = std::iter::once(cols[0].0)
        .chain(cols.windows(2).map(|p| (p[0].1 + p[1].0) / 2.0))
        .chain(std::iter::once(cols[cols.len() - 1].1))
        .collect();
    Some(block_table(&xs, &[y0, cut, y1], words))
}

/// 欄の境 `xs` と行の境 `ys` で囲んだ升ごとに 1 つのセルにした表（升の中は行を分けない）。升の中の行は、右端まで
/// 組んだ行の次（折り返し）ならつなぎ（金額で終わる行の次は除く）、ほかは改行で区切る。
fn block_table(xs: &[f32], ys: &[f32], words: &[Col]) -> trace::Table {
    let mut cells = Vec::new();
    for row in 0..ys.len() - 1 {
        for col in 0..xs.len() - 1 {
            let ws: Vec<Col> = words
                .iter()
                .filter(|w| {
                    let (cx, cy) = (x_center(w), y_center(w));
                    xs[col] <= cx && (cx < xs[col + 1] || col + 2 == xs.len()) && ys[row] <= cy && (cy < ys[row + 1] || row + 2 == ys.len())
                })
                .cloned()
                .collect();
            if ws.is_empty() {
                continue;
            }
            let ls = group_cell_lines(&ws);
            // 行末の金額（「…に係るもの　1.000」）の次の行は折り返しではない。
            let mut text = String::new();
            for (k, l) in ls.iter().enumerate() {
                if k > 0 && (amount_end(&ls[k - 1]) || !wrapped(&ls[k - 1], l, xs[col + 1] + 0.5)) {
                    text.push('\n');
                }
                text.push_str(&render_line(&l.words));
            }
            cells.push(trace::TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: xs[col],
                y0: ys[row],
                x1: xs[col + 1],
                y1: ys[row + 1],
                text,
                words: ws.iter().map(|w| w.id).collect(),
            });
        }
    }
    let mut used: Vec<usize> = cells.iter().map(|c| c.row).collect();
    used.dedup();
    for c in &mut cells {
        c.row = used.iter().position(|&r| r == c.row).unwrap();
    }
    trace::Table { x0: xs[0], y0: ys[0], x1: xs[xs.len() - 1], y1: ys[ys.len() - 1], rows: used.len(), cols: xs.len() - 1, cells }
}

/// 左右に分ける前に上下に切る y（`cut` で分けた片側の端に離れて付く行を、もう片側と同じ列に入れない）。
///
/// 片側の上端か下端の行が、その側の残りの行から離れ（間にもう片側の行が 3 行以上並ぶ）、もう片側の行がすべてその行より
/// 下（上端の行）か上（下端の行）にあるもの（表の左の欄の下に続く次の告示の見出し「〇国土交通省告示第…号」）。
/// 右の上端の行の高さに左の行が揃い、左が日付で右が署名なら、日付と署名の 1 行（「令和八年十月十五日　国土交通大臣　…」）として
/// その下で切る。どちらも、そこをまたぐ語の無いときだけ。離れた行との間に横の罫線があれば、囲みの列の次の囲み
/// （決算公告）なので切らない。
fn stray_cut(words: &[Col], blocks: &[Block], hrules: &[(f32, f32, f32)], cut: f32) -> Option<f32> {
    let (left, right): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| x_center(w) < cut);
    let (ll, rl) = (group_lines(&left), group_lines(&right));
    if ll.is_empty() || rl.is_empty() {
        return None;
    }
    let clean = |y: f32| {
        !words.iter().any(|w| w.y < y - 1.0 && w.y1 > y + 1.0)
            && !blocks.iter().any(|b| b.1 < y && b.3 > y)
            && words.iter().any(|w| y_center(w) < y)
            && words.iter().any(|w| y_center(w) > y)
    };
    let between = |o: &[Line], a: f32, b: f32| o.iter().filter(|l| l.y0 >= a - 1.0 && l.y1 <= b + 1.0).count();
    // 離れた行との間に、その側に掛かる横の罫線があれば、囲みの列の次の囲み（決算公告）なので切らない。
    let ruled = |s: &[Line], a: f32, b: f32| {
        let x0 = s.iter().flat_map(|l| l.words.iter()).map(|w| w.x).fold(f32::INFINITY, f32::min);
        let x1 = s.iter().flat_map(|l| l.words.iter()).map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
        hrules.iter().any(|r| r.0 > a - 2.0 && r.0 < b + 2.0 && r.1 < x1 && r.2 > x0)
    };
    for (s, o) in [(&ll, &rl), (&rl, &ll)] {
        let o_top = o.iter().map(|l| l.y0).fold(f32::INFINITY, f32::min);
        let o_bottom = o.iter().map(|l| l.y1).fold(f32::NEG_INFINITY, f32::max);
        // 下端の行。
        let k = s.iter().position(|l| l.y0 >= o_bottom - 1.0).unwrap_or(s.len());
        if k > 0 && k < s.len() {
            let (rest, tail) = (s[k - 1].y1, s[k].y0);
            let y = (rest.max(o_bottom) + tail) / 2.0;
            if between(o, rest, tail) >= 3 && !ruled(s, rest, tail) && clean(y) {
                return Some(y);
            }
        }
        // 上端の行。
        let k = s.iter().rposition(|l| l.y1 <= o_top + 1.0).map_or(0, |k| k + 1);
        if k > 0 && k < s.len() {
            let (head, rest) = (s[k - 1].y1, s[k].y0);
            let y = (head + rest.min(o_top)) / 2.0;
            if between(o, head, rest) >= 3 && !ruled(s, head, rest) && clean(y) {
                return Some(y);
            }
        }
    }
    // 日付と署名の行。
    let date = |l: &Line| {
        let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
        (t.starts_with("令和") || t.starts_with("平成")) && t.ends_with('日') && t.contains('年') && t.contains('月') && t.chars().count() <= 14
    };
    let i = ll.iter().position(|l| (l.center() - rl[0].center()).abs() <= ALIGN_TOL)?;
    // 署名は官職（「国土交通大臣」「神奈川労働局長」「長官」）と氏名。
    let signer = |l: &Line| l.words.len() >= 2 && l.words[0].text.ends_with(['臣', '長', '官']);
    if i + 1 < ll.len() && rl.len() >= 2 && date(&ll[i]) && signer(&rl[0]) {
        let y = (ll[i].y1.max(rl[0].y1) + ll[i + 1].y0.min(rl[1].y0)) / 2.0;
        if clean(y) {
            return Some(y);
        }
    }
    None
}

/// 決算公告の囲みの中で左右に並ぶ 2 つの表（貸借対照表と損益計算書。間は 12pt 以内）の間の x と、その上の左右の欄を
/// 読む範囲 `(x, 上端の y, 下端の y)`。表のすぐ上（120pt 以内）に表の間をまたぐ囲みの見出し（「…公告」の行。字間を空けた
/// 「第　31　期　決　算　公　告」は 1 つの語とみなす）があり、それより下で表の間をまたぐ語が無ければ、見出しの下から
/// 表の下端までを左右に分けて読む（見出しが溝をふさぎ、会社名の行と損益計算書の見出しの「自　令和７年４月１日」が
/// 1 行につながっていた）。見出しから表の下端までの語が 2 つの表の幅（から 20pt 以内）に収まるときだけ。
fn paired_tables_cut(words: &[Col], tables: &[Block]) -> Option<(f32, f32, f32)> {
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let y1 = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    // 表の中ほどが領域の幅に入るもの（囲みが紙面の段の境をまたぎ、表の左端が段の境より左に出ることがある）。
    let inside: Vec<&Block> = tables.iter().filter(|t| (t.0 + t.2) / 2.0 >= x0 && t.2 <= x1 + 5.0 && t.1 >= y0 && t.1 < y1).collect();
    let lines = group_lines(words);
    for a in &inside {
        for b in &inside {
            let gap = b.0 - a.2;
            let overlap = a.3.min(b.3) - a.1.max(b.1);
            if !(0.0..=12.0).contains(&gap) || overlap < (a.3 - a.1).min(b.3 - b.1) * 0.5 {
                continue;
            }
            let cut = (a.2 + b.0) / 2.0;
            let tables_top = a.1.min(b.1);
            let bottom = a.3.max(b.3);
            // 行の語を字間空けの見出しはつないで、表の間をまたぐか。
            let crosses = |l: &Line| {
                let mut spans: Vec<(f32, f32)> = Vec::new();
                for (w, joined) in l.words.iter().zip(letter_spaced(&l.words, true)) {
                    match spans.last_mut() {
                        Some(span) if joined => span.1 = w.x1,
                        _ => spans.push((w.x, w.x1)),
                    }
                }
                spans.iter().any(|&(a, b)| a < cut - 1.0 && b > cut + 1.0)
            };
            let heading = |l: &Line| l.words.iter().map(|w| w.text.as_str()).collect::<String>().contains("公告");
            // 見出しが左の欄の上だけにある囲み（「第106期決算公告」の右に「損益計算書の要旨」が並ぶ）は、見出しの行から
            // 左右に分ける（見出しは左の欄の頭）。
            let head = lines
                .iter()
                .filter(|l| l.y1 <= tables_top + 1.0 && heading(l))
                .map(|l| {
                    let left_only = l.words.iter().filter(|w| w.text.contains("公告") || w.text.ends_with('告')).all(|w| w.x1 <= cut - 1.0);
                    if crosses(l) { l.y1 } else if left_only { l.y0 - 0.5 } else { f32::NEG_INFINITY }
                })
                .fold(f32::NEG_INFINITY, f32::max);
            // 見出しから表の下端までの語は 2 つの表の幅に収まる（左右に並ぶ別の囲みの語があれば、先に囲みの列に分ける）。
            let outside = words.iter().any(|w| y_center(w) > head && y_center(w) <= bottom && (w.x < a.0 - 20.0 || w.x1 > b.2 + 20.0));
            // 見出しより下は語のまま見る（字間空けの判定は字の大きい会社名と「至」をつなぐことがある）。
            // 表の間で語が割れた行（日付の「令和８年８月」「25日」。空きが 0.5em 未満）もまたぐとみなす。
            let zone_lines: Vec<&Line> = lines.iter().filter(|l| l.center() > head && l.center() <= bottom).collect();
            let crossed = zone_lines.iter().any(|l| {
                l.words.iter().any(|w| w.x < cut - 1.0 && w.x1 > cut + 1.0)
                    || l.words.windows(2).any(|p| x_center(&p[0]) < cut && x_center(&p[1]) >= cut && p[1].x - p[0].x1 < (l.y1 - l.y0) * 0.5)
            });
            if !head.is_finite() || tables_top - head > 120.0 || outside || crossed {
                continue;
            }
            return Some((cut, head, bottom));
        }
    }
    None
}

/// 項目名｜内容の表（国債の発行条件の「５　募入決定の方法」「（１）　価格競争入札発行」｜「各申込みのうち…」）。
/// 左の狭い欄（幅 120pt 未満）が項目名の欄（[`label_column`]）なら、2 欄の表に組む。行は項目名の欄の行（行間を
/// 空けずに続く項目名の折り返しを除く）と、左右で揃う行。内容の折り返しは同じ行のセルにつなぐ。行ごとに左右を
/// つないで組むと、項目名の折り返し（「者・第Ⅰ非価格競」「争入札発行」）が内容の行に挟まる。
fn label_table(words: &[Col], lines: &[Line]) -> Option<trace::Table> {
    if lines.len() < 3 {
        return None;
    }
    let cut = label_cut(words)?;
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let (y0, y1) = (words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min), words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max));
    let g = Grid {
        xs: vec![x0 - 0.5, cut, x1 + 0.5],
        ys: vec![y0 - 0.5, y1 + 0.5],
        cells: vec![GridCell { x0: 0, x1: 1, y0: 0, y1: 1 }, GridCell { x0: 1, x1: 2, y0: 0, y1: 1 }],
    };
    grid_table(&g, words, 1)
}

/// 項目名｜内容の表（[`label_table`]）の項目名の欄と内容の欄の境の x。
fn label_cut(words: &[Col]) -> Option<f32> {
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    // 左の欄の右端の候補: 語の右端のうち、そこをまたぐ語が無く、右に語があるもの。
    let mut ends: Vec<f32> = words.iter().map(|w| w.x1).filter(|&e| e - x0 < 120.0).collect();
    ends.sort_by(f32::total_cmp);
    ends.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    ends.into_iter().rev().find_map(|e| {
        let right_start = words.iter().filter(|w| w.x >= e).map(|w| w.x).fold(f32::INFINITY, f32::min);
        if !right_start.is_finite() || right_start - e < MIN_GUTTER || words.iter().any(|w| w.x < e - 0.5 && w.x1 > e + 0.5) {
            return None;
        }
        let cut = (e + right_start) / 2.0;
        let (left, right): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| x_center(w) < cut);
        let (ll, rl) = (group_lines(&left), group_lines(&right));
        let right_width = right.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - right_start;
        // 項目番号だけの欄（「（１）」「ア」「①」。番号を付けた段落のぶら下げ）は項目名の欄ではない。
        let number = |l: &Line| {
            let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
            t.chars().count() <= 4 && !t.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) && !"一二三四五六七八九十".contains(c))
        };
        let numbers = ll.iter().filter(|l| number(l)).count();
        // 内容の欄は 1 欄の文（行の中に欄の間ほどの空き（1em）のある行が 2 割未満）。欄がさらに分かれる表（地方交付税の
        // 補正の表の「区分｜数値」）は項目名｜内容の表ではない。
        let split = rl
            .iter()
            .filter(|l| {
                let em = (l.y1 - l.y0).max(1.0);
                l.words.windows(2).any(|p| p[1].x - p[0].x1 >= em)
            })
            .count();
        // 内容は文（行の半分以上が 20 字以上。文の行が 6 つ以上あれば 3 分の 1 以上。国債の発行条件の「発行日」｜「令和８年
        // ８月17日」のような短い内容や算式の行が多いもの）。短い語の並ぶ表（「六十五歳以上人口」「段階補正」）は項目名｜内容の表に
        // しない。
        let long = rl.iter().filter(|l| l.words.iter().map(|w| w.text.chars().count()).sum::<usize>() >= 20).count();
        let prose = long * 2 >= rl.len() || (long >= 6 && long * 3 >= rl.len());
        (numbers * 2 < ll.len() && prose && split * 5 < rl.len() && right_width >= MIN_TEXT_COLUMN_WIDTH && label_column(&ll, &rl)).then_some(cut)
    })
}

/// 決算公告の囲みの列の中で、囲みと囲みを分ける横の罫線の y。領域の語の幅の 95% 以上に掛かり、さらに領域の外へ
/// 20pt 以上延び（紙面の幅いっぱいの段の境の線を、列に分けた後の領域で見たもの）、語も表もまたがず、上下に語が
/// あるもののうち一番上。列に分ける前（紙面の幅の領域）では線は外へ延びないので切らない（囲みの段ごとに読むと、
/// 囲みが並ぶ順と変わる）。
fn box_rule_cut(words: &[Col], blocks: &[Block], hrules: &[(f32, f32, f32)]) -> Option<f32> {
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    hrules
        .iter()
        .filter(|r| r.2.min(x1) - r.1.max(x0) >= (x1 - x0) * 0.95 && (r.1 < x0 - 20.0 || r.2 > x1 + 20.0))
        .map(|r| r.0)
        .filter(|&y| {
            !words.iter().any(|w| w.y < y - 1.0 && w.y1 > y + 1.0)
                && !blocks.iter().any(|b| b.1 < y - 1.0 && b.3 > y + 1.0)
                && words.iter().any(|w| y_center(w) < y)
                && words.iter().any(|w| y_center(w) > y)
        })
        .reduce(f32::min)
}

/// 領域の語の左右の端を両方とも越えて 1 本で引いた横の罫線の y（語も表もまたがず、上下に語のあるもののうち一番上）。
fn band_rule_cut(words: &[Col], blocks: &[Block], hrules: &[(f32, f32, f32)], tables: &[Block]) -> Option<f32> {
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    // 取り出した表の行の線（紙面の幅いっぱいの表の枠・行の境）は段の境ではない（表の上の標題だけが切り離され、
    // 表の下の領域が囲みとして読まれて、キャッシュ・フロー計算書の科目と金額が別々の列に分かれた）。
    let in_table = |r: &(f32, f32, f32)| {
        tables.iter().any(|t| r.0 >= t.1 - 2.0 && r.0 <= t.3 + 2.0 && r.2.min(t.2) - r.1.max(t.0) >= (r.2 - r.1) * 0.8)
    };
    hrules
        .iter()
        .filter(|r| r.1 <= x0 + 2.0 && r.2 >= x1 - 2.0 && x1 - x0 >= MIN_TEXT_COLUMN_WIDTH * 2.0 && !in_table(r))
        .map(|r| r.0)
        .filter(|&y| {
            // 上下とも 2 行以上（表の欄見出しの行「変更後　変更前」の下の線では切らない。表を取り出した後の段は、表の上の
            // 見出しの 2 行だけのことがある）。
            let (above, below): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| y_center(w) < y);
            !words.iter().any(|w| w.y < y - 1.0 && w.y1 > y + 1.0)
                && !blocks.iter().any(|b| b.1 < y - 1.0 && b.3 > y + 1.0)
                && multi_row(&above)
                && multi_row(&below)
        })
        .reduce(f32::min)
}

/// 語が 2 行以上に並ぶか。横に並ぶ段の見出しどうしは 1 行にまとまることがあるので、語の高さの 2 倍以上の高さに
/// 広がるものも含める。
fn multi_row(words: &[Col]) -> bool {
    if group_lines(words).len() >= 2 {
        return true;
    }
    let h = words.iter().map(|w| w.y1 - w.y).fold(f32::INFINITY, f32::min);
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let y1 = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    words.len() >= 2 && y1 - y0 >= h * 2.0
}

/// 表の枠の下端の横の罫線の y。領域の語の幅の 9 割以上に掛かり、領域の高さの上の部分の半分以上に掛かる縦の罫線が
/// そこで終わり（左右の欄の境の線）、語も表もまたがず、上下に語があるもののうち一番上。
fn table_bottom_cut(words: &[Col], blocks: &[Block], hrules: &[(f32, f32, f32)], vrules: &[(f32, f32, f32)]) -> Option<f32> {
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let vrules = join_vrules(vrules);
    hrules
        .iter()
        .filter(|r| r.2.min(x1) - r.1.max(x0) >= (x1 - x0) * 0.9)
        .filter(|r| {
            vrules.iter().any(|v| v.0 > r.1 + 10.0 && v.0 < r.2 - 10.0 && (v.2 - r.0).abs() <= 2.0 && v.2 - v.1.max(y0) >= (r.0 - y0) * 0.5)
        })
        .map(|r| r.0)
        .filter(|&y| {
            !words.iter().any(|w| w.y < y - 1.0 && w.y1 > y + 1.0)
                && !blocks.iter().any(|b| b.1 < y - 1.0 && b.3 > y + 1.0)
                && words.iter().any(|w| y_center(w) < y)
                && words.iter().any(|w| y_center(w) > y)
        })
        .reduce(f32::min)
}

/// 紙面を左右に分ける縦の罫線が途中で終わり、その下の語（下の表の標題「純資産変動計算書」と期間）だけが線の x を
/// またぐなら、線の下端の y（上下に分けてから、上を線で左右に分ける。下の標題が線をふさいで、左の損益計算書の科目の
/// 欄と金額の欄の間の溝で先に切られ、科目の後に金額だけが読まれていた）。線は領域の上端から始まり、上の部分の高さの
/// 7 割以上に掛かり、左右とも段の幅以上の語があるもの。
fn divider_end_cut(words: &[Col], blocks: &[Block], vrules: &[(f32, f32, f32)]) -> Option<f32> {
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let joined = join_vrules(vrules);
    // 線の下端に掛かる行（線の下端より 1pt 下で終わる右の欄の注記）の下で切る（10pt まで）。
    let solid = |w: &&Col| !w.text.trim().is_empty();
    let end = |ry1: f32| words.iter().filter(solid).filter(|w| w.y < ry1 - 0.5 && w.y1 > ry1 + 0.5).map(|w| w.y1 + 0.5).fold(ry1, f32::max);
    joined
        .iter()
        .map(|&(x, ry0, ry1)| (x, ry0, ry1, end(ry1)))
        .filter(|&(_, _, ry1, cut)| cut - ry1 <= 10.0)
        .map(|(x, ry0, _, cut)| (x, ry0, cut))
        .filter(|&(x, ry0, ry1)| {
            let above: Vec<&Col> = words.iter().filter(|w| y_center(w) < ry1).collect();
            let below: Vec<&Col> = words.iter().filter(|w| y_center(w) > ry1).collect();
            let width = |ws: &[&Col]| ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - ws.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
            let left: Vec<&Col> = above.iter().copied().filter(|w| x_center(w) < x).collect();
            let right: Vec<&Col> = above.iter().copied().filter(|w| x_center(w) > x).collect();
            let a1 = above.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
            // 紙面を左右に分ける線は領域の幅の中ほど（35〜65%）にある（片側の表の欄の線ではない）。
            let (wx0, wx1) = (words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min), words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max));
            let middle = (x - wx0) >= (wx1 - wx0) * 0.35 && (x - wx0) <= (wx1 - wx0) * 0.65;
            // 左右とも線の下端の近く（3 em 以内）まで字があり（左右の段が線の下端で終わる）、線をまたぐ下の語がすぐ下（3 em
            // 以内）から始まる（下の表の標題。片側の表の枠の下端で切ると、もう一方の段の上の部分が先に読まれる）。
            // 字の高さの中央値（縦に組んだ欄の名前の長い語で大きくならない）。
            let em = {
                let mut hs: Vec<f32> = words.iter().map(|w| w.y1 - w.y).collect();
                hs.sort_by(f32::total_cmp);
                hs[hs.len() / 2].max(1.0)
            };
            let ends = |ws: &[&Col]| ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max) >= ry1 - em * 3.0;
            // 線をまたぐ語は線を中ほどにした標題（線の左右とも 40pt 以上に掛かり、中ほどが線から領域の幅の 1 割以内）。
            // 片側の段の右端まで組んだ注記・囲みの行は線をわずかにまたぐだけ。
            let title = |w: &&&Col| w.x < x - 40.0 && w.x1 > x + 40.0 && (x_center(w) - x).abs() <= (wx1 - wx0) * 0.1;
            let titles: Vec<&&Col> = below.iter().filter(title).collect();
            let ttop = titles.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
            let (tx0, tx1) = (titles.iter().map(|w| w.x).fold(f32::INFINITY, f32::min), titles.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max));
            // 線の下端から標題までの語（字間を空けた標題の字「純　資　産…」）も標題の左右の範囲にある（左右の段が線の下へ
            // 続くなら段の境ではない）。
            // 標題の行の語も（右寄せの「（単位：千円）」は除く）。
            let unit = |w: &Col| w.text.trim_start().starts_with("（単位") || w.text.trim_start().starts_with("(単位");
            let clear = below.iter().filter(|w| w.y < ttop + em && !unit(w)).all(|w| w.x >= tx0 - em * 2.0 && w.x1 <= tx1 + em * 2.0);
            let next = ttop <= ry1 + em * 3.0 && clear;
            middle
                && ends(&left)
                && ends(&right)
                && next
                && ry0 <= y0 + 2.0
                && ry1 - ry0 >= (a1 - y0) * 0.7
                && !above.iter().any(|w| w.x < x - 1.0 && w.x1 > x + 1.0)
                && below.iter().any(|w| w.x < x - 1.0 && w.x1 > x + 1.0)
                && !blocks.iter().any(|b| b.1 < ry1 - 1.0 && b.3 > ry1 + 1.0)
                // 字の無い語（下線の字形）は見ない。
                && !words.iter().any(|w| !w.text.trim().is_empty() && w.y < ry1 - 0.5 && w.y1 > ry1 + 0.5)
                && width(&left) >= MIN_TEXT_COLUMN_WIDTH
                && width(&right) >= MIN_TEXT_COLUMN_WIDTH
        })
        .map(|(_, _, cut)| cut)
        .reduce(f32::min)
}

/// 領域を左右に分ける縦の罫線の x（段の境の線・紙面を左右に分ける線）。領域の語の高さの 7 割以上に掛かり、
/// どの語もまたがず（表をまたいで引かれた線ではない）、左右が別の列（[`is_column_split`]）になる線のうち、
/// 掛かる高さの最も長いもの。
///
/// 溝は左から順に探すので、左の欄の中の溝（貸借対照表の科目と金額の間）で先に切ると、紙面の右半分まで
/// 金額の側に入って、科目と金額が別々に読まれる。線で左右に分けてから溝を探す。
///
/// 語の高さいっぱい（95% 以上）に掛かり、横の罫線と交わらず、左右とも段の幅以上の線は、左右の行が揃っていても
/// 紙面を左右に分ける線（横倒しのページの郵便番号の一覧が左右の段に組まれたもの。表の列とみなすと左右の段の
/// 行が 1 行ずつ混ざっていた）。
fn rule_cut(words: &[Col], blocks: &[Block], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Option<f32> {
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let y1 = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    let (wx0, wx1) = (words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min), words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max));
    let joined = join_vrules(vrules);
    let vrules = joined.as_slice();
    vrules
        .iter()
        .filter(|&&(x, ry0, ry1)| {
            ry1.min(y1) - ry0.max(y0) >= (y1 - y0) * 0.7
                && !words.iter().any(|w| w.x < x - 1.0 && w.x1 > x + 1.0)
                // 領域の上下の範囲に（2pt より多く）掛かる表だけ（上の表の下端がわずかに掛かるだけなら見ない）。
                && !blocks.iter().any(|b| b.0 < x - 1.0 && b.2 > x + 1.0 && b.3.min(y1) - b.1.max(y0) > 2.0)
        })
        .filter(|&&(x, ry0, ry1)| {
            let (left, right): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| x_center(w) < x);
            let width = |ws: &[Col]| ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - ws.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
            // 線の上端は、領域の上端から 6pt までの低さでもよい（上の表の下の枠の線のすぐ下から始まる段の境の線。低い帯では
            // 高さの 5% が 2pt ほどしかない）。そこの枠の線（領域の上端の 2pt 以内）は線をまたぐ横線に数えない。
            let divider = ry0 <= y0 + ((y1 - y0) * 0.05).max(6.0)
                && ry1 >= y1 - (y1 - y0) * 0.05
                && !hrules.iter().any(|&(hy, hx0, hx1)| hy > y0 + 2.0 && hy < y1 && hx0 < x - 5.0 && hx1 > x + 5.0)
                // 表の列の線は、同じ高さに引いた線が語の範囲にほかにもある（列が上下の罫線の無い表の続き）。
                && !vrules.iter().any(|&(vx, vy0, vy1)| (vx - x).abs() > 2.0 && vx > wx0 && vx < wx1 && (vy0 - ry0).abs() < 3.0 && (vy1 - ry1).abs() < 3.0)
                && !left.is_empty()
                && !right.is_empty()
                && width(&left) >= MIN_TEXT_COLUMN_WIDTH
                && width(&right) >= MIN_TEXT_COLUMN_WIDTH;
            // 文字の層に見出しだけがあるページ（本文が画像の家裁公告の 4 段組）は、語が少なく段の幅の行が無いので、紙面の
            // 高さいっぱい（400pt 以上）の段の境の線なら列に分ける（見出しが上から順に読まれ、段の順にならなかった）。
            // 左右に同じ高さで並ぶ行（表の行）があれば見出しではない。
            let side_by_side = left.iter().any(|l| right.iter().any(|r| l.y < r.y1 && r.y < l.y1));
            let sparse = ry1 - ry0 >= 400.0 && !left.is_empty() && !right.is_empty() && group_lines(words).len() <= 12 && !side_by_side;
            divider || sparse || is_column_split(&left, &right, false)
        })
        .max_by(|a, b| (a.2.min(y1) - a.1.max(y0)).total_cmp(&(b.2.min(y1) - b.1.max(y0))).then(b.0.total_cmp(&a.0)))
        .map(|r| r.0)
}

/// 領域を左右の列に分ける縦の溝の x。左から順に、列分割として妥当な最初の溝を返す。
///
/// 溝は、各語を両端から [`GUTTER_OVERLAP`] ずつ縮めた区間の切れ目として探す。行末のぶら下げ
/// （句読点・括弧）や、回転した縦書き部分の字下げで、隣の列と数 pt 重なることがあるため。
/// 語は中心 x で左右に振り分ける（[`x_center`]）。`blocks`（取り除いた表の外枠）も溝をふさぐ。
fn column_cut(words: &[Col], blocks: &[Block]) -> Option<f32> {
    column_gutter(words, blocks, &[]).map(|g| g.0)
}

/// [`column_cut`] の溝の `(x, 幅)`。
///
/// `tables`（囲みの段の中の表の外枠）が空でなければ、表の右端のすぐ右（20pt 以内）で別の表の左端のすぐ左になる溝を
/// 囲みの境とみなす（[`is_column_split`] の `boxed`）。
fn column_gutter(words: &[Col], blocks: &[Block], tables: &[Block]) -> Option<(f32, f32)> {
    column_gutters(words, blocks, tables, !tables.is_empty()).map(|(g, _)| g)
}

/// [`column_gutter`] と、列の境になる溝すべて（x, 幅, 左右の狭い方の幅。左から）。
///
/// `boxed`（囲みの段の中）なら、20pt 以上の溝で左右とも 100pt 以上の幅の組は囲みの境。
fn column_gutters(words: &[Col], blocks: &[Block], tables: &[Block], boxed: bool) -> Option<((f32, f32), Vec<(f32, f32, f32)>)> {
    let (y0, y1) = (words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min), words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max));
    // 段の中の表（語の下に続く表も。表を取り出した後の段の語は、表の上の見出しの行だけのことがある）。
    let near: Vec<&Block> = tables.iter().filter(|t| t.1 < y1 + 40.0 && y0 < t.3).collect();
    let between = |cut: f32| {
        near.iter().any(|t| t.2 <= cut + 2.0 && cut - t.2 < 20.0) && near.iter().any(|t| t.0 >= cut - 2.0 && t.0 - cut < 20.0)
    };
    // 字間空けの見出し（「中　間　決　算　公　告」）は 1 字語の並びだが、1 語として溝をふさぐ。
    // 桁区切りで語が割れた数（「225,」「582,」「082,」「247」）も 1 語として溝をふさぐ（右揃えの金額の
    // 桁の切れ目が行をまたいで縦に揃い、列の溝に見える）。
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for line in group_lines(words) {
        for (i, (w, joined)) in line.words.iter().zip(letter_spaced(&line.words, true)).enumerate() {
            let number = i > 0 && number_continues(&line.words[i - 1], w);
            // 間を空けずに続く語（傍線の切れ目で割れた本文の行「…第５条の19第２項」「において準用する…」）も溝をふさぐ。
            // 表の欄の語（短い語）と、隣り合う段の行（罫線の囲みの中で段が空きなく接する。行の高さがずれる）は除く。
            let touching = i > 0 && {
                let p = &line.words[i - 1];
                let prose = |c: &Col| c.text.chars().count() >= 6;
                (w.x - p.x1).abs() < 0.5 && (w.y - p.y).abs() < 0.1 && (w.y1 - p.y1).abs() < 0.1 && prose(p) && prose(w)
            };
            match spans.last_mut() {
                Some(span) if joined || number || touching => span.1 = w.x1,
                _ => spans.push((w.x, w.x1)),
            }
        }
    }
    // 囲みの段の中では、語の上下の範囲に掛からない表（段の上の囲みの中の縦組みの合併公告）は溝をふさがない。
    spans.extend(blocks.iter().filter(|b| !boxed || (b.1 < y1 && b.3 > y0)).map(|b| (b.0, b.2)));
    let mut spans: Vec<(f32, f32)> = spans
        .into_iter()
        .map(|(x, x1)| (x + GUTTER_OVERLAP, x1 - GUTTER_OVERLAP))
        .filter(|(a, b)| a < b)
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut right_edge = f32::NEG_INFINITY;
    // 列の境になる溝（x, 幅, 左右の狭い方の幅）。
    let mut cuts: Vec<(f32, f32, f32)> = Vec::new();
    for (a, b) in spans {
        if right_edge.is_finite() && a - right_edge >= MIN_GUTTER {
            let cut = (right_edge + a) / 2.0;
            let (left, right): (Vec<Col>, Vec<Col>) = words.iter().cloned().partition(|w| x_center(w) < cut);
            let width = |ws: &[Col]| ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - ws.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
            // 囲みの段の中の 20pt 以上の溝で、左右とも 100pt 以上の幅の組なら囲みの境（3 列に並ぶ決算公告の囲みの見出しが
            // 同じ高さに並び、表の溝とみなされて 1 行ずつ混ざっていた）。
            // 金額の欄（行の半分以上が金額だけの行）は囲みではない（科目と金額の間の広い空き）。
            // 金額で始まる行（財産目録の「900,787,332,294　普通預金・当座預金　三菱ＵＦＪ銀行外三行」。金額の右に摘要の欄が
            // 続く）も金額の欄の行。
            let figures = |ws: &[Col]| {
                let ls = group_lines(ws);
                let starts = |l: &Line| {
                    l.words.first().is_some_and(|w| {
                        let t = w.text.trim();
                        t.starts_with(|c: char| c.is_ascii_digit() || "△▲".contains(c)) && t.chars().filter(|c| c.is_ascii_digit()).count() >= 3
                    })
                };
                ls.iter().filter(|l| amount_line(l) || starts(l)).count() * 2 >= ls.len()
            };
            let boxes = boxed && a - right_edge >= 20.0 && width(&left) >= 100.0 && width(&right) >= 100.0 && !figures(&left) && !figures(&right);
            if !latin_word_gap(words, cut) && !item_numbers(&left, &right) && !repeat_marks(&left, &right) && (boxes || is_column_split(&left, &right, between(cut))) {
                cuts.push((cut, a - right_edge, width(&left).min(width(&right))));
            }
        }
        right_edge = right_edge.max(b);
    }
    // 左の溝から切る。ただし、2 倍以上広い溝（12pt 以上）で左右とも幅 100pt 以上の列に分かれるものがあれば先にそこで
    // 切る（決算公告の囲みの列の間。左の列の囲みの中の貸借対照表と損益計算書の間の狭い溝で先に切ると、どの囲みも貸借
    // 対照表だけが先に読まれる。項目名と金額の間の広い溝は、金額の側が狭い）。
    let first = *cuts.first()?;
    let widest = cuts.iter().copied().filter(|c| c.2 >= 100.0).max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or(first);
    let pick = if widest.1 >= WIDE_GUTTER && widest.1 >= first.1 * 2.0 { widest } else { first };
    Some(((pick.0, pick.1), cuts))
}

/// 横組みの読み順の処理に渡す罫線と表。
struct Context<'a> {
    vrules: &'a [(f32, f32, f32)],
    hrules: &'a [(f32, f32, f32)],
    tables: &'a [Block],
}

/// 囲みの列の間とみなす広い溝の幅(pt)。
const WIDE_GUTTER: f32 = 12.0;

/// 数の続きか（桁区切りで割れた語「35」「,」「952」）。空きが半 em 未満で、前の語が数字か
/// 桁区切りで終わり、次の語が数字か桁区切りで始まる。
fn number_continues(prev: &Col, next: &Col) -> bool {
    let em = (prev.y1 - prev.y).min(next.y1 - next.y).max(1.0);
    let digitish = |c: char| c.is_ascii_digit() || matches!(c, ',' | '.');
    next.x - prev.x1 < em * 0.5
        && prev.text.chars().last().is_some_and(digitish)
        && next.text.chars().next().is_some_and(digitish)
}

/// 溝の左が項目番号の列か（「384」「385」…、「第二」「一」「二」が各項目の最初の行の左に置かれる）。
/// 左の語がすべて 4 字までの数字・漢数字（「第」付きも）で、どれも右の語のすぐ左に同じ高さで並ぶなら、
/// 番号は項目の行の頭とみなす。溝のすぐ左の列が表の行の見出しの番号（「第2821号」）だけのときも同じ。
fn item_numbers(left: &[Col], right: &[Col]) -> bool {
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    let number = |w: &Col| {
        let t = w.text.trim();
        (1..=4).contains(&t.chars().count())
            && (t.chars().all(digit) || t.trim_start_matches('第').chars().all(|c| "〇一二三四五六七八九十".contains(c)) && t != "第")
    };
    // 表の行の見出しの番号（「第2821号」「第165回」）。
    let key = |w: &Col| {
        let t = w.text.trim();
        let inner = t.strip_prefix('第').and_then(|t| t.strip_suffix(['号', '回']));
        inner.is_some_and(|i| (1..=6).contains(&i.chars().count()) && i.chars().all(digit))
    };
    let next_to = |n: &Col, reach: f32| {
        let em = (n.y1 - n.y).max(1.0);
        right.iter().any(|w| (y_center(w) - y_center(n)).abs() <= ALIGN_TOL && w.x >= n.x1 && w.x - n.x1 < em * reach)
    };
    if left.len() >= 2 && left.iter().all(|n| number(n) && next_to(n, 2.0)) {
        return true;
    }
    // 溝のすぐ左の列が行の見出しの番号だけで、どれも右の欄の語と同じ高さに並ぶ（溝の右に隣の段の
    // 行が混ざっていても、語の単位で見れば揃う）。
    let edge = left.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let column: Vec<&Col> = left.iter().filter(|w| edge - w.x1 < (w.y1 - w.y).max(1.0) * 3.0).collect();
    if column.len() >= 3 && column.iter().all(|n| key(n) && next_to(n, 6.0)) {
        return true;
    }
    // 溝の左の領域の左端の列が行の見出しの番号で、その 4 分の 3 以上が溝の右の語と同じ高さに並ぶなら、
    // 溝は同じ表の欄の間（項目名が折り返して行数が揃わなくても）。
    let start = left.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let keys: Vec<&Col> = left.iter().filter(|w| w.x - start < (w.y1 - w.y).max(1.0) * 3.0 && key(w)).collect();
    let lined = keys.iter().filter(|n| right.iter().any(|w| (y_center(w) - y_center(n)).abs() <= ALIGN_TOL)).count();
    keys.len() >= 3 && lined * 4 >= keys.len() * 3
}

/// 溝のすぐ右の列が「〃」（上の行と同じの印）だけで、どれも左の欄の語と同じ高さに並ぶか（表の欄）。
fn repeat_marks(left: &[Col], right: &[Col]) -> bool {
    let edge = right.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let column: Vec<&Col> = right.iter().filter(|w| w.x - edge < (w.y1 - w.y).max(1.0) * 3.0).collect();
    column.len() >= 3
        && column.iter().all(|n| {
            let em = (n.y1 - n.y).max(1.0);
            !n.text.trim().is_empty()
                && n.text.trim().chars().all(|c| c == '〃' || c == '\u{3000}')
                && left.iter().any(|w| (y_center(w) - y_center(n)).abs() <= ALIGN_TOL && w.x1 <= n.x && n.x - w.x1 < em * 12.0)
        })
}

/// 溝が欧文の語間か。両端揃えの欧文の段落では、語間がたまたま行をまたいで縦に揃い、列の溝に
/// 見える。溝の両側に語のある行の半分以上で、溝の左右の語が欧文の語の境目なら語間とみなす。
fn latin_word_gap(words: &[Col], cut: f32) -> bool {
    let (mut both, mut latin) = (0, 0);
    for line in group_lines(words) {
        let left = line.words.iter().filter(|w| x_center(w) < cut).max_by(|a, b| a.x1.total_cmp(&b.x1));
        let right = line.words.iter().filter(|w| x_center(w) >= cut).min_by(|a, b| a.x.total_cmp(&b.x));
        if let (Some(l), Some(r)) = (left, right) {
            both += 1;
            let em = (l.y1 - l.y).min(r.y1 - r.y).max(1.0);
            if r.x - l.x1 < em * 1.5 && latin_word_break(&l.text, &r.text) {
                latin += 1;
            }
        }
    }
    both > 0 && latin * 2 >= both
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
///
/// `boxed`（囲みの段の中の、表と表の間の溝。[`column_gutter`]）なら、金額の欄でも項目名の欄でもなければ囲みの境か、
/// 囲みの中の貸借対照表と損益計算書の境とみなす（囲みは段の幅より狭く、隣の囲みと行が揃って並ぶ）。
fn is_column_split(left: &[Col], right: &[Col], boxed: bool) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let (ll, rl) = (group_lines(left), group_lines(right));
    // 溝の左が短い語（3em 以内）だけで、右と同じ高さに並ぶ行がどれも 1em ほどしか離れていなければ、1 行の中の空き
    // （番号「（１）」と項目名「未払金」。1 字の語は溝を探すときに縮めて消え、番号の下に折り返した行の頭までが溝に見える）。
    let close: Vec<bool> = ll
        .iter()
        .filter_map(|a| {
            let last = a.words.last()?;
            let b = rl.iter().find(|b| (a.center() - b.center()).abs() <= ALIGN_TOL)?;
            Some(b.words[0].x - last.x1 <= (last.y1 - last.y).max(1.0) * 1.1)
        })
        .collect();
    let short = ll.iter().all(|l| {
        let em = (l.y1 - l.y0).max(1.0);
        l.words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) - l.words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min) <= em * 3.0
    });
    if !boxed && short && close.len() >= 2 && close.iter().all(|&c| c) {
        return false;
    }
    // 左が 1 字の語だけ（損益計算書の期間の「自」「至」）で、どれも右の行の高さの範囲にあり、1 字分ほどしか離れていなければ、
    // 囲みの中でも 1 行の中の空き（右の行に住所の行が重なって高さの中心がずれる）。
    let singles = ll.len() >= 2 && ll.iter().all(|l| l.words.len() == 1 && l.words[0].text.trim().chars().count() == 1);
    if singles
        && ll.iter().all(|a| {
            let w = &a.words[0];
            rl.iter().any(|b| b.y0 < a.center() && a.center() < b.y1 && b.words.iter().any(|r| r.x >= w.x1 - 0.5 && r.x - w.x1 <= (w.y1 - w.y).max(1.0) * 1.1 && (y_center(r) - a.center()).abs() <= ALIGN_TOL))
        })
    {
        return false;
    }
    // 金額の欄の上の単位（「（単位：円）」）は欄の行に数えない（表の見出しの行と高さが揃わず、金額でもない）。
    let unit = |l: &Line| l.words.first().is_some_and(|w| w.text.starts_with("（単位") || w.text.starts_with("(単位"));
    let (ll, rl): (Vec<Line>, Vec<Line>) = (ll.into_iter().filter(|l| !unit(l)).collect(), rl.into_iter().filter(|l| !unit(l)).collect());
    if ll.is_empty() || rl.is_empty() {
        return false;
    }
    // 右が 1 行だけで、左の 1 行目と高さが揃い、1.5 字分までしか離れていなければ、1 行の中の字間（「「Ⅰ」「重要な会計方針」
    // 「６．収益及び費用の計上基」を全角の空白で区切った行。左の 2 行目「準」に記載…」が間に入っていた）。
    if !boxed && rl.len() == 1 && (2..=3).contains(&ll.len()) {
        let b = &rl[0];
        let start = b.words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
        let same_line = ll.iter().enumerate().any(|(k, a)| {
            let em = (a.y1 - a.y0).max(1.0);
            let end = a.words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
            (a.center() - b.center()).abs() <= ALIGN_TOL
                && start - end <= em * 1.5
                && ll.iter().enumerate().all(|(j, l)| j == k || l.y0 >= a.y1 - 0.5 || l.y1 <= a.y0 + 0.5)
        });
        if same_line {
            return false;
        }
    }
    // 2 行までの短い帯で、一方が他方のどの行よりも上の 1 行だけなら、中央寄せの標題とその下の行（「附属明細書」の下の
    // 左寄せの「【船員保険勘定】」「１．有形固定資産…の明細」。標題が後に読まれていた）。
    let above = |a: &[Line], b: &[Line]| a.len() == 1 && b.iter().all(|l| a[0].y1 <= l.y0 + 0.5);
    if !boxed && ll.len() <= 2 && rl.len() <= 2 && (above(&ll, &rl) || above(&rl, &ll)) {
        return false;
    }
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
    // 金額の欄（行の 8 割以上が金額だけの行）なら、欄の幅が広くても（右揃えの金額の欄が 2〜3 列並ぶ）表の列。
    // 後ろに単位の付いた行（表の標題の右半分「至　令和８年３月31日）　（単位：円）」）は数えない。
    let amounts = |ls: &[Line]| {
        let ls: Vec<&Line> = ls.iter().filter(|l| !l.words.iter().any(|w| w.text.starts_with("（単位") || w.text.starts_with("(単位"))).collect();
        ls.len() >= 2 && ls.iter().filter(|l| amount_line(l)).count() * 10 >= ls.len() * 8
    };
    // 番号付きの項目名が 1〜3 行だけの狭い左の欄（前のページから続く国債の発行条件の「17　中途換金の特例」「18　元利金
    // 支払場所」）も、右の内容の行と高さが揃えば項目名の欄（項目名を全部読んでから内容を読んでいた）。
    let few_labels = wl < 120.0 && (1..=3).contains(&ll.len()) && rl.len() >= 3 && {
        let near = near_lines(right, true);
        // 項目の下の次の告示の標題（「〇財務省告示第…号」）は数えない。
        let text = |l: &Line| l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect::<String>();
        let items: Vec<&Line> = ll.iter().filter(|l| !crate::segment::is_heading_mark(&text(l))).collect();
        let numbered = |l: &Line| text(l).starts_with(|c: char| c.is_ascii_digit() || ('０'..='９').contains(&c));
        // 番号の行の下に行間を空けずに折り返した項目名の続き（「２　発行の根拠法律及び」「その条項」）も項目名。
        let wrapped = |k: usize| k > 0 && numbered(items[k - 1]) && items[k].y0 - items[k - 1].y1 < (items[k].y1 - items[k].y0) * 0.5 && text(items[k]).chars().count() <= 8;
        !items.is_empty() && items.iter().any(|l| numbered(l)) && items.iter().enumerate().all(|(k, l)| {
            let t = text(l);
            let named = t.chars().filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c) || ('\u{3040}'..='\u{30FF}').contains(c)).count() >= 3;
            wrapped(k) || (numbered(l) && named && near.iter().any(|m| (m.center() - l.center()).abs() <= ALIGN_TOL.max((l.y1 - l.y0) * 0.25)))
        })
    };
    // 狭い左の欄（60pt 未満）の短い区分の名前（「10種地」「９種地」…）が、どれも右の行の高さの範囲（右の行の上下 1 行分の間）に
    // あり、右の行の方が多いなら、表の区分の欄（交付税の補正係数の表。区分の名前を全部読んでから値の行が読まれていた）。
    let group_labels = wl < 60.0 && ll.len() >= 3 && rl.len() > ll.len() && {
        // 数だけの語（目次のページ数「七」「二八」）は区分の名前ではない。
        let numeral = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c) || "〇一二三四五六七八九十百".contains(c);
        let short = ll.iter().all(|l| {
            let t: String = l.words.iter().map(|w| w.text.as_str()).collect();
            t.chars().count() <= 8 && !amount_line(l) && t.chars().any(|c| !numeral(c) && !c.is_whitespace())
        });
        let beside = ll.iter().filter(|a| rl.iter().any(|b| (a.center() - b.center()).abs() <= (b.y1 - b.y0).max(1.0) * 1.2)).count();
        short && beside * 10 >= ll.len() * 9
    };
    let labels = few_labels || group_labels || (wl < 120.0 && label_column(&ll, &near_lines(right, true))) || (wr < 120.0 && label_column(&rl, &near_lines(left, false)));
    if boxed {
        return !labels && !(aligned as f32 >= few.len() as f32 * 0.75 && (amounts(&rl) || amounts(&ll)));
    }
    // 右が金額だけの狭い欄（150pt 未満）なら、行が揃わなくても表の欄（合計の金額は科目の無い行に置く）。
    let amount_column = wr < MIN_TEXT_COLUMN_WIDTH && amounts(&rl);
    // 左の行と高さの揃う右の行がほとんど金額だけなら、右の上の欄見出しの行が多くても表の列（財政資金対民間収支実績表の
    // 「—」の字の無い欄の空きが溝に見え、科目と 4 月の収入の欄と、ほかの月の欄が別々に読まれていた）。
    let paired: Vec<Line> = rl.iter().filter(|b| ll.iter().any(|a| (a.center() - b.center()).abs() <= ALIGN_TOL)).cloned().collect();
    // 高さの揃う行がどれも、左の科目（数の無い行）と右の金額だけの行の組なら、科目と金額の表（「当期受入額」「当期支出額」と
    // 金額。右の下に次の表の標題と日付が中央揃えで続いても列ではない）。
    // 金額の行の組が 5 つ以上あれば、表の上の見出しの行（右の「佐賀県佐賀市本庄町１番地」と左の「令和８年10月16日」）が
    // 高さを揃えて並んでいても（組の 8 割以上が金額なら）科目と金額の表。
    let amount_pairs: Vec<&Line> = paired.iter().filter(|b| amount_line(b)).collect();
    let label_amounts = paired.len() >= 2
        && (paired.iter().all(amount_line) || (amount_pairs.len() >= 5 && amount_pairs.len() * 10 >= paired.len() * 8))
        && amount_pairs.iter().all(|b| {
            ll.iter()
                .filter(|a| (a.center() - b.center()).abs() <= ALIGN_TOL)
                .all(|a| !a.words.iter().any(|w| w.text.chars().any(|c| c.is_ascii_digit())))
        });
    // 左右に並べた貸借対照表（資産の科目｜金額｜負債の科目｜金額）の左の科目の欄と、その右（金額で始まり、負債の科目と
    // 金額が続く行）も、科目と金額の表。左は数の無い狭い（200pt 未満の）科目の欄で、高さの揃う右の行の 8 割以上が金額で
    // 始まるもの。
    let starts_amount = |b: &Line| {
        b.words.first().is_some_and(|w| {
            let t = w.text.trim();
            t.chars().any(|c| c.is_ascii_digit()) && t.chars().all(|c| c.is_ascii_digit() || ",△▲()（）".contains(c))
        })
    };
    let leading: Vec<&Line> = paired.iter().filter(|b| starts_amount(b)).collect();
    let label_leading = wl < 200.0
        && leading.len() >= 5
        && leading.len() * 10 >= paired.len() * 8
        && leading.iter().all(|b| {
            ll.iter()
                .filter(|a| (a.center() - b.center()).abs() <= ALIGN_TOL)
                .all(|a| !a.words.iter().any(|w| w.text.chars().any(|c| c.is_ascii_digit())))
        });
    let label_amounts = label_amounts || label_leading;
    let table_like = (aligned as f32 >= few.len() as f32 * 0.75 && (wl.min(wr) < MIN_TEXT_COLUMN_WIDTH || amounts(&rl) || (paired.len() >= 5 && amounts(&paired))))
        || label_amounts
        || labels
        || amount_column;
    // 本文の段なら、段の幅いっぱいの行がある。見出しだけの側（「官　庁　事　項」と
    // 「人事院公示第26号」が離れて並ぶ等）は、全体の幅が広くても 1 行ずつは短い。
    let longest_line = |ls: &[Line]| ls.iter().map(|l| width(&l.words)).fold(0.0, f32::max);
    !table_like && longest_line(&ll).max(longest_line(&rl)) >= MIN_TEXT_COLUMN_WIDTH
}

/// 項目名の欄の隣の内容の欄の行。溝の側の端から 300pt（段 2 つ分）以内に収まる語だけで組む（紙面の右半分の記事の行を
/// 内容の行に入れない）。`from_left` なら溝は左にある。
fn near_lines(words: &[Col], from_left: bool) -> Vec<Line> {
    let reach = MIN_TEXT_COLUMN_WIDTH * 2.0;
    let near: Vec<Col> = if from_left {
        let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
        words.iter().filter(|w| w.x1 <= x0 + reach).cloned().collect()
    } else {
        let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
        words.iter().filter(|w| w.x >= x1 - reach).cloned().collect()
    };
    group_lines(&near)
}

/// `labels` が項目名の欄（項目名｜内容の表の左の欄）か。どの行も、内容の行と高さが揃うか（項目の最初の行）、
/// 前の行に行間を空けずに続くか（項目名の折り返し）、内容の行と高さが重ならない（「６　発行額」のような
/// 見出しの行）。6 行以上で、揃う行が 3 つ以上、項目の頭の行の半分以上が番号で始まり、半分以上が字の名前を持つこと。
fn label_column(labels: &[Line], contents: &[Line]) -> bool {
    // 項目名と内容は字の大きさが違うことがある（行の中ほどが 1pt 余りずれる）ので、行の高さの 4 分の 1 まで。
    let aligned = |l: &Line| contents.iter().any(|m| (m.center() - l.center()).abs() <= ALIGN_TOL.max((l.y1 - l.y0) * 0.25));
    let alone = |l: &Line| !contents.iter().any(|m| m.y0 < l.y1 && l.y0 < m.y1);
    let n = labels.iter().filter(|l| aligned(l)).count();
    let tight = |k: usize| k > 0 && labels[k].y0 - labels[k - 1].y1 < (labels[k].y1 - labels[k].y0) * 0.25;
    let explained = labels.iter().enumerate().filter(|&(k, l)| aligned(l) || alone(l) || tight(k)).count();
    // 項目名は番号付き（「５　募入決定の方法」「（１）　価格競争入札発行」）。番号の無い短い行の並び（決算公告の
    // 「損益計算書の要旨」「自」「至」、歳入歳出の表の「歳入」「歳出」）を項目名の欄にしない。
    let heads: Vec<&Line> = labels.iter().enumerate().filter(|&(k, _)| !tight(k)).map(|(_, l)| l).collect();
    let numbered = heads
        .iter()
        .filter(|l| l.words.first().and_then(|w| w.text.chars().next()).is_some_and(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || "（(①②③④⑤⑥⑦⑧⑨⑩".contains(c)))
        .count();
    // 番号だけの欄（登録番号「23740」の並ぶ表）は項目名の欄ではない。項目名は字（漢字・仮名）で書く。
    let named = heads
        .iter()
        .filter(|l| l.words.iter().flat_map(|w| w.text.chars()).filter(|c| ('\u{3040}'..='\u{30FF}').contains(c) || ('\u{4E00}'..='\u{9FFF}').contains(c)).count() >= 2)
        .count();
    labels.len() >= 6 && numbered * 2 >= heads.len() && named * 2 >= heads.len() && n >= 3 && explained * 10 >= labels.len() * 9
}

/// 金額だけの行か（「1,248,173,132」「△13,370,826」「3,028,272　4,123,850,771」「1,949,555,027円」）。
fn amount_line(l: &Line) -> bool {
    let t: String = l.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect();
    // 単位の「円」で終わる金額（「1,949,555,027円」）も。
    let t = t.strip_suffix('円').filter(|r| r.contains(',')).unwrap_or(&t);
    t.chars().any(|c| c.is_ascii_digit()) && t.chars().all(|c| c.is_ascii_digit() || ",.△▲－-()（）".contains(c))
}

/// 上下に切る y。切った上側か下側が列組み（[`column_cut`] で切れる 2 行以上）になる
/// 行間のうち、列組み側の語数が最大になるものを選ぶ。`blocks`（取り除いた表の外枠）の中では
/// 切らず、切った側の列組みの判定にも入れる。
fn band_cut(lines: &[Line], blocks: &[Block]) -> Option<f32> {
    // 行と表を上端の順に並べ、そのどちらも跨がない行間で切る。
    let mut items: Vec<(f32, f32, Option<&Line>)> =
        lines.iter().map(|l| (l.y0, l.y1, Some(l))).chain(blocks.iter().map(|b| (b.1, b.3, None))).collect();
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    // 数が 5 つ以上並ぶ行（月ごとの欄の並ぶ広い統計の表の行）どうしの間では切らない（財政資金対民間収支実績表で、字の層に
    // 無い「—」の欄の空きが溝に見え、表の上の数行だけが列組みとして切り出されていた）。
    let numbers = |l: &Line| {
        let mut n = 0;
        for (i, w) in l.words.iter().enumerate() {
            if w.text.chars().any(|c| c.is_ascii_digit()) && !(i > 0 && number_continues(&l.words[i - 1], w)) {
                n += 1;
            }
        }
        n
    };
    let figures = |it: &(f32, f32, Option<&Line>)| it.2.is_some_and(|l| numbers(l) >= 5);
    let mut best: Option<(usize, f32)> = None;
    let mut bottom_edge = f32::NEG_INFINITY;
    for (i, &(y0, y1, _)) in items.iter().enumerate() {
        if i > 0 && y0 > bottom_edge && !(figures(&items[i - 1]) && figures(&items[i])) {
            let cut = (bottom_edge + y0) / 2.0;
            let (top, bottom) = items.split_at(i);
            let score = [top, bottom]
                .iter()
                .enumerate()
                .filter_map(|(k, part)| {
                    let part_lines: Vec<&Line> = part.iter().filter_map(|it| it.2).collect();
                    if part_lines.len() < 2 {
                        return None;
                    }
                    let ws: Vec<Col> = part_lines.iter().flat_map(|l| l.words.iter().cloned()).collect();
                    let bs: Vec<Block> = blocks.iter().filter(|b| ((b.1 + b.3) / 2.0 < cut) == (k == 0)).copied().collect();
                    // 項目名｜内容の表になる側（上に告示の前文がある国債の発行条件）も。
                    column_cut(&ws, &bs).or_else(|| label_cut(&ws)).map(|_| ws.len())
                })
                .max();
            if let Some(score) = score {
                if best.map(|(s, _)| score > s).unwrap_or(true) {
                    best = Some((score, cut));
                }
            }
        }
        bottom_edge = bottom_edge.max(y1);
    }
    best.map(|(_, cut)| cut)
}

/// 欧文の行で語間とみなす空き(em 比)。両端揃えで詰めた行は語間が 0.28 em ほどになる。
const LATIN_SPACE_EM: f32 = 0.2;
/// 和文の行の中の欧文で語間とみなす空き(em 比)。字の種類の切り替えで 0.25 em ほど空く
/// （「〇30 by 30目標」）ものは詰める。
const MIXED_LATIN_SPACE_EM: f32 = 0.3;

/// 欧文の語と語の境目か（「Zavod Iskra; JSC Research」）。数字どうし（表の金額の桁の
/// 区切りで語が割れた「3,415,」「549」）は語間にしない。
fn latin_word_break(prev: &str, next: &str) -> bool {
    let (Some(p), Some(n)) = (prev.chars().last(), next.chars().next()) else {
        return false;
    };
    let p_ok = p.is_ascii_alphabetic() || matches!(p, ';' | ',' | '.' | ':');
    let n_ok = n.is_ascii_alphabetic() || (n.is_ascii_digit() && p.is_ascii_alphabetic());
    p_ok && n_ok && !(p.is_ascii_punctuation() && n.is_ascii_digit())
}

/// 1 行の語（x 昇順）を連結する。
///
/// - 1em の 3/4 以上空いていれば全角空白を挟む（「佐賀労働局長　田之上英治」、表のセル境界）。
/// - 欧文の語と語の間は半角空白を挟む（[`latin_word_break`]。空きの下限は欧文の行と和文の行で変える）。
/// - 1 字ずつの語が並ぶ字間空けの見出し（「労　働」「公　告」「附　則」「名　　称」、表の欄見出し
///   「改　　正　　後」は 7em 空き）は詰めて 1 語にする。ただし字間が急に広がったところ
///   （表の次のセル）では区切る。
fn render_line(words: &[Col]) -> String {
    let mut s = String::new();
    let chars = || words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace());
    let latin_line = chars().filter(|c| c.is_ascii_alphabetic()).count() * 2 >= chars().count();
    let space_em = if latin_line { LATIN_SPACE_EM } else { MIXED_LATIN_SPACE_EM };
    for (i, (w, joined)) in words.iter().zip(letter_spaced(words, false)).enumerate() {
        if i > 0 && !joined {
            let p = &words[i - 1];
            let em = (p.y1 - p.y).min(w.y1 - w.y).max(1.0);
            let gap = w.x - p.x1;
            // 両端揃えの欧文の行は語間が 1 em を超えることがある（字と字の間なら 2 em までは語間と
            // みなす。括弧の前後の広い空きは表のセルの境目「(現在)　(単位：千円)」）。
            let letters = s.chars().last().is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, ';' | ','))
                && w.text.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
            let wide = if letters { em * 2.0 } else { em * 0.75 };
            // 欧文の行では、読点の後の数・数の後の語も語間を空ける（「September 18, 2026」
            // 「162, 2830 Willebroek」。金額の桁区切りの後には空きが無い）。
            let comma_number = latin_line && s.ends_with(',') && w.text.starts_with(|c: char| c.is_ascii_digit());
            // 数の後の語（「2830 Willebroek」「1 set」。単位「0.5mL」は空きが無い）。
            let number_word = latin_line && s.ends_with(|c: char| c.is_ascii_digit()) && w.text.starts_with(|c: char| c.is_ascii_alphabetic());
            // 隣り合う欄の金額（「6,328,496,709」「55,344,218,656」）。欄の間が狭くても、桁区切りの最後の 3 桁で
            // 終わる金額の後に少しでも空いて数が続けば別の金額。
            let tail = s.rsplit(|c: char| !(c.is_ascii_digit() || c == ',')).next().unwrap_or("");
            let amounts = tail.contains(',') && tail.rsplit(',').next().is_some_and(|g| g.len() == 3) && w.text.starts_with(|c: char| c.is_ascii_digit() || c == '△');
            // 小数点の後に数字の続く数（「0.000860」「0.4380」「−3.0070」の係数の欄）の後に空いて数が続けば別の数（日付の「8.6.4」は
            // 点が 2 つあるので除く）。
            let decimal = s
                .rsplit(|c: char| !(c.is_ascii_digit() || c == '.'))
                .next()
                .is_some_and(|t| t.matches('.').count() == 1 && t.split_once('.').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty()))
                && w.text.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '−' | '－' | '-' | '△' | '▲'));
            // 科目の後の金額（「租税及印紙収入」「80,698,000,000」。字間を詰めた和文の中の数は空かない）。
            // 金額は桁区切りを含む数（語が桁区切りで割れていれば、続く語まで見る）。「第67期」の数は金額ではない。
            let number: String = std::iter::once(w)
                .chain(words[i + 1..].iter().scan(w, |p, n| number_continues(p, n).then(|| { *p = n; n })))
                .map(|w| w.text.as_str())
                .collect();
            // 小数（「その他の市」「0.000040」の係数）も。
            let label_amount = s.chars().last().is_some_and(|c| !c.is_ascii() && c.is_alphanumeric())
                && number.starts_with(|c: char| c.is_ascii_digit())
                && (number.contains(',') && number.chars().all(|c| c.is_ascii_digit() || c == ',')
                    || number.matches('.').count() == 1 && !number.ends_with('.') && number.chars().all(|c| c.is_ascii_digit() || c == '.'));
            // 金額の後の科目（T 字の表の「100,068」「流動負債」。金額の後に続く単位「円」は空かない）。
            let amount_label = tail.contains(',') && w.text.starts_with(|c: char| !c.is_ascii() && c.is_alphanumeric());
            if (em * space_em..wide).contains(&gap) && (latin_word_break(&s, &w.text) || comma_number || number_word) {
                s.push(' ');
            } else if gap >= em * 0.75 || ((amounts || amount_label || decimal) && gap >= em * 0.2) || (label_amount && gap >= em * 0.5) {
                s.push('\u{3000}');
            }
        }
        s.push_str(&w.text);
    }
    s
}

/// 1 行の語（x 昇順）のそれぞれが、直前の語と字間空けの見出しとしてつながるか（先頭は false）。
/// 1 字語どうしで、字間が 8em 以内かつ見出しの中でほぼ一定なら、つながる。
///
/// `spaced_words` なら、字間を空けて組んだ複数字の語（kanpo-pdf が字間（Tc）で空けた字を 1 語に
/// したもの）も 1 字語の並びと同じに扱い、後ろの 1 字語がその語の字間とほぼ同じ空きならつなぐ。
/// 列の溝を探すときに使う（1 字語の並びだったときと同じく、表の欄の中で字間を空けて組んだ語と
/// 隣の欄の 1 字の間の溝を列の境目とみなさない）。
fn letter_spaced(words: &[Col], spaced_words: bool) -> Vec<bool> {
    let mut out = vec![false; words.len()];
    // 字間空けの見出しの字間（直前の 1 字語どうしの空き）。
    let mut spaced_gap: Option<f32> = None;
    for i in 1..words.len() {
        let (p, w) = (&words[i - 1], &words[i]);
        let em = (p.y1 - p.y).min(w.y1 - w.y).max(1.0);
        let gap = w.x - p.x1;
        let n = p.text.chars().count();
        let inner_gap = (spaced_words && n >= 2).then(|| ((p.x1 - p.x) - em * n as f32) / (n - 1) as f32).filter(|g| *g >= em * 0.2);
        if inner_gap.is_some() && spaced_gap.is_none() {
            spaced_gap = inner_gap;
        }
        // 数字 1 字どうし（統計の表の隣り合う欄の「9」と「4,512」の「4」）は字間空けの見出しではない（つなぐと
        // 「94,512」と別の数になる）。
        let digit = |t: &str| t.trim().chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c)) && !t.trim().is_empty();
        let digits = digit(&p.text) && digit(&w.text) && gap > em * 0.5;
        // 金額の単位の「円」と隣の欄の金額の頭の 1 桁（「237,029,519」「円」「2」「,706,…」）もつながない。
        let unit = p.text.trim() == "円" && digit(&w.text);
        // 字間空けの見出しの中の期の数（「第　36　期　決　算　公　告」の「36」）は 1 字とみる（数字どうしは除く）。
        // 「第」の後ろで、次も 1 字の漢字（「期」）のときだけ（字間空けの科目「剰　余　金」「合　計」の後ろの金額の「98」「93」は除く）。
        let number = |t: &str| (1..=3).contains(&t.trim().chars().count()) && t.trim().chars().all(|c| c.is_ascii_digit());
        let kanji = |k: usize| words.get(k).is_some_and(|o| o.text.trim().chars().count() == 1 && o.text.trim().chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
        let spaced_number = |k: usize| number(&words[k].text) && k > 0 && words[k - 1].text.trim() == "第" && kanji(k + 1);
        let one = |k: usize| glyphs(&words[k].text) == 1 || spaced_number(k);
        let single = (one(i - 1) || inner_gap.is_some()) && one(i) && !digits && !unit;
        out[i] = single
            && match spaced_gap {
                None => gap <= em * 8.0,
                Some(g) => (gap - g).abs() <= em,
            };
        // 番号（「Ⅱ　資　本　剰　余　金」の「Ⅱ」）と 1 字目の間の空きは、見出しの字間より狭いことがあるので字間にしない
        // （「Ⅱ資」と「本剰余金」に割れていた）。
        let numeral = p.text.trim().chars().all(|c| ('Ⅰ'..='Ⅻ').contains(&c) || c.is_ascii_digit() || ('０'..='９').contains(&c));
        spaced_gap = out[i].then_some(gap).filter(|_| !(numeral && (i == 1 || !out[i - 1])));
    }
    out
}

/// 字間空けの見出しの字数。見出しを括弧で囲んだ「（号　外）」は、括弧が頭と尻の字にくっついて
/// 「（号」「外）」の語になるので、閉じていない括弧は数えない（「（略）」のように閉じた括弧は数える）。
fn glyphs(text: &str) -> usize {
    let n = text.chars().count();
    let open = text.starts_with(['（', '(', '〔']) && !text.contains(['）', ')', '〕']);
    let close = text.ends_with(['）', ')', '〕']) && !text.contains(['（', '(', '〔']);
    n - usize::from(open) - usize::from(close)
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

    #[test]
    fn letter_spaced_word_bridges_to_next_single_glyph_only_for_gutters() {
        // 表の欄の中で字間を 0.5em 空けて組んだ語（kanpo-pdf が 1 語にしたもの）と、隣の欄の 1 字。
        // 1 字語の並びだったときと同じく、列の溝を探すときはつなぐ（溝を列の境目とみなさない）が、
        // 行の文字列では別のセルとして空白を挟む。
        let words = vec![w(173.0, 253.0, 100.0, "ル入り粒状複合"), w(261.0, 269.0, 100.0, "社")];
        assert_eq!(letter_spaced(&words, true), [false, true]);
        assert_eq!(letter_spaced(&words, false), [false, false]);
        assert_eq!(render_line(&words), "ル入り粒状複合\u{3000}社");
        // 字間を空けていない語の後ろの 1 字はつながない。
        let words = vec![w(173.0, 221.0, 100.0, "くみあい苦土"), w(229.0, 237.0, 100.0, "社")];
        assert_eq!(letter_spaced(&words, true), [false, false]);
    }

    #[test]
    fn latin_words_are_spaced_and_not_split_into_columns() {
        // 欧文の語は半角空白で区切る。表の金額の桁で割れた語や、和文の欄の境目は従来どおり。
        let line = vec![w(81.0, 104.0, 47.0, "Zavod"), w(108.0, 127.0, 47.0, "Iskra;"), w(131.0, 145.0, 47.0, "JSC")];
        assert_eq!(render_line(&line), "Zavod Iskra; JSC");
        assert_eq!(render_line(&[w(81.0, 101.0, 47.0, "3,415,"), w(104.0, 116.0, 47.0, "549")]), "3,415,549");
        assert_eq!(render_line(&[w(81.0, 97.0, 47.0, "〇30"), w(99.0, 108.0, 47.0, "by"), w(110.0, 126.0, 47.0, "30目標")]), "〇30by30目標");
        assert_eq!(render_line(&[w(81.0, 161.0, 47.0, "(31日現在)"), w(170.0, 230.0, 47.0, "(単位：千円)")]), "(31日現在)\u{3000}(単位：千円)");
        // 欧文の行の読点の後の数・数の後の語。
        let line = vec![w(81.0, 117.0, 47.0, "December"), w(121.0, 133.0, 47.0, "31,"), w(137.0, 153.0, 47.0, "2025")];
        assert_eq!(render_line(&line), "December 31, 2025");
        let line = vec![w(81.0, 109.0, 47.0, "System,"), w(113.0, 117.0, 47.0, "1"), w(121.0, 133.0, 47.0, "set.")];
        assert_eq!(render_line(&line), "System, 1 set.");
        // 両端揃えの欧文の段落で、語間がたまたま縦に揃っても列の溝とみなさない。
        let words = vec![
            w(81.0, 200.0, 47.0, "Proizvodstvennoe"),
            w(204.0, 250.0, 47.0, "Predpriyatie"),
            w(254.0, 420.0, 47.0, "Zavod Iskra; JSC Research and Production"),
            w(81.0, 248.0, 60.0, "Enterprise Plant Iskra; NPP Plant Iskra;"),
            w(252.0, 420.0, 60.0, "NPP Zavod Iskra; Aktsionernoe Obshchestvo"),
        ];
        assert_eq!(
            read(words),
            "Proizvodstvennoe Predpriyatie Zavod Iskra; JSC Research and Production\nEnterprise Plant Iskra; NPP Plant Iskra; NPP Zavod Iskra; Aktsionernoe Obshchestvo"
        );
    }

    #[test]
    fn cell_lines_keep_stacked_amounts_apart_and_join_wrapped_amounts() {
        let col = |x: f32, x1: f32, y: f32, y1: f32, text: &str| Col { id: 0, x, x1, y, y1, text: text.to_string() };
        // 行送りが字の高さとほぼ同じ金額の欄で、半行ずれて真下に置かれた「0」は別の行。
        let lines = group_cell_lines(&[col(173.0, 199.0, 163.5, 171.7, "1,627,662"), col(195.0, 199.0, 167.5, 175.7, "0")]);
        assert_eq!(lines.len(), 2);
        // 字面の高さの違う「3」「,298,613」は同じ行。
        let lines = group_cell_lines(&[col(352.5, 356.5, 71.6, 79.8, "3"), col(358.5, 384.5, 72.6, 78.8, ",298,613")]);
        assert_eq!(lines.len(), 1);
        // 欄に収まらずに折り返した金額はつなぐ。桁の揃った金額の後の数はつながない。
        let line = |text: &str, y: f32| Line { y0: y, y1: y + 8.0, words: vec![col(332.5, 368.5, y, y + 8.0, text)] };
        assert!(amount_tail(&line("113,191,23", 682.5), &line("1", 690.5)));
        assert!(!amount_tail(&line("29,077,108,832", 682.5), &line("2", 690.5)));
    }

    #[test]
    fn row_keys_stay_with_their_rows_even_beside_another_column() {
        // 罫線の無い表の行の見出しの番号（「第2821号」）。溝の右に隣の段の行（高さが揃わない）が
        // 混ざっていても、番号を表の行の頭に付ける。
        let mut words = Vec::new();
        for (k, (key, name)) in [("第2821号", "第一種膨脹式救"), ("第2822号", "〃"), ("第2823号", "〃")].into_iter().enumerate() {
            let y = 100.0 + 12.0 * k as f32;
            words.push(w(61.0, 61.0 + 4.0 * key.chars().count() as f32 + 8.0, y, key));
            words.push(w(130.0, 130.0 + 8.0 * name.chars().count() as f32, y, name));
            // 隣の段の行（行送りが違う）。
            words.push(w(449.0, 620.0, 95.0 + 13.0 * k as f32, "隣の段の本文の行で、表の行とは高さが揃わない"));
        }
        let text = read(words);
        assert!(text.starts_with("第2821号\u{3000}第一種膨脹式救\n第2822号\u{3000}〃\n第2823号\u{3000}〃"), "{text}");
    }

    #[test]
    fn small_glyphs_below_the_last_row_do_not_stall_the_table_bottom() {
        // 最後の行のすぐ下のルビ（字の高さが半分の「だい」）が表の下端になると、次の行を探す範囲に
        // そのルビがまた入り、下端が伸びないまま止まらなかった（官報本紙第1787号 4 ページ）。
        let mut words = Vec::new();
        for (k, key) in ["第31860号", "第31861号", "第31862号"].into_iter().enumerate() {
            let y = 100.0 + 12.0 * k as f32;
            words.push(w(61.0, 101.0, y, key));
            words.push(w(130.0, 170.0, y, "ＣａＴＵ"));
            words.push(w(200.0, 216.0, y, "〃"));
        }
        words.push(Col { id: 0, x: 130.0, x1: 138.0, y: 133.0, y1: 136.5, text: "だい".to_string() });
        let words: Vec<Col> = words.into_iter().enumerate().map(|(id, c)| Col { id, ..c }).collect();
        let (tables, rest) = keyed_tables(words, &[]);
        assert_eq!(tables.len(), 1);
        assert_eq!((tables[0].rows, tables[0].cols, tables[0].y1), (3, 3, 136.5));
        assert!(rest.is_empty());
    }

    #[test]
    fn unruled_table_with_row_keys_is_read_as_a_table() {
        // 罫線の無い表: 行の見出しの番号｜名称（折り返す）｜型式。右に段の境の縦の線と隣の段。
        let mut words = Vec::new();
        let mut y = 100.0;
        for (key, name, kind) in [("第2821号", vec!["第一種膨脹式救", "命いかだ（投下", "用）"]), ("第2822号", vec!["〃"]), ("第4922号", vec!["救命浮器（膨脹", "式）（船舶救命設", "備規則第57条第", "３項の規定に基", "づくもの）"])]
            .into_iter()
            .zip(["ＦＲＮ－ＳＡ－25型", "ＦＲＮ－ＳＡ－20型", "ＦＲＮ－ＰＦ－42型"])
            .map(|((k, n), t)| (k, n, t))
        {
            words.push(w(61.0, 93.0, y, key));
            for (i, part) in name.iter().enumerate() {
                words.push(w(130.0, 130.0 + 8.0 * part.chars().count() as f32, y + 8.0 * i as f32, part));
            }
            words.push(w(200.0, 270.0, y, kind));
            words.push(w(300.0, 308.0, y, "〃"));
            y += 8.0 * name.len() as f32 + 2.0;
        }
        words.push(w(449.0, 600.0, 104.0, "隣の段の本文"));
        let words: Vec<Col> = words.into_iter().enumerate().map(|(id, mut c)| {
            c.id = id;
            c
        }).collect();
        let lines = horizontal_lines_with_tables(words, &[], &[(425.0, 40.0, 500.0)], &[]);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts[0],
            "第2821号\u{3000}第一種膨脹式救命いかだ（投下用）\u{3000}ＦＲＮ－ＳＡ－25型\u{3000}〃\n第2822号\u{3000}〃\u{3000}ＦＲＮ－ＳＡ－20型\u{3000}〃\n第4922号\u{3000}救命浮器（膨脹式）（船舶救命設備規則第57条第３項の規定に基づくもの）\u{3000}ＦＲＮ－ＰＦ－42型\u{3000}〃"
        );
        assert_eq!(texts[1], "隣の段の本文");
    }

    #[test]
    fn ruled_table_is_read_row_by_row() {
        use crate::grid::{Grid, GridCell};
        // 貸借対照表の要旨: 縦組みの欄の名前｜科目｜金額。行を区切る罫線の無いセルに項目が並ぶ。
        let cell = |x0, x1| GridCell { x0, x1, y0: 0, y1: 1 };
        let total = GridCell { x0: 0, x1: 3, y0: 1, y1: 2 };
        let grid = Grid { xs: vec![60.0, 72.0, 200.0, 300.0], ys: vec![100.0, 140.0, 148.0], cells: vec![cell(0, 1), cell(1, 2), cell(2, 3), total] };
        let mut words = vec![w(62.0, 70.0, 104.0, "資"), w(62.0, 70.0, 112.0, "産"), w(62.0, 70.0, 120.0, "の"), w(62.0, 70.0, 128.0, "部")];
        for (k, (label, amount)) in [("流動資産", "47,934"), ("固定資産", "3,865"), ("合計", "51,800")].into_iter().enumerate() {
            let y = 104.0 + 10.0 * k as f32;
            words.push(w(80.0, 80.0 + 8.0 * label.chars().count() as f32, y, label));
            words.push(w(300.0 - 4.0 * amount.chars().count() as f32 - 2.0, 298.0, y, amount));
        }
        let mut words: Vec<Col> = words.into_iter().enumerate().map(|(id, mut c)| {
            c.id = id;
            c
        }).collect();
        words.push(Col { id: 99, x: 61.0, x1: 200.0, y: 156.0, y1: 164.0, text: "表の下の文".to_string() });
        let lines = horizontal_lines_with_tables(words, &[grid], &[], &[]);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["資産の部\u{3000}流動資産\u{3000}47,934\n固定資産\u{3000}3,865\n合計\u{3000}51,800", "表の下の文"]);
    }

    #[test]
    fn digit_groups_of_right_aligned_amounts_are_not_column_gutters() {
        // 桁区切りで語が割れた右揃えの金額（「5,」「369,」「515,」「116」）。桁の切れ目が行をまたいで
        // 縦に揃っても列の溝とみなさず、金額を 1 つにつなぐ。
        let mut words = Vec::new();
        for (y, label, groups) in [(100.0, "政治活動費", ["2,", "994,", "093,", "136"]), (112.0, "合計", ["5,", "369,", "543,", "815"])] {
            words.push(w(61.0, 61.0 + 8.0 * label.chars().count() as f32, y, label));
            let mut x = 300.0;
            for g in groups {
                let x1 = x + 4.0 * g.chars().count() as f32;
                words.push(w(x, x1, y, g));
                x = x1 + 2.0;
            }
        }
        assert_eq!(read(words), "政治活動費\u{3000}2,994,093,136\n合計\u{3000}5,369,543,815");
    }

    #[test]
    fn item_numbers_stay_at_the_head_of_their_rows() {
        // 項目番号（「384」）は、項目の最初の行の頭に付ける（番号の列として先にまとめない）。
        let words = vec![
            w(61.0, 73.0, 114.0, "384"),
            w(81.0, 201.0, 114.0, "株式会社アエロエレクトロマシュ"),
            w(81.0, 240.0, 127.0, "Joint Stock Company Aeroelektromash"),
            w(61.0, 73.0, 194.0, "385"),
            w(81.0, 209.0, 194.0, "株式会社モトヴィリチャ・プランツ"),
        ];
        assert_eq!(
            read(words),
            "384\u{3000}株式会社アエロエレクトロマシュ\nJoint Stock Company Aeroelektromash\n385\u{3000}株式会社モトヴィリチャ・プランツ"
        );
        // 漢数字の項目番号（「第二」「一」「二」）も同じ。
        let words = vec![
            w(61.0, 77.0, 100.0, "第二"),
            w(85.0, 245.0, 100.0, "くろまぐろ（大型魚）"),
            w(69.0, 77.0, 112.0, "一"),
            w(85.0, 245.0, 112.0, "漁獲可能量（法第15条第１項第１号関"),
            w(85.0, 101.0, 124.0, "係）"),
            w(69.0, 77.0, 136.0, "二"),
            w(85.0, 245.0, 136.0, "都道府県別漁獲可能量（法第15条第１"),
        ];
        assert_eq!(
            read(words),
            "第二\u{3000}くろまぐろ（大型魚）\n一\u{3000}漁獲可能量（法第15条第１項第１号関\n係）\n二\u{3000}都道府県別漁獲可能量（法第15条第１"
        );
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
        // 括弧で囲んだ字間空けの見出し（括弧は頭と尻の字にくっつく）。閉じた括弧の語どうしは別のセル。
        let gougai = vec![w(481.5, 501.5, 210.0, "（号"), w(521.5, 541.5, 210.0, "外）")];
        assert_eq!(render_line(&gougai), "（号外）");
        let ryaku = vec![w(100.0, 124.0, 210.0, "（略）"), w(160.0, 184.0, 210.0, "（略）")];
        assert_eq!(render_line(&ryaku), "（略）\u{3000}（略）");
    }

    #[test]
    fn columns_between_vertical_rules_only_are_read_as_a_table() {
        // 損益計算書: 科目｜金額｜金額｜合計の欄を縦の罫線だけで区切る（行の横線も枠の上下の線も無い）。金額の無い
        // 科目「財務費用」と、行間を空けずに続く科目の折り返し「掛金収」「入」。
        let words: Vec<Col> = [
            w(75.0, 139.0, 52.0, "賞与引当金繰入"),
            w(200.0, 240.0, 52.0, "57,622"),
            w(75.0, 107.0, 65.0, "財務費用"),
            w(83.0, 115.0, 78.0, "支払利息"),
            w(200.0, 240.0, 78.0, "1,248"),
            w(260.0, 300.0, 78.0, "1,248"),
            w(83.0, 123.0, 91.0, "掛金及過去"),
            w(83.0, 91.0, 99.0, "入"),
            w(200.0, 240.0, 91.0, "484,251"),
            w(75.0, 123.0, 112.0, "経常費用合計"),
            w(330.0, 370.0, 112.0, "586,425"),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, mut c)| {
            c.id = id;
            c
        })
        .collect();
        let rules = [(70.0, 46.0, 140.0), (196.0, 46.0, 140.0), (256.0, 46.0, 140.0), (316.0, 46.0, 140.0), (376.0, 46.0, 140.0)];
        let lines = horizontal_lines_with_tables(words, &[], &rules, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].text,
            "賞与引当金繰入\u{3000}57,622\n財務費用\n支払利息\u{3000}1,248\u{3000}1,248\n掛金及過去入\u{3000}484,251\n経常費用合計\u{3000}586,425"
        );
    }

    #[test]
    fn page_dividing_rule_is_cut_before_gutters_inside_a_column() {
        // 紙面を左右に分ける線（x=425）の左に科目｜金額の欄、右に別の記事。溝を左から探すと科目と金額の間で先に切れて、
        // 金額が右の記事と一緒に読まれる。
        let mut words = Vec::new();
        for (k, (label, amount)) in [("未払金", "8,875,868,726"), ("前受金", "296,591,444"), ("預り金", "43,897,136")].into_iter().enumerate() {
            let y = 100.0 + 12.0 * k as f32;
            words.push(w(89.0, 113.0, y, label));
            words.push(w(330.0, 400.0, y, amount));
        }
        for k in 0..4 {
            words.push(w(440.0, 780.0, 95.0 + 13.0 * k as f32, "右の記事の本文の行で、段の幅いっぱいまで組んだ行"));
        }
        let words: Vec<Col> = words
            .into_iter()
            .enumerate()
            .map(|(id, mut c)| {
                c.id = id;
                c
            })
            .collect();
        let lines = horizontal_lines_around(words, &[], &[(425.0, 46.0, 200.0)], &[], &[]);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(&texts[..3], ["未払金\u{3000}8,875,868,726", "前受金\u{3000}296,591,444", "預り金\u{3000}43,897,136"]);
    }

    #[test]
    fn adjacent_amounts_and_label_are_separated() {
        // 欄の間の狭い金額（空き 0.25em）と、科目の後の金額（空き 0.5em）。
        let row = vec![w(61.0, 117.0, 231.0, "租税及印紙収入"), w(121.0, 173.0, 231.0, "80,698,000,000"), w(175.0, 227.0, 231.0, "6,328,496,709")];
        assert_eq!(render_line(&row), "租税及印紙収入\u{3000}80,698,000,000\u{3000}6,328,496,709");
        // 「第67期」の数は金額ではない。
        let heading = vec![w(100.0, 108.0, 50.0, "第"), w(112.0, 120.0, 50.0, "67"), w(120.0, 128.0, 50.0, "期")];
        assert_eq!(render_line(&heading), "第67期");
    }

    #[test]
    fn numbered_labels_beside_sentences_are_read_as_rows() {
        // 国債の発行条件: 番号付きの項目名の欄（折り返しは行間を空けずに続く）｜内容の文の欄。
        let mut words = Vec::new();
        let mut y = 50.0;
        for (label, wrap, content) in [
            ("１　名称及び記号", None, vec!["利付国庫債券（10年）（第383回）を発行するものとし、その名称及び記号は次のとおりとする。"]),
            ("２　発行の根拠法律及び", Some("その条項"), vec!["財政法（昭和22年法律第34号）第４条第１項及び財政運営に必要な財源の", "確保を図るための公債の発行の特例に関する法律第３条第１項"]),
            ("３　振替法の適用等", None, vec!["社債、株式等の振替に関する法律（平成13年法律第75号）の規定の適用を受ける"]),
            ("４　発行方法", None, vec!["価格を競争に付して行われる入札による発行及び価格競争入札と同時に行われる", "入札による発行とする。"]),
            ("５　募入決定の方法", None, vec!["各申込みのうち応募価格の高いものからその応募額を順次割り当てる方法とする。"]),
            ("６　発行額", None, vec!["額面金額で1,905,900,000,000円とし、その内訳は財務大臣が定めるところによる。"]),
        ] {
            words.push(w(61.0, 61.0 + 8.0 * label.chars().count() as f32, y, label));
            if let Some(wr) = wrap {
                words.push(w(77.0, 77.0 + 8.0 * wr.chars().count() as f32, y + 8.0, wr));
            }
            for (k, c) in content.iter().enumerate() {
                words.push(w(157.0, 157.0 + 8.0 * c.chars().count().min(32) as f32, y + 10.5 * k as f32, c));
            }
            y += 10.5 * content.len().max(2) as f32 + 3.0;
        }
        let words: Vec<Col> = words
            .into_iter()
            .enumerate()
            .map(|(id, mut c)| {
                c.id = id;
                c
            })
            .collect();
        let lines = horizontal_lines(words);
        let text: Vec<String> = lines.iter().flat_map(|l| l.text.lines().map(str::to_string).collect::<Vec<_>>()).collect();
        assert_eq!(text.len(), 6, "{text:?}");
        assert!(text[1].starts_with("２　発行の根拠法律及びその条項\u{3000}財政法"), "{}", text[1]);
        assert!(text[1].ends_with("財源の確保を図るための公債の発行の特例に関する法律第３条第１項"), "{}", text[1]);
    }

    #[test]
    fn kanji_registration_numbers_are_row_keys() {
        for key in ["令08全保1384", "生第100209号", "輸第4774号", "交Ｎ26－67"] {
            assert!(row_key(key), "{key}");
        }
        for text in ["令和８年７月10日", "1,000円", "第三条", "北海道知事", "(胆振９)661", "香川55－７", "1142番地３", "町１－21－１"] {
            assert!(!row_key(text), "{text}");
        }
    }

    #[test]
    fn article_columns_between_rules_are_not_a_table() {
        // 段の境の線と紙面の枠の線に挟まれた記事（文の入った欄）は表にしない。
        let words: Vec<Col> = [w(75.0, 180.0, 52.0, "公告します。"), w(200.0, 240.0, 52.0, "1,248"), w(260.0, 300.0, 52.0, "2,000"), w(320.0, 360.0, 52.0, "3,000")]
            .into_iter()
            .enumerate()
            .map(|(id, mut c)| {
                c.id = id;
                c
            })
            .collect();
        let rules = [(70.0, 46.0, 140.0), (196.0, 46.0, 140.0), (256.0, 46.0, 140.0), (316.0, 46.0, 140.0), (376.0, 46.0, 140.0)];
        let (tables, rest) = rule_column_tables(words, &rules, &[]);
        assert!(tables.is_empty());
        assert_eq!(rest.len(), 4);
    }
}
