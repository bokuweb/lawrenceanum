//! 改め文の構造化表現。`reconstruct_vertical` が出すマーカー付きテキストを、
//! HTML や表など任意の形式に変換しやすい構造化データへ整える。
//!
//! - `Document` — 1 件の改め文。形式（prose/shinkyu）とブロック列。
//! - `Block` — 段落（散文）か、新旧対照表（改正後/改正前）。
//! - `Run` — 連続するテキスト片。傍線（下線）の有無を持つ。
//!
//! 傍線は現状の `pdftotext`/`pdftocairo` 出力からは復元できないため、いまは常に
//! `false`（型としてのみ用意し、将来の傍線抽出で充填できるようにする）。

use serde::Serialize;

use crate::format::detect_format;

/// 改め文 1 件の構造化表現。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    /// "prose" | "shinkyu" | "unknown"。
    pub format: String,
    /// 文書を構成するブロック列（出現順）。
    pub blocks: Vec<Block>,
}

/// 文書ブロック。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// 散文（前文・散文改め文など）。
    Paragraph { runs: Vec<Run> },
    /// 新旧対照表の 1 段。条（別表）ごとの行に分け、各行で改正後/改正前を対応させる。
    Shinkyu { rows: Vec<ShinkyuRow> },
}

/// 新旧対照表の 1 行（条・別表など 1 単位）。改正後/改正前のセル。
/// 片側のみ存在する行（新設・削除）はもう一方が空 Vec になる。
/// 別表（罫線で区切られた表）の行では `after_table`/`before_table` に 2D 表が入る。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShinkyuRow {
    pub after: Vec<Run>,
    pub before: Vec<Run>,
    /// 別表の改正後側 2D 表（あれば）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_table: Option<NestedTable>,
    /// 別表の改正前側 2D 表（あれば）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_table: Option<NestedTable>,
}

/// 別表（罫線グリッド）を復元した 2D 表。`rows[r][c]` がセルの Run 列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NestedTable {
    pub rows: Vec<Vec<Vec<Run>>>,
}

/// 同一スタイルが続くテキスト片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    /// 傍線（下線）が付いているか。現状は常に false（将来の傍線抽出用の予約フィールド）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Run {
    /// 傍線無しのプレーンな Run を作る。
    pub fn plain(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            underline: false,
        }
    }
}

/// テキストの 1 セル（複数行）を 1 つの Run に畳む。空なら空 Vec。
/// 縦書きの列折り返しで途中改行された行は結合し、条・号・文末などの構造的改行のみ残す。
fn cell_runs(lines: &[&str]) -> Vec<Run> {
    let text = join_wrapped_lines(lines);
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Run::plain(text)]
    }
}

/// 縦書きの列折り返し（語の途中での改行）を結合する。
///
/// 各行は復元時の 1 縦列に相当し、列幅で途中改行される。文末（`。`）で終わる行や、
/// 次行が構造マーカー（`第○条` / 号（一・二…/イ・ロ…/数字）/ `（見出し）` / `別表` /
/// `附則`）で始まる箇所だけ改行を残し、それ以外は連結して読みやすくする。
fn join_wrapped_lines(lines: &[&str]) -> String {
    let lines: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        out.push_str(line);
        if idx + 1 < lines.len() {
            let ends_sentence = line.ends_with('。') || line.ends_with('！') || line.ends_with('？');
            if ends_sentence || starts_structural_marker(lines[idx + 1]) {
                out.push('\n');
            }
        }
    }
    out.trim().to_string()
}

/// 行頭が条文の構造マーカー（条・号・見出し・別表・附則）か。列折り返し結合で改行を残す判定に使う。
fn starts_structural_marker(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some('第') | Some('（') | Some('別') | Some('附') => true,
        // 号: 漢数字・カタカナ列記・全角/半角数字で始まる。
        Some(c) if "一二三四五六七八九十".contains(c) => true,
        Some(c) if "イロハニホヘトチリヌルヲワカヨタレソツネ".contains(c) => true,
        Some(c) if c.is_ascii_digit() || ('０'..='９').contains(&c) => true,
        _ => false,
    }
}

impl Document {
    /// `reconstruct_vertical` 由来のマーカー付きテキストを構造化する。
    ///
    /// 「改正後」「改正前」の独立見出し行を境に新旧対照表ブロックを作り、
    /// それ以外の連続行は段落ブロックにする。形式は [`detect_format`] で判定する。
    pub fn from_text(text: &str) -> Document {
        let format = detect_format(text);
        let lines: Vec<&str> = text.lines().collect();
        let mut blocks: Vec<Block> = Vec::new();
        let mut para: Vec<&str> = Vec::new();
        let flush_para = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
            let runs = cell_runs(para);
            if !runs.is_empty() {
                blocks.push(Block::Paragraph { runs });
            }
            para.clear();
        };
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "改正後" {
                flush_para(&mut para, &mut blocks);
                // 改正後セル: 次の「改正前」まで。
                let after_start = i + 1;
                let mut j = after_start;
                while j < lines.len() && lines[j].trim() != "改正前" {
                    j += 1;
                }
                let after_lines = &lines[after_start..j];
                // 改正前セル: 次の「改正後」まで（次段の開始）。
                let before_start = (j + 1).min(lines.len());
                let mut k = before_start;
                while k < lines.len() && lines[k].trim() != "改正後" {
                    k += 1;
                }
                let before_lines = &lines[before_start..k];
                // 附則(施行期日・経過措置)は改正全体の付則で、新旧対照表の条ではない。
                // 表の行から外し、表ブロックの後ろに段落として置く。
                let mut rows = align_rows(after_lines, before_lines);
                let mut furisoku: Vec<Vec<Run>> = Vec::new();
                rows.retain(|r| {
                    let is_furi = |runs: &[Run]| {
                        runs.first().map(|x| x.text.trim_start().starts_with("附則")).unwrap_or(false)
                    };
                    if is_furi(&r.after) || is_furi(&r.before) {
                        let content = if !r.after.is_empty() { r.after.clone() } else { r.before.clone() };
                        if !content.is_empty() {
                            furisoku.push(content);
                        }
                        false
                    } else {
                        true
                    }
                });
                if !rows.is_empty() {
                    blocks.push(Block::Shinkyu { rows });
                }
                for runs in furisoku {
                    blocks.push(Block::Paragraph { runs });
                }
                i = k;
            } else {
                para.push(lines[i]);
                i += 1;
            }
        }
        flush_para(&mut para, &mut blocks);
        Document { format, blocks }
    }

    /// 構造化表現を `reconstruct_vertical` 互換のフラットテキストへ戻す。
    /// 記事分割・標題突合など既存のテキスト前提処理との後方互換に使う。
    pub fn to_text(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        let runs_text = |runs: &[Run]| -> String { runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("") };
        for block in &self.blocks {
            match block {
                Block::Paragraph { runs } => {
                    let t = runs_text(runs);
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
                Block::Shinkyu { rows } => {
                    let after: String = rows
                        .iter()
                        .map(|r| runs_text(&r.after))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let before: String = rows
                        .iter()
                        .map(|r| runs_text(&r.before))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push("改正後".to_string());
                    out.push(after);
                    out.push("改正前".to_string());
                    out.push(before);
                }
            }
        }
        out.join("\n").trim().to_string()
    }
}

/// 新旧対照表 1 段を条（別表）ごとの行に分け、改正後/改正前を対応させる。
///
/// 各帯を行見出し（`（…）`の条見出し・`別表`・`附則`）で区切り、見出しテキストの
/// 最長共通部分列(LCS)で対応付ける。共通見出しが揃った条は同じ行に、片側だけの条
/// （新設・削除）はもう一方が空セルの行になる。見出しが揃わない/少ない場合は単一行に
/// フォールバックする（＝従来挙動、退行なし）。
fn align_rows(after_lines: &[&str], before_lines: &[&str]) -> Vec<ShinkyuRow> {
    let a = segment_band(after_lines);
    let b = segment_band(before_lines);
    let a_head = a.iter().filter(|s| s.heading.is_some()).count();
    let b_head = b.iter().filter(|s| s.heading.is_some()).count();
    // 見出しが両側に 2 つ以上無ければ行分割しない。
    if a_head < 2 || b_head < 2 {
        return vec![single_row(after_lines, before_lines)];
    }
    let keys_a: Vec<&str> = a.iter().map(|s| s.heading.as_deref().unwrap_or("")).collect();
    let keys_b: Vec<&str> = b.iter().map(|s| s.heading.as_deref().unwrap_or("")).collect();
    let aln = lcs_align(&keys_a, &keys_b);
    // 共通見出しが 1 つも無ければ対応付け不能としてフォールバック。
    if !aln.iter().any(|(i, j)| i.is_some() && j.is_some()) {
        return vec![single_row(after_lines, before_lines)];
    }
    let mut rows = Vec::new();
    for (ai, bi) in aln {
        let after = ai.map(|i| cell_runs_from_str(&a[i].text)).unwrap_or_default();
        let before = bi.map(|j| cell_runs_from_str(&b[j].text)).unwrap_or_default();
        if !after.is_empty() || !before.is_empty() {
            rows.push(ShinkyuRow { after, before, ..Default::default() });
        }
    }
    rows
}

/// 文字列（複数行）を列折り返し結合してセルの Run 列にする。
fn cell_runs_from_str(text: &str) -> Vec<Run> {
    let lines: Vec<&str> = text.lines().collect();
    cell_runs(&lines)
}

fn single_row(after_lines: &[&str], before_lines: &[&str]) -> ShinkyuRow {
    ShinkyuRow {
        after: cell_runs(after_lines),
        before: cell_runs(before_lines),
        ..Default::default()
    }
}

/// 帯の 1 セグメント（行見出し＋本文、または先頭の見出し無し前置き）。
struct Seg {
    heading: Option<String>,
    text: String,
}

/// 帯テキストを行見出しで区切る。各セグメントは見出し行から次の見出し行の手前まで。
/// 先頭の見出し無し部分は `heading=None` の前置きセグメントになる。
fn segment_band(lines: &[&str]) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut heading: Option<String> = None;
    let mut cur: Vec<&str> = Vec::new();
    for &line in lines {
        if is_row_heading(line) {
            if !cur.is_empty() {
                let text = cur.join("\n").trim().to_string();
                if !text.is_empty() {
                    segs.push(Seg { heading: heading.take(), text });
                }
                cur.clear();
            }
            heading = Some(line.trim().to_string());
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        let text = cur.join("\n").trim().to_string();
        if !text.is_empty() {
            segs.push(Seg { heading, text });
        }
    }
    segs
}

/// 行見出し（新旧対照表の 1 行の先頭）か。
/// - `別表` / `附則` 始まり
/// - 単一の `（…）` 見出し行（中身 3 文字以上、途中に `）` を含まない）。`（略）`/`（新設）`は除外。
fn is_row_heading(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("別表") || t.starts_with("附則") {
        return true;
    }
    if let Some(rest) = t.strip_prefix('（') {
        if let Some(inner) = rest.strip_suffix('）') {
            // 凡例「（傍線部分は…）」は条見出しではない（注記であり、片側だけの偽行を生む）。
            if inner.starts_with("傍線") {
                return false;
            }
            return inner.chars().count() >= 3 && !inner.contains('）');
        }
    }
    false
}

/// 2 列の系列を最長共通部分列で対応付ける。返り値は `(a の index, b の index)` の列で、
/// 片側のみのギャップは `None`。
fn lcs_align(a: &[&str], b: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut res = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            res.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            res.push((Some(i), None));
            i += 1;
        } else {
            res.push((None, Some(j)));
            j += 1;
        }
    }
    while i < n {
        res.push((Some(i), None));
        i += 1;
    }
    while j < m {
        res.push((None, Some(j)));
        j += 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structures_prose_as_single_paragraph() {
        let doc = Document::from_text("第一条中「甲」を「乙」に改める。");
        assert_eq!(doc.format, "prose");
        assert_eq!(
            doc.blocks,
            vec![Block::Paragraph {
                runs: vec![Run::plain("第一条中「甲」を「乙」に改める。")]
            }]
        );
    }

    #[test]
    fn structures_shinkyu_into_after_before() {
        // 見出しが無い小さなセルは単一行になる。
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.format, "shinkyu");
        assert_eq!(
            doc.blocks,
            vec![
                Block::Paragraph {
                    runs: vec![Run::plain("前文")]
                },
                Block::Shinkyu {
                    rows: vec![ShinkyuRow {
                        after: vec![Run::plain("甲種")],
                        before: vec![Run::plain("乙類")],
                        ..Default::default()
                    }],
                },
            ]
        );
    }

    #[test]
    fn aligns_rows_by_heading_lcs() {
        // 共通見出し（特例A/特例B）は同じ行に、改正後のみの新設（特例C）は片側行に。
        let after = "（特例A）\n第一条新\n（特例B）\n第二条新\n（特例C）\n第三条新";
        let before = "（特例A）\n第一条旧\n（特例B）\n第二条旧";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let rows = match &doc.blocks[0] {
            Block::Shinkyu { rows } => rows,
            _ => panic!("shinkyu block"),
        };
        assert_eq!(rows.len(), 3);
        // 行1: 特例A 改正後/改正前 両方。
        assert!(rows[0].after[0].text.contains("第一条新") && rows[0].before[0].text.contains("第一条旧"));
        // 行2: 特例B 両方。
        assert!(rows[1].after[0].text.contains("第二条新") && rows[1].before[0].text.contains("第二条旧"));
        // 行3: 特例C は改正後のみ（新設）。改正前セルは空。
        assert!(rows[2].after[0].text.contains("第三条新"));
        assert!(rows[2].before.is_empty());
    }

    #[test]
    fn handles_multiple_shinkyu_segments() {
        // 多ページ表: 改正後/改正前 が複数段。
        let text = "改正後\nA1\n改正前\nB1\n改正後\nA2\n改正前\nB2";
        let doc = Document::from_text(text);
        let shinkyu: Vec<_> = doc
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Shinkyu { .. }))
            .collect();
        assert_eq!(shinkyu.len(), 2);
        assert_eq!(
            doc.blocks[1],
            Block::Shinkyu {
                rows: vec![ShinkyuRow {
                    after: vec![Run::plain("A2")],
                    before: vec![Run::plain("B2")],
                    ..Default::default()
                }]
            }
        );
    }

    #[test]
    fn to_text_roundtrips_markers() {
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.to_text(), text);
    }

    #[test]
    fn joins_column_wrapped_lines_keeping_structure() {
        // 列折り返し（語の途中改行）は結合し、号・条・文末の改行は残す。
        let lines = vec![
            "第六十二条かつお・まぐろ漁業者は、次に掲げる行為をしな",
            "ければならない。",
            "一（略）",
            "二当該さめを所持すること。",
        ];
        let runs = cell_runs(&lines);
        let text = &runs[0].text;
        // 「しな」+「ければ」は連結される。
        assert!(text.contains("行為をしなければならない。"), "{text}");
        // 号「一」「二」は改行で分かれる。
        let after_first = text.split('\n').collect::<Vec<_>>();
        assert!(after_first.iter().any(|l| l.trim() == "一（略）"), "{text}");
        assert!(after_first.iter().any(|l| l.starts_with("二当該さめ")), "{text}");
    }

    #[test]
    fn serializes_to_json() {
        let doc = Document::from_text("改正後\n甲\n改正前\n乙");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"format\":\"shinkyu\""));
        assert!(json.contains("\"kind\":\"shinkyu\""));
        // underline は false のとき省略される。
        assert!(!json.contains("underline"));
    }
}
