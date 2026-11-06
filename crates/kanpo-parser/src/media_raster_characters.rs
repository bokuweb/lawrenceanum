//! Character alternatives retain model scalar ranges and source positions.
//! Ink counts and recognition remain producer observations, not optical proof.
use super::{hash, ids, inside, pixel, Canvas, Layout, Pixels, Source};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: usize,
    pub observation: usize,
    pub candidate: usize,
    pub character: usize,
    pub unicode_scalar_offset: usize,
    pub unicode_scalar_length: usize,
    pub text: String,
    pub quad_source_pixels: Option<[[f64; 2]; 4]>,
    pub source_pixels: Option<Pixels>,
    pub glyph_right_axis: Option<usize>,
    pub ink_pixels: usize,
    pub ink_bounds_source_pixels: Option<Pixels>,
    pub support_sha256: Option<String>,
    pub cells: Vec<usize>,
    pub cell: Option<usize>,
    pub containing_words: Vec<usize>,
    pub overlapping_words: Vec<usize>,
    pub geometry_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub observation: usize,
    pub candidate: usize,
    pub text: String,
    pub confidence: f64,
    pub characters: Vec<usize>,
    pub fragments: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    pub id: usize,
    pub observation: usize,
    pub candidate: usize,
    pub characters: Vec<usize>,
    pub text: String,
    pub cell: Option<usize>,
    pub glyph_right_axis: Option<usize>,
    pub bounds_source_pixels: Option<Pixels>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unresolved {
    pub reason: String,
    pub characters: Vec<usize>,
    pub fragments: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub schema: usize,
    pub kind: String,
    pub source: Source,
    pub canvas: Canvas,
    pub recognition_parent_sha256: String,
    pub report_sha256: String,
    pub raw_report: Value,
    pub implementation_sha256: BTreeMap<String, String>,
    pub characters: Vec<Character>,
    pub candidates: Vec<Candidate>,
    pub fragments: Vec<Fragment>,
    pub unresolved: Vec<Unresolved>,
    pub association_policy: String,
    pub fragment_policy: String,
    pub coverage_verified: bool,
    pub visibility: String,
}

fn whitespace(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

fn overlaps(a: Pixels, b: Pixels) -> bool {
    a[0].max(b[0]) < a[2].min(b[2]) && a[1].max(b[1]) < a[3].min(b[3])
}

fn union(boxes: impl Iterator<Item = Pixels>) -> Option<Pixels> {
    boxes.reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}

fn character_groups(characters: &[Character], refs: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut pending = Vec::new();
    let mut previous = None;
    for &id in refs {
        let character = &characters[id];
        if whitespace(&character.text) {
            pending.push(id);
            continue;
        }
        let key = (
            character.cell,
            character.glyph_right_axis,
            character.geometry_status.as_str(),
        );
        if !groups.is_empty() && previous == Some(key) {
            let group = groups.last_mut().unwrap();
            group.append(&mut pending);
            group.push(id);
        } else {
            if !pending.is_empty() {
                groups.push(std::mem::take(&mut pending));
            }
            groups.push(vec![id]);
        }
        previous = Some(key);
    }
    if !pending.is_empty() {
        groups.push(pending);
    }
    groups
}

fn geometry(raw: &Value, w: usize, h: usize) -> Option<(usize, [[f64; 2]; 4], Pixels)> {
    let quad: [[f64; 2]; 4] = serde_json::from_value(raw["quad"].clone()).ok()?;
    let points = quad.map(|[x, y]| [x * w as f64, (1. - y) * h as f64]);
    if points.iter().any(|[x, y]| {
        !x.is_finite()
            || !y.is_finite()
            || *x < -0.5
            || *y < -0.5
            || *x > w as f64 + 0.5
            || *y > h as f64 + 0.5
    }) {
        return None;
    }
    let axis = |a: usize, b: usize| {
        let dx = points[b][0] - points[a][0];
        let dy = points[b][1] - points[a][1];
        if dx.abs() == dy.abs() {
            None
        } else if dx.abs() > dy.abs() {
            Some(if dx > 0. { 0 } else { 2 })
        } else {
            Some(if dy > 0. { 1 } else { 3 })
        }
    };
    let top = axis(0, 1)?;
    let bottom = axis(3, 2)?;
    let left = axis(0, 3)?;
    let right = axis(1, 2)?;
    if top != bottom || left != right || left != (top + 1) % 4 {
        return None;
    }
    let min_x = points
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.);
    let min_y = points
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.);
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(w as f64);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(h as f64);
    let bounds = [
        min_x as usize,
        min_y as usize,
        max_x as usize,
        max_y as usize,
    ];
    pixel(bounds, w, h).then_some((top, points, bounds))
}

impl Layer {
    /// Check complete raw candidate/range preservation and source associations.
    /// Original copy pixels must also be checked by the producer/decoder path.
    pub fn validate(&self, layout: &Layout) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.kind == "source_derived_raster_characters"
                && !self.coverage_verified
                && self.visibility == "unverified"
                && self.source == layout.source
                && self.canvas == layout.canvas
                && self.recognition_parent_sha256 == layout.provenance.recognition_parent_sha256
                && hash(&self.report_sha256),
            "character layer differs from original raster source"
        );
        ensure!(
            self.association_policy == "complete_copy_ink_bounds_in_unique_innermost_source_cell"
                && self.fragment_policy
                    == "raw_character_ranges_split_at_supported_cell_or_axis_changes",
            "unsupported character association policy"
        );
        let names = [
            "raster_characters.py",
            "raster_layout.py",
            "raster_orientation.py",
            "source_gutters.py",
        ];
        ensure!(
            self.implementation_sha256.len() == names.len()
                && names
                    .iter()
                    .all(|n| self.implementation_sha256.get(*n).is_some_and(|v| hash(v))),
            "missing character producer identity"
        );
        let raw = &self.raw_report;
        ensure!(
            raw["schema"] == 1
                && raw["kind"] == "source_derived_raster_orientation"
                && raw["source"] == serde_json::to_value(&self.source)?
                && raw["recognition_parent_sha256"] == self.recognition_parent_sha256,
            "raw character report belongs to another source"
        );
        let page = &raw["vision"]["page"];
        ensure!(
            page["revision"] == 3 && page["language_correction"] == false,
            "expected uncorrected character observations"
        );
        let observations = page["observations"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing raw character observations"))?;
        let (w, h) = (self.canvas.pixel_width, self.canvas.pixel_height);
        let mut candidate_index = 0;
        let mut character_index = 0;
        let mut used_fragments = BTreeSet::new();
        let mut expected_issues = BTreeSet::new();
        for (oi, observation) in observations.iter().enumerate() {
            let candidates = observation["candidates"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing raw character candidates"))?;
            ensure!(!candidates.is_empty(), "empty raw character candidates");
            for (rank, raw_candidate) in candidates.iter().enumerate() {
                let candidate = self
                    .candidates
                    .get(candidate_index)
                    .ok_or_else(|| anyhow::anyhow!("missing character candidate"))?;
                candidate_index += 1;
                ensure!(
                    candidate.observation == oi
                        && candidate.candidate == rank
                        && raw_candidate["text"].as_str() == Some(&candidate.text)
                        && raw_candidate["confidence"].as_f64() == Some(candidate.confidence)
                        && candidate.confidence.is_finite()
                        && (0. ..=1.).contains(&candidate.confidence),
                    "character candidate text, rank or confidence changed"
                );
                let ranges = raw_candidate["characters"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("missing raw character ranges"))?;
                ensure!(
                    candidate.characters.len() == ranges.len(),
                    "raw character ranges were dropped"
                );
                let mut cursor = 0;
                let mut text = String::new();
                for (ci, raw_character) in ranges.iter().enumerate() {
                    let character = self
                        .characters
                        .get(character_index)
                        .ok_or_else(|| anyhow::anyhow!("missing source character"))?;
                    ensure!(
                        character.id == character_index
                            && candidate.characters[ci] == character_index
                            && character.observation == oi
                            && character.candidate == rank
                            && character.character == ci
                            && raw_character["text"].as_str() == Some(&character.text)
                            && !character.text.is_empty()
                            && raw_character["unicode_scalar_offset"].as_u64()
                                == Some(cursor as u64)
                            && character.unicode_scalar_offset == cursor
                            && raw_character["unicode_scalar_length"].as_u64()
                                == Some(character.text.chars().count() as u64)
                            && character.unicode_scalar_length == character.text.chars().count(),
                        "raw character text or scalar range changed"
                    );
                    cursor += character.unicode_scalar_length;
                    text.push_str(&character.text);
                    let geometry = geometry(raw_character, w, h);
                    let space = whitespace(&character.text);
                    let status = if space {
                        "whitespace_position_unverified"
                    } else if geometry.is_some() {
                        "source_character_quad_candidate"
                    } else {
                        "invalid_or_unavailable_quad"
                    };
                    ensure!(
                        character.geometry_status == status,
                        "character geometry status changed"
                    );
                    if let Some((axis, quad, bounds)) = geometry {
                        ensure!(
                            character.glyph_right_axis == Some(axis)
                                && character.quad_source_pixels == Some(quad)
                                && character.source_pixels == Some(bounds),
                            "character geometry differs from raw source quadrilateral"
                        );
                    } else {
                        ensure!(
                            character.glyph_right_axis.is_none()
                                && character.quad_source_pixels.is_none()
                                && character.source_pixels.is_none(),
                            "missing character geometry borrowed a position"
                        );
                    }
                    if let Some(bounds) = character.ink_bounds_source_pixels {
                        ensure!(
                            character.source_pixels.is_some_and(|b| inside(b, bounds))
                                && pixel(bounds, w, h)
                                && character.ink_pixels > 0
                                && character.ink_pixels
                                    <= (bounds[2] - bounds[0]) * (bounds[3] - bounds[1])
                                && character.support_sha256.as_ref().is_some_and(|s| hash(s)),
                            "invalid character copy ink support"
                        );
                    } else {
                        ensure!(
                            character.ink_pixels == 0 && character.support_sha256.is_none(),
                            "empty character support gained ink"
                        );
                    }
                    let mut inner = BTreeSet::new();
                    let mut containing = BTreeSet::new();
                    if let Some(bounds) = character.ink_bounds_source_pixels.filter(|_| !space) {
                        let covering = layout
                            .cells
                            .iter()
                            .filter(|c| inside(c.interior_source_pixels, bounds))
                            .collect::<Vec<_>>();
                        inner = covering
                            .iter()
                            .filter(|c| {
                                !covering.iter().any(|o| {
                                    c.interior_source_pixels != o.interior_source_pixels
                                        && inside(
                                            c.interior_source_pixels,
                                            o.interior_source_pixels,
                                        )
                                })
                            })
                            .map(|c| c.id)
                            .collect();
                        containing = layout
                            .words
                            .iter()
                            .filter(|word| inside(word.bounds_source_pixels, bounds))
                            .map(|word| word.id)
                            .collect();
                    }
                    ensure!(
                        ids(&character.cells) == inner
                            && character.cells.len() == inner.len()
                            && character.cell
                                == if inner.len() == 1 {
                                    inner.first().copied()
                                } else {
                                    None
                                }
                            && ids(&character.containing_words) == containing
                            && character.containing_words.len() == containing.len(),
                        "character source cell or word association changed"
                    );
                    ensure!(
                        ids(&character.overlapping_words).len()
                            == character.overlapping_words.len()
                            && character.overlapping_words.iter().all(|&id| !containing
                                .contains(&id)
                                && layout.words.get(id).is_some_and(|word| !space
                                    && character
                                        .ink_bounds_source_pixels
                                        .is_some_and(|b| overlaps(b, word.bounds_source_pixels)))),
                        "foreign partial character/word association"
                    );
                    let reason = if status != "source_character_quad_candidate" {
                        Some(status)
                    } else if character.ink_pixels == 0 {
                        Some("no_copy_ink_in_character_rectangle")
                    } else if inner.len() > 1 {
                        Some("ambiguous_character_cell_containment")
                    } else if character.cell.is_none()
                        && character.ink_bounds_source_pixels.is_some_and(|b| {
                            layout
                                .cells
                                .iter()
                                .any(|c| overlaps(c.interior_source_pixels, b))
                        })
                    {
                        Some("character_crosses_candidate_cell_boundary")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        expected_issues.insert((reason.to_string(), character.id));
                    }
                    character_index += 1;
                }
                let groups = character_groups(&self.characters, &candidate.characters);
                ensure!(
                    text == candidate.text && candidate.fragments.len() == groups.len(),
                    "fragments lost candidate text or boundaries"
                );
                for (&id, group) in candidate.fragments.iter().zip(groups) {
                    let fragment = self
                        .fragments
                        .get(id)
                        .ok_or_else(|| anyhow::anyhow!("foreign character fragment"))?;
                    ensure!(
                        fragment.id == id
                            && used_fragments.insert(id)
                            && fragment.observation == oi
                            && fragment.candidate == rank
                            && fragment.characters == group
                            && fragment.status == "model_sequence_fragment_candidate",
                        "character fragment ranges changed"
                    );
                    let members = group
                        .iter()
                        .map(|&i| &self.characters[i])
                        .collect::<Vec<_>>();
                    let supported = members
                        .iter()
                        .copied()
                        .filter(|c| !whitespace(&c.text))
                        .collect::<Vec<_>>();
                    ensure!(
                        fragment.text
                            == members.iter().map(|c| c.text.as_str()).collect::<String>()
                            && fragment.cell == supported.first().and_then(|c| c.cell)
                            && fragment.glyph_right_axis
                                == supported.first().and_then(|c| c.glyph_right_axis)
                            && fragment.bounds_source_pixels
                                == union(
                                    supported.iter().filter_map(|c| c.ink_bounds_source_pixels)
                                ),
                        "character fragment text or placement changed"
                    );
                }
            }
        }
        ensure!(
            candidate_index == self.candidates.len()
                && character_index == self.characters.len()
                && used_fragments.len() == self.fragments.len(),
            "foreign or omitted character candidate material"
        );
        let mut issues = BTreeSet::new();
        for issue in &self.unresolved {
            ensure!(
                issue.characters.len() == 1
                    && issue.fragments.is_empty()
                    && issues.insert((issue.reason.clone(), issue.characters[0])),
                "foreign or repeated unresolved character reference"
            );
        }
        ensure!(
            issues == expected_issues,
            "unresolved character geometry was hidden or invented"
        );
        Ok(())
    }
}
