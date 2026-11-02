//! 解析結果（[`kanpo_parser::Analysis`]）を DOCX に書く。
//!
//! 記事ごとに、最初の行を太字の見出しにし、本文の段落（行ごとの段落）・本文の中の表・新旧対照表（2 列の表）を
//! 順に書く。記事の間は空の段落で区切る。表の結合セルは、左上のマスに `gridSpan` と縦の結合（`vMerge`）を付け、
//! 結合で覆われるマスは書かない（横）か、縦の結合の続きにする（縦）。

use anyhow::{anyhow, Result};
use docx_rs::{Docx, Paragraph, Run as DRun, RunFonts, Table, TableCell, TableRow, VMergeType};
use kanpo_parser::model::{Block, CellMerge, NestedTable, Run};

/// 本文の字の書体（明朝）と大きさ（半ポイント、10.5pt）。
const FONT: &str = "ＭＳ 明朝";
const SIZE: usize = 21;

/// 解析結果を DOCX のバイト列にする。
pub fn write(analysis: &kanpo_parser::Analysis) -> Result<Vec<u8>> {
    let mut docx = Docx::new().default_fonts(RunFonts::new().ascii(FONT).east_asia(FONT).hi_ansi(FONT)).default_size(SIZE);
    for (k, article) in analysis.articles.iter().enumerate() {
        if k > 0 {
            docx = docx.add_paragraph(Paragraph::new());
        }
        let mut first = true;
        for block in &article.document.blocks {
            match block {
                Block::Paragraph { runs } => {
                    for item in paragraph_items(runs) {
                        match item {
                            Item::Line(line) => {
                                let mut p = Paragraph::new();
                                for (text, underline) in line {
                                    let mut r = DRun::new().add_text(text);
                                    if first {
                                        r = r.bold();
                                    }
                                    if underline {
                                        r = r.underline("single");
                                    }
                                    p = p.add_run(r);
                                }
                                docx = docx.add_paragraph(p);
                                first = false;
                            }
                            Item::Table(t) => {
                                docx = docx.add_table(table(t));
                                first = false;
                            }
                        }
                    }
                }
                Block::Shinkyu { rows, labels } => {
                    let (after, before) = labels.as_ref().map_or(("改正後", "改正前"), |l| (l.after, l.before));
                    let mut trs = vec![TableRow::new(vec![cell_text(after, true), cell_text(before, true)])];
                    for row in rows {
                        let side = |runs: &[Run], t: &Option<NestedTable>| {
                            let mut c = cell_runs(runs);
                            if let Some(t) = t {
                                c = c.add_table(table(t));
                            }
                            c
                        };
                        trs.push(TableRow::new(vec![side(&row.after, &row.after_table), side(&row.before, &row.before_table)]));
                    }
                    docx = docx.add_table(Table::new(trs));
                    first = false;
                }
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    docx.build().pack(&mut out).map_err(|e| anyhow!("docx: {e}"))?;
    Ok(out.into_inner())
}

enum Item<'a> {
    /// 1 行の字（`(字, 傍線)` の並び）。
    Line(Vec<(String, bool)>),
    Table(&'a NestedTable),
}

/// 段落の字を改行で行に分け、本文の中の表を間に挟む。
fn paragraph_items(runs: &[Run]) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    let mut line: Vec<(String, bool)> = Vec::new();
    let flush = |line: &mut Vec<(String, bool)>, items: &mut Vec<Item>| {
        if line.iter().any(|(t, _)| !t.trim().is_empty()) {
            items.push(Item::Line(std::mem::take(line)));
        }
        line.clear();
    };
    for r in runs {
        if let Some(t) = &r.table {
            flush(&mut line, &mut items);
            items.push(Item::Table(t));
            continue;
        }
        let mut parts = r.text.split('\n');
        if let Some(head) = parts.next() {
            if !head.is_empty() {
                line.push((head.to_string(), r.underline));
            }
        }
        for part in parts {
            flush(&mut line, &mut items);
            if !part.is_empty() {
                line.push((part.to_string(), r.underline));
            }
        }
    }
    flush(&mut line, &mut items);
    items
}

fn cell_text(text: &str, bold: bool) -> TableCell {
    let mut r = DRun::new().add_text(text);
    if bold {
        r = r.bold();
    }
    TableCell::new().add_paragraph(Paragraph::new().add_run(r))
}

/// セルの字（行ごとの段落）。字が無くても段落を 1 つ持つ（Word はセルに段落を要する）。
fn cell_runs(runs: &[Run]) -> TableCell {
    let mut cell = TableCell::new();
    let mut any = false;
    for item in paragraph_items(runs) {
        match item {
            Item::Line(line) => {
                let mut p = Paragraph::new();
                for (text, underline) in line {
                    let mut r = DRun::new().add_text(text);
                    if underline {
                        r = r.underline("single");
                    }
                    p = p.add_run(r);
                }
                cell = cell.add_paragraph(p);
                any = true;
            }
            Item::Table(t) => {
                cell = cell.add_table(table(t)).add_paragraph(Paragraph::new());
                any = true;
            }
        }
    }
    if !any {
        cell = cell.add_paragraph(Paragraph::new());
    }
    cell
}

/// 罫線の表（結合セル付き）を DOCX の表にする。
fn table(t: &NestedTable) -> Table {
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    // 各マスが、どの結合の左上・横の続き・縦の続きか。
    let owner = |r: usize, c: usize| -> Option<&CellMerge> { t.merges.iter().find(|m| r >= m.row && r < m.row + m.rowspan && c >= m.col && c < m.col + m.colspan) };
    let mut rows = Vec::with_capacity(t.rows.len());
    for (r, row) in t.rows.iter().enumerate() {
        let mut cells = Vec::new();
        let mut c = 0;
        while c < cols {
            let runs: &[Run] = row.get(c).map(Vec::as_slice).unwrap_or(&[]);
            match owner(r, c) {
                Some(m) if m.col == c => {
                    let mut cell = if m.row == r { cell_runs(runs) } else { TableCell::new().add_paragraph(Paragraph::new()) };
                    if m.colspan > 1 {
                        cell = cell.grid_span(m.colspan);
                    }
                    if m.rowspan > 1 {
                        cell = cell.vertical_merge(if m.row == r { VMergeType::Restart } else { VMergeType::Continue });
                    }
                    cells.push(cell);
                    c += m.colspan.max(1);
                }
                // 横の結合の続きのマス（左上のマスの gridSpan が覆う）。
                Some(_) => c += 1,
                None => {
                    cells.push(cell_runs(runs));
                    c += 1;
                }
            }
        }
        rows.push(TableRow::new(cells));
    }
    Table::new(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanpo_parser::model::Run;

    fn xml(bytes: &[u8]) -> String {
        let mut zip = zip_reader(bytes);
        zip.remove("word/document.xml").expect("document.xml")
    }

    /// DOCX（zip）の部品を名前と中身にする（テスト用の小さな読み手）。
    fn zip_reader(bytes: &[u8]) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        let mut i = 0;
        while i + 30 <= bytes.len() && bytes[i..i + 4] == [0x50, 0x4b, 0x03, 0x04] {
            let u16_at = |p: usize| u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
            let u32_at = |p: usize| u32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]) as usize;
            let (method, size, name_len, extra_len) = (u16_at(i + 8), u32_at(i + 18), u16_at(i + 26), u16_at(i + 28));
            let name = String::from_utf8_lossy(&bytes[i + 30..i + 30 + name_len]).into_owned();
            let start = i + 30 + name_len + extra_len;
            let data = &bytes[start..start + size];
            let content = if method == 8 { miniz_oxide::inflate::decompress_to_vec(data).unwrap() } else { data.to_vec() };
            out.insert(name, String::from_utf8_lossy(&content).into_owned());
            i = start + size;
        }
        out
    }

    #[test]
    fn articles_become_paragraphs_with_a_bold_first_line() {
        let text = "〇法務省告示第百七号\n左記の者の申請に係る日本国に帰化の件は、これを許可する。";
        let analysis = kanpo_parser::Analysis {
            pages: vec![],
            text: text.into(),
            format: "prose".into(),
            articles: vec![kanpo_parser::ArticleAnalysis { text: text.into(), document: kanpo_parser::model::Document::from_text(text), notices: vec![] }],
        };
        let doc = xml(&write(&analysis).unwrap());
        assert!(doc.contains("〇法務省告示第百七号") && doc.contains("これを許可する。"), "{doc}");
        let head = &doc[doc.find("〇法務省告示第百七号").unwrap().saturating_sub(300)..];
        assert!(head.contains("<w:b />") || head.contains("<w:b/>"), "first line is bold");
    }

    #[test]
    fn merged_cells_span_columns_and_rows() {
        let r = |s: &str| vec![Run::plain(s)];
        let t = NestedTable {
            rows: vec![vec![r("名称"), vec![], r("所在地")], vec![r("甲"), r("乙"), vec![]]],
            merges: vec![CellMerge { row: 0, col: 0, rowspan: 1, colspan: 2 }, CellMerge { row: 0, col: 2, rowspan: 2, colspan: 1 }],
        };
        let mut docx = Docx::new().add_table(table(&t));
        let mut out = std::io::Cursor::new(Vec::new());
        docx = docx.default_size(SIZE);
        docx.build().pack(&mut out).unwrap();
        let doc = xml(&out.into_inner());
        assert!(doc.contains("w:gridSpan w:val=\"2\""), "{doc}");
        assert!(doc.contains("w:vMerge w:val=\"restart\"") && doc.contains("w:vMerge w:val=\"continue\""), "{doc}");
    }
}
