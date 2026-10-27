//! Original horizontal line moves; semantic grouping requires independent cell evidence.
use crate::{object::Object, page::Glyph};
#[derive(Debug, Clone, Default)]
pub(crate) struct Tape {
    edges: Vec<Edge>,
    complete: bool,
    obstacles: Vec<[f64; 4]>,
    ink: Vec<Ink>,
    scene_images: Vec<[f64;4]>,
    scene_backgrounds: Vec<(usize,BackgroundProgram)>,
    scene_rectangles: std::collections::BTreeMap<usize,[f64;4]>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edge {
    pub first: usize,
    pub at: usize,
    pub after: usize,
}
impl Tape {
    pub(crate) fn coverage_attribute(&self, page: &crate::Page) -> Option<String> {
        if !self.complete || self.edges.is_empty() {
            return None;
        }
        let mut owned = std::collections::BTreeSet::new();
        for w in crate::bbox::words(page) {
            for id in w.glyphs {
                if id >= page.glyphs.len() || !owned.insert(id) {
                    return None;
                }
            }
        }
        let ignored = page
            .glyphs
            .iter()
            .enumerate()
            .filter(|(id, g)| {
                !owned.contains(id)
                    && (g.invisible
                        || (!g.text.is_empty() && g.text.chars().all(char::is_whitespace)))
            })
            .map(|(id, _)| id.to_string())
            .collect::<Vec<_>>();
        Some(format!(
            "1 {} {}",
            page.glyphs.len(),
            if ignored.is_empty() {
                "-".into()
            } else {
                ignored.join(",")
            }
        ))
    }
    pub(crate) fn obstacle_attribute(&self) -> Option<String> {
        if !self.complete || self.edges.is_empty() {
            return None;
        }
        Some(format!(
            "1 {}",
            if self.obstacles.is_empty() {
                "-".into()
            } else {
                self.obstacles
                    .iter()
                    .map(|b| b.iter().map(f64::to_string).collect::<Vec<_>>().join(","))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        ))
    }
    pub(crate) fn edges(&self) -> &[Edge] {
        &self.edges
    }
    pub(crate) fn word_attribute(
        &self,
        page: &crate::Page,
        word: &crate::bbox::Word,
    ) -> Option<String> {
        if !self.complete {
            return None;
        }
        let (first, after) = crate::bbox::native_glyph_range(page, word)?;
        crate::bbox::native_flow_tuple(page, word)?;
        if self
            .edges
            .iter()
            .any(|e| e.first >= e.at || e.at >= e.after || e.after > page.glyphs.len())
        {
            return None;
        }
        let mut selected = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for e in &self.edges {
            if e.first < after && first < e.after {
                if !((e.first <= first && after <= e.at) || (e.at <= first && after <= e.after))
                    || !seen.insert((e.first, e.at, e.after))
                {
                    return None;
                }
                selected.push(format!("{}:{}:{}", e.first, e.at, e.after));
            }
        }
        if selected.is_empty() {
            return None;
        }
        let raw = word
            .text
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Some(format!(
            "1 {first} {after} {:.6} {:.6} {:.6} {:.6} {raw} {}",
            word.x_min,
            word.y_min,
            word.x_max,
            word.y_max,
            selected.join(" ")
        ))
    }
}
#[derive(Clone, Copy)]
pub(crate) struct State {
    pub font: usize,
    pub values: [f64; 16],
    pub origin: [f64; 2],
    pub leading: f64,
}
#[derive(Clone, Copy)]
struct Show {
    first: usize,
    after: usize,
    state: State,
}
pub(crate) struct Capture {
    tape: Tape,
    previous: Option<Show>,
    armed: bool,
    pending_images: Vec<([f64; 4], Option<crate::ObjRef>)>,
}
impl Default for Capture {
    fn default() -> Self {
        Self {
            tape: Tape {
                edges: vec![],
                complete: true,
                obstacles: vec![],
                ink: vec![],
                scene_images: vec![],
                scene_backgrounds: vec![],
                scene_rectangles: Default::default(),
            },
            previous: None,
            armed: false,
            pending_images: vec![],
        }
    }
}
impl Capture {
    pub(crate) fn image(&mut self, b: [f64; 4], background_source: Option<crate::ObjRef>) {
        if b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3] {
            self.tape.scene_images.push(b);
            self.pending_images.push((b, background_source));
        } else {
            self.incomplete();
        }
    }
    pub(crate) fn incomplete(&mut self) {
        self.tape.complete = false;
    }
    pub(crate) fn reset(&mut self) {
        self.previous = None;
        self.armed = false;
    }
    pub(crate) fn finish(mut self, doc: &crate::Document) -> Tape {
        // Keep only an eligible background program, without decoding pixels.
        if self.tape.complete {
            for (index,(_,r)) in self.pending_images.iter().enumerate(){
                if let Some(program)=r.and_then(|r|background_program(doc,r)){self.tape.scene_backgrounds.push((index,program));}
            }
        }
        // Decode only when a source move could be used; never inspect image pixels
        // on the ordinary no-move path. OCR and text recognition are not involved.
        if self.tape.complete && !self.tape.edges.is_empty() {
            for (b, r) in self.pending_images {
                if !r.is_some_and(|r| uniform_background_samples(doc, r)) {
                    self.tape.obstacles.push(b);
                }
            }
        }
        self.tape
    }
    pub(crate) fn observe(
        &mut self,
        op: &[u8],
        args: &[Object],
        first: usize,
        glyphs: &[Glyph],
        state: Option<State>,
    ) {
        let Some(state) = state.filter(|s| {
            s.values
                .iter()
                .chain(s.origin.iter())
                .all(|x| x.is_finite())
        }) else {
            self.reset();
            return;
        };
        if matches!(op, b"TD" | b"Td" | b"T*") {
            let valid = if op == b"T*" {
                args.is_empty() && state.leading.is_finite() && state.leading > 0.
            } else {
                args.len() == 2
                    && args[0].as_f64() == Some(0.)
                    && args[1].as_f64().is_some_and(|x| x.is_finite() && x < 0.)
            };
            if valid && !self.armed && self.previous.is_some() {
                self.armed = true;
            } else {
                self.reset();
            }
            return;
        }
        let valid_show = if op == b"Tj" {
            args.len() == 1 && args[0].as_str().is_some()
        } else if op == b"TJ" {
            args.len() == 1
                && args[0].as_array().is_some_and(|a| {
                    a.iter()
                        .all(|x| x.as_str().is_some() || x.as_f64().is_some_and(f64::is_finite))
                })
        } else {
            false
        };
        if !valid_show {
            self.reset();
            return;
        }
        let after = glyphs.len();
        let Some(g) = glyphs.get(first).filter(|_| first < after) else {
            self.reset();
            return;
        };
        if glyphs[first..].iter().any(|x| {
            x.invisible
                || x.vertical
                || ![x.x, x.y, x.end_x, x.end_y, x.size]
                    .iter()
                    .all(|n| n.is_finite())
                || x.size <= 0.
                || x.end_x <= x.x
                || x.end_y != x.y
                || x.y != g.y
        }) {
            self.reset();
            return;
        }
        if let Some(prev) = self.previous.filter(|p| {
            self.armed
                && p.after == first
                && p.state.font == state.font
                && p.state.values == state.values
                && p.state.origin[0] == state.origin[0]
                && p.state.origin[1] < state.origin[1]
                && glyphs[p.first].y < g.y
        }) {
            self.tape.edges.push(Edge {
                first: prev.first,
                at: first,
                after,
            });
        }
        self.previous = Some(Show {
            first,
            after,
            state,
        });
        self.armed = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input(k: &str) -> Vec<u8> {
        std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../kanpo-parser/tests/data/native-lineage160")
                .join(k),
        )
        .unwrap()
    }
    fn source(k: &str) -> crate::Aj1PageSources {
        crate::extract_pages_with_aj1_sources(&input(k))
            .unwrap()
            .remove(0)
    }
    fn words(s: &crate::Aj1PageSources, e: Edge) -> (String, String) {
        let g = &s.page().glyphs;
        let text = |a, b| g[a..b].iter().map(|x| x.text.as_str()).collect::<String>();
        (text(e.first, e.at), text(e.at, e.after))
    }
    #[test]
    fn original_TD_has_original_adjacent_show_ranges() {
        let s = source("explicit-TD160.pdf");
        assert!(s
            .lineage
            .edges()
            .iter()
            .any(|&e| words(&s, e) == ("alpha".into(), "omega".into())));
    }
    #[test]
    fn identical_public_geometry_never_manufactures_a_Tm_linefeed() {
        let a = source("explicit-TD160.pdf");
        let b = source("independent-Tm160.pdf");
        assert_eq!(
            serde_json::to_string(a.page()).unwrap(),
            serde_json::to_string(b.page()).unwrap()
        );
        assert!(b.lineage.edges().is_empty());
    }
    #[test]
    fn numeric_column_linefeed_is_a_fact_not_a_prose_group() {
        for k in ["aligned-full160.pdf", "aligned-empty-second160.pdf"] {
            let s = source(k);
            assert!(s
                .lineage
                .edges()
                .iter()
                .any(|&e| words(&s, e) == ("alpha".into(), "omega".into())));
        }
    }
    #[test]
    fn original_real_TD_and_Tstar_both_capture_their_own_source() {
        let pdf = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../kanpo-parser/tests/data/native-table-scopes/20260724g001650702.pdf"),
        )
        .unwrap();
        let s = crate::extract_pages_with_aj1_sources(&pdf)
            .unwrap()
            .remove(0);
        for e in [
            Edge {
                first: 1126,
                at: 1134,
                after: 1138,
            },
            Edge {
                first: 1144,
                at: 1152,
                after: 1156,
            },
        ] {
            assert!(s.lineage.edges().contains(&e));
        }
    }
    #[test]
    fn public_native_values_remain_exact_with_optional_capture() {
        for k in [
            "explicit-TD160.pdf",
            "independent-Tm160.pdf",
            "aligned-full160.pdf",
            "aligned-empty-second160.pdf",
        ] {
            let a = crate::extract_pages(&input(k)).unwrap();
            let b = source(k);
            assert_eq!(
                serde_json::to_string(&a[0]).unwrap(),
                serde_json::to_string(b.page()).unwrap()
            );
        }
    }
    #[test]
    fn valid_Td_and_Tstar_preserve_original_line_moves() {
        for k in ["valid-Td160.pdf", "valid-Tstar160.pdf"] {
            let s = source(k);
            assert!(s
                .lineage
                .edges()
                .iter()
                .any(|&e| words(&s, e) == ("alpha".into(), "omega".into())));
        }
    }
    #[test]
    fn diagonal_upward_zero_and_font_reset_never_borrow_a_line_move() {
        for k in [
            "diagonal-TD160.pdf",
            "upward-TD160.pdf",
            "zero-TD160.pdf",
            "font-reset160.pdf",
        ] {
            let s = source(k);
            assert!(s.lineage.edges().is_empty(), "{k}");
        }
    }
    #[test]
    fn malformed_Td_does_not_borrow_interpreter_default_zero() {
        let s = source("malformed-Td160.pdf");
        assert!(s.lineage.edges().is_empty());
    }
    #[test]
    fn malformed_Tj_with_extra_arguments_is_not_source_authority() {
        let s = source("malformed-string-show160.pdf");
        assert!(s.lineage.edges().is_empty());
    }
    #[test]
    fn malformed_TJ_adjustment_is_not_source_authority() {
        let s = source("malformed-array-show160.pdf");
        assert!(s.lineage.edges().is_empty());
    }

    #[test]
    fn transport_only_owned_native_words_carry_source_edges() {
        let s = source("explicit-TD160.pdf");
        let ws = crate::bbox::words(s.page());
        for w in ws.iter().filter(|w| w.text == "alpha" || w.text == "omega") {
            let attr = s
                .lineage
                .word_attribute(s.page(), w)
                .expect("both complete source word occurrences need their original move record");
            assert!(attr.ends_with("0:5:10"));
        }
        let side = ws.iter().find(|w| w.text == "side").unwrap();
        assert!(s.lineage.word_attribute(s.page(), side).is_none());
    }
    #[test]
    fn stale_raw_box_or_noncontiguous_glyphs_never_borrow_transport() {
        let s = source("explicit-TD160.pdf");
        let a = crate::bbox::words(s.page())
            .into_iter()
            .find(|w| w.text == "alpha")
            .unwrap();
        let mut b = a.clone();
        b.text = "other".into();
        assert!(s.lineage.word_attribute(s.page(), &b).is_none());
        b = a.clone();
        b.x_min += 1.;
        assert!(s.lineage.word_attribute(s.page(), &b).is_none());
        b = a;
        b.glyphs.remove(1);
        assert!(s.lineage.word_attribute(s.page(), &b).is_none());
    }
    #[test]
    fn source_edges_never_appear_on_legacy_or_Tm_native_words() {
        for k in ["explicit-TD160.pdf", "independent-Tm160.pdf"] {
            let s = source(k);
            let legacy = crate::bbox::bbox_layout(&[s.page().clone()]);
            assert!(!legacy.contains("data-native-line-moves"));
            let basic = crate::bbox::bbox_layout_with_aj1_sources(&[s.clone()]);
            assert!(!basic.contains("data-native-line-moves"));
            let native = crate::bbox::bbox_layout_with_native_styles(&[s]);
            assert_eq!(
                native.contains("data-native-line-moves"),
                k == "explicit-TD160.pdf"
            );
        }
    }

    #[test]
    fn native_page_coverage_declares_total_and_only_proven_ignored_glyphs() {
        let s = source("explicit-TD160.pdf");
        let t = s.lineage.coverage_attribute(s.page()).unwrap();
        assert_eq!(t, format!("1 {} -", s.page().glyphs.len()));
    }
    #[test]
    fn malformed_show_context_cannot_certify_absent_row_partners() {
        for k in [
            "malformed-Td160.pdf",
            "malformed-string-show160.pdf",
            "malformed-array-show160.pdf",
        ] {
            let s = source(k);
            assert!(s.lineage.coverage_attribute(s.page()).is_none(), "{k}");
        }
    }
}

#[cfg(test)]
mod bounded_image_tests {
    use super::*;
    fn source(k: &str) -> crate::Aj1PageSources {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join(k);
        crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0)
    }
    #[test]
    fn known_image_extent_does_not_erase_unrelated_native_source() {
        for k in ["image-outside160.pdf", "image-inside160.pdf"] {
            let s = source(k);
            assert!(s.lineage.coverage_attribute(s.page()).is_some(), "{k}");
        }
    }
    #[test]
    fn real_page_uniform_background_does_not_erase_its_native_header_flow() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-table-scopes/20260724g001650702.pdf");
        let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0);
        assert!(s.lineage.coverage_attribute(s.page()).is_some());
    }
    #[test]
    fn missing_image_resource_keeps_absence_of_source_unknown() {
        let s = source("image-unknown160.pdf");
        assert!(s.lineage.coverage_attribute(s.page()).is_none());
    }
}

#[cfg(test)]
mod uniform_image_tests {
    fn x(k: &str) -> String {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join(format!("{k}160.pdf"));
        let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0);
        s.lineage.obstacle_attribute().unwrap()
    }
    #[test]
    fn unmasked_uniform_samples_have_no_internal_spatial_text() {
        assert_eq!(x("uniform-background"), "1 -");
    }
    #[test]
    fn a_mask_or_incomplete_samples_cannot_borrow_uniformity() {
        for k in ["masked-uniform", "truncated-uniform", "image-inside"] {
            assert_ne!(x(k), "1 -", "{k}");
        }
    }
}

// A strictly decoded, unmasked constant sample tuple has no internal spatial
// structure. Unknown filters, external alternatives, masks and partial data keep
// the full image rectangle as an obstacle. No whiteness/brightness threshold.
// One last program only: bounded entry count; eviction/reordering never changes
// classification. All source dictionary/context checks precede the cache. Key
// equality uses exact decrypted encoded bytes, codec, component stride and full
// expected length, not a digest, page identifier or a known pixel value.
#[derive(Debug,Clone)]
struct BackgroundProgram {
    flate: bool,
    expected: usize,
    components: usize,
    encoded: Vec<u8>,
    uniform: bool,
}
static BACKGROUND_PROGRAM_CACHE: std::sync::OnceLock<std::sync::Mutex<Option<BackgroundProgram>>> =
    std::sync::OnceLock::new();
fn uniform_background_samples(doc: &crate::Document, r: crate::ObjRef) -> bool {
    uniform_background_samples_with_cache(doc, r, true)
}
fn background_program(doc:&crate::Document,r:crate::ObjRef)->Option<BackgroundProgram>{
    let obj = doc.resolve(&Object::Ref(r));
    let Some(s) = obj.as_stream() else {
        return None;
    };
    let d = &s.dict;
    if [
        b"SMask".as_slice(),
        b"Mask",
        b"Alternates",
        b"OPI",
        b"F",
        b"FFilter",
        b"FDecodeParms",
        b"DecodeParms",
        b"DP",
    ]
    .iter()
    .any(|k| d.contains_key(*k))
    {
        return None;
    }
    if doc
        .dict_get(d, b"ImageMask")
        .is_some_and(|o| o != Object::Bool(false))
    {
        return None;
    }
    if doc
        .dict_get(d, b"BitsPerComponent")
        .and_then(|o| o.as_i64())
        != Some(8)
    {
        return None;
    }
    let components = match doc
        .dict_get(d, b"ColorSpace")
        .as_ref()
        .and_then(Object::as_name)
    {
        Some(b"DeviceGray") => 1usize,
        Some(b"DeviceRGB") => 3,
        Some(b"DeviceCMYK") => 4,
        _ => return None,
    };
    if let Some(decode) = doc.dict_get(d, b"Decode") {
        let Some(a) = decode.as_array() else {
            return None;
        };
        if a.len() != 2 * components || !a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)) {
            return None;
        }
    }
    if let Some(filter) = doc.dict_get(d, b"Filter") {
        match filter {
            Object::Name(n) if n == b"FlateDecode" => {}
            _ => return None,
        }
    }
    let Some(w) = crate::source_images::dimension(doc.dict_get(d, b"Width"))
        .and_then(|v| usize::try_from(v).ok())
    else {
        return None;
    };
    let Some(h) = crate::source_images::dimension(doc.dict_get(d, b"Height"))
        .and_then(|v| usize::try_from(v).ok())
    else {
        return None;
    };
    let Some(expected) = w.checked_mul(h).and_then(|v| v.checked_mul(components)) else {
        return None;
    };
    let flate = doc.dict_get(d, b"Filter").is_some();

 Some(BackgroundProgram{flate,expected,components,encoded:s.data.clone(),uniform:false})
}
fn uniform_background_samples_with_cache(
    doc: &crate::Document,
    r: crate::ObjRef,
    cached: bool,
) -> bool {
    let Some(program)=background_program(doc,r)else{return false;};
    let (expected,components,flate)=(program.expected,program.components,program.flate);
    if cached {
        if let Ok(cache) = BACKGROUND_PROGRAM_CACHE
            .get_or_init(Default::default)
            .lock()
        {
            if let Some(entry) = cache.as_ref() {
                if entry.flate == flate
                    && entry.expected == expected
                    && entry.components == components
                    && entry.encoded == program.encoded
                {
                    return entry.uniform;
                }
            }
        }
    }
    let uniform = doc.resolve(&Object::Ref(r)).as_stream().and_then(|s|doc.decode_stream(s).ok()).is_some_and(|bytes| {
        bytes.len() == expected
            && bytes
                .get(..components)
                .is_some_and(|first| bytes.chunks_exact(components).all(|tuple| tuple == first))
    });
    if cached {
        if let Ok(mut cache) = BACKGROUND_PROGRAM_CACHE
            .get_or_init(Default::default)
            .lock()
        {
            *cache = Some(BackgroundProgram {
                flate,
                expected,
                components,
                encoded: program.encoded.clone(),
                uniform,
            });
        }
    }
    uniform
}
#[cfg(test)]
mod completeness_tests {
    fn source(k: &str) -> crate::Aj1PageSources {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join(format!("{k}160.pdf"));
        crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0)
    }
    #[test]
    fn restoring_a_graphics_state_never_erases_an_unknown_source_gap() {
        let s = source("unknown-restored-context");
        assert!(!s.lineage.edges().is_empty());
        assert!(s.lineage.coverage_attribute(s.page()).is_none());
    }
    #[test]
    fn an_undecoded_font_cannot_certify_a_complete_native_inventory() {
        let s = source("missing-font-source");
        assert!(!s.lineage.edges().is_empty());
        assert!(s.lineage.coverage_attribute(s.page()).is_none());
    }
    #[test]
    fn only_unowned_whitespace_or_invisible_glyphs_are_ignored() {
        for (text, invisible, ignored) in [
            (" ", false, true),
            ("x", true, true),
            ("", false, false),
            ("x", false, false),
        ] {
            let s = source("explicit-TD");
            let mut p = s.page().clone();
            let id = p.glyphs.len();
            let mut g = p.glyphs[0].clone();
            g.text = text.into();
            g.invisible = invisible;
            g.x = 40.;
            g.end_x = 45.;
            g.bbox = [40., 40., 45., 50.];
            p.glyphs.push(g);
            let a = s.lineage.coverage_attribute(&p).unwrap();
            let token = a.split_whitespace().nth(2).unwrap();
            let unowned = !crate::bbox::words(&p)
                .iter()
                .any(|w| w.glyphs.contains(&id));
            assert_eq!(
                token.split(',').any(|s| s == id.to_string()),
                ignored && unowned,
                "{text:?} {invisible}"
            );
        }
    }
}

#[cfg(test)]
mod distinct_show_controls {
    #[test]
    fn malformed_string_and_array_shows_are_distinct_source_operators() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160");
        let a = std::fs::read(root.join("malformed-string-show160.pdf")).unwrap();
        let b = std::fs::read(root.join("malformed-array-show160.pdf")).unwrap();
        assert_ne!(a, b);
        assert!(a
            .windows(b"(alpha) 7 Tj".len())
            .any(|s| s == b"(alpha) 7 Tj"));
        assert!(b
            .windows(b"[(alpha) (bad) /Unknown] TJ".len())
            .any(|s| s == b"[(alpha) (bad) /Unknown] TJ"));
    }
}

#[cfg(test)]
mod background_scope_tests {
    fn x(k: &str) -> String {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join(format!("{k}160.pdf"));
        let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0);
        s.lineage.obstacle_attribute().unwrap()
    }
    #[test]
    fn a_small_solid_fill_can_be_a_dash_or_another_local_mark() {
        assert_ne!(x("uniform"), "1 -");
    }
    #[test]
    fn a_clipped_or_later_uniform_image_is_not_a_page_background() {
        for k in ["uniform-overpaint", "uniform-clipped-background"] {
            assert_ne!(x(k), "1 -", "{k}");
        }
    }
}

#[derive(Debug, Clone)]
enum Ink {
    Line { center: [f64; 4], paint: [f64; 4] },
    Fill([f64; 4]),
}
impl Tape {
    pub(crate) fn ink_attribute(&self) -> Option<String> {
        if !self.complete || self.edges.is_empty() {
            return None;
        }
        let values = self
            .ink
            .iter()
            .map(|ink| match ink {
                Ink::Line { center, paint } => format!(
                    "L:{}",
                    center
                        .iter()
                        .chain(paint)
                        .map(f64::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                Ink::Fill(b) => format!(
                    "B:{}",
                    b.iter().map(f64::to_string).collect::<Vec<_>>().join(",")
                ),
            })
            .collect::<Vec<_>>();
        Some(format!(
            "1 {}",
            if values.is_empty() {
                "-".into()
            } else {
                values.join(" ")
            }
        ))
    }
}
impl Capture {
    pub(crate) fn paint(
        &mut self,
        paths: &[crate::SubPath],
        stroke: bool,
        fill: bool,
        width: f64,
        conformal: bool,
    ) {
        for path in paths {
            if path.curved || path.points.iter().flatten().any(|n| !n.is_finite()) {
                self.incomplete();
                return;
            }
            if path.points.is_empty() {
                continue;
            }
            let bounds = |points: &[[f64; 2]]| {
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
            };
            if fill {
                let b = bounds(&path.points);
                if b[0] < b[2] && b[1] < b[3] {
                    self.tape.ink.push(Ink::Fill(b));
                }
            }
            if !stroke {
                continue;
            }
            if !conformal || !width.is_finite() || width <= 0. {
                self.incomplete();
                return;
            }
            let mut points = path.points.clone();
            if path.closed && points.first() != points.last() {
                points.push(points[0]);
            }
            let axis = points
                .windows(2)
                .all(|p| p[0][0] == p[1][0] || p[0][1] == p[1][1]);
            if !axis && points.len() != 2 {
                self.incomplete();
                return;
            }
            if points.len() == 1 {
                points.push(points[0]);
            }
            for pair in points.windows(2) {
                let b = bounds(pair);
                let r = width / 2.;
                let paint = [b[0] - r, b[1] - r, b[2] + r, b[3] + r];
                if !paint.iter().all(|x| x.is_finite()) {
                    self.incomplete();
                    return;
                }
                if (b[0] == b[2]) != (b[1] == b[3]) {
                    self.tape.ink.push(Ink::Line { center: b, paint });
                } else {
                    self.tape.ink.push(Ink::Fill(paint));
                }
            }
        }
    }
}
#[cfg(test)]
mod source_ink_tests {
    fn source(k: &str) -> crate::Aj1PageSources {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join(format!("{k}160.pdf"));
        crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0)
    }
    #[test]
    fn original_vector_dash_and_filled_mark_are_not_absent_glyphs() {
        let a = source("vector-row-dash");
        assert_eq!(a.lineage.ink_attribute().unwrap().matches("L:").count(), 7);
        let b = source("vector-filled-row-mark");
        assert!(b.lineage.ink_attribute().unwrap().contains("B:"));
    }
    #[test]
    fn original_table_sides_keep_their_complete_source_centerlines() {
        let a = source("explicit-TD");
        assert_eq!(a.lineage.ink_attribute().unwrap().matches("L:").count(), 6);
    }
}

#[cfg(test)]
mod unknown_ink_context_tests {
    #[test]
    fn unbounded_curve_or_unverified_stroke_metric_cannot_certify_absent_marks() {
        for k in ["curve-ink-context", "nonconformal-stroke"] {
            let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../kanpo-parser/tests/data/native-lineage160")
                .join(format!("{k}160.pdf"));
            let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
                .unwrap()
                .remove(0);
            assert!(!s.lineage.edges().is_empty());
            assert!(s.lineage.coverage_attribute(s.page()).is_none(), "{k}");
            assert!(s.lineage.ink_attribute().is_none(), "{k}");
        }
    }
}

#[cfg(test)]
mod shading_gap_tests {
    #[test]
    fn an_unobserved_shading_operator_cannot_certify_missing_graphic_values() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-lineage160")
            .join("unknown-shading160.pdf");
        let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
            .unwrap()
            .remove(0);
        assert!(!s.lineage.edges().is_empty());
        assert!(s.lineage.coverage_attribute(s.page()).is_none());
    }
}

#[cfg(test)]
mod background_program_cache_tests {
    use super::*;
    #[test]
    fn repeated_actual_underlay_matches_uncached_classification() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-table-scopes/20260724g001650702.pdf");
        let d = crate::Document::load(std::fs::read(p).unwrap()).unwrap();
        let pages = d.pages();
        let x = d.dict_get(&pages[0].resources, b"XObject").unwrap();
        let r = x
            .as_dict()
            .unwrap()
            .get(b"Im0".as_slice())
            .unwrap()
            .as_ref()
            .unwrap();
        let begin = std::time::Instant::now();
        for _ in 0..8 {
            assert!(uniform_background_samples_with_cache(&d, r, false));
        }
        let old = begin.elapsed();
        assert!(uniform_background_samples(&d, r));
        let begin = std::time::Instant::now();
        for _ in 0..8 {
            assert!(uniform_background_samples(&d, r));
        }
        eprintln!(
            "actual702eight_uncached_ms={} cached_ms={}",
            old.as_secs_f64() * 1000.,
            begin.elapsed().as_secs_f64() * 1000.
        );
    }
}

#[cfg(test)]
mod program_identity_contract_tests {
    use super::*;
    fn source(k: &str) -> crate::Document {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-background-cache161")
            .join(format!("{k}161.pdf"));
        crate::Document::load(std::fs::read(p).unwrap()).unwrap()
    }
    #[test]
    fn exact_program_memoization_keeps_stride_length_codec_mask_and_changed_bytes_separate() {
        let r = crate::ObjRef { num: 6, gen: 0 };
        for (name, expected) in [
            ("rgb", true),
            ("same-length-gray", false),
            ("wrong-length", false),
            ("changed-program", false),
            ("different-codec", false),
            ("masked", false),
        ] {
            let primer = source("rgb");
            assert!(uniform_background_samples(&primer, r));
            let d = source(name);
            assert_eq!(
                uniform_background_samples_with_cache(&d, r, false),
                expected,
                "uncached {name}"
            );
            assert_eq!(
                uniform_background_samples(&d, r),
                expected,
                "cached after identical RGB byte stream {name}"
            );
            assert_eq!(
                uniform_background_samples(&primer, r),
                true,
                "a negative cache entry cannot taint the original"
            );
        }
    }
}

#[path="source_scene_capture.rs"]
mod scene;
