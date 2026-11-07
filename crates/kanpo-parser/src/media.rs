//! Image-aware instrument documents. Preserve original image bytes and place
//! them only through complete source field streams and physical source bounds.
//! Explicit raster observations can supply editable positioned candidates;
//! recognition and final compositing remain unverified.
pub use crate::source_underlines::Field;
use crate::trace::{PageTrace, Role};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[path = "media_source_metadata.rs"]
mod metadata;
#[path = "media_model.rs"]
mod model;
#[path = "media_ruby.rs"]
pub mod ruby;
#[path = "media_native_glyphs.rs"]
pub mod native_glyphs;
#[path = "media_underline_styles.rs"]
pub mod underline_styles;
#[path = "media_frames.rs"]
pub mod frames;
#[path = "media_baseline_shifts.rs"]
pub mod baseline_shifts;
#[path = "media_fonts.rs"]
pub mod fonts;
#[path = "media_raster.rs"]
pub mod raster;
pub use model::{Block, Document, Inline, Row, Table};
type Rect = [f64; 4];

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    pub id: usize,
    pub page: usize,
    pub draw: usize,
    pub document_sha256: String,
    pub encoded_sha256: String,
    /// Decrypted stream bytes still retain their declared PDF filters.
    pub encoded_bytes: Vec<u8>,
    pub source: serde_json::Value,
    pub media_box: Rect,
    pub crop_box: Rect,
    pub rotate: i32,
    pub quad: Option<[[f64; 2]; 4]>,
    pub bounds: Option<Rect>,
    pub visibility_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raster_layout: Option<Box<raster::Layout>>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// Non-whitespace character offset across the complete document field.
    Stream { field: Field, offset: usize },
    /// One original native table cell, matched to one complete document table.
    Cell {
        field: Field,
        page: usize,
        region: usize,
        line: usize,
        row: usize,
        col: usize,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct PagePlacement {
    pub asset: usize,
    pub page: usize,
    /// Geometric page ownership; no legal article is inferred from empty space.
    pub kind: PageImageKind,
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all="snake_case")]
pub enum PageImageKind { PageSurface, IndependentGraphic }
#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    pub asset: usize,
    pub target: Target,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub asset: Option<usize>,
    pub page: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct UnderlineAudit {
    pub scope: &'static str,
    pub coverage_verified: bool,
    pub applied_fields: Vec<Field>,
    pub unresolved_fields: Vec<crate::source_underlines::UnresolvedField>,
    pub source_issues: Vec<crate::source_underlines::SourceIssue>,
    pub glyph_bindings: Vec<crate::source_underlines::GlyphBinding>,
    pub parallel_bindings:Vec<underline_styles::Binding>,
    pub unresolved_parallel:Vec<underline_styles::Unresolved>,
    pub unresolved_parallel_words:Vec<crate::source_underlines::SourceWord>,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub line_projections:Vec<crate::source_line_projection::Record>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PaintAudit {
    pub bindings: Vec<crate::painted_text::Binding>,
    pub unresolved: Vec<crate::painted_text::Unresolved>,
    pub observation_report: serde_json::Value,
}
#[derive(Debug, Clone, Serialize)]
pub struct TableContinuation {
    /// Complete source-certified logical table, before presentation run splits.
    pub table: crate::model::NestedTable,
    /// (source table ID, source row, source column, target row, target column).
    pub cells: Vec<(usize, usize, usize, usize, usize)>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MediaAnalysis {
    pub schema: usize,
    pub kind: &'static str,
    pub native: crate::Analysis,
    /// Source-certified layout after nontext drawing glyphs and annotations
    /// are resolved. Raw native material remains in `native` for auditing.
    /// This in-memory view avoids deriving layout back from rendered output.
    #[serde(skip)]
    pub source_layout: crate::Analysis,
    /// In-memory positional provenance for open cells crossing native pages.
    #[serde(skip)]
    pub table_continuations: Vec<TableContinuation>,
    pub documents: Vec<Document>,
    pub assets: Vec<Asset>,
    /// Original vector construction and paint state, retained at page scope.
    pub page_graphics: Vec<serde_json::Value>,
    pub placements: Vec<Placement>,
    pub page_placements: Vec<PagePlacement>,
    pub unresolved: Vec<Unresolved>,
    pub underlines: UnderlineAudit,
    pub ruby: ruby::Audit,
    pub native_glyphs: native_glyphs::Audit,
    pub frames:frames::Audit,
    pub baseline_shifts:baseline_shifts::Audit,
    pub fonts:fonts::Audit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub painted_text: Option<PaintAudit>,
    pub coverage_verified: bool,
}

fn positive(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn union(boxes: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for b in boxes {
        if !positive(b) {
            return None;
        }
        out = Some(out.map_or(b, |a| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        }));
    }
    out
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn index(field: Field) -> usize {
    match field {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
fn field(role: Role) -> Field {
    match role {
        Role::After => Field::After,
        Role::Before => Field::Before,
        _ => Field::Paragraph,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relation {
    Before,
    After,
}
fn relation(a: Rect, b: Rect) -> Option<Relation> {
    let x = if a[0] >= b[2] {
        Some(Relation::Before)
    } else if a[2] <= b[0] {
        Some(Relation::After)
    } else {
        None
    };
    let y = if a[3] <= b[1] {
        Some(Relation::Before)
    } else if a[1] >= b[3] {
        Some(Relation::After)
    } else {
        None
    };
    match (x, y) {
        (Some(x), Some(y)) if x != y => None,
        (Some(x), _) => Some(x),
        (_, Some(y)) => Some(y),
        _ => None,
    }
}
fn quad(
    source: &kanpo_pdf::XObjectPageSources,
    draw: &kanpo_pdf::ImageDraw,
) -> Option<[[f64; 2]; 4]> {
    let [a, b, c, d, e, f] = draw.image_to_user;
    let [x0, y0, x1, y1] = source.crop_box();
    let (w, h) = (x1 - x0, y1 - y0);
    if ![a, b, c, d, e, f, w, h].iter().all(|n| n.is_finite())
        || a * d - b * c == 0.
        || w <= 0.
        || h <= 0.
    {
        return None;
    }
    let points = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]].map(|[u, v]| {
        let (x, y) = (a * u + c * v + e - x0, b * u + d * v + f - y0);
        match source.page().rotate {
            0 => [x, h - y],
            90 => [y, x],
            180 => [w - x, y],
            270 => [h - y, w - x],
            _ => [f64::NAN; 2],
        }
    });
    points
        .iter()
        .flatten()
        .all(|n| n.is_finite())
        .then_some(points)
}
fn quad_bounds(q: [[f64; 2]; 4]) -> Option<Rect> {
    let b = [
        q.iter().map(|p| p[0]).reduce(f64::min)?,
        q.iter().map(|p| p[1]).reduce(f64::min)?,
        q.iter().map(|p| p[0]).reduce(f64::max)?,
        q.iter().map(|p| p[1]).reduce(f64::max)?,
    ];
    positive(b).then_some(b)
}

#[derive(Clone)]
struct SourceLine {
    field: Field,
    start: usize,
    end: usize,
    bounds: Option<Rect>,
}
struct PageLayout {
    starts: [usize; 3],
    lines: Vec<SourceLine>,
    comparison: bool,
    interrupts: bool,
}
fn word_bounds(page: &PageTrace, native: &kanpo_pdf::Page) -> BTreeMap<usize, Rect> {
    let mut occurrences = BTreeMap::<(String, [u32; 4]), Vec<Vec<usize>>>::new();
    for source in kanpo_pdf::bbox::words(native) {
        let b = [source.x_min, source.y_min, source.x_max, source.y_max]
            .map(|n| format!("{n:.6}").parse::<f32>().unwrap().to_bits());
        occurrences
            .entry((source.text, b))
            .or_default()
            .push(source.glyphs);
    }
    let mut out = BTreeMap::new();
    for word in &page.words {
        let key = (word.raw.clone(), word.source_box.map(f32::to_bits));
        let Some(ids) = occurrences.get(&key).filter(|v| v.len() == 1) else {
            continue;
        };
        if let Some(b) = union(ids[0].iter().map(|&i| native.glyphs[i].bbox)) {
            out.insert(word.id, b);
        }
    }
    out
}
fn source_layout(
    analysis: &crate::Analysis,
    sources: &[kanpo_pdf::XObjectPageSources],
) -> ([String; 3], Vec<PageLayout>) {
    let mut streams: [String; 3] = Default::default();
    let mut layouts = Vec::new();
    for (page, source) in analysis.pages.iter().zip(sources) {
        let starts = streams.each_ref().map(|s| s.chars().count());
        let boxes = word_bounds(page, source.page());
        let mut lines = Vec::new();
        for region in &page.regions {
            let field = field(region.role);
            for line in &region.lines {
                let start = streams[index(field)].chars().count();
                streams[index(field)].push_str(&compact(&line.text));
                let end = streams[index(field)].chars().count();
                let b = if let Some(table) = &line.table {
                    let b = [
                        table.x0 as f64,
                        table.y0 as f64,
                        table.x1 as f64,
                        table.y1 as f64,
                    ];
                    positive(b).then_some(b)
                } else if line.words.iter().all(|id| boxes.contains_key(id)) {
                    union(line.words.iter().map(|id| boxes[id]))
                } else {
                    None
                };
                lines.push(SourceLine {
                    field,
                    start,
                    end,
                    bounds: b,
                });
            }
        }
        let comparison = page
            .regions
            .iter()
            .any(|r| matches!(r.role, Role::After | Role::Before));
        let interrupts = page.regions.iter().any(|r| {
            r.role == Role::Body
                && r.lines.iter().any(|l| {
                    !compact(&l.text).is_empty()
                        && (!l.words.is_empty()
                            || l.table
                                .as_ref()
                                .is_some_and(|t| t.cells.iter().any(|c| !c.words.is_empty())))
                })
        });
        layouts.push(PageLayout {
            starts,
            lines,
            comparison,
            interrupts,
        });
    }
    (streams, layouts)
}

#[derive(Deserialize)]
struct Scene {
    lines: Vec<(Rect, Rect)>,
}
// Use original f64 stroke centers. Grid guesses and text extents do not close a frame.
fn closed_rectangles(source: &kanpo_pdf::XObjectPageSources, image: Rect) -> Vec<Rect> {
    let xhtml = kanpo_pdf::bbox::bbox_layout_with_native_styles(std::slice::from_ref(
        source.native_sources(),
    ));
    let html = scraper::Html::parse_document(&xhtml);
    let selector = scraper::Selector::parse("page").unwrap();
    let scene = html
        .select(&selector)
        .next()
        .and_then(|n| n.value().attr("data-native-scene"))
        .and_then(|s| serde_json::from_str::<Scene>(s).ok());
    let Some(scene) = scene else {
        return Vec::new();
    };
    let vs = scene
        .lines
        .iter()
        .map(|(c, _)| c)
        .filter(|c| c[0] == c[2] && c[1] <= image[1] && image[3] <= c[3])
        .collect::<Vec<_>>();
    let hs = scene
        .lines
        .iter()
        .map(|(c, _)| c)
        .filter(|c| c[1] == c[3] && c[0] <= image[0] && image[2] <= c[2])
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for left in &vs {
        if left[0] > image[0] {
            continue;
        }
        for right in &vs {
            if right[0] < image[2] || left[0] >= right[0] {
                continue;
            }
            for top in &hs {
                if top[1] > image[1] {
                    continue;
                }
                for bottom in &hs {
                    if bottom[1] < image[3] {
                        continue;
                    }
                    let b = [left[0], top[1], right[0], bottom[1]];
                    if positive(b)
                        && [left, right].iter().all(|v| v[1] <= b[1] && b[3] <= v[3])
                        && [top, bottom].iter().all(|h| h[0] <= b[0] && b[2] <= h[2])
                    {
                        out.push(b);
                    }
                }
            }
        }
    }
    out
}

fn stream_target(
    layout: &PageLayout,
    image: Rect,
    frames: &[Rect],
    inherited: bool,
) -> Result<Target> {
    let field = if layout.comparison {
        ensure!(
            layout
                .lines
                .iter()
                .all(|l| l.bounds.is_some() || l.start == l.end),
            "comparison source has unknown line bounds"
        );
        let mut roles = BTreeSet::new();
        for &frame in frames {
            if !contains(frame, image) {
                continue;
            }
            let owned = layout
                .lines
                .iter()
                .filter_map(|l| {
                    l.bounds
                        .filter(|&b| overlaps(frame, b))
                        .map(|_| index(l.field))
                })
                .collect::<BTreeSet<_>>();
            let full = layout
                .lines
                .iter()
                .filter_map(|l| {
                    l.bounds
                        .filter(|&b| contains(frame, b))
                        .map(|_| index(l.field))
                })
                .collect::<BTreeSet<_>>();
            if owned.len() == 1 && full == owned {
                roles.extend(owned);
            }
        }
        ensure!(
            roles.len() == 1,
            "image does not have one original ruled field"
        );
        match *roles.first().unwrap() {
            1 => Field::After,
            2 => Field::Before,
            _ => Field::Paragraph,
        }
    } else {
        ensure!(
            !inherited || layout.interrupts,
            "image-only continuation has unresolved comparison scope"
        );
        Field::Paragraph
    };
    let mut after = false;
    let mut offset = layout.starts[index(field)];
    for line in layout.lines.iter().filter(|l| l.field == field) {
        let bounds = line
            .bounds
            .ok_or_else(|| anyhow::anyhow!("original line bounds unavailable"))?;
        match relation(bounds, image) {
            Some(Relation::Before) => {
                ensure!(!after, "image does not have one ordered line boundary");
                offset = line.end;
            }
            Some(Relation::After) => {
                if !after {
                    ensure!(offset == line.start, "source field boundary differs");
                }
                after = true;
            }
            None => anyhow::bail!("image overlaps or conflicts with original line order"),
        }
    }
    Ok(Target::Stream { field, offset })
}

fn target(
    asset: &Asset,
    analysis: &crate::Analysis,
    layout: &PageLayout,
    source: &kanpo_pdf::XObjectPageSources,
    inherited: bool,
) -> Result<Target> {
    let b = asset
        .bounds
        .ok_or_else(|| anyhow::anyhow!("invalid image placement"))?;
    let draw = &source.images()[asset.draw];
    ensure!(
        draw.source_geometry_complete && source.capture_gaps().is_empty(),
        "incomplete original image geometry/traversal"
    );
    ensure!(
        contains([0., 0., source.page().width, source.page().height], b),
        "image extends outside page"
    );
    ensure!(
        !source.page().glyphs.iter().any(|g| !g.invisible
            && !g.text.is_empty()
            && !g.text.chars().all(char::is_whitespace)
            && source
                .page()
                .fonts
                .get(g.font)
                .is_some_and(kanpo_pdf::bbox::poppler_readable)
            && positive(g.bbox)
            && overlaps(b, g.bbox)),
        "image overlaps native text"
    );
    let page = &analysis.pages[asset.page];
    let mut cells = Vec::new();
    for (ri, region) in page.regions.iter().enumerate() {
        for (li, line) in region.lines.iter().enumerate() {
            if let Some(table) = &line.table {
                for cell in &table.cells {
                    if contains(
                        [
                            cell.x0 as f64,
                            cell.y0 as f64,
                            cell.x1 as f64,
                            cell.y1 as f64,
                        ],
                        b,
                    ) {
                        cells.push((ri, li, region.role, cell));
                    }
                }
            }
        }
    }
    if !cells.is_empty() {
        ensure!(cells.len() == 1, "image has multiple native table cells");
        let (ri, li, role, cell) = cells[0];
        ensure!(
            compact(&cell.text).is_empty() && cell.words.is_empty(),
            "image cell also contains native text"
        );
        return Ok(Target::Cell {
            field: field(role),
            page: asset.page,
            region: ri,
            line: li,
            row: cell.row,
            col: cell.col,
        });
    }
    let frames = if layout.comparison {
        closed_rectangles(source, b)
    } else {
        Vec::new()
    };
    stream_target(layout, b, &frames, inherited)
}

fn figure_order(ids: &[usize], assets: &[Asset]) -> Option<Vec<usize>> {
    if ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len() {
        return None;
    }
    let mut predecessors = BTreeMap::<usize, BTreeSet<usize>>::new();
    for &id in ids {
        predecessors.insert(id, BTreeSet::new());
    }
    for (a, &left) in ids.iter().enumerate() {
        for &right in &ids[a + 1..] {
            let l = assets.get(left)?;
            let r = assets.get(right)?;
            let relation = if l.page < r.page {
                Some(Relation::Before)
            } else if l.page > r.page {
                Some(Relation::After)
            } else {
                relation(l.bounds?, r.bounds?)
            };
            match relation? {
                Relation::Before => {
                    predecessors.get_mut(&right)?.insert(left);
                }
                Relation::After => {
                    predecessors.get_mut(&left)?.insert(right);
                }
            }
        }
    }
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    while done.len() < ids.len() {
        let ready = ids
            .iter()
            .copied()
            .filter(|id| !done.contains(id) && predecessors[id].is_subset(&done))
            .collect::<Vec<_>>();
        if ready.len() != 1 {
            return None;
        }
        let id = ready[0];
        done.insert(id);
        out.push(id);
    }
    Some(out)
}

/// One instrument with explicitly represented raster sources, including empty
/// table cells and whole-form pages. Multi-article image ownership, overlapping
/// images/text and ambiguous geometry remain unresolved rather than guessed.
pub fn analyze_pdf(pdf: &[u8]) -> Result<MediaAnalysis> {
    analyze(pdf, None)
}

/// Include explicitly supplied paint observations in the image-aware document.
/// Recognition remains unverified; the original native result is retained.
pub fn analyze_pdf_with_observations(
    pdf: &[u8],
    report: serde_json::Value,
) -> Result<MediaAnalysis> {
    analyze(pdf, Some(report))
}

fn analyze(pdf: &[u8], report: Option<serde_json::Value>) -> Result<MediaAnalysis> {
    let mut styled = crate::source_underlines::analyze_pdf(pdf)?;
    let native=styled.native.clone();
    let glyph_readings=report.as_ref().and_then(|r|r.get("native_glyph_observations")).cloned();
    let painted_text = if let Some(report) = report {
        let painted = crate::painted_text::analyze_pdf(pdf, report)?;
        let mut observed = painted.observed;
        styled.applied_fields.retain(|&field| {
            if crate::source_underlines::transfer_field(&styled.styled, &mut observed, field) {
                true
            } else {
                styled
                    .unresolved_fields
                    .push(crate::source_underlines::UnresolvedField {
                    field,
                    reason:
                        "paint observations change this complete field; underline transfer pending"
                            .into(),
                });
                false
            }
        });
        styled.styled = observed;
        Some(PaintAudit {
            bindings: painted.bindings,
            unresolved: painted.unresolved,
            observation_report: painted.observation_report,
        })
    } else {
        None
    };
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf)?;
    let (mut repaired,native_glyphs)=native_glyphs::apply(&styled.styled,&sources,painted_text.as_ref(),glyph_readings.as_ref())?;
    if !native_glyphs.bindings.is_empty(){
        styled.applied_fields.retain(|&field|{
            if crate::source_underlines::transfer_field(&styled.styled,&mut repaired,field){true}else{
                styled.unresolved_fields.push(crate::source_underlines::UnresolvedField{field,reason:"native glyph observations change this complete field; underline transfer pending".into()});false
            }
        });
    }
    styled.styled=repaired;
    let (framed,enclosures,frame_unresolved)=frames::apply(&styled.styled,&sources,painted_text.as_ref())?;
    if !enclosures.is_empty() {
        let source_pages=sources.iter().map(|s|s.native_sources().clone()).collect::<Vec<_>>();
        styled=crate::source_underlines::analyze_observed(&source_pages,framed,&frames::nontext(&enclosures))?;
    }
    let continuation_inputs=crate::page_inputs_from_sources(&sources.iter().map(|s|s.native_sources().clone()).collect::<Vec<_>>());
    let continuation_xhtml=crate::combine_pages(continuation_inputs.iter().map(|p|p.xhtml.as_str()));
    let continuation_rules=continuation_inputs.iter().map(|p|p.rules.clone()).collect::<Vec<_>>();
    let mut continuation_pages=styled.styled.pages.clone();
    let continuation_pairs=crate::native_sparse_table_continuations::restore(&mut continuation_pages,&continuation_xhtml,&continuation_rules);
    let table_continuations=crate::native_sparse_table_continuations::groups(&continuation_pages,&continuation_pairs);
    let base = &styled.styled;
    ensure!(
        base.pages.len() == sources.len(),
        "source page counts differ"
    );
    let mut documents = base
        .articles
        .iter()
        .map(|a| Document::from(&a.document))
        .collect::<Vec<_>>();
    let (streams, layouts) = source_layout(base, &sources);
    let matched = [Field::Paragraph, Field::After, Field::Before].map(|field| {
        let mut text = String::new();
        for document in &documents {
            model::visit(document, field, &mut |runs| {
                text.push_str(&model::text(runs))
            });
        }
        text == streams[index(field)]
    });
    let mut assets = Vec::new();
    let mut unresolved = Vec::new();
    let mut plans = Vec::new();
    let mut page_placements = Vec::new();
    let mut inherited = false;
    for (pi, source) in sources.iter().enumerate() {
        if !source.capture_gaps().is_empty() {
            unresolved.push(Unresolved {
                asset: None,
                page: pi,
                reason: "source image capture has gaps".into(),
            });
        }
        for draw in source.images() {
            let q = quad(source, draw);
            let b = q.and_then(quad_bounds);
            let id = assets.len();
            let asset = Asset {
                id,
                page: pi,
                draw: draw.sequence,
                document_sha256: metadata::hex(&source.document_sha256()),
                encoded_sha256: metadata::hex(&draw.encoded_sha256()),
                encoded_bytes: draw.encoded_bytes().to_vec(),
                source: metadata::describe(draw),
                media_box: source.media_box(),
                crop_box: source.crop_box(),
                rotate: source.page().rotate,
                quad: q,
                bounds: b,
                visibility_verified: false,
                raster_layout: None,
            };
            let page_kind = asset.bounds.and_then(|bounds| {
                if !draw.source_geometry_complete {return None;}
                let page=source.page();
                if bounds==[0.,0.,page.width,page.height] {
                    return Some(PageImageKind::PageSurface);
                }
                // A free-standing draw can retain its own page position without
                // guessing an article or a character boundary in that article.
                // Overlaps with native content still require compositing review.
                let clear=base.pages[pi].words.iter().filter(|w|w.status==crate::trace::WordStatus::Used)
                    .all(|w|!overlaps(bounds,[w.x0 as f64,w.y0 as f64,w.x1 as f64,w.y1 as f64]));
                clear.then_some(PageImageKind::IndependentGraphic)
            });
            if matches!(page_kind,Some(PageImageKind::PageSurface)) {
                page_placements.push(PagePlacement{asset:id,page:pi,kind:PageImageKind::PageSurface});
                assets.push(asset);
                continue;
            }
            let plan = if documents.len() == 1 {
                target(&asset, base, &layouts[pi], source, inherited)
            } else {
                Err(anyhow::anyhow!(
                    "image ownership needs one native instrument article"
                ))
            };
            match plan {
                Ok(plan) => {
                    let f = match &plan {
                        Target::Cell { field, .. } | Target::Stream { field, .. } => *field,
                    };
                    if matched[index(f)] {
                        plans.push(Placement {
                            asset: id,
                            target: plan,
                        });
                    } else {
                        unresolved.push(Unresolved {
                            asset: Some(id),
                            page: pi,
                            reason: "complete source/document field streams differ".into(),
                        });
                    }
                }
                Err(error) => if documents.len()>1 && page_kind.is_some() {
                    page_placements.push(PagePlacement{asset:id,page:pi,kind:page_kind.unwrap()});
                } else {unresolved.push(Unresolved {
                    asset: Some(id), page: pi, reason: error.to_string(),
                })},
            }
            assets.push(asset);
        }
        inherited = if layouts[pi].comparison {
            true
        } else if layouts[pi].interrupts {
            false
        } else {
            inherited
        };
    }
    // Same-source-page overlaps require a compositor/group, never two duplicate
    // crops or a draw-order guess. Preserve both source payloads unresolved.
    let mut rejected = BTreeSet::new();
    for (a, left) in assets.iter().enumerate() {
        for right in &assets[a + 1..] {
            if left.page == right.page
                && left
                    .bounds
                    .zip(right.bounds)
                    .is_some_and(|(a, b)| overlaps(a, b))
            {
                rejected.insert(left.id);
                rejected.insert(right.id);
            }
        }
    }
    plans.retain(|p| {
        if rejected.contains(&p.asset) {
            unresolved.push(Unresolved {
                asset: Some(p.asset),
                page: assets[p.asset].page,
                reason: "overlapping image sources require compositing".into(),
            });
            false
        } else {
            true
        }
    });
    // Prove an order for figures sharing one original text boundary.
    let mut groups = BTreeMap::<[usize; 7], Vec<usize>>::new();
    for p in &plans {
        let key = match p.target {
            Target::Stream { field, offset } => [1, index(field), offset, 0, 0, 0, 0],
            Target::Cell {
                field,
                page,
                region,
                line,
                row,
                col,
            } => [0, index(field), page, region, line, row, col],
        };
        groups.entry(key).or_default().push(p.asset);
    }
    let mut order = BTreeMap::new();
    for ids in groups.values() {
        if let Some(ordered) = figure_order(ids, &assets) {
            for (rank, id) in ordered.into_iter().enumerate() {
                order.insert(id, rank);
            }
        } else {
            rejected.extend(ids.iter().copied());
        }
    }
    plans.retain(|p| {
        if rejected.contains(&p.asset) {
            unresolved.push(Unresolved {
                asset: Some(p.asset),
                page: assets[p.asset].page,
                reason: "figures at one source boundary have no unique reading order".into(),
            });
            false
        } else {
            true
        }
    });
    // Cell targets first, then original text offsets in reverse order. Adding
    // figures changes neither text character counts nor the original offsets.
    plans.sort_by_key(|p| match p.target {
        Target::Cell {
            field,
            page,
            region,
            line,
            row,
            col,
        } => (
            0,
            index(field),
            page,
            region,
            line,
            row,
            col,
            order[&p.asset],
        ),
        Target::Stream { field, offset } => (
            1,
            index(field),
            usize::MAX - offset,
            usize::MAX - order[&p.asset],
            0,
            0,
            0,
            0,
        ),
    });
    let mut placements = Vec::new();
    for plan in plans {
        let applied = match plan.target {
            Target::Stream { field, offset } => {
                model::insert_stream(&mut documents[0], field, offset, plan.asset)
            }
            Target::Cell {
                field,
                page,
                region,
                line,
                row,
                col,
            } => {
                let table = base.pages[page].regions[region].lines[line]
                    .table
                    .as_ref()
                    .unwrap()
                    .to_nested();
                model::insert_cell(&mut documents[0], field, &table, row, col, plan.asset)
            }
        };
        if applied {
            placements.push(plan);
        } else {
            unresolved.push(Unresolved {
                asset: Some(plan.asset),
                page: assets[plan.asset].page,
                reason: "no unique original document container for image".into(),
            });
        }
    }
    placements.sort_by_key(|p| p.asset);
    unresolved.sort_by_key(|u| (u.page, u.asset));
    let ruby = ruby::analyze(base, &documents, &sources);
    let (parallel_bindings,unresolved_parallel)=underline_styles::bind(&styled.parallel_underlines,&styled.native,&documents,&styled.applied_fields,&sources);
    let frames=frames::bind(&enclosures,frame_unresolved,base,&documents,&sources);
    let baseline_shifts=baseline_shifts::analyze(base,&documents,&sources);
    let fonts=fonts::analyze(base,&documents,&sources,&ruby,&styled.line_projections);
    Ok(MediaAnalysis {
        schema: 1,
        kind: "source_media_analysis",
        native,
        source_layout: styled.styled,
        table_continuations,
        fonts,
        documents,
        assets,
        page_graphics:sources.iter().enumerate().map(|(page,source)|serde_json::json!({
            "page":page,"document_sha256":metadata::hex(&source.document_sha256()),
            "crop_box":source.crop_box(),"rotate":source.page().rotate,
            "font_rectangles":source.page().glyphs.iter().enumerate().filter_map(|(id,g)|{
                let font=source.page().fonts.get(g.font)?;
                if kanpo_pdf::bbox::poppler_readable(font){return None;}
                let rectangles=source.native_sources().glyph_separate_painted_rectangles(id)
                    .map(|rects|rects.to_vec()).or_else(||source.native_sources().glyph_painted_rectangle(id).map(|r|vec![r]))?;
                Some(serde_json::json!({"glyph":id,"font":g.font,"code":g.code,"cid":g.cid,"rectangles":rectangles}))
            }).collect::<Vec<_>>(),
            "paths":source.paths().iter().map(metadata::path).collect::<Vec<_>>()
        })).collect(),
        placements,
        page_placements,
        unresolved,
        underlines: UnderlineAudit {
            scope: styled.scope,
            coverage_verified: styled.coverage_verified,
            applied_fields: styled.applied_fields,
            unresolved_fields: styled.unresolved_fields,
            source_issues: styled.source_issues,
            glyph_bindings: styled.glyph_bindings,
            parallel_bindings,
            unresolved_parallel,
            unresolved_parallel_words:styled.unresolved_parallel_words,
            line_projections:styled.line_projections,
        },
        painted_text,
        ruby,
        native_glyphs,
        frames,
        baseline_shifts,
        coverage_verified: false,
    })
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
