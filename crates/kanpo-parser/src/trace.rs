//! 復元過程の可視化用データ（playground・デバッグ用）。
//!
//! [`crate::analyze`] は、ページの各語（`pdftotext` の `<word>`）がどの領域（前文・改正後・
//! 改正前・表外・本文）のどの行に組まれたか、捨てられたならなぜかを返す。本文テキストも
//! この構造から組み立てるので、可視化と実際の出力は必ず一致する。

use serde::Serialize;

/// 復元した 1 行（縦書きの 1 列、表ならセルを連結した 1 行）と、それを構成する語の ID。
/// 罫線の表は 1 つの行として持ち、`table` にセル構造を入れる（`text` は表を行ごとに
/// 書き下したもの）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub text: String,
    pub words: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Table>,
}

impl Line {
    pub fn text(text: String, words: Vec<usize>) -> Self {
        Line { text, words, table: None }
    }
}

/// 罫線から復元した表。座標は枠（pt）、セルは論理上の行・列（縦組みの表は転置済み:
/// 右の縦の列が 1 行目、上の欄が 1 列目）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Table {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<TableCell>,
}

/// 表のセル。結合セルは `rowspan`/`colspan` が 2 以上。`x0..y1` はセルの枠（pt）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableCell {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub text: String,
    pub words: Vec<usize>,
}

impl Table {
    /// テキスト出力用に、行ごとにセルを全角空白で区切って書き下す（セルの中の改行も全角空白に
    /// する。「大管工業㈱」「代表取締役　中島　栄一」のような項目を続けて書かないため）。
    pub fn render(&self) -> String {
        (0..self.rows)
            .map(|r| {
                let mut cells: Vec<&TableCell> = self.cells.iter().filter(|c| c.row == r).collect();
                cells.sort_by_key(|c| c.col);
                cells.iter().map(|c| c.text.replace('\n', "\u{3000}")).collect::<Vec<_>>().join("\u{3000}")
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 構造化結果（[`crate::model::NestedTable`]）へ。
    pub fn to_nested(&self) -> crate::model::NestedTable {
        use crate::model::{CellMerge, NestedTable, Run};
        let mut rows: Vec<Vec<Vec<Run>>> = vec![vec![Vec::new(); self.cols]; self.rows];
        let mut merges = Vec::new();
        for c in &self.cells {
            if !c.text.is_empty() {
                rows[c.row][c.col] = vec![Run::plain(c.text.clone())];
            }
            if c.rowspan > 1 || c.colspan > 1 {
                merges.push(CellMerge {
                    row: c.row,
                    col: c.col,
                    rowspan: c.rowspan,
                    colspan: c.colspan,
                });
            }
        }
        NestedTable { rows, merges }
    }
}

/// 行が属する領域の役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// 通常の本文（新旧対照表ではない段）。
    Body,
    /// 新旧対照表の欄見出しより右（前文・標題）。
    Preamble,
    /// 改正後欄。
    After,
    /// 改正前欄。
    Before,
    /// 新旧対照表と同じ段にあるが表の外の列（同居する別記事など）。
    Outside,
}

/// 同じ役割の行のまとまり。`unit` はページ内の段（または表）の通し番号、`part` は
/// 表がページ内の下の段へ続く場合の何段目か（0 始まり）。
#[derive(Debug, Clone, Serialize)]
pub struct Region {
    pub role: Role,
    pub unit: usize,
    pub part: usize,
    pub lines: Vec<Line>,
}

/// 語の扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WordStatus {
    /// どこかの行に組まれた。
    Used,
    /// 上下の余白帯（柱）にあるため捨てた。
    Margin,
    /// 正規化後に空になった（傍線などの私用領域グリフだけの語）。
    Empty,
    /// 新旧対照表の欄見出し（改/正/後/前）。
    HeaderLabel,
    /// 柱と判定した行（発行日・号数など）に含まれていた。
    Noise,
    /// 振り仮名（親字の脇に小さく組んだ読み）。
    Ruby,
}

/// ページ上の 1 語。座標は `pdftotext -bbox-layout` と同じ（pt、左上原点）。
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    pub id: usize,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// PDF から取れた生の文字列。
    pub raw: String,
    /// 正規化後（縦書き約物・私用 CID グリフの復元後）の文字列。
    pub text: String,
    pub status: WordStatus,
}

/// 1 ページの復元結果。
#[derive(Debug, Clone, Serialize)]
pub struct PageTrace {
    pub width: f32,
    pub height: f32,
    /// ページ全体をどう扱ったか: `"tiers"`（段に分けて復元）/ `"headerless_shinkyu"`
    /// （欄見出しの無い新旧対照表の継続ページとして上下帯に分けた）/ `"horizontal"`
    /// （横組みのページとして pdftotext の行順で組んだ）。
    pub layout: &'static str,
    pub words: Vec<Word>,
    pub regions: Vec<Region>,
    /// このページの復元テキスト（[`crate::reconstruct_vertical`] のページ分）。
    pub text: String,
    /// 構造化用のテキスト。表は [`table_marker`] の 1 行になる。
    #[serde(skip)]
    pub(crate) doc_text: String,
    /// このページの表（`doc_text` の目印の番号はページをまたいだ通し番号）。
    #[serde(skip)]
    pub(crate) tables: Vec<Table>,
}

/// 構造化用テキストの中で表の位置を示す目印（私用領域の文字で囲んだ通し番号）。
pub(crate) fn table_marker(index: usize) -> String {
    format!("\u{E000}{index}\u{E001}")
}

/// 構造化用テキストで新旧対照表の終わりを示す行。改正前セルはここで閉じる（表の後ろの
/// 前文・附則を改正前セルに取り込まない）。
pub(crate) const SHINKYU_END: &str = "\u{E002}";

/// 文字列が表の目印だけの行なら、その通し番号。
pub(crate) fn parse_table_marker(s: &str) -> Option<usize> {
    s.trim().strip_prefix('\u{E000}')?.strip_suffix('\u{E001}')?.parse().ok()
}

/// 段（または表）1 つ分の復元結果。テキストと領域の両方をここから作る。
pub(crate) enum Unit {
    Plain(Vec<Line>),
    Shinkyu {
        preamble: Vec<Line>,
        after: Vec<Vec<Line>>,
        before: Vec<Vec<Line>>,
        outside: Vec<Vec<Line>>,
    },
}

/// 行列を改行で連結する（前後の空白は落とす）。
pub(crate) fn join_lines(lines: &[Line]) -> String {
    join_lines_with(lines, &mut |l| l.text.clone())
}

fn join_lines_with(lines: &[Line], render: &mut dyn FnMut(&Line) -> String) -> String {
    lines.iter().map(render).collect::<Vec<_>>().join("\n").trim().to_string()
}

impl Unit {
    /// `改正後`／`改正前` の欄見出し行を冠したテキストに組む。見出し行はフロントの
    /// 新旧対照表組み(parseShinkyu)と `detect_format` の shinkyu 判定の双方に整合する。
    pub(crate) fn text(&self) -> String {
        self.text_with(&mut |l| l.text.clone(), false)
    }

    /// 構造化用のテキスト。表の行は目印に置き換え、表を `tables` に積む（目印の番号は
    /// `first_index` からの通し番号）。
    pub(crate) fn doc_text(&self, tables: &mut Vec<Table>, first_index: usize) -> String {
        self.text_with(
            &mut |l| match &l.table {
                Some(t) => {
                    tables.push(t.clone());
                    table_marker(first_index + tables.len() - 1)
                }
                None => l.text.clone(),
            },
            true,
        )
    }

    /// `end_marker` なら新旧対照表の改正前の後ろに [`SHINKYU_END`] を置く（構造化用）。
    fn text_with(&self, render: &mut dyn FnMut(&Line) -> String, end_marker: bool) -> String {
        match self {
            Unit::Plain(lines) => join_lines_with(lines, render),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                let mut out: Vec<String> = Vec::new();
                let pre = join_lines_with(preamble, render);
                let mut join = |parts: &[Vec<Line>]| -> String {
                    parts
                        .iter()
                        .map(|p| join_lines_with(p, render))
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                if !pre.is_empty() {
                    out.push(pre);
                }
                let (after, before) = (join(after), join(before));
                if !after.is_empty() || !before.is_empty() {
                    out.push("改正後".to_string());
                    out.push(after);
                    out.push("改正前".to_string());
                    out.push(before);
                    if end_marker {
                        out.push(SHINKYU_END.to_string());
                    }
                }
                let rest = join(outside);
                if !rest.is_empty() {
                    out.push(rest);
                }
                out.join("\n").trim().to_string()
            }
        }
    }

    /// 可視化用の領域列（空の領域は省く）。
    pub(crate) fn regions(self, unit: usize) -> Vec<Region> {
        let mut out = Vec::new();
        let mut push = |role: Role, part: usize, lines: Vec<Line>| {
            if !lines.is_empty() {
                out.push(Region { role, unit, part, lines });
            }
        };
        match self {
            Unit::Plain(lines) => push(Role::Body, 0, lines),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                push(Role::Preamble, 0, preamble);
                for (i, l) in after.into_iter().enumerate() {
                    push(Role::After, i, l);
                }
                for (i, l) in before.into_iter().enumerate() {
                    push(Role::Before, i, l);
                }
                for (i, l) in outside.into_iter().enumerate() {
                    push(Role::Outside, i, l);
                }
            }
        }
        out
    }
}
