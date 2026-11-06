//! Image-aware documents retain the legacy native model without changing Run.
use crate::{model, ComparisonLabels};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub format: String,
    pub blocks: Vec<Block>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        runs: Vec<Inline>,
    },
    Shinkyu {
        rows: Vec<Row>,
        labels: Option<ComparisonLabels>,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub after: Vec<Inline>,
    pub before: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_table: Option<Table>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_table: Option<Table>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inline {
    Text { text: String, underline: bool },
    Table { table: Table },
    Image { asset: usize },
}
#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub rows: Vec<Vec<Vec<Inline>>>,
    pub merges: Vec<model::CellMerge>,
}
impl From<&model::NestedTable> for Table {
    fn from(table: &model::NestedTable) -> Self {
        Self {
            rows: table
                .rows
                .iter()
                .map(|r| r.iter().map(|c| inlines(c)).collect())
                .collect(),
            merges: table.merges.clone(),
        }
    }
}
fn inlines(runs: &[model::Run]) -> Vec<Inline> {
    let mut out = Vec::new();
    for run in runs {
        if !run.text.is_empty() || run.table.is_none() {
            out.push(Inline::Text {
                text: run.text.clone(),
                underline: run.underline,
            });
        }
        if let Some(table) = &run.table {
            out.push(Inline::Table {
                table: Table::from(table),
            });
        }
    }
    out
}
impl From<&model::Document> for Document {
    fn from(document: &model::Document) -> Self {
        Self {
            format: document.format.clone(),
            blocks: document
                .blocks
                .iter()
                .map(|block| match block {
                    model::Block::Paragraph { runs } => Block::Paragraph {
                        runs: inlines(runs),
                    },
                    model::Block::Shinkyu { rows, labels } => Block::Shinkyu {
                        labels: *labels,
                        rows: rows
                            .iter()
                            .map(|row| Row {
                                after: inlines(&row.after),
                                before: inlines(&row.before),
                                after_table: row.after_table.as_ref().map(Table::from),
                                before_table: row.before_table.as_ref().map(Table::from),
                            })
                            .collect(),
                    },
                })
                .collect(),
        }
    }
}

impl Document {
    /// Complete non-whitespace field text, including nested and standalone
    /// table cells in document order. Ruby offsets use Unicode scalar indices
    /// in this stream; images and ruby readings do not consume body positions.
    pub fn field_text(&self, field: super::Field) -> String {
        let mut out=String::new();
        visit(self,field,&mut|runs|out.push_str(&text(runs)));
        out
    }
}

pub(super) fn text(runs: &[Inline]) -> String {
    let mut out = String::new();
    for run in runs {
        match run {
            Inline::Text { text, .. } => out.extend(text.chars().filter(|c| !c.is_whitespace())),
            Inline::Table { table } => {
                for row in &table.rows {
                    for cell in row {
                        out.push_str(&text_of(cell));
                    }
                }
            }
            Inline::Image { .. } => {}
        }
    }
    out
}
fn text_of(runs: &[Inline]) -> String {
    text(runs)
}

pub(super) fn visit(document: &Document, field: super::Field, f: &mut impl FnMut(&[Inline])) {
    use super::Field;
    for block in &document.blocks {
        match block {
            Block::Paragraph { runs } if field == Field::Paragraph => f(runs),
            Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                for row in rows {
                    let (runs, table) = if field == Field::After {
                        (&row.after, &row.after_table)
                    } else {
                        (&row.before, &row.before_table)
                    };
                    f(runs);
                    if let Some(table) = table {
                        for r in &table.rows {
                            for cell in r {
                                f(cell);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
pub(super) fn visit_mut(
    document: &mut Document,
    field: super::Field,
    f: &mut impl FnMut(&mut Vec<Inline>),
) {
    use super::Field;
    for block in &mut document.blocks {
        match block {
            Block::Paragraph { runs } if field == Field::Paragraph => f(runs),
            Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                for row in rows {
                    let (runs, table) = if field == Field::After {
                        (&mut row.after, &mut row.after_table)
                    } else {
                        (&mut row.before, &mut row.before_table)
                    };
                    f(runs);
                    if let Some(table) = table {
                        for r in &mut table.rows {
                            for cell in r {
                                f(cell);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn put(runs: &mut Vec<Inline>, offset: usize, cursor: &mut usize, asset: usize) -> bool {
    for index in 0..runs.len() {
        match &mut runs[index] {
            Inline::Text { text, underline } => {
                let count = text.chars().filter(|c| !c.is_whitespace()).count();
                if *cursor <= offset && offset <= *cursor + count {
                    let own = offset - *cursor;
                    let byte = if own == 0 {
                        0
                    } else {
                        text.char_indices()
                            .filter(|(_, c)| !c.is_whitespace())
                            .nth(own - 1)
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap()
                    };
                    let (before, after) = (text[..byte].to_owned(), text[byte..].to_owned());
                    let style = *underline;
                    let mut replacement = Vec::new();
                    if !before.is_empty() {
                        replacement.push(Inline::Text {
                            text: before,
                            underline: style,
                        });
                    }
                    replacement.push(Inline::Image { asset });
                    if !after.is_empty() {
                        replacement.push(Inline::Text {
                            text: after,
                            underline: style,
                        });
                    }
                    runs.splice(index..=index, replacement);
                    return true;
                }
                *cursor += count;
            }
            Inline::Table { table } => {
                for row in &mut table.rows {
                    for cell in row {
                        if put(cell, offset, cursor, asset) {
                            return true;
                        }
                    }
                }
            }
            Inline::Image { .. } => {}
        }
    }
    false
}
pub(super) fn insert_stream(
    document: &mut Document,
    field: super::Field,
    offset: usize,
    asset: usize,
) -> bool {
    let mut cursor = 0;
    let mut applied = false;
    visit_mut(document, field, &mut |runs| {
        if !applied {
            applied = put(runs, offset, &mut cursor, asset);
        }
    });
    applied
}

// Full table shape/text comparison excludes only annotations and Run grouping.
// Repeated equal tables are ambiguous and must be rejected by the caller.
fn same_table(a: &Table, b: &model::NestedTable) -> bool {
    if a.merges != b.merges || a.rows.len() != b.rows.len() {
        return false;
    }
    a.rows.iter().zip(&b.rows).all(|(ar,br)|ar.len()==br.len()&&ar.iter().zip(br).all(|(ac,bc)|{
        let rich=Table{rows:vec![vec![ac.clone()]],merges:vec![]};
        let plain=Table::from(&model::NestedTable{rows:vec![vec![bc.clone()]],merges:vec![]});
        fn shape(table:&Table)->serde_json::Value{
            serde_json::json!({"merges":table.merges,"rows":table.rows.iter().map(|r|r.iter().map(|c|{
                let mut parts=Vec::new();let mut s=String::new();
                for inline in c {match inline{
                    Inline::Text{text,..}=>s.push_str(text),
                    Inline::Table{table}=>{parts.push(serde_json::json!(std::mem::take(&mut s)));parts.push(shape(table));},
                    Inline::Image{..}=>{},
                }}parts.push(serde_json::json!(s));parts
            }).collect::<Vec<_>>()).collect::<Vec<_>>()})
        }
        shape(&rich)==shape(&plain)
    }))
}
fn table_count(runs: &[Inline], target: &model::NestedTable) -> usize {
    runs.iter()
        .map(|inline| match inline {
            Inline::Table { table } => {
                usize::from(same_table(table, target))
                    + table
                        .rows
                        .iter()
                        .flatten()
                        .map(|c| table_count(c, target))
                        .sum::<usize>()
            }
            _ => 0,
        })
        .sum()
}
fn put_cell(
    runs: &mut [Inline],
    target: &model::NestedTable,
    row: usize,
    col: usize,
    asset: usize,
) -> bool {
    for inline in runs {
        if let Inline::Table { table } = inline {
            if same_table(table, target) {
                let Some(cell) = table.rows.get_mut(row).and_then(|r| r.get_mut(col)) else {
                    return false;
                };
                if !text(cell).is_empty() {
                    return false;
                }
                cell.push(Inline::Image { asset });
                return true;
            }
            for cells in &mut table.rows {
                for cell in cells {
                    if put_cell(cell, target, row, col, asset) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
pub(super) fn insert_cell(
    document: &mut Document,
    field: super::Field,
    target: &model::NestedTable,
    row: usize,
    col: usize,
    asset: usize,
) -> bool {
    // Standalone after_table/before_table are exposed as their cells by visit;
    // source-native inline tables are located through their complete shape.
    let mut count = 0;
    visit(document, field, &mut |runs| {
        count += table_count(runs, target)
    });
    if count != 1 {
        return false;
    }
    let mut applied = false;
    visit_mut(document, field, &mut |runs| {
        if !applied {
            applied = put_cell(runs, target, row, col, asset);
        }
    });
    applied
}
