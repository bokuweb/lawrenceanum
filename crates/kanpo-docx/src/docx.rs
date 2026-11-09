//! 解析結果（[`kanpo_parser::Analysis`]）を DOCX に書く。
//!
//! 記事ごとに、最初の行を太字の見出しにし、本文の段落（行ごとの段落）・本文の中の表・新旧対照表（2 列の表）を
//! 順に書く。記事の間は空の段落で区切る。表の結合セルは、左上のマスに `gridSpan` と縦の結合（`vMerge`）を付け、
//! 結合で覆われるマスは書かない（横）か、縦の結合の続きにする（縦）。

use crate::decorations::{SourceStyles, Styles};
use crate::layout::{header_rows, Layout};
use crate::pictures::Pictures;
use anyhow::{anyhow, ensure, Result};
use docx_rs::{
    AlignmentType, Docx, HeightRule, PageOrientationType, Paragraph, Run as DRun, RunFonts, Table,
    TableCell, TableCellContent, TableCellMargins, TableLayoutType, TableRow, TextDirectionType, VAlignType, VMergeType, WidthType,
};
use kanpo_parser::media::{Block, Document, Field, Inline, Table as MediaTable};
use kanpo_parser::model::{CellMerge, NestedTable, Run};

/// 本文の書体。字の大きさは原本の有効サイズを半ポイントで保持する。
const FONT: &str = "ＭＳ 明朝";
#[cfg(test)]
const SIZE: usize = 21;

/// 解析結果を DOCX のバイト列にする。
pub fn write(analysis: &kanpo_parser::Analysis) -> Result<Vec<u8>> {
    let documents = analysis
        .articles
        .iter()
        .map(|a| Document::from(&a.document))
        .collect::<Vec<_>>();
    write_documents(
        analysis,
        &documents,
        &Pictures::default(),
        &SourceStyles::default(),
    )
}

pub(crate) fn write_documents(
    analysis: &kanpo_parser::Analysis,
    documents: &[Document],
    pictures: &Pictures,
    source_styles: &SourceStyles,
) -> Result<Vec<u8>> {
    ensure!(
        analysis.articles.len() == documents.len(),
        "native and rich article inventories differ"
    );
    let fields = [Field::Paragraph, Field::After, Field::Before];
    let original = fields.map(|field| kanpo_parser::source_underlines::field_text(analysis, field));
    for (field, expected) in fields.into_iter().zip(&original) {
        ensure!(
            documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>()
                == *expected,
            "rich document changed native {:?} text",
            field
        );
    }
    let layout = Layout::new(analysis);
    let mut docx = Docx::new()
        .default_fonts(RunFonts::new().ascii(FONT).east_asia(FONT).hi_ansi(FONT))
        .default_size(source_styles.default_font());
    // Preserve the predominant source orientation. The default A4 margins are 1701
    // twips on each side; nested widths use the containing cell, not the page.
    let landscape = analysis.pages.iter().filter(|p| p.width > p.height).count() * 2
        > analysis.pages.len()
        || comparative_tables_need_landscape(analysis, &layout);
    let page_width = if landscape { 16838 } else { 11906 };
    if landscape {
        docx = docx
            .page_size(16838, 11906)
            .page_orient(PageOrientationType::Landscape);
    }
    let width = page_width - 2 * 1701;
    let mut documents = documents.to_vec();
    crate::continuations::apply(&mut documents, &layout);
    crate::paragraph_gaps::apply(analysis, &mut documents);
    crate::comparison_geometry::apply(analysis, &mut documents);
    crate::paragraph_gaps::apply_headings(analysis, &mut documents);
    crate::paragraph_gaps::apply_citation_joins(analysis, &mut documents);
    let signature_leading = crate::formalities::apply(analysis, &mut documents);
    for (field, expected) in fields.into_iter().zip(&original) {
        let bag = |s: &str| {
            let mut b = std::collections::BTreeMap::new();
            for c in s.chars().filter(|c| !c.is_whitespace()) {
                *b.entry(c).or_insert(0usize) += 1;
            }
            b
        };
        let actual = documents
            .iter()
            .map(|d| d.field_text(field))
            .collect::<String>();
        ensure!(
            bag(&actual) == bag(expected),
            "source layout projection changed {:?} characters",
            field
        );
    }
    let styles = source_styles.bind(&documents);
    for (k, (article, document)) in analysis.articles.iter().zip(&documents).enumerate() {
        if k > 0 {
            docx = docx.add_paragraph(Paragraph::new());
        }
        let mut first = true;
        let mut paragraph_cursor = 0usize;
        let source_heading = article
            .text
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(str::trim)
            .filter(|line| {
                kanpo_parser::segment::is_heading_mark(line)
                    && line.ends_with('号')
                    && line.contains('第')
            });
        for block in &document.blocks {
            match block {
                Block::Paragraph { runs } => {
                    for item in paragraph_items(runs) {
                        match item {
                            Item::Line(line) => {
                                for line in split_source_heading(
                                    line,
                                    if first { source_heading } else { None },
                                )
                                .into_iter()
                                {
                                    let line = source_label_spaces(line, &layout, true);
                                    let mut p = Paragraph::new();
                                    if let Some(&twips) = signature_leading[k].get(&paragraph_cursor) {
                                        p = p.line_spacing(docx_rs::LineSpacing::new().line_rule(docx_rs::LineSpacingType::Exact).line(twips as i32));
                                    }
                                    paragraph_cursor += line.iter().map(|(s,_)| s.chars().filter(|c| !c.is_whitespace()).count()).sum::<usize>();
                                    if let Some(size)=styles.paragraph_font(Field::Paragraph,&line.iter().map(|r|r.0.as_str()).collect::<String>()){p=p.size(size);}
                                    for (text, underline) in line {
                                        for r in styles.word_runs(
                                            Field::Paragraph,
                                            &text,
                                            underline,
                                            first,
                                        ) {
                                            p = p.add_run(r);
                                        }
                                    }
                                    docx = docx.add_paragraph(p);
                                    first = false;
                                }
                            }
                            Item::Table(t) => {
                                paragraph_cursor += table_scalar_count(t);
                                if contains_images(t)
                                    && layout
                                        .physical_image_table(&plain_table(t), width, 596.)
                                        .is_some()
                                {
                                    docx = docx
                                        .add_paragraph(Paragraph::new().page_break_before(true));
                                }
                                docx = docx.add_table(table(
                                    t,
                                    width,
                                    &layout,
                                    pictures,
                                    &styles,
                                    Field::Paragraph,
                                ));
                                first = false;
                            }
                            Item::Image(asset) => {
                                if let Some(p) = pictures.paragraph(
                                    asset,
                                    width,
                                    if landscape { 411. } else { 632. },
                                ) {
                                    docx = docx.add_paragraph(p)
                                }
                            }
                        }
                    }
                }
                Block::Shinkyu { rows, labels } => {
                    let (after, before) = labels
                        .as_ref()
                        .map_or(("改正後", "改正前"), |l| (l.after, l.before));
                    let field_width = width / 2;
                    let labels = || {
                        TableRow::new(vec![
                            cell_text(after, true).width(field_width, WidthType::Dxa),
                            cell_text(before, true).width(width - field_width, WidthType::Dxa),
                        ])
                    };
                    let mut trs = vec![labels()];
                    for row in rows {
                        let side = |runs: &[Inline], t: Option<&MediaTable>, field: Field| {
                            let mut c = cell_runs(
                                runs,
                                field_width.saturating_sub(110),
                                false,
                                true,
                                &layout,
                                pictures,
                                &styles,
                                field,
                            )
                            .width(field_width, WidthType::Dxa);
                            if let Some(t) = t {
                                c = c
                                    .add_table(table(
                                        t,
                                        field_width.saturating_sub(110),
                                        &layout,
                                        pictures,
                                        &styles,
                                        field,
                                    ))
                                    .add_paragraph(Paragraph::new());
                            }
                            c
                        };
                        let after = comparison_parts(&row.after, row.after_table.as_ref());
                        let before = comparison_parts(&row.before, row.before_table.as_ref());
                        let compound_headers = after.iter().zip(&before).any(|((_, a), (_, b))| {
                            a.is_some_and(|t| header_rows(&plain_table(t)) > 0)
                                && b.is_some_and(|t| header_rows(&plain_table(t)) > 0)
                        });
                        if after.len() == before.len() && after.len() > 1 && compound_headers {
                            // Separate prose from paired embedded tables. An
                            // application can then move the table's header as a
                            // unit, rather than split it while breaking a long
                            // outer cell that started with the preceding prose.
                            for ((a, at), (b, bt)) in after.into_iter().zip(before) {
                                if a.is_empty() && b.is_empty() && at.is_none() && bt.is_none() {
                                    continue;
                                }
                                if trs.len() > 1
                                    && at.is_some_and(|t| header_rows(&plain_table(t)) > 0)
                                    && bt.is_some_and(|t| header_rows(&plain_table(t)) > 0)
                                {
                                    // Nested multi-row headers can be split by
                                    // Word-compatible renderers while breaking
                                    // the outer row. Start the paired table on
                                    // a fresh page, keeping each inner table
                                    // intact and editable (no raster fallback).
                                    docx = docx.add_table(comparison_table(
                                        std::mem::take(&mut trs),
                                        width,
                                    ));
                                    docx = docx
                                        .add_paragraph(Paragraph::new().page_break_before(true));
                                    trs.push(labels());
                                }
                                trs.push(TableRow::new(vec![
                                    side(a, at, Field::After),
                                    side(b, bt, Field::Before),
                                ]));
                            }
                        } else {
                            trs.push(TableRow::new(vec![
                                side(&row.after, row.after_table.as_ref(), Field::After),
                                side(&row.before, row.before_table.as_ref(), Field::Before),
                            ]));
                        }
                    }
                    docx = docx.add_table(comparison_table(trs, width));
                    first = false;
                }
            }
        }
    }
    for image in pictures.remaining() {
        docx = docx.add_paragraph(
            Paragraph::new()
                .keep_next(true)
                .add_run(DRun::new().add_text(format!("原本{}ページの画像", image.page + 1))),
        );
        if let Some(p) = pictures.paragraph(image.asset, width, if landscape { 411. } else { 632. })
        {
            docx = docx.add_paragraph(p)
        }
    }
    styles.finish()?;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut package = docx.build();
    // docx-rs 0.4 has no tblHeader API. Only source-certified header rows
    // receive cantSplit in this writer; mark those rows as a repeated header
    // before packing, so applications also keep the multi-row band together.
    package.document = styles
        .finish_xml(String::from_utf8(package.document)?)?
        .replace("<w:cantSplit />", "<w:cantSplit /><w:tblHeader />")
        .into_bytes();
    package.pack(&mut out).map_err(|e| anyhow!("docx: {e}"))?;
    Ok(out.into_inner())
}

fn comparative_tables_need_landscape(
    analysis: &kanpo_parser::Analysis,
    layout: &Layout<'_>,
) -> bool {
    // Keep the source width of a refined comparison grid whenever the portrait
    // field would squeeze its real columns into one-character strips.
    let portrait_field = (11906. - 2. * 1701.) / 40. - 110. / 20.;
    analysis
        .articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .any(|b| {
            let kanpo_parser::model::Block::Shinkyu { rows, .. } = b else {
                return false;
            };
            rows.iter()
                .flat_map(|r| {
                    r.after
                        .iter()
                        .chain(&r.before)
                        .filter_map(|r| r.table.as_ref())
                        .chain(r.after_table.iter())
                        .chain(r.before_table.iter())
                })
                .any(|t| {
                    t.rows.iter().map(Vec::len).max().unwrap_or(0) >= 4
                        && layout.source_columns(t).is_some_and(|(_, _, _, edges)| {
                            edges.last().is_some_and(|&w| w > portrait_field)
                        })
                })
        })
}

// Restore only an independently recorded source heading at the article's
// beginning. Do not search the body for a law-name pattern or invent a title.
fn split_source_heading(
    line: Vec<(String, bool)>,
    heading: Option<&str>,
) -> Vec<Vec<(String, bool)>> {
    let Some(heading) = heading else {
        return vec![line];
    };
    let text = line.iter().map(|(s, _)| s.as_str()).collect::<String>();
    if text.len() <= heading.len() || !text.starts_with(heading) {
        return vec![line];
    }
    let (mut before, mut after, mut remaining) = (Vec::new(), Vec::new(), heading.len());
    for (text, style) in line {
        let n = remaining.min(text.len());
        if n > 0 {
            before.push((text[..n].into(), style));
            remaining -= n;
        }
        if n < text.len() {
            after.push((text[n..].into(), style));
        }
    }
    vec![before, after]
}

fn source_label_spaces(
    line: Vec<(String, bool)>,
    layout: &Layout<'_>,
    prose: bool,
) -> Vec<(String, bool)> {
    let text = line.iter().map(|(s, _)| s.as_str()).collect::<String>();
    let gaps = layout.label_gaps(&text, prose);
    if gaps.is_empty() {
        return line;
    }
    let mut gaps = gaps.into_iter().peekable();
    let mut at = 0;
    let mut whitespace = true;
    let mut out: Vec<(String, bool)> = Vec::new();
    for (text, style) in line {
        for c in text.chars() {
            if !c.is_whitespace() {
                if let Some(&(offset, count)) = gaps.peek() {
                    if offset == at {
                        if !whitespace {
                            out.push(("　".repeat(count), false));
                        }
                        gaps.next();
                    }
                }
                at += 1;
            }
            if let Some((text, _)) = out.last_mut().filter(|(_, old)| *old == style) {
                text.push(c);
            } else {
                out.push((c.to_string(), style));
            }
            whitespace = c.is_whitespace();
        }
    }
    out
}

fn comparison_table(rows: Vec<TableRow>, width: usize) -> Table {
    Table::new(rows)
        .set_grid(vec![width / 2, width - width / 2])
        .width(width, WidthType::Dxa)
        .layout(TableLayoutType::Fixed)
}

fn comparison_parts<'a>(
    runs: &'a [Inline],
    extra: Option<&'a MediaTable>,
) -> Vec<(&'a [Inline], Option<&'a MediaTable>)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, r) in runs.iter().enumerate() {
        if let Inline::Table { table: t } = r {
            out.push((&runs[start..i], None));
            out.push((&[], Some(t)));
            start = i + 1;
        }
    }
    out.push((&runs[start..], None));
    if let Some(t) = extra {
        out.push((&[], Some(t)));
        out.push((&[], None));
    }
    out
}

enum Item<'a> {
    /// 1 行の字（`(字, 傍線)` の並び）。
    Line(Vec<(String, bool)>),
    Table(&'a MediaTable),
    Image(usize),
}

fn table_scalar_count(t: &MediaTable) -> usize {
    t.rows.iter().flatten().flatten().map(|r| match r {
        Inline::Text { text, .. } => text.chars().filter(|c|!c.is_whitespace()).count(),
        Inline::Table { table } => table_scalar_count(table),
        Inline::Image { .. } => 0,
    }).sum()
}

/// 段落の字を改行で行に分け、本文の中の表を間に挟む。
fn paragraph_items(runs: &[Inline]) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    let mut line: Vec<(String, bool)> = Vec::new();
    let flush = |line: &mut Vec<(String, bool)>, items: &mut Vec<Item>| {
        if line.iter().any(|(t, _)| !t.trim().is_empty()) {
            items.push(Item::Line(std::mem::take(line)));
        }
        line.clear();
    };
    for r in runs {
        if let Inline::Table { table: t } = r {
            flush(&mut line, &mut items);
            items.push(Item::Table(t));
            continue;
        }
        if let Inline::Image { asset } = r {
            flush(&mut line, &mut items);
            items.push(Item::Image(*asset));
            continue;
        }
        let Inline::Text { text, underline } = r else {
            unreachable!()
        };
        let mut parts = text.split('\n');
        if let Some(head) = parts.next() {
            if !head.is_empty() {
                line.push((head.to_string(), *underline));
            }
        }
        for part in parts {
            flush(&mut line, &mut items);
            if !part.is_empty() {
                line.push((part.to_string(), *underline));
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
    TableCell::new().add_paragraph(Paragraph::new().keep_next(true).add_run(r))
}

/// Keep the end of an opening table row with the next row, without declaring
/// its text a semantic/repeating header or making a long row unbreakable.
/// Only the last direct paragraph is linked: preceding paragraphs can still
/// flow normally, including cells that contain nested tables or images.
fn keep_opening_cell_with_next(mut cell: TableCell) -> TableCell {
    if let Some(p) = cell.children.iter_mut().rev().find_map(|child| {
        if let TableCellContent::Paragraph(p) = child { Some(p) } else { None }
    }) {
        p.property.keep_next = Some(true);
    }
    cell
}

fn align_source_cell(mut cell: TableCell, alignment: Option<crate::layout::Alignment>) -> TableCell {
    if let Some(alignment)=alignment {
        for child in &mut cell.children {
            if let TableCellContent::Paragraph(p)=child {
                p.property.alignment=Some(docx_rs::Justification::new(alignment.value()));
            }
        }
    }
    cell
}

/// セルの字（行ごとの段落）。字が無くても段落を 1 つ持つ（Word はセルに段落を要する）。
fn cell_runs(
    runs: &[Inline],
    width: usize,
    header: bool,
    prose: bool,
    layout: &Layout<'_>,
    pictures: &Pictures,
    styles: &Styles,
    field: Field,
) -> TableCell {
    cell_runs_in_canvas(runs,width,header,prose,layout,pictures,styles,field,1.)
}
fn cell_runs_in_canvas(runs:&[Inline],width:usize,header:bool,prose:bool,layout:&Layout<'_>,pictures:&Pictures,styles:&Styles,field:Field,scale:f32)->TableCell{
    let mut cell = TableCell::new();
    let mut any = false;
    for item in paragraph_items(runs) {
        match item {
            Item::Line(line) => {
                let line = source_label_spaces(line, layout, prose);
                let mut p = Paragraph::new();
                let size=styles.paragraph_font(field,&line.iter().map(|r|r.0.as_str()).collect::<String>()).unwrap_or_else(||styles.default_font());p=p.size((size as f32*scale).round().max(1.) as usize);
                if header {
                    p = p
                        .align(AlignmentType::Center)
                        .keep_lines(true)
                        .keep_next(true);
                }
                for (text, underline) in line {
                    for r in styles.word_runs_scaled(field, &text, underline, false,scale) {
                        p = p.add_run(r);
                    }
                }
                cell = cell.add_paragraph(p);
                any = true;
            }
            Item::Table(t) => {
                cell = cell
                    .add_table(table_in_canvas(t, width, layout, pictures, styles, field,scale))
                    .add_paragraph(Paragraph::new());
                any = true;
            }
            Item::Image(asset) => {
                if let Some(p) = pictures.paragraph(asset, width, 500.) {
                    cell = cell.add_paragraph(p);
                    any = true;
                }
            }
        }
    }
    if !any {
        cell = cell.add_paragraph(Paragraph::new());
    }
    if header {
        cell = cell.vertical_align(VAlignType::Center);
    }
    cell
}

/// 罫線の表（結合セル付き）を DOCX の表にする。
pub(crate) fn plain_table(t: &MediaTable) -> NestedTable {
    NestedTable {
        merges: t.merges.clone(),
        rows: t
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        cell.iter()
                            .filter_map(|r| match r {
                                Inline::Text { text, underline } => Some(Run {
                                    text: text.clone(),
                                    underline: *underline,
                                    table: None,
                                }),
                                Inline::Table { table } => Some(Run {
                                    text: String::new(),
                                    underline: false,
                                    table: Some(plain_table(table)),
                                }),
                                Inline::Image { .. } => None,
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect(),
    }
}

fn contains_images(t: &MediaTable) -> bool {
    t.rows.iter().flatten().flatten().any(|r| match r {
        Inline::Image { .. } => true,
        Inline::Table { table } => contains_images(table),
        _ => false,
    })
}

fn physical_image_table(t: &MediaTable) -> (MediaTable, Vec<((usize, usize), (usize, usize))>) {
    let rows = t.rows.len();
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = MediaTable {
        rows: vec![vec![vec![]; rows]; cols],
        merges: Vec::new(),
    };
    let mut map = Vec::new();
    for (r, row) in t.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let merge = t.merges.iter().find(|m| {
                m.row <= r && r < m.row + m.rowspan && m.col <= c && c < m.col + m.colspan
            });
            if merge.is_some_and(|m| m.row != r || m.col != c) {
                continue;
            }
            let (rs, cs) = merge.map_or((1, 1), |m| (m.rowspan, m.colspan));
            let (nr, nc) = (c, rows - r - rs);
            out.rows[nr][nc] = cell.clone();
            map.push(((r, c), (nr, nc)));
            if rs > 1 || cs > 1 {
                out.merges.push(CellMerge {
                    row: nr,
                    col: nc,
                    rowspan: cs,
                    colspan: rs,
                });
            }
        }
    }
    (out, map)
}

fn table(
    t: &MediaTable,
    width: usize,
    layout: &Layout<'_>,
    pictures: &Pictures,
    styles: &Styles,
    field: Field,
) -> Table {
    table_in_canvas(t,width,layout,pictures,styles,field,1.)
}
fn table_in_canvas(t:&MediaTable,width:usize,layout:&Layout<'_>,pictures:&Pictures,styles:&Styles,field:Field,canvas_scale:f32)->Table{
    let plain = plain_table(t);
    let plan = contains_images(t)
        .then(|| layout.physical_image_table(&plain, width, 596.))
        .flatten();
    let borders = styles.table_borders(&plain, field, plan.is_some());
    let alignments=if !contains_images(t) {layout.alignments(&plain)} else {Default::default()};
    let font_scale=plan.as_ref().map_or(canvas_scale,|p|p.scale);
    let physical = plan.as_ref().map(|_| physical_image_table(t));
    if let Some((physical, map)) = &physical {
        styles.remap_table(field, t, physical, map);
    }
    let t = physical.as_ref().map_or(t, |(physical, _)| physical);
    let plain = plain_table(t);
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    let grid = plan
        .as_ref()
        .map_or_else(|| layout.columns(&plain, width), |p| p.widths.clone());
    let heading_rows = header_rows(&plain);
    // 各マスが、どの結合の左上・横の続き・縦の続きか。
    let owner = |r: usize, c: usize| -> Option<&CellMerge> {
        t.merges
            .iter()
            .find(|m| r >= m.row && r < m.row + m.rowspan && c >= m.col && c < m.col + m.colspan)
    };
    let mut rows = Vec::with_capacity(t.rows.len());
    for (r, row) in t.rows.iter().enumerate() {
        let mut cells = Vec::new();
        let mut c = 0;
        while c < cols {
            let runs: &[Inline] = row.get(c).map(Vec::as_slice).unwrap_or(&[]);
            match owner(r, c) {
                Some(m) if m.col == c => {
                    let cell_width: usize = grid[c..(c + m.colspan).min(cols)].iter().sum();
                    let mut cell = if m.row == r {
                        cell_runs_in_canvas(
                            runs,
                            cell_width.saturating_sub(110),
                            r < heading_rows,
                            false,
                            layout,
                            pictures,
                            styles,
                            field,
                            font_scale,
                        )
                    } else {
                        TableCell::new().add_paragraph(Paragraph::new().keep_next(r < heading_rows))
                    };
                    cell = cell.width(cell_width, WidthType::Dxa);
                    if m.colspan > 1 {
                        cell = cell.grid_span(m.colspan);
                    }
                    if m.rowspan > 1 {
                        cell = cell.vertical_merge(if m.row == r {
                            VMergeType::Restart
                        } else {
                            VMergeType::Continue
                        });
                    }
                    if plan.is_some() {
                        cell = physical_cell(cell, runs);
                    }
                    cell =
                        borders.apply(cell, m.row, m.col, r == m.row, r + 1 == m.row + m.rowspan);
                    if r == m.row {cell = align_source_cell(cell,alignments.get(&(m.row,m.col)).copied());}
                    if r == 0 && t.rows.len() > 1 && plan.is_none() {
                        cell = keep_opening_cell_with_next(cell);
                    }
                    cells.push(cell);
                    c += m.colspan.max(1);
                }
                // 横の結合の続きのマス（左上のマスの gridSpan が覆う）。
                Some(_) => c += 1,
                None => {
                    let mut cell = cell_runs_in_canvas(
                        runs,
                        grid[c].saturating_sub(110),
                        r < heading_rows,
                        false,
                        layout,
                        pictures,
                        styles,
                        field,
                        font_scale,
                    )
                    .width(grid[c], WidthType::Dxa);
                    if plan.is_some() {
                        cell = physical_cell(cell, runs);
                    }
                    cell = borders.apply(cell, r, c, true, true);
                    cell = align_source_cell(cell,alignments.get(&(r,c)).copied());
                    if r == 0 && t.rows.len() > 1 && plan.is_none() {
                        cell = keep_opening_cell_with_next(cell);
                    }
                    cells.push(cell);
                    c += 1;
                }
            }
        }
        let mut row = TableRow::new(cells);
        if let Some(p) = &plan {
            row = row
                .row_height(p.heights[r])
                .height_rule(HeightRule::AtLeast);
        }
        rows.push(if r < heading_rows {
            row.cant_split()
        } else {
            row
        });
    }
    let padding=layout.padding(&plain,width);
    let out=Table::new(rows)
        .set_grid(grid)
        .width(
            plan.as_ref().map_or(width, |p| p.widths.iter().sum()),
            WidthType::Dxa,
        )
        .layout(TableLayoutType::Fixed);
    if let Some(padding)=padding {out.margins(TableCellMargins::new().margin(0,padding,0,padding))}else{out}
}

fn physical_cell(mut cell: TableCell, runs: &[Inline]) -> TableCell {
    if !runs
        .iter()
        .any(|r| matches!(r, Inline::Image { .. } | Inline::Table { .. }))
    {
        cell = cell
            .text_direction(TextDirectionType::TbRl)
            .vertical_align(VAlignType::Center);
        // Font/ruby scaling already occurred before annotation XML is bound.
    }
    cell
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanpo_parser::model::Run;

    #[test]
    fn closing_signers_after_an_embedded_table_have_independent_paragraphs() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-12.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let paragraphs = doc.split("</w:p>").map(|p| {
            p.split("<w:t xml:space=\"preserve\">").skip(1)
                .filter_map(|s| s.split_once("</w:t>").map(|(s,_)|s)).collect::<String>()
        }).filter(|s|!s.is_empty()).collect::<Vec<_>>();
        let compact = |s:&str|s.chars().filter(|c|!c.is_whitespace()).collect::<String>();
        let n = paragraphs.len();
        assert_eq!(compact(&paragraphs[n-2]), "内閣総理大臣高市早苗");
        assert_eq!(compact(&paragraphs[n-1]), "総務大臣林芳正");
        assert_eq!(paragraphs[n-2], "内閣総理大臣　高市　早苗");
        assert_eq!(paragraphs[n-1], "総務大臣　林　芳正");
        assert!(paragraphs[n-3].ends_with("から施行する。"));
    }

    #[test]
    fn short_native_comparison_items_remain_independent_word_paragraphs() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-10-listed-3.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let paragraphs = doc.split("</w:p>").map(|p| {
            p.split("<w:t xml:space=\"preserve\">").skip(1)
                .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                .collect::<String>()
        }).collect::<Vec<_>>();
        for item in ["（略）", "アナキンラ製剤", "セトメラノチド酢酸塩製剤", "ベピロビルセンナトリウム製剤"] {
            assert!(paragraphs.iter().any(|p| p == item), "independent native item: {item}");
        }
        assert_eq!(paragraphs.iter().filter(|p| *p == "アナキンラ製剤").count(), 2);
        assert!(paragraphs.iter().any(|p| p.contains("及び注入器用注射針加算")), "a full original column still wraps within the title");
        for item in ["セトメラノチド酢酸塩製剤", "ベピロビルセンナトリウム製剤"] {
            let paragraph = doc.split("</w:p>").find(|p| p.contains(item)).unwrap();
            assert!(paragraph.contains("<w:u "), "native underline preserved");
        }
    }

    /// Comparison rows as (改正後 paragraphs, 改正前 paragraphs), and the
    /// paragraphs outside tables.
    fn rows_and_paragraphs(doc: &str) -> (Vec<(Vec<String>, Vec<String>)>, Vec<String>) {
        let texts = |s: &str| {
            s.split("</w:p>")
                .map(|p| {
                    p.split("<w:t xml:space=\"preserve\">")
                        .skip(1)
                        .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                        .collect::<String>()
                })
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        };
        let mut rows = Vec::new();
        let mut outside = String::new();
        let mut rest = doc;
        while let Some(start) = rest.find("<w:tbl>") {
            outside.push_str(&rest[..start]);
            let end = rest[start..].find("</w:tbl>").unwrap() + start;
            for row in rest[start..end].split("<w:tr>").skip(1) {
                let cells = row.split("<w:tc>").skip(1).map(texts).collect::<Vec<_>>();
                if let [a, b] = &cells[..] {
                    rows.push((a.clone(), b.clone()));
                }
            }
            rest = &rest[end..];
        }
        outside.push_str(rest);
        (rows, texts(&outside))
    }

    #[test]
    fn evaluated_comparison_rows_follow_the_original_lines() {
        // 環境省令第二十五号（添付一覧 No.1）の評価で指摘された箇所。
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-01.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let (rows, paragraphs) = rows_and_paragraphs(&doc);
        let starts = |cell: &[String], s: &str| cell.first().is_some_and(|p| p.starts_with(s));
        // 改正前の第一条の七の六は、改正後の「（削る）」と同じ行に並ぶ。
        let deleted = rows.iter().find(|(_, b)| b.iter().any(|p| p.starts_with("第一条の七の六"))).unwrap();
        assert_eq!(deleted.0, vec!["（削る）".to_string()], "{deleted:?}");
        assert!(starts(&deleted.1, "（受託者が他人に委託して"), "{deleted:?}");
        // 前ページから続く第一条の二の三の号は、その条と同じ行に残る。
        let continued = rows.iter().find(|(a, _)| starts(a, "（都道府県による非常災害廃棄物")).unwrap();
        assert!(continued.0.iter().any(|p| p.starts_with("ニ　その他非常災害廃棄物")), "{continued:?}");
        assert_eq!(continued.1, vec!["（新設）".to_string()]);
        // 条の中の号も、原本で並ぶ相手と同じ行に置く。
        for branch in ["一の二　法第六条の二第三項前段", "一の三　法第六条の二第五項前段"] {
            let row = rows.iter().filter(|(a, _)| starts(a, branch)).collect::<Vec<_>>();
            assert_eq!(row.len(), 2, "{branch}");
            assert!(row.iter().all(|(a, b)| a.len() == 1 && b == &vec!["（新設）".to_string()]), "{row:?}");
        }
        // 列の折り返しで「第九条」「イ、」「（１）」から始まった行は、前の文に続ける。
        for (a, b) in &rows {
            for p in a.iter().chain(b) {
                for wrapped in ["第九条の三第一項に規定する", "イ、第四号イ", "イ及び第四号イ", "ニ、第四号ニ", "イ中「", "（１）」とあるのは", "以下同じ。）又は", "第三号中「、第三号ハ"] {
                    assert!(!p.starts_with(wrapped), "{p}");
                }
            }
        }
        // 号の細分・号の枝の後の空き、附則の見出し。
        assert!(rows.iter().any(|(a, _)| a.iter().any(|p| p == "イ　非常災害廃棄物の処理に係る費用負担")));
        let tail = &paragraphs[paragraphs.len() - 2..];
        assert_eq!(tail[0], "附　則", "{tail:?}");
        assert!(tail[1].starts_with("この省令は、"), "{tail:?}");
    }

    #[test]
    fn a_sideways_quotation_continues_its_amendment_sentence() {
        // 国土交通省令第八十二号（添付一覧 No.16）第三条: 横倒しの引用の終わりが様式の後ろへ移っていた。
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-16.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let (_, paragraphs) = rows_and_paragraphs(&doc);
        let i = paragraphs.iter().position(|p| p.starts_with("別記様式第二十五号の四記載要領７")).unwrap();
        assert!(paragraphs[i].ends_with("を「その国籍等（住民基本台帳法（昭和42年法律第81号）第30条の45に規定する国籍等をいう。）」に改める。"), "{}", paragraphs[i]);
        assert!(!paragraphs.iter().any(|p| p == "その国籍等（住民基本台帳法（昭和42年法律第81号）"));
    }

    fn xml(bytes: &[u8]) -> String {
        let mut zip = zip_reader(bytes);
        zip.remove("word/document.xml").expect("document.xml")
    }

    /// DOCX（zip）の部品を名前と中身にする（テスト用の小さな読み手）。
    fn zip_reader(bytes: &[u8]) -> std::collections::HashMap<String, String> {
        zip_raw(bytes)
            .into_iter()
            .map(|(k, v)| (k, String::from_utf8_lossy(&v).into_owned()))
            .collect()
    }

    fn zip_raw(bytes: &[u8]) -> std::collections::HashMap<String, Vec<u8>> {
        let mut out = std::collections::HashMap::new();
        let mut i = 0;
        while i + 30 <= bytes.len() && bytes[i..i + 4] == [0x50, 0x4b, 0x03, 0x04] {
            let u16_at = |p: usize| u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
            let u32_at = |p: usize| {
                u32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]) as usize
            };
            let (method, size, name_len, extra_len) = (
                u16_at(i + 8),
                u32_at(i + 18),
                u16_at(i + 26),
                u16_at(i + 28),
            );
            let name = String::from_utf8_lossy(&bytes[i + 30..i + 30 + name_len]).into_owned();
            let start = i + 30 + name_len + extra_len;
            let data = &bytes[start..start + size];
            let content = if method == 8 {
                miniz_oxide::inflate::decompress_to_vec(data).unwrap()
            } else {
                data.to_vec()
            };
            out.insert(name, content);
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
            articles: vec![kanpo_parser::ArticleAnalysis {
                text: text.into(),
                document: kanpo_parser::model::Document::from_text(text),
                notices: vec![],
            }],
        };
        let doc = xml(&write(&analysis).unwrap());
        assert!(
            doc.contains("〇法務省告示第百七号") && doc.contains("これを許可する。"),
            "{doc}"
        );
        let head = &doc[doc
            .find("〇法務省告示第百七号")
            .unwrap()
            .saturating_sub(300)..];
        assert!(
            head.contains("<w:b />") || head.contains("<w:b/>"),
            "first line is bold"
        );
    }

    #[test]
    fn merged_cells_span_columns_and_rows() {
        let r = |s: &str| vec![Run::plain(s)];
        let t = NestedTable {
            rows: vec![
                vec![r("名称"), vec![], r("所在地")],
                vec![r("甲"), r("乙"), vec![]],
            ],
            merges: vec![
                CellMerge {
                    row: 0,
                    col: 0,
                    rowspan: 1,
                    colspan: 2,
                },
                CellMerge {
                    row: 0,
                    col: 2,
                    rowspan: 2,
                    colspan: 1,
                },
            ],
        };
        let analysis = kanpo_parser::Analysis {
            pages: vec![],
            text: String::new(),
            format: "prose".into(),
            articles: vec![],
        };
        let mut docx = Docx::new().add_table(table(
            &MediaTable::from(&t),
            8000,
            &Layout::new(&analysis),
            &Pictures::default(),
            &SourceStyles::default().bind(&[]),
            Field::Paragraph,
        ));
        let mut out = std::io::Cursor::new(Vec::new());
        docx = docx.default_size(SIZE);
        docx.build().pack(&mut out).unwrap();
        let doc = xml(&out.into_inner());
        assert!(doc.contains("w:gridSpan w:val=\"2\""), "{doc}");
        assert!(
            doc.contains("w:vMerge w:val=\"restart\"")
                && doc.contains("w:vMerge w:val=\"continue\""),
            "{doc}"
        );
    }

    #[test]
    fn original_comparison_header_keeps_four_columns_and_both_vertical_merges() {
        // Reviewed against the original ruling: the first two columns have a
        // shared top label, while each remaining label spans both header rows.
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-04.pdf");
        let analysis = crate::analyze(pdf, &[]).unwrap();
        let tables = analysis
            .articles
            .iter()
            .flat_map(|a| &a.document.blocks)
            .filter_map(|b| {
                if let kanpo_parser::model::Block::Shinkyu { rows, .. } = b {
                    Some(rows)
                } else {
                    None
                }
            })
            .flatten()
            .flat_map(|r| r.after.iter().chain(&r.before))
            .filter_map(|r| r.table.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(tables.len(), 2);
        let layout = Layout::new(&analysis);
        for t in tables {
            assert_eq!(header_rows(t), 2);
            assert!(t.merges.contains(&CellMerge {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 2
            }));
            for col in [2, 3] {
                assert!(t.merges.contains(&CellMerge {
                    row: 0,
                    col,
                    rowspan: 2,
                    colspan: 1
                }));
            }
            assert_eq!(t.rows[0][2].iter().map(|r|r.text.as_str()).collect::<String>(),"電気通信番号により識別する電気通信設備又は提供すべき電気通信役務の種類若しくは内容");
            assert_eq!(layout.columns(t, 6608), vec![645, 1289, 1289, 3385]);
        }
        let doc = xml(&write(&analysis).unwrap());
        assert!(doc.contains("w:orient=\"landscape\""));
        assert!(doc.contains("<w:gridCol w:w=\"645\""));
        assert!(doc.contains("<w:gridCol w:w=\"3385\""));
        assert!(doc.contains("<w:vAlign w:val=\"center\""));
        let headers=doc.split("<w:tr>").skip(1).filter_map(|row|row.split_once("</w:tr>").map(|p|p.0)).filter(|row|row.contains("<w:tblHeader")).collect::<Vec<_>>();
        assert_eq!(headers.iter().map(|row|row.matches("w:vMerge w:val=\"restart\"").count()).sum::<usize>(),4);
        assert_eq!(headers.iter().map(|row|row.matches("w:vMerge w:val=\"continue\"").count()).sum::<usize>(),4);
        // The original sparse body now keeps its own cross-page rowspans too.
        assert_eq!(doc.matches("w:vMerge w:val=\"restart\"").count(), 8);
        assert_eq!(doc.matches("w:vMerge w:val=\"continue\"").count(), 10);
        assert_eq!(doc.matches("<w:tblHeader").count(), 4);
        assert!(
            doc.contains("<w:pageBreakBefore"),
            "the complete compound header starts on a fresh page"
        );
        assert_eq!(
            doc.matches("<w:tbl>").count(),
            4,
            "two comparison blocks and two complete editable inner tables"
        );
    }

    #[test]
    fn an_ordinary_opening_row_links_only_its_tail_without_becoming_a_header() {
        let r = |s: &str| vec![Run::plain(s)];
        let source = NestedTable {
            rows: vec![
                vec![r("arbitrary opening\ncontinued opening"), r("other opening")],
                vec![r("long body\nbody continuation"), r("other body")],
                vec![r("last body"), r("last value")],
            ],
            merges: vec![],
        };
        let analysis = kanpo_parser::Analysis {
            pages: vec![], text: String::new(), format: "prose".into(), articles: vec![],
        };
        let rich = MediaTable::from(&source);
        let generated = table(&rich, 8000, &Layout::new(&analysis), &Pictures::default(),
            &SourceStyles::default().bind(&[]), Field::Paragraph);
        let mut out = std::io::Cursor::new(Vec::new());
        Docx::new().add_table(generated).build().pack(&mut out).unwrap();
        let doc = xml(&out.into_inner());
        let rows = doc.split("<w:tr>").skip(1).map(|s|s.split_once("</w:tr>").unwrap().0).collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].matches("<w:keepNext").count(), 2);
        assert!(rows[0].find("continued opening").unwrap() > rows[0].find("<w:keepNext").unwrap());
        assert!(rows[1..].iter().all(|r| !r.contains("<w:keepNext")));
        assert!(!doc.contains("<w:keepLines"));
        assert!(!doc.contains("<w:cantSplit"));
        assert!(!doc.contains("<w:tblHeader"));
        for text in ["arbitrary opening", "continued opening", "other opening", "long body", "body continuation", "other body", "last body", "last value"] {
            assert_eq!(doc.matches(text).count(), 1, "table text is neither lost nor repeated");
        }
    }

    #[test]
    fn original_numbered_heading_is_its_own_paragraph_and_article_gap_is_preserved() {
        for pdf in [
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-02.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-07.pdf").as_slice(),
        ] {
            let a = crate::analyze(pdf, &[]).unwrap();
            let heading = a.articles[0].text.lines().next().unwrap();
            let doc = xml(&write(&a).unwrap());
            let start = doc.find(heading).unwrap();
            let after = &doc[start + heading.len()..];
            assert!(
                after.starts_with("</w:t></w:r></w:p>"),
                "source heading must terminate its paragraph"
            );
            if a.articles[0]
                .text
                .contains("消費税法施行規則（昭和六十三年大蔵省令第五十三号）")
            {
                let paragraphs = doc
                    .split("</w:p>")
                    .map(|p| {
                        p.split("<w:t xml:space=\"preserve\">")
                            .skip(1)
                            .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>();
                assert!(paragraphs
                    .iter()
                    .any(|p| p.starts_with("第一条　消費税法施行規則")));
                let text = zip_reader(&write(&a).unwrap())
                    .remove("word/document.xml")
                    .unwrap();
                assert!(
                    text.contains("第一条第一項中"),
                    "an inline article reference must not acquire a space"
                );
            }
        }
    }

    #[test]
    fn source_heading_split_requires_a_separate_source_line_and_preserves_styles() {
        let line = vec![("〇任意機関告示第123号本文".into(), true)];
        assert_eq!(split_source_heading(line.clone(), None), vec![line.clone()]);
        assert_eq!(
            split_source_heading(line, Some("〇任意機関告示第123号")),
            vec![
                vec![("〇任意機関告示第123号".into(), true)],
                vec![("本文".into(), true)]
            ]
        );
    }

    #[test]
    fn promulgation_seal_date_and_signature_have_independent_word_paragraphs() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-08.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let paragraphs = doc
            .split("</w:p>")
            .map(|p| {
                p.split("<w:t xml:space=\"preserve\">")
                    .skip(1)
                    .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let index = paragraphs.iter().position(|s| s == "御名　御璽").unwrap();
        assert_eq!(paragraphs[index + 1], "令和八年三月三十一日");
        assert_eq!(paragraphs[index + 2], "内閣総理大臣　高市　早苗");
        assert!(paragraphs[index - 1].ends_with("をここに公布する。"));
        assert_eq!(paragraphs[index + 3], "政令第九十三号");
        assert_eq!(paragraphs[index + 4], "所得税法施行令の一部を改正する政令");
        assert!(paragraphs[index + 5].starts_with("内閣は、"));
    }

    #[test]
    fn source_columns_restore_a_preamble_signature_displaced_by_tier_reading_order() {
        for (pdf, number) in [
            (
                include_bytes!("../../kanpo-parser/tests/data/workbook/case-13.pdf").as_slice(),
                "政令第九十八号",
            ),
            (
                include_bytes!("../../kanpo-parser/tests/data/workbook/case-18.pdf").as_slice(),
                "政令第二百八十五号",
            ),
        ] {
            let doc = xml(&crate::convert(pdf, &[]).unwrap());
            let ps = doc
                .split("</w:p>")
                .map(|p| {
                    p.split("<w:t xml:space=\"preserve\">")
                        .skip(1)
                        .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            let i = ps.iter().position(|s| s == "御名　御璽").unwrap();
            assert!(ps[i + 1].starts_with("令和八年"));
            assert_eq!(ps[i + 2], "内閣総理大臣　高市　早苗");
            assert_eq!(ps[i + 3], number);
            assert!(!ps[i + 4].contains("内閣は、"));
            assert!(ps[i + 5].starts_with("内閣は、"));
        }
    }

    #[test]
    fn a_joint_ministry_header_keeps_each_signer_and_the_repeated_title_separate() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-17.pdf");
        let doc = xml(&crate::convert(pdf, &[]).unwrap());
        let ps = doc
            .split("</w:p>")
            .map(|p| {
                p.split("<w:t xml:space=\"preserve\">")
                    .skip(1)
                    .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let i = ps
            .iter()
            .position(|s| s.starts_with("令和八年九月十六日"))
            .unwrap();
        assert!(ps[i].ends_with("内閣総理大臣　高市　早苗"));
        assert_eq!(
            ps[i + 1]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
            "財務大臣片山さつき"
        );
        assert_eq!(ps[i + 2], "株式会社国際協力銀行の会計に関する命令");
        assert_eq!(ps[i + 3], "（目的）");
        assert!(
            ps.iter().any(|p| p.starts_with("一　財務諸表　貸借対照表")),
            "the two original eight-point gaps separate the item, defined term, and definition"
        );
    }

    #[test]
    fn source_certified_underlined_characters_survive_word_conversion_without_changing_text() {
        use std::collections::BTreeMap;
        fn chars(text: &str, out: &mut BTreeMap<char, usize>) {
            for c in text.chars().filter(|c| !c.is_whitespace()) {
                *out.entry(c).or_default() += 1;
            }
        }
        fn marked(v: &serde_json::Value, out: &mut BTreeMap<char, usize>) {
            match v {
                serde_json::Value::Object(o) => {
                    if o.get("underline").and_then(|v| v.as_bool()) == Some(true) {
                        if let Some(text) = o.get("text").and_then(|v| v.as_str()) {
                            chars(text, out);
                        }
                    }
                    for v in o.values() {
                        marked(v, out);
                    }
                }
                serde_json::Value::Array(a) => {
                    for v in a {
                        marked(v, out)
                    }
                }
                _ => {}
            }
        }
        let text = |xml: &str| {
            xml.split("<w:t xml:space=\"preserve\">")
                .skip(1)
                .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                .collect::<String>()
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
        };
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-03.pdf");
        let source = kanpo_parser::source_underlines::analyze_pdf(pdf).unwrap();
        let mut expected = BTreeMap::new();
        marked(
            &serde_json::to_value(&source.styled.articles).unwrap(),
            &mut expected,
        );
        assert!(
            !expected.is_empty(),
            "the actual original contains certified CFF underline marks"
        );
        let plain = xml(&write(&crate::analyze(pdf, &[]).unwrap()).unwrap());
        let converted = xml(&crate::convert(pdf, &[]).unwrap());
        assert_eq!(
            text(&plain),
            text(&converted),
            "format recovery must not supply or remove text"
        );
        let mut actual = BTreeMap::new();
        for r in converted.split("</w:r>").filter(|r| r.contains("<w:u ")) {
            chars(&text(r), &mut actual);
        }
        assert_eq!(
            actual, expected,
            "only the original certified marked characters are underlined"
        );
    }

    #[test]
    fn original_double_underlines_keep_their_exact_source_characters_in_word() {
        use std::collections::BTreeMap;
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-11.pdf");
        let source = kanpo_parser::source_underlines::analyze_pdf(pdf).unwrap();
        let mut expected = BTreeMap::new();
        for span in source
            .parallel_underlines
            .iter()
            .filter(|s| s.parallel_strokes == 2 && source.applied_fields.contains(&s.field))
        {
            let text = kanpo_parser::source_underlines::field_text(&source.native, span.field);
            for c in text.chars().skip(span.offset).take(span.length) {
                *expected.entry(c).or_insert(0usize) += 1;
            }
        }
        assert!(!expected.is_empty());
        let converted = xml(&crate::convert(pdf, &[]).unwrap());
        let mut actual = BTreeMap::new();
        for r in converted
            .split("</w:r>")
            .filter(|r| r.contains("<w:u w:val=\"double\""))
        {
            for text in r
                .split("<w:t xml:space=\"preserve\">")
                .skip(1)
                .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
            {
                for c in text.chars().filter(|c| !c.is_whitespace()) {
                    *actual.entry(c).or_insert(0usize) += 1;
                }
            }
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn original_ruby_readings_remain_attached_to_their_own_parent_characters() {
        for pdf in [
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-11.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-10-listed-3.pdf")
                .as_slice(),
        ] {
            let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
            assert!(!source.ruby.bindings.is_empty());
            let converted = xml(&crate::convert(pdf, &[]).unwrap());
            let ruby = converted
                .split("<w:ruby>")
                .skip(1)
                .map(|s| s.split_once("</w:ruby>").unwrap().0)
                .collect::<Vec<_>>();
            assert_eq!(ruby.len(), source.ruby.bindings.len());
            let mut expected = source
                .ruby
                .bindings
                .iter()
                .map(|b| (b.text.clone(), b.reading.clone()))
                .collect::<Vec<_>>();
            let text = |s: &str| {
                s.split("<w:t xml:space=\"preserve\">")
                    .skip(1)
                    .filter_map(|s| s.split_once("</w:t>").map(|(s, _)| s))
                    .collect::<String>()
            };
            let mut actual = ruby
                .iter()
                .map(|r| {
                    let rt = r
                        .split_once("<w:rt>")
                        .unwrap()
                        .1
                        .split_once("</w:rt>")
                        .unwrap()
                        .0;
                    let base = r
                        .split_once("<w:rubyBase>")
                        .unwrap()
                        .1
                        .split_once("</w:rubyBase>")
                        .unwrap()
                        .0;
                    (text(base), text(rt))
                })
                .collect::<Vec<_>>();
            expected.sort();
            actual.sort();
            assert_eq!(actual, expected);
            assert!(!converted.contains("KanpoSourceRuby"));
        }
    }

    #[test]
    fn source_exponents_and_enclosed_text_remain_formatting_in_word() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf");
        let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
        let converted = xml(&crate::convert(pdf, &[]).unwrap());
        assert_eq!(source.baseline_shifts.bindings.len(), 3);
        let shifted = converted
            .split("</w:r>")
            .filter(|r| r.contains("w:val=\"superscript\""))
            .map(|r| {
                r.split("<w:t xml:space=\"preserve\">")
                    .nth(1)
                    .unwrap()
                    .split_once("</w:t>")
                    .unwrap()
                    .0
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(shifted, vec!["−６"; 3]);
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-04.pdf");
        let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
        let converted = xml(&crate::convert(pdf, &[]).unwrap());
        assert_eq!(source.frames.bindings.len(), 2);
        assert_eq!(converted.matches("<w:bdr ").count(), 2);
        assert_eq!(
            converted.matches("□").count(),
            0,
            "a drawing is not an extra body character"
        );
        assert_eq!(
            converted
                .split("</w:r>")
                .filter(|r| r.contains("<w:bdr "))
                .count(),
            2
        );
    }

    #[test]
    fn original_dashed_table_enclosures_keep_their_line_style_in_both_fields() {
        for pdf in [
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-04.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-11.pdf").as_slice(),
        ] {
            let converted = xml(&crate::convert(pdf, &[]).unwrap());
            assert!(
                converted.contains("w:val=\"dashed\""),
                "source dashed table edges must remain dashed in Word"
            );
            assert!(converted.contains("<w:tcBorders>"));
        }
    }

    #[test]
    fn recognized_form_still_embeds_the_full_original_image() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-10.pdf");
        let original = crate::page_images(pdf).unwrap().remove(0);
        let ocr = vec![Some(crate::OcrPage {
            width: original.page_width * original.scale,
            height: original.page_height * original.scale,
            regions: vec![crate::OcrRegion {
                text: "認識された帳票本文".into(),
                bbox: [
                    original.left * original.scale,
                    original.top * original.scale,
                    original.left * original.scale + 200.,
                    original.top * original.scale + 20.,
                ],
                vertical: false,
            }],
        })];
        let files = zip_raw(&crate::convert(pdf, &ocr).unwrap());
        let images = files
            .iter()
            .filter(|(name, _)| name.starts_with("word/media/") && name.ends_with(".png"))
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 1);
        let mut reader = png::Decoder::new(std::io::Cursor::new(images[0].1))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(
            (frame.width, frame.height),
            (original.image.width, original.image.height)
        );
        assert_eq!(&pixels[..frame.buffer_size()], &original.image.pixels);
        let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("認識された帳票本文"));
        assert!(xml.contains("<w:drawing>"));
        assert!(xml.contains("ＭＳ ゴシック"),"adding OCR text must retain native heading typography");
    }

    #[test]
    fn image_only_form_is_embedded_at_full_original_pixel_resolution_without_ocr() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-10.pdf");
        let originals = crate::page_images(pdf).unwrap();
        assert_eq!(originals.len(), 1);
        let bytes = crate::convert(pdf, &[]).unwrap();
        let files = zip_raw(&bytes);
        let images = files
            .iter()
            .filter(|(name, _)| name.starts_with("word/media/") && name.ends_with(".png"))
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 1);
        let decoder = png::Decoder::new(std::io::Cursor::new(images[0].1));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(
            (frame.width, frame.height),
            (originals[0].image.width, originals[0].image.height)
        );
        assert_eq!(&pixels[..frame.buffer_size()], &originals[0].image.pixels);
        let doc = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(doc.contains("<w:drawing>"));
        assert!(
            !doc.contains("原本1ページの画像"),
            "the source-bound form belongs in the document stream"
        );
    }

    #[test]
    fn original_table_figure_keeps_its_physical_merged_cell_in_both_ocr_modes() {
        struct NeverOcr;
        impl crate::OcrEngine for NeverOcr {
            fn recognize(&mut self, _: &crate::PageImage) -> anyhow::Result<Vec<crate::OcrRegion>> {
                panic!("a source-bound table figure must stay an original figure")
            }
        }
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-18.pdf");
        let modes = [
            Vec::new(),
            crate::recognize_pages(pdf, &mut NeverOcr).unwrap(),
        ];
        for ocr in modes {
            let bytes = crate::convert(pdf, &ocr).unwrap();
            let files = zip_raw(&bytes);
            assert_eq!(
                files
                    .keys()
                    .filter(|name| name.starts_with("word/media/") && name.ends_with(".png"))
                    .count(),
                1
            );
            let doc = String::from_utf8(files["word/document.xml"].clone()).unwrap();
            let image = doc.find("<w:drawing>").unwrap();
            let start = doc[..image].rfind("<w:tc>").unwrap();
            assert!(doc[start..image].contains("w:gridSpan w:val=\"2\""));
            assert!(
                !doc[start..image].contains("w:vMerge"),
                "the physical figure is a horizontal spanning cell"
            );
            assert!(doc.contains("w:textDirection w:val=\"tbRl\""));
            assert!(!doc.contains("原本2ページの画像"));
        }
    }
}
