//! Native scene capture uses original private paint, not horizontal move edges.
use super::*;

impl Capture {
    pub(crate) fn scene_rectangle(&mut self, id: usize, b: Option<[f64; 4]>) {
        if let Some(b) = b {
            self.tape.scene_rectangles.insert(id, b);
        }
    }
}
impl Tape {
    #[cfg(test)]
    pub(crate) fn scene_capture(&self, page: &crate::Page) -> Option<crate::source_scene::Scene> {
        self.scene_capture_with_words(page, &crate::bbox::words(page))
    }
    fn scene_capture_with_words(
        &self,
        page: &crate::Page,
        native_words: &[crate::bbox::Word],
    ) -> Option<crate::source_scene::Scene> {
        if !self.complete {
            return None;
        }
        let mut owned = std::collections::BTreeSet::new();
        for word in native_words {
            for &id in &word.glyphs {
                if id >= page.glyphs.len() || !owned.insert(id) {
                    return None;
                }
            }
        }
        let mut unknown = vec![];
        let mut rectangles = vec![];
        let mut ignored = vec![];
        let mut rectangle_ids = vec![];
        let mut word_obstacles=vec![];
        let words = native_words
            .iter()
            .map(|w| {
                let (a,z)=if let Some(range)=crate::bbox::native_glyph_range(page,w){range}else{
                    let o=crate::bbox::native_source_occurrence(page,w)?;
                    word_obstacles.push((o.first,o.after,o.bounds));
                    (o.first,o.after)
                };
                Some((
                    a,
                    z,
                    w.text.clone(),
                    [w.x_min, w.y_min, w.x_max, w.y_max]
                        .map(|x| format!("{x:.6}").parse::<f64>().ok())
                        .into_iter()
                        .collect::<Option<Vec<_>>>()?
                        .try_into()
                        .ok()?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        for (id, g) in page.glyphs.iter().enumerate() {
            if owned.contains(&id) {
                continue;
            }
            // A Unicode whitespace value is not evidence of an empty painted outline.
            if g.invisible {
                ignored.push(id);
                continue;
            }
            if let Some(b) = self.scene_rectangles.get(&id) {
                rectangles.push(*b);
                rectangle_ids.push(id);
            } else {
                unknown.push(id);
            }
        }
        let mut lines = vec![];
        let mut fills = vec![];
        for ink in &self.ink {
            match ink {
                Ink::Line { center, paint } => lines.push((*center, *paint)),
                Ink::Fill(b) => fills.push(*b),
            }
        }
        let mut rulings = vec![];
        let mut rectangle_obstacles = vec![];
        let mut rects = vec![None; page.glyphs.len()];
        for (&id, &rect) in rectangle_ids.iter().zip(&rectangles) {
            rects[id] = Some(rect);
        }
        let mut at = 0;
        while at < rectangle_ids.len() {
            let mut end = at + 1;
            while end < rectangle_ids.len() && rectangle_ids[end - 1] + 1 == rectangle_ids[end] {
                end += 1;
            }
            let ids = rectangle_ids[at..end].to_vec();
            let text = ids
                .iter()
                .map(|&id| crate::bbox::glyph_text(&page.glyphs[id], &page.fonts))
                .collect::<String>();
            let w = crate::bbox::Word {
                text,
                x_min: 0.,
                y_min: 0.,
                x_max: 0.,
                y_max: 0.,
                glyphs: ids,
            };
            if let Some((_, rect)) = crate::source_ruling::word_ruling(page, &w, &rects) {
                rulings.push(rect);
            } else {
                rectangle_obstacles.extend_from_slice(&rectangles[at..end]);
            }
            at = end;
        }
        Some(crate::source_scene::Scene {
            version: if word_obstacles.is_empty(){1}else{2},
            glyphs: page.glyphs.len(),
            ignored,
            rectangle_ids,
            words,
            word_obstacles,
            unknown,
            rectangles,
            rulings,
            rectangle_obstacles,
            images: self
                .scene_images
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    !self
                        .scene_backgrounds
                        .iter()
                        .any(|(id, p)| i == id && uniform_scene_program(p))
                })
                .map(|(_, b)| *b)
                .collect(),
            lines,
            fills,
        })
    }
}

impl Tape {
    pub(crate) fn scene_attribute(
        &self,
        page: &crate::Page,
        words: &[crate::bbox::Word],
    ) -> Option<String> {
        let scene = self.scene_capture_with_words(page, words)?;
        if !scene.unknown.is_empty() {
            return None;
        }
        serde_json::to_string(&scene).ok()
    }
}

// Scene programs retain only exact encoded bytes and validated dictionary data;
// pixel decoding is lazy and never occurs in ordinary source extraction.

static SCENE_BACKGROUND_CACHE: std::sync::OnceLock<std::sync::Mutex<Option<BackgroundProgram>>> =
    std::sync::OnceLock::new();
#[cfg(test)]
thread_local! {static SCENE_PROGRAM_REQUESTS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
fn uniform_scene_program(program: &BackgroundProgram) -> bool {
    #[cfg(test)]
    SCENE_PROGRAM_REQUESTS.with(|n| n.set(n.get() + 1));
    let same = |a: &BackgroundProgram| {
        a.flate == program.flate
            && a.expected == program.expected
            && a.components == program.components
            && a.encoded == program.encoded
    };
    if let Ok(cache) = SCENE_BACKGROUND_CACHE.get_or_init(Default::default).lock() {
        if let Some(a) = cache.as_ref().filter(|a| same(a)) {
            return a.uniform;
        }
    }
    // Unlike the legacy tolerant stream reader, a partial/invalid zlib program
    // does not certify absent spatial structure.
    let bytes = if program.flate {
        miniz_oxide::inflate::decompress_to_vec_zlib(&program.encoded).ok()
    } else {
        Some(program.encoded.clone())
    };
    let uniform = bytes.is_some_and(|b| {
        b.len() == program.expected
            && crate::uniform_samples::constant(&b, program.components)
    });
    if let Ok(mut cache) = SCENE_BACKGROUND_CACHE.get_or_init(Default::default).lock() {
        let mut entry = program.clone();
        entry.uniform = uniform;
        *cache = Some(entry);
    }
    uniform
}

#[cfg(test)]
mod scene_background_controls {
    use super::*;
    fn program(bytes: &[u8], components: usize) -> BackgroundProgram {
        BackgroundProgram {
            flate: false,
            expected: bytes.len(),
            components,
            encoded: bytes.to_vec(),
            uniform: false,
        }
    }
    #[test]
    fn constant_component_tuples_have_no_brightness_or_known_value_rule() {
        for pixel in [[0, 0, 0], [17, 39, 221], [255, 255, 255]] {
            assert!(uniform_scene_program(&program(&pixel.repeat(5), 3)));
        }
        assert!(!uniform_scene_program(&program(&[1, 2, 3, 1, 2, 4], 3)));
    }
    #[test]
    fn cache_identity_keeps_stride_length_and_codec() {
        let a = program(&[1, 2, 1, 2], 2);
        assert!(uniform_scene_program(&a));
        let mut b = a.clone();
        b.components = 1;
        assert!(!uniform_scene_program(&b));
        let mut b = a.clone();
        b.expected += 1;
        assert!(!uniform_scene_program(&b));
        let mut b = a.clone();
        b.flate = true;
        assert!(!uniform_scene_program(&b));
    }
    #[test]
    fn incomplete_or_corrupt_zlib_never_proves_an_empty_background() {
        let encoded = miniz_oxide::deflate::compress_to_vec_zlib(&[17; 64], 6);
        let valid = BackgroundProgram {
            flate: true,
            expected: 64,
            components: 1,
            encoded: encoded.clone(),
            uniform: false,
        };
        assert!(uniform_scene_program(&valid));
        for bytes in [encoded[..encoded.len() - 1].to_vec(), {
            let mut b = encoded.clone();
            *b.last_mut().unwrap() ^= 1;
            b
        }] {
            let mut bad = valid.clone();
            bad.encoded = bytes;
            assert!(!uniform_scene_program(&bad));
        }
    }
    #[test]
    fn ordinary_source_extraction_keeps_background_pixels_lazy() {
        SCENE_PROGRAM_REQUESTS.with(|n| n.set(0));
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-marker-records/source.pdf");
        let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0);
        SCENE_PROGRAM_REQUESTS.with(|n| assert_eq!(n.get(), 0));
        assert!(s.lineage.scene_capture(s.page()).unwrap().images.is_empty());
        SCENE_PROGRAM_REQUESTS.with(|n| assert!(n.get() > 0));
    }
}
