//! Positioned, editable OCR candidates bound to one original image draw.
//! Stream/frame correspondence is checked here. Decoder pixels, recognition,
//! cell semantics, reading order and final visibility remain unverified.
use super::{Asset, MediaAnalysis};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type Pixels = [usize; 4];
type Rect = [f64; 4];

#[path = "media_raster_characters.rs"]
pub mod characters;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub document_sha256: String,
    pub encoded_sha256: String,
    #[serde(rename = "source_PNG_sha256")]
    pub source_png_sha256: String,
    #[serde(rename = "source_RGBA_sha256")]
    pub source_rgba_sha256: String,
    pub page: usize,
    pub draw: usize,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub pixel_width: usize,
    pub pixel_height: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub pixel_width: usize,
    pub pixel_height: usize,
    pub image_width: usize,
    pub image_height: usize,
    pub recognition_to_image: [f64; 6],
    pub image_to_user: [f64; 6],
    pub crop_box: Rect,
    pub rotate: i32,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub preparation_sha256: String,
    pub ocr_index_sha256: String,
    pub recognition_parent_sha256: String,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Orientation {
    pub quarter_turns_ccw: usize,
    pub globally_resolved: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: usize,
    pub tile: usize,
    pub region: usize,
    pub text: String,
    pub confidence: f64,
    pub is_vertical: bool,
    pub source_pixels: Pixels,
    pub core: Pixels,
    pub ink_pixels: usize,
    pub ink_bounds_source_pixels: Option<Pixels>,
    pub ownership: String,
    pub word: Option<usize>,
    pub raw: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub text: String,
    pub observations: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub id: usize,
    pub text: Option<String>,
    pub alternatives: Vec<Alternative>,
    pub bounds_source_pixels: Pixels,
    pub bounds_layout_pixels: Pixels,
    pub support_sha256: String,
    pub support_pixels: usize,
    pub cell: Option<usize>,
    pub cells: Vec<usize>,
    pub writing_modes: Vec<bool>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub id: usize,
    pub interior_source_pixels: Pixels,
    pub frame_source_pixels: Rect,
    pub frame_layout_pixels: Rect,
    pub boundary_kind: String,
    pub words: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub cell: usize,
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub id: usize,
    pub cells: Vec<usize>,
    pub x_cuts: Vec<f64>,
    pub y_cuts: Vec<f64>,
    pub slots: Vec<Slot>,
    pub uncovered_slots: Vec<[usize; 2]>,
    pub overlapping_cells: Vec<[usize; 2]>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub id: usize,
    pub cell: usize,
    pub words: Vec<usize>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unresolved {
    pub reason: String,
    pub observations: Vec<usize>,
    pub words: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedTile {
    pub tile: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub schema: usize,
    pub kind: String,
    pub source: Source,
    pub canvas: Canvas,
    pub source_frame: Frame,
    pub provenance: Provenance,
    pub orientation: Orientation,
    pub observations: Vec<Observation>,
    pub runs: Vec<Value>,
    pub words: Vec<Word>,
    pub cells: Vec<Cell>,
    pub tables: Vec<Table>,
    pub lines: Vec<Line>,
    pub unresolved: Vec<Unresolved>,
    pub outside_words: Vec<usize>,
    pub failed_tiles: Vec<FailedTile>,
    pub ownership_policy: String,
    pub alias_policy: String,
    pub coverage_verified: bool,
    pub visibility: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_layer: Option<Box<characters::Layer>>,
}

fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn rect(b: Rect, w: usize, h: usize) -> bool {
    b.iter().all(|v| v.is_finite())
        && 0. <= b[0]
        && b[0] < b[2]
        && b[2] <= w as f64
        && 0. <= b[1]
        && b[1] < b[3]
        && b[3] <= h as f64
}
fn pixel(b: Pixels, w: usize, h: usize) -> bool {
    rect(b.map(|v| v as f64), w, h)
}
fn inside(a: Pixels, b: Pixels) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn transformed(b: Rect, w: usize, h: usize, t: usize) -> Rect {
    let points = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]].map(|[x, y]| match t {
        1 => [y, w as f64 - x],
        2 => [w as f64 - x, h as f64 - y],
        3 => [h as f64 - y, x],
        _ => [x, y],
    });
    [
        points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        points
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max),
        points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max),
    ]
}
fn ids(values: &[usize]) -> BTreeSet<usize> {
    values.iter().copied().collect()
}

impl Frame {
    /// Project a positioned candidate from parent raster pixels to PDF page
    /// points. Original clipping and final visibility are separate metadata.
    pub fn project(&self, bounds: Rect) -> Result<[[f64; 2]; 4]> {
        ensure!(
            rect(bounds, self.pixel_width, self.pixel_height)
                && self.image_width > 0
                && self.image_height > 0
                && [0, 90, 180, 270].contains(&self.rotate),
            "invalid raster projection input"
        );
        let [a, b, c, d, e, f] = self.recognition_to_image;
        let [aa, bb, cc, dd, ee, ff] = self.image_to_user;
        let [x0, y0, x1, y1] = self.crop_box;
        let (w, h) = (x1 - x0, y1 - y0);
        ensure!(
            [x0, y0, x1, y1, w, h].iter().all(|v| v.is_finite()) && w > 0. && h > 0.,
            "invalid raster CropBox"
        );
        let points = [
            [bounds[0], bounds[1]],
            [bounds[2], bounds[1]],
            [bounds[2], bounds[3]],
            [bounds[0], bounds[3]],
        ]
        .map(|[x, y]| {
            let u = (a * x + c * y + e) / self.image_width as f64;
            let v = (self.image_height as f64 - (b * x + d * y + f)) / self.image_height as f64;
            let x = aa * u + cc * v + ee - x0;
            let y = bb * u + dd * v + ff - y0;
            match self.rotate {
                90 => [y, x],
                180 => [w - x, y],
                270 => [h - y, w - x],
                _ => [x, h - y],
            }
        });
        ensure!(
            points.iter().flatten().all(|v| v.is_finite()),
            "raster projection overflow"
        );
        let ux = points[1][0] - points[0][0];
        let uy = points[1][1] - points[0][1];
        let vx = points[3][0] - points[0][0];
        let vy = points[3][1] - points[0][1];
        ensure!(
            (ux * vy - uy * vx).is_finite() && ux * vy - uy * vx != 0.,
            "raster projection lost its area"
        );
        Ok(points)
    }
}

impl Layout {
    /// Check serialized consistency and correspondence to the native PDF draw.
    /// This does not certify the optical text or the producer's ink pixels.
    pub fn validate_for_asset(&self, asset: &Asset) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.kind == "source_derived_raster_layout"
                && !self.coverage_verified
                && self.visibility == "unverified",
            "expected unverified source raster layout"
        );
        ensure!(
            self.ownership_policy == "actual_copy_ink_bbox_center_in_half_open_source_core"
                && self.alias_policy
                    == "exact_global_copy_ink_row_runs_and_exact_raw_text_agreement",
            "unsupported raster layout policy"
        );
        let s = &self.source;
        ensure!(
            s.document_sha256 == asset.document_sha256
                && s.encoded_sha256 == asset.encoded_sha256
                && s.page == asset.page
                && s.draw == asset.draw,
            "raster layout belongs to another original image draw"
        );
        for h in [
            &s.document_sha256,
            &s.encoded_sha256,
            &s.source_png_sha256,
            &s.source_rgba_sha256,
            &self.provenance.preparation_sha256,
            &self.provenance.ocr_index_sha256,
            &self.provenance.recognition_parent_sha256,
            &self.provenance.producer_sha256,
        ] {
            ensure!(hash(h), "invalid raster source digest");
        }
        let (w, h) = (self.canvas.pixel_width, self.canvas.pixel_height);
        let f = &self.source_frame;
        ensure!(
            w > 0
                && h > 0
                && w.checked_mul(h).is_some()
                && w <= u32::MAX as usize
                && h <= u32::MAX as usize,
            "invalid raster source canvas"
        );
        ensure!(
            f.pixel_width == w
                && f.pixel_height == h
                && f.image_width > 0
                && f.image_height > 0
                && f.rotate == asset.rotate
                && f.crop_box == asset.crop_box,
            "raster source frame differs from original page"
        );
        ensure!(
            asset.source["pixel_width"].as_u64() == Some(f.image_width as u64)
                && asset.source["pixel_height"].as_u64() == Some(f.image_height as u64),
            "raster intrinsic image dimensions differ"
        );
        let matrix = asset.source["image_to_user"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing native image matrix"))?;
        ensure!(
            matrix.len() == 6
                && matrix
                    .iter()
                    .zip(f.image_to_user)
                    .all(|(v, n)| v.as_f64() == Some(n))
                && f.image_to_user.iter().all(|v| v.is_finite()),
            "raster placement differs from original image matrix"
        );
        let [a, b, c, d, e, g] = f.recognition_to_image;
        ensure!(
            [a, b, c, d].iter().all(|v| [-1., 0., 1.].contains(v))
                && a * a + b * b == 1.
                && c * c + d * d == 1.
                && a * c + b * d == 0.
                && e.is_finite()
                && g.is_finite(),
            "unsupported source pixel permutation"
        );
        let corners = [
            [0., 0.],
            [w as f64, 0.],
            [w as f64, h as f64],
            [0., h as f64],
        ]
        .map(|[x, y]| [a * x + c * y + e, b * x + d * y + g]);
        let expected = [
            [0., 0.],
            [f.image_width as f64, 0.],
            [f.image_width as f64, f.image_height as f64],
            [0., f.image_height as f64],
        ];
        ensure!(
            corners.iter().all(|p| expected.contains(p))
                && corners
                    .iter()
                    .enumerate()
                    .all(|(i, p)| !corners[..i].contains(p)),
            "raster pixel permutation does not cover the complete original image"
        );
        f.project([0., 0., w as f64, h as f64])?;
        let t = self.orientation.quarter_turns_ccw;
        ensure!(
            t < 4 && (self.orientation.globally_resolved || t == 0),
            "unresolved orientation cannot choose a rotation"
        );
        let mut observed = BTreeSet::new();
        for (i, o) in self.observations.iter().enumerate() {
            ensure!(
                o.id == i
                    && observed.insert((o.tile, o.region))
                    && pixel(o.source_pixels, w, h)
                    && pixel(o.core, w, h)
                    && o.confidence.is_finite()
                    && (0. ..=1.).contains(&o.confidence),
                "invalid raster word observation"
            );
            let expected = if let Some(bounds) = o.ink_bounds_source_pixels {
                ensure!(
                    pixel(bounds, w, h) && inside(o.source_pixels, bounds) && o.ink_pixels > 0,
                    "invalid observed ink bounds"
                );
                ensure!(
                    o.ink_pixels <= (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]),
                    "ink count exceeds observed bounds"
                );
                let x = (bounds[0] as f64 + bounds[2] as f64) / 2.;
                let y = (bounds[1] as f64 + bounds[3] as f64) / 2.;
                if o.core[0] as f64 <= x
                    && x < (o.core[2] as f64)
                    && o.core[1] as f64 <= y
                    && y < (o.core[3] as f64)
                {
                    "owned_ink"
                } else {
                    "context_ink"
                }
            } else {
                ensure!(o.ink_pixels == 0, "empty support has nonzero ink");
                "no_copy_ink_unverified"
            };
            ensure!(
                o.ownership == expected && (o.ownership == "owned_ink") == o.word.is_some(),
                "inconsistent copy ink ownership"
            );
            ensure!(
                o.raw["text"].as_str() == Some(&o.text)
                    && o.raw["confidence"].as_f64() == Some(o.confidence)
                    && o.raw["is_vertical"].as_bool() == Some(o.is_vertical),
                "raw recognition fields were changed"
            );
        }
        let cells = self
            .cells
            .iter()
            .map(|c| (c.id, c))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            cells.len() == self.cells.len(),
            "duplicate raster cell identity"
        );
        for c in &self.cells {
            ensure!(
                pixel(c.interior_source_pixels, w, h)
                    && rect(c.frame_source_pixels, w, h)
                    && c.frame_layout_pixels == transformed(c.frame_source_pixels, w, h, t),
                "invalid raster cell geometry"
            );
            let b = c.interior_source_pixels.map(|v| v as f64);
            let f = c.frame_source_pixels;
            ensure!(
                f[0] <= b[0] && f[1] <= b[1] && b[2] <= f[2] && b[3] <= f[3],
                "cell interior escapes its boundary"
            );
            ensure!(
                [
                    "solid_source",
                    "includes_source_band_completion",
                    "includes_inferred_periodic_spans"
                ]
                .contains(&c.boundary_kind.as_str()),
                "unrecognized cell boundary evidence"
            );
        }
        let mut referenced = BTreeSet::new();
        for (i, word) in self.words.iter().enumerate() {
            ensure!(
                word.id == i
                    && word.status == "candidate_unverified"
                    && pixel(word.bounds_source_pixels, w, h)
                    && word.bounds_layout_pixels.map(|v| v as f64)
                        == transformed(word.bounds_source_pixels.map(|v| v as f64), w, h, t)
                    && hash(&word.support_sha256)
                    && word.support_pixels > 0
                    && !word.alternatives.is_empty(),
                "invalid editable raster word"
            );
            let mut texts = BTreeSet::new();
            let mut modes = BTreeSet::new();
            for alternative in &word.alternatives {
                ensure!(
                    texts.insert(&alternative.text) && !alternative.observations.is_empty(),
                    "duplicate or empty raster alternative"
                );
                for &id in &alternative.observations {
                    let o = self
                        .observations
                        .get(id)
                        .ok_or_else(|| anyhow::anyhow!("foreign raster observation"))?;
                    ensure!(
                        referenced.insert(id)
                            && o.word == Some(i)
                            && o.text == alternative.text
                            && o.ink_bounds_source_pixels == Some(word.bounds_source_pixels)
                            && o.ink_pixels == word.support_pixels,
                        "word alias differs from its source observation"
                    );
                    modes.insert(o.is_vertical);
                }
            }
            ensure!(
                if word.alternatives.len() == 1 {
                    word.text.as_ref() == Some(&word.alternatives[0].text)
                } else {
                    word.text.is_none()
                },
                "conflicting word readings cannot be silently selected"
            );
            ensure!(
                word.writing_modes.iter().copied().collect::<BTreeSet<_>>() == modes
                    && word.writing_modes.len() == modes.len(),
                "word writing modes changed"
            );
            let covering = self
                .cells
                .iter()
                .filter(|c| inside(c.interior_source_pixels, word.bounds_source_pixels))
                .collect::<Vec<_>>();
            let inner = covering
                .iter()
                .filter(|c| {
                    !covering.iter().any(|other| {
                        c.interior_source_pixels != other.interior_source_pixels
                            && inside(c.interior_source_pixels, other.interior_source_pixels)
                    })
                })
                .map(|c| c.id)
                .collect::<BTreeSet<_>>();
            ensure!(
                ids(&word.cells) == inner
                    && word.cells.len() == inner.len()
                    && word.cell
                        == if inner.len() == 1 {
                            inner.first().copied()
                        } else {
                            None
                        },
                "word cell ownership differs from source containment"
            );
        }
        ensure!(
            referenced
                == self
                    .observations
                    .iter()
                    .filter(|o| o.word.is_some())
                    .map(|o| o.id)
                    .collect(),
            "missing owned raster observations"
        );
        for c in &self.cells {
            let expected = self
                .words
                .iter()
                .filter(|w| w.cell == Some(c.id))
                .map(|w| w.id)
                .collect::<BTreeSet<_>>();
            ensure!(
                ids(&c.words) == expected && c.words.len() == expected.len(),
                "cell words changed or repeated"
            );
        }
        let outside = self
            .words
            .iter()
            .filter(|w| w.cell.is_none())
            .map(|w| w.id)
            .collect::<BTreeSet<_>>();
        ensure!(
            ids(&self.outside_words) == outside && self.outside_words.len() == outside.len(),
            "outside raster words were lost"
        );
        self.validate_tables(&cells)?;
        let mut line_words = BTreeSet::new();
        for (i, line) in self.lines.iter().enumerate() {
            ensure!(
                line.id == i && cells.contains_key(&line.cell) && !line.words.is_empty(),
                "invalid raster line"
            );
            for &id in &line.words {
                ensure!(
                    self.words
                        .get(id)
                        .is_some_and(|w| w.cell == Some(line.cell))
                        && line_words.insert(id),
                    "line words are missing, repeated or in another cell"
                );
            }
            ensure!(
                [
                    "ordered_disjoint_horizontal_words",
                    "reading_order_unresolved"
                ]
                .contains(&line.status.as_str()),
                "unsupported raster line status"
            );
            if line.status == "ordered_disjoint_horizontal_words" {
                let words = line
                    .words
                    .iter()
                    .map(|&id| &self.words[id])
                    .collect::<Vec<_>>();
                ensure!(
                    self.orientation.globally_resolved
                        && words
                            .iter()
                            .all(|w| w.text.is_some() && w.writing_modes == [false])
                        && words.iter().map(|w| w.bounds_layout_pixels[1]).max()
                            < words.iter().map(|w| w.bounds_layout_pixels[3]).min()
                        && words.windows(2).all(|pair| pair[0].bounds_layout_pixels[2]
                            <= pair[1].bounds_layout_pixels[0]),
                    "unproven raster line order"
                );
            }
        }
        ensure!(
            line_words
                == self
                    .words
                    .iter()
                    .filter(|w| w.cell.is_some())
                    .map(|w| w.id)
                    .collect(),
            "cell words lack line records"
        );
        for u in &self.unresolved {
            ensure!(
                !u.reason.is_empty()
                    && u.observations.iter().all(|&i| i < self.observations.len())
                    && u.words.iter().all(|&i| i < self.words.len()),
                "foreign unresolved raster reference"
            );
        }
        if let Some(layer) = &self.character_layer {
            layer.validate(self)?;
        }
        Ok(())
    }

    fn validate_tables(&self, cells: &BTreeMap<usize, &Cell>) -> Result<()> {
        let mut used = BTreeSet::new();
        for (i, table) in self.tables.iter().enumerate() {
            ensure!(
                table.id == i
                    && table.status == "geometric_candidates_unverified"
                    && !table.cells.is_empty()
                    && table.x_cuts.len() >= 2
                    && table.y_cuts.len() >= 2
                    && table
                        .x_cuts
                        .windows(2)
                        .all(|p| p[0].is_finite() && p[0] < p[1])
                    && table
                        .y_cuts
                        .windows(2)
                        .all(|p| p[0].is_finite() && p[0] < p[1])
                    && table.x_cuts.last().unwrap().is_finite()
                    && table.y_cuts.last().unwrap().is_finite(),
                "invalid candidate raster table"
            );
            ensure!(
                ids(&table.cells).len() == table.cells.len()
                    && table.slots.len() == table.cells.len(),
                "repeated or missing raster cells"
            );
            let mut slot_ids = BTreeSet::new();
            let mut occupied = BTreeMap::new();
            let mut overlaps = BTreeSet::new();
            let mut x_cuts = Vec::new();
            let mut y_cuts = Vec::new();
            for slot in &table.slots {
                let cell = cells
                    .get(&slot.cell)
                    .ok_or_else(|| anyhow::anyhow!("foreign raster cell"))?;
                ensure!(
                    slot_ids.insert(slot.cell)
                        && table.cells.contains(&slot.cell)
                        && used.insert(slot.cell),
                    "repeated raster table cell"
                );
                let r = slot
                    .row
                    .checked_add(slot.rowspan)
                    .ok_or_else(|| anyhow::anyhow!("raster rowspan overflow"))?;
                let c = slot
                    .col
                    .checked_add(slot.colspan)
                    .ok_or_else(|| anyhow::anyhow!("raster colspan overflow"))?;
                ensure!(
                    slot.rowspan > 0
                        && slot.colspan > 0
                        && r < table.y_cuts.len()
                        && c < table.x_cuts.len(),
                    "invalid raster cell span"
                );
                ensure!(
                    cell.frame_layout_pixels
                        == [
                            table.x_cuts[slot.col],
                            table.y_cuts[slot.row],
                            table.x_cuts[c],
                            table.y_cuts[r]
                        ],
                    "raster slot differs from actual cell boundaries"
                );
                x_cuts.extend([cell.frame_layout_pixels[0], cell.frame_layout_pixels[2]]);
                y_cuts.extend([cell.frame_layout_pixels[1], cell.frame_layout_pixels[3]]);
                for y in slot.row..r {
                    for x in slot.col..c {
                        let others = occupied.entry((y, x)).or_insert_with(Vec::<usize>::new);
                        for &other in others.iter() {
                            overlaps.insert([other.min(slot.cell), other.max(slot.cell)]);
                        }
                        others.push(slot.cell);
                    }
                }
            }
            x_cuts.sort_by(f64::total_cmp);
            x_cuts.dedup_by(|a, b| a == b);
            y_cuts.sort_by(f64::total_cmp);
            y_cuts.dedup_by(|a, b| a == b);
            ensure!(
                table.x_cuts == x_cuts && table.y_cuts == y_cuts,
                "raster table invents source boundary cuts"
            );
            let holes = (0..table.y_cuts.len() - 1)
                .flat_map(|y| (0..table.x_cuts.len() - 1).map(move |x| [y, x]))
                .filter(|p| !occupied.contains_key(&(p[0], p[1])))
                .collect::<BTreeSet<_>>();
            ensure!(
                table
                    .uncovered_slots
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    == holes
                    && table.uncovered_slots.len() == holes.len(),
                "uncovered raster slots were filled or removed"
            );
            ensure!(
                table
                    .overlapping_cells
                    .iter()
                    .map(|p| [p[0].min(p[1]), p[0].max(p[1])])
                    .collect::<BTreeSet<_>>()
                    == overlaps,
                "overlapping raster cells were hidden"
            );
        }
        ensure!(
            used == cells.keys().copied().collect(),
            "raster cells missing from editable tables"
        );
        Ok(())
    }
}

/// Attach all reports atomically after checking original draw identities.
/// Native documents, image streams and placements are retained unchanged.
pub fn attach(analysis: &mut MediaAnalysis, layouts: Vec<Layout>) -> Result<()> {
    let mut targets = BTreeSet::new();
    let mut pending = Vec::new();
    for layout in layouts {
        let candidates = analysis
            .assets
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.document_sha256 == layout.source.document_sha256
                    && a.page == layout.source.page
                    && a.draw == layout.source.draw
            })
            .collect::<Vec<_>>();
        ensure!(
            candidates.len() == 1,
            "raster report has no unique original image draw"
        );
        let (index, asset) = candidates[0];
        ensure!(
            targets.insert(index) && asset.raster_layout.is_none(),
            "multiple raster layouts for one original image draw"
        );
        layout.validate_for_asset(asset)?;
        pending.push((index, layout));
    }
    for (index, layout) in pending {
        analysis.assets[index].raster_layout = Some(Box::new(layout));
    }
    Ok(())
}

#[cfg(test)]
#[path = "media_raster_tests.rs"]
mod tests;
