//! Original decoded images, kept at full resolution and sized for Word.
use anyhow::{ensure, Result};
use docx_rs::{AlignmentType, Paragraph, Pic, Run};
use kanpo_parser::media::{self, Block, Document, Field, Inline, Target};
use std::{cell::RefCell, collections::BTreeSet};

pub(crate) struct Picture {
    pub asset: usize,
    pub page: usize,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub width_points: f64,
    pub height_points: f64,
}
#[derive(Default)]
pub(crate) struct Pictures {
    pub images: Vec<Picture>,
    emitted: RefCell<BTreeSet<usize>>,
}

pub(crate) fn table_figures(pdf: &[u8]) -> Result<Vec<(usize, [f64; 4])>> {
    let source = media::analyze_pdf(pdf)?;
    Ok(source
        .placements
        .iter()
        .filter(|p| matches!(p.target, Target::Cell { .. }))
        .filter_map(|p| {
            let a = source.assets.iter().find(|a| a.id == p.asset)?;
            Some((a.page, a.bounds?))
        })
        .collect())
}
pub(crate) fn same_image(image: &crate::PageImage, page: usize, bounds: [f64; 4]) -> bool {
    let b = [
        image.left,
        image.top,
        image.left + image.image.width as f64 / image.scale,
        image.top + image.image.height as f64 / image.scale,
    ];
    image.page == page && bounds.iter().zip(b).all(|(a, b)| (a - b).abs() <= 0.5)
}
impl Pictures {
    pub fn paragraph(&self, asset: usize, width: usize, max_height: f64) -> Option<Paragraph> {
        let image = self.images.iter().find(|p| p.asset == asset)?;
        let scale = 1_f64
            .min(width as f64 / 20. / image.width_points)
            .min(max_height / image.height_points);
        let pic = Pic::new_with_dimensions(image.png.clone(), image.width, image.height).size(
            (image.width_points * scale * 12700.).round() as u32,
            (image.height_points * scale * 12700.).round() as u32,
        );
        self.emitted.borrow_mut().insert(asset);
        Some(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().add_image(pic)),
        )
    }
    pub fn remaining(&self) -> Vec<&Picture> {
        self.images
            .iter()
            .filter(|p| !self.emitted.borrow().contains(&p.asset))
            .collect()
    }
}

fn image_order(docs: &[Document]) -> Vec<usize> {
    fn runs(items: &[Inline], out: &mut Vec<usize>) {
        for item in items {
            match item {
                Inline::Image { asset } => out.push(*asset),
                Inline::Table { table } => {
                    for row in &table.rows {
                        for cell in row {
                            runs(cell, out)
                        }
                    }
                }
                Inline::Text { .. } => {}
            }
        }
    }
    let mut out = Vec::new();
    for doc in docs {
        for b in &doc.blocks {
            match b {
                Block::Paragraph { runs: r } => runs(r, &mut out),
                Block::Shinkyu { rows, .. } => {
                    for row in rows {
                        runs(&row.after, &mut out);
                        runs(&row.before, &mut out);
                        for t in [&row.after_table, &row.before_table].into_iter().flatten() {
                            for row in &t.rows {
                                for cell in row {
                                    runs(cell, &mut out)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn prepare_source(
    pdf: &[u8],
    analysis: &kanpo_parser::Analysis,
    source: &media::MediaAnalysis,
) -> Result<(Vec<Document>, Pictures)> {
    let mut docs = analysis
        .articles
        .iter()
        .map(|a| Document::from(&a.document))
        .collect::<Vec<_>>();
    // Recognition adds editable text; it must never discard the source image.
    // If OCR changes field offsets, the unmatched image is retained by the
    // writer's source-page appendix rather than placed at an unproved offset.
    let originals = crate::page_images(pdf)?;
    if originals.is_empty() {
        return Ok((docs, Pictures::default()));
    }
    let mut images = Vec::new();
    for original in originals {
        let bounds = [
            original.left,
            original.top,
            original.left + original.image.width as f64 / original.scale,
            original.top + original.image.height as f64 / original.scale,
        ];
        let matches = source
            .assets
            .iter()
            .filter(|a| {
                a.page == original.page
                    && a.bounds
                        .is_some_and(|b| b.iter().zip(bounds).all(|(a, b)| (a - b).abs() <= 0.5))
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "original image has no unique source placement on page {}",
            original.page + 1
        );
        let mut png = Vec::new();
        {
            let mut encoder =
                png::Encoder::new(&mut png, original.image.width, original.image.height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()?
                .write_image_data(&original.image.pixels)?;
        }
        images.push(Picture {
            asset: matches[0].id,
            page: original.page,
            png,
            width: original.image.width,
            height: original.image.height,
            width_points: original.image.width as f64 / original.scale,
            height_points: original.image.height as f64 / original.scale,
        });
    }
    let allowed = images.iter().map(|i| i.asset).collect::<BTreeSet<_>>();
    let fields = [Field::Paragraph, Field::After, Field::Before];
    let matched = fields.map(|f| {
        docs.iter().map(|d| d.field_text(f)).collect::<String>()
            == source
                .documents
                .iter()
                .map(|d| d.field_text(f))
                .collect::<String>()
    });
    let field_ok = |field| {
        fields
            .iter()
            .position(|f| *f == field)
            .is_some_and(|i| matched[i])
    };
    if docs.len() == 1 {
        let order = image_order(&source.documents);
        for asset in &order {
            if !allowed.contains(asset) {
                continue;
            }
            if let Some(plan) = source.placements.iter().find(|p| p.asset == *asset) {
                if let Target::Cell {
                    field,
                    page,
                    region,
                    line,
                    row,
                    col,
                } = plan.target
                {
                    if field_ok(field) {
                        if let Some(t) = source.native.pages[page].regions[region].lines[line]
                            .table
                            .as_ref()
                        {
                            docs[0].insert_image_in_table(field, &t.to_nested(), row, col, *asset);
                        }
                    }
                }
            }
        }
        for asset in order.into_iter().rev() {
            if !allowed.contains(&asset) {
                continue;
            }
            if let Some(plan) = source.placements.iter().find(|p| p.asset == asset) {
                if let Target::Stream { field, offset } = plan.target {
                    if field_ok(field) {
                        docs[0].insert_image(field, offset, asset);
                    }
                }
            }
        }
    }
    Ok((
        docs,
        Pictures {
            images,
            emitted: RefCell::new(BTreeSet::new()),
        },
    ))
}
