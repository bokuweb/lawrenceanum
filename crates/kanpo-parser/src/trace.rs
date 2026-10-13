//! 復元過程の可視化用データ（playground・デバッグ用）。
//!
//! [`crate::analyze`] は、ページの各語（`pdftotext` の `<word>`）がどの領域（前文・改正後・
//! 改正前・表外・本文）のどの行に組まれたか、捨てられたならなぜかを返す。本文テキストも
//! この構造から組み立てるので、可視化と実際の出力は必ず一致する。

use serde::Serialize;

/// 復元した 1 行（縦書きの 1 列、表ならセルを連結した 1 行）と、それを構成する語の ID。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub text: String,
    pub words: Vec<usize>,
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
    /// （欄見出しの無い新旧対照表の継続ページとして上下帯に分けた）。
    pub layout: &'static str,
    pub words: Vec<Word>,
    pub regions: Vec<Region>,
    /// このページの復元テキスト（[`crate::reconstruct_vertical`] のページ分）。
    pub text: String,
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
    lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n").trim().to_string()
}

impl Unit {
    /// `改正後`／`改正前` の欄見出し行を冠したテキストに組む。見出し行はフロントの
    /// 新旧対照表組み(parseShinkyu)と `detect_format` の shinkyu 判定の双方に整合する。
    pub(crate) fn text(&self) -> String {
        match self {
            Unit::Plain(lines) => join_lines(lines),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                let join = |parts: &[Vec<Line>]| -> String {
                    parts
                        .iter()
                        .map(|p| join_lines(p))
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let mut out: Vec<String> = Vec::new();
                let pre = join_lines(preamble);
                if !pre.is_empty() {
                    out.push(pre);
                }
                let (after, before) = (join(after), join(before));
                if !after.is_empty() || !before.is_empty() {
                    out.push("改正後".to_string());
                    out.push(after);
                    out.push("改正前".to_string());
                    out.push(before);
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
