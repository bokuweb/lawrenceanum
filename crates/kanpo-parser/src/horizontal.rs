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

use crate::grid::Grid;
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
    horizontal_lines_around(cols, &[])
}

/// 語を取り除いた表の外枠 `(x0, y0, x1, y1)`。
type Block = (f32, f32, f32, f32);

/// [`horizontal_lines`] で、表として先に取り除いた語の外枠 `blocks` を、列の溝・上下の切れ目を
/// ふさぐものとして扱う版。2 段抜きの表を抜いた跡が列の溝に見え、表の上の行（日付と署名が
/// 1 行に並ぶ「令和八年十月十五日　九州運輸局長　日向　弘基」）が下の段組の列に分かれないようにする。
fn horizontal_lines_around(cols: Vec<Col>, blocks: &[Block]) -> Vec<trace::Line> {
    let mut lines: Vec<trace::Line> = Vec::new();
    order_region(cols, blocks, &mut lines);
    lines
}

/// 横組みの語を、列に分けずに上から 1 行ずつ組む（縦書きのページの中の小さな横組みの欄用。
/// 欄は段の幅より狭く、段組みは無い。表の行は左のセルから順に読む）。
pub(crate) fn row_lines(cols: Vec<Col>) -> Vec<trace::Line> {
    group_lines(&cols)
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
    !numeric && !labeled(&next) && text.chars().count() >= 4 && right - end <= (prev.y1 - prev.y0).max(1.0) * WRAP_EM
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
pub(crate) fn horizontal_lines_with_tables(cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)]) -> Vec<trace::Line> {
    let mut rest = cols;
    let mut tables: Vec<trace::Table> = Vec::new();
    for g in grids.iter().filter(|g| g.xs.len() >= 3 && g.ys.len() >= 3 && !grids.iter().any(|o| g.encloses(o))) {
        let (mine, others): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(|c| g.contains(x_center(c), y_center(c), 0.5));
        rest = others;
        match grid_table(g, &mine) {
            Some(t) => tables.push(t),
            None => rest.extend(mine),
        }
    }
    let (keyed, others) = keyed_tables(rest, vrules);
    rest = others;
    tables.extend(keyed);
    if tables.is_empty() {
        return horizontal_lines(rest);
    }
    let geom: std::collections::HashMap<usize, (f32, f32, f32)> = rest.iter().map(|c| (c.id, (c.x, c.x1, c.y))).collect();
    // 2 段抜きの表（本文の段 2 つ分の幅）だけ。決算公告の囲みの中の表は囲みの列ごとに並び、
    // 囲みの列の溝は本文の段の溝と揃わないので、溝をふさぐと囲みの読み順が崩れる。
    let blocks: Vec<Block> =
        tables.iter().filter(|t| t.x1 - t.x0 >= MIN_TEXT_COLUMN_WIDTH * 2.0).map(|t| (t.x0, t.y0, t.x1, t.y1)).collect();
    let mut lines = horizontal_lines_around(rest, &blocks);
    let mut placed: Vec<(usize, trace::Table)> = tables
        .into_iter()
        .map(|t| {
            let after = |l: &trace::Line| {
                let g: Vec<(f32, f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
                let x0 = g.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let x1 = g.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                let y0 = g.iter().map(|p| p.2).fold(f32::INFINITY, f32::min);
                !g.is_empty() && (x0 >= t.x1 - 2.0 || (x0 < t.x1 && x1 > t.x0 && y0 >= t.y1 - 2.0))
            };
            (lines.iter().position(after).unwrap_or(lines.len()), t)
        })
        .collect();
    // 後ろの位置から差し込む。同じ位置には、読み順で後の表（下・右）から差し込む。
    placed.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.x0.total_cmp(&a.1.x0)).then(b.1.y0.total_cmp(&a.1.y0)));
    for (at, t) in placed {
        lines.insert(at, trace::Line { text: t.render(), words: Vec::new(), table: Some(t) });
    }
    lines
}

/// 表の行の見出しの番号（「第2821号」「第165回」「第Ｐ－280号」）か。
fn row_key(text: &str) -> bool {
    let t = text.trim();
    let inner = t.strip_prefix('第').and_then(|t| t.strip_suffix(['号', '回']));
    inner.is_some_and(|i| {
        (1..=8).contains(&i.chars().count())
            && i.chars().any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
            && i.chars().all(|c| c.is_ascii_alphanumeric() || ('０'..='９').contains(&c) || ('Ａ'..='Ｚ').contains(&c) || "－-".contains(c))
    })
}

/// 罫線の無い横組みの表で、左端の列に行の見出しの番号（[`row_key`]）が 3 つ以上縦に並ぶものを表に
/// する。`(表, 表に入らなかった語)`。
///
/// - 行: 番号の上端から次の番号の上端まで。最後の行は、番号の列に字の無い行が続く限り（折り返しの続き）
/// - 欄: 番号の行で番号の右に並ぶ語の左端。右端は、番号の行の高さに掛かる縦の罫線（段の境の線）が
///   あればそこまで、無ければ番号から右へ 5 em 未満の空きで続く語まで（隣の段の行を入れない）
/// - セルの中の行は折り返しの続きとしてつなぐ（「第一種膨脹式救」「命いかだ（投下」「用）」）
fn keyed_tables(words: Vec<Col>, vrules: &[(f32, f32, f32)]) -> (Vec<trace::Table>, Vec<Col>) {
    let mut words = words;
    let mut tables = Vec::new();
    loop {
        let mut keys: Vec<Col> = words.iter().filter(|w| row_key(&w.text)).cloned().collect();
        keys.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
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
        let mut cluster: Vec<Col> = Vec::new();
        for k in keys {
            let em = (k.y1 - k.y).max(1.0);
            let joins = cluster
                .last()
                .is_some_and(|l| (k.x - cluster[0].x).abs() < 2.0 && k.y > l.y && (k.y - l.y < em * 6.0 || continuous(l, &k)));
            if joins {
                cluster.push(k);
            } else if cluster.len() >= 3 {
                break;
            } else {
                cluster = vec![k];
            }
        }
        if cluster.len() < 3 {
            break;
        }
        cluster.sort_by(|a, b| a.y.total_cmp(&b.y));
        let em = cluster.iter().map(|k| k.y1 - k.y).fold(0.0, f32::max).max(1.0);
        let left = cluster[0].x - 1.0;
        let (first, final_y) = (cluster[0].y, cluster.last().unwrap().y1);
        let fence = vrules
            .iter()
            .filter(|&&(x, y0, y1)| x > cluster[0].x1 && y0 <= first && final_y <= y1)
            .map(|&(x, _, _)| x)
            .reduce(f32::min);
        // 番号の行で番号から右へ続く語（欄の左端）。
        let mut starts: Vec<f32> = Vec::new();
        let mut right = cluster.iter().map(|k| k.x1).fold(f32::NEG_INFINITY, f32::max);
        for k in &cluster {
            let mut row: Vec<&Col> = words.iter().filter(|w| (y_center(w) - y_center(k)).abs() <= em * 0.35 && w.x >= k.x1 - 0.5).collect();
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            let mut end = k.x1;
            for w in row {
                let outside = match fence {
                    Some(f) => w.x1 > f,
                    None => w.x - end >= em * 5.0,
                };
                if outside {
                    break;
                }
                if w.x - end >= em * 0.75 {
                    starts.push(w.x);
                }
                end = end.max(w.x1);
            }
            right = right.max(end);
        }
        starts.sort_by(f32::total_cmp);
        let mut columns: Vec<f32> = vec![left];
        for x in starts {
            if columns.last().is_some_and(|&c| x - c > em) {
                columns.push(x - 1.0);
            }
        }
        // 表の語: 番号の列の左端から右端まで、最初の番号の上端から。最後の行は、番号の列に字の無い行が
        // 1 em 未満の空きで続く限り。
        let top = cluster[0].y - 0.5;
        let last = cluster.last().unwrap().clone();
        let in_x = |w: &Col| w.x >= left && w.x1 <= right + 2.0;
        let mut bottom = last.y1;
        let key_end = columns.get(1).copied().unwrap_or(right) - 1.0;
        loop {
            // 次の行（いまの下端から 1 em 以内に始まる語）。番号の列に字があれば表の外。
            let next: Vec<&Col> = words
                .iter()
                .filter(|w| in_x(w) && w.y >= bottom - em * 0.5 && w.y < bottom + em && w.y > last.y + em * 0.5)
                .collect();
            if next.is_empty() || next.iter().any(|w| w.x < key_end) {
                break;
            }
            bottom = next.iter().map(|w| w.y1).fold(bottom, f32::max);
        }
        let (mine, others): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| in_x(w) && w.y >= top && w.y1 <= bottom + 0.5);
        words = others;
        let row_of = |w: &Col| cluster.iter().rposition(|k| k.y - em * 0.35 <= w.y).unwrap_or(0);
        let col_of = |w: &Col| columns.iter().rposition(|&c| c <= w.x + 1.0).unwrap_or(0);
        let mut cells: std::collections::BTreeMap<(usize, usize), Vec<Col>> = std::collections::BTreeMap::new();
        for w in &mine {
            cells.entry((row_of(w), col_of(w))).or_default().push(w.clone());
        }
        let nc = columns.len();
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
                    y0: cluster[row].y,
                    x1: columns.get(col + 1).copied().unwrap_or(right),
                    y1: cluster.get(row + 1).map_or(bottom, |k| k.y),
                    text: lines.iter().map(|l| render_line(&l.words)).collect::<String>(),
                    words: ws.iter().map(|w| w.id).collect(),
                }
            })
            .collect();
        tables.push(trace::Table { x0: left, y0: top, x1: right, y1: bottom, rows: cluster.len(), cols: nc, cells });
    }
    (tables, words)
}

/// 格子のセルに語を割り当てて横組みの表にする。中の語が横組みでなければ（縦組みの表）、
/// 字が 1 つのセルにしか無ければ、文章の入ったセル（囲み記事）があれば `None`。
///
/// 行を区切る罫線の無い欄（1 つのセルに複数の項目が並ぶ）は、項目ごとの行に分ける。行の頭は、
/// ほかの欄の行と高さが揃う行（揃わない行はセルの中の折り返し）。
fn grid_table(g: &Grid, words: &[Col]) -> Option<trace::Table> {
    // 2 字以上の語の字数で、横長の語が縦長の語より多いこと。
    let (h_chars, v_chars) = words.iter().fold((0, 0), |(h, v), w| {
        let n = w.text.chars().count();
        if n < 2 {
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
    let prose = cell_lines.iter().any(|ls| {
        let t: String = ls.iter().flat_map(|l| l.words.iter().map(|w| w.text.as_str())).collect();
        t.chars().count() > 200 || t.matches('。').count() >= 2
    });
    if prose {
        return None;
    }
    // セル i の k 行目が、前の行の折り返しの続きか。前の行が文で、セルの右端まで組まれている
    // （右揃えの金額の行は右端に届くが、折り返しではない）。
    let wraps = |i: usize, k: usize| k > 0 && wrapped(&cell_lines[i][k - 1], &cell_lines[i][k], g.xs[g.cells[i].x1]);
    // 行の頭の高さ（格子の行ごと）。
    let ny = g.ys.len() - 1;
    let mut anchors: Vec<f32> = Vec::new();
    for r in 0..ny {
        let band: Vec<usize> = (0..g.cells.len()).filter(|&i| g.cells[i].y0 <= r && r < g.cells[i].y1).collect();
        anchors.push(g.ys[r]);
        let (top, bottom) = (g.ys[r], g.ys[r + 1]);
        for &i in &band {
            for (k, l) in cell_lines[i].iter().enumerate() {
                let c = l.center();
                // 欄によって字の種類（数字と漢字）で字面の高さが少し違うので、行の高さの 4 割まで。
                let tol = ((l.y1 - l.y0) * 0.4).max(ALIGN_TOL);
                // 揃う 2 行がどちらも折り返しの続きなら、行の頭ではない（2 つの欄が同時に折り返した）。
                let aligned = band.iter().filter(|&&j| g.cells[j].x0 != g.cells[i].x0).any(|&j| {
                    cell_lines[j].iter().enumerate().any(|(n, m)| !(wraps(i, k) && wraps(j, n)) && (m.center() - c).abs() <= tol)
                });
                if top < c && c < bottom && aligned {
                    anchors.push(l.y0 - 0.5);
                }
            }
        }
    }
    anchors.sort_by(f32::total_cmp);
    anchors.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    let row_of = |y: f32| anchors.iter().rposition(|&a| a <= y).unwrap_or(0);
    let mut cells: Vec<trace::TableCell> = Vec::new();
    for (i, ls) in cell_lines.iter().enumerate() {
        let gc = g.cells[i];
        let mut by_row: std::collections::BTreeMap<usize, Vec<&Line>> = std::collections::BTreeMap::new();
        // 縦組みの欄の名前（1 字ずつ縦に積んだ字）は、行に分けずに一番上の行に置く。
        let all: Vec<&Col> = ls.iter().flat_map(|l| l.words.iter()).collect();
        let whole = stacked(&all);
        for l in ls {
            let row = if whole { row_of(ls[0].center()) } else { row_of(l.center()) };
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
                        if k > 0 && !wrapped(ls[k - 1], l, g.xs[gc.x1]) && !amount_tail(ls[k - 1], l) {
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

/// 語が縦に積まれているか（縦組みの欄の名前）。2 語・3 字以上で、どの語も 1〜2 字か縦長で、全体が
/// 横幅以上の高さ（2 列に組んだ「資産｜の部」は正方形に近い）。
fn stacked(words: &[&Col]) -> bool {
    let (x0, x1) = words.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), w| (a.min(w.x), b.max(w.x1)));
    let (y0, y1) = words.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), w| (a.min(w.y), b.max(w.y1)));
    let upright = |w: &&Col| (w.text.chars().count() <= 2 && w.x1 - w.x <= (w.y1 - w.y) * 1.5) || w.y1 - w.y > (w.x1 - w.x) * 1.5;
    // 同じ欄に縦に並んだ「〃」（行ごとの繰り返しの印）は欄の名前ではない。
    let text: String = words.iter().map(|w| w.text.as_str()).collect();
    // 横の行に 1〜2 字の語が 3 つ以上並ぶなら、字間を空けた横組みの項目名（「事　業　収　益」）。
    // 縦長の語（「及び純資産の部」）は行にまとめると何行にも掛かるので数えない。
    let owned: Vec<Col> = words.iter().filter(|w| w.text.chars().count() <= 2).map(|w| (*w).clone()).collect();
    words.len() >= 2
        && text.chars().count() >= 3
        && !text.chars().all(|c| c == '〃')
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
    lines
        .into_iter()
        .map(|(_, mut l)| {
            l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
            l
        })
        .collect()
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
///
/// `blocks`（取り除いた表の外枠）は語と同じく溝・切れ目をふさぎ、中心で左右・上下に振り分ける。
fn order_region(words: Vec<Col>, blocks: &[Block], out: &mut Vec<trace::Line>) {
    if words.is_empty() {
        return;
    }
    let lines = group_lines(&words);
    if lines.len() >= 2 {
        if let Some(cut) = column_cut(&words, blocks) {
            let (left, right): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| x_center(w) < cut);
            let (bl, br): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.0 + b.2) / 2.0 < cut);
            order_region(left, &bl, out);
            order_region(right, &br, out);
            return;
        }
        if let Some(cut) = band_cut(&lines, blocks) {
            let (top, bottom): (Vec<Col>, Vec<Col>) = words.into_iter().partition(|w| y_center(w) < cut);
            let (bt, bb): (Vec<Block>, Vec<Block>) = blocks.iter().partition(|b| (b.1 + b.3) / 2.0 < cut);
            order_region(top, &bt, out);
            order_region(bottom, &bb, out);
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
/// 語は中心 x で左右に振り分ける（[`x_center`]）。`blocks`（取り除いた表の外枠）も溝をふさぐ。
fn column_cut(words: &[Col], blocks: &[Block]) -> Option<f32> {
    // 字間空けの見出し（「中　間　決　算　公　告」）は 1 字語の並びだが、1 語として溝をふさぐ。
    // 桁区切りで語が割れた数（「225,」「582,」「082,」「247」）も 1 語として溝をふさぐ（右揃えの金額の
    // 桁の切れ目が行をまたいで縦に揃い、列の溝に見える）。
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for line in group_lines(words) {
        for (i, (w, joined)) in line.words.iter().zip(letter_spaced(&line.words, true)).enumerate() {
            let number = i > 0 && number_continues(&line.words[i - 1], w);
            match spans.last_mut() {
                Some(span) if joined || number => span.1 = w.x1,
                _ => spans.push((w.x, w.x1)),
            }
        }
    }
    spans.extend(blocks.iter().map(|b| (b.0, b.2)));
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
            if !latin_word_gap(words, cut) && !item_numbers(&left, &right) && !repeat_marks(&left, &right) && is_column_split(&left, &right) {
                return Some(cut);
            }
        }
        right_edge = right_edge.max(b);
    }
    None
}

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
/// 行間のうち、列組み側の語数が最大になるものを選ぶ。`blocks`（取り除いた表の外枠）の中では
/// 切らず、切った側の列組みの判定にも入れる。
fn band_cut(lines: &[Line], blocks: &[Block]) -> Option<f32> {
    // 行と表を上端の順に並べ、そのどちらも跨がない行間で切る。
    let mut items: Vec<(f32, f32, Option<&Line>)> =
        lines.iter().map(|l| (l.y0, l.y1, Some(l))).chain(blocks.iter().map(|b| (b.1, b.3, None))).collect();
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best: Option<(usize, f32)> = None;
    let mut bottom_edge = f32::NEG_INFINITY;
    for (i, &(y0, y1, _)) in items.iter().enumerate() {
        if i > 0 && y0 > bottom_edge {
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
                    column_cut(&ws, &bs).map(|_| ws.len())
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
            if (em * space_em..wide).contains(&gap) && (latin_word_break(&s, &w.text) || comma_number || number_word) {
                s.push(' ');
            } else if gap >= em * 0.75 {
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
        let single = (glyphs(&p.text) == 1 || inner_gap.is_some()) && glyphs(&w.text) == 1;
        out[i] = single
            && match spaced_gap {
                None => gap <= em * 8.0,
                Some(g) => (gap - g).abs() <= em,
            };
        spaced_gap = out[i].then_some(gap);
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
        let lines = horizontal_lines_with_tables(words, &[], &[(425.0, 40.0, 500.0)]);
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
        let lines = horizontal_lines_with_tables(words, &[grid], &[]);
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
}
