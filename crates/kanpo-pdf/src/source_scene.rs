//! Source paint inventory independent of horizontal text moves.
#[cfg(test)]
use crate::Aj1PageSources;
#[derive(Debug, serde::Serialize)]
pub(crate) struct Scene {
    pub version: u8,
    pub glyphs: usize,
    pub ignored: Vec<usize>,
    pub rectangle_ids: Vec<usize>,
    pub words: Vec<(usize, usize, String, [f64; 4])>,
    pub unknown: Vec<usize>,
    pub rectangles: Vec<[f64; 4]>,
    pub rulings: Vec<[f64; 4]>,
    pub rectangle_obstacles: Vec<[f64; 4]>,
    pub images: Vec<[f64; 4]>,
    pub lines: Vec<([f64; 4], [f64; 4])>,
    pub fills: Vec<[f64; 4]>,
}
#[cfg(test)]
pub(crate) fn capture(source: &Aj1PageSources) -> Option<Scene> {
    source.lineage.scene_capture(source.page())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn original() -> Aj1PageSources {
        crate::extract_pages_with_aj1_sources(
            &std::fs::read(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../kanpo-parser/tests/data/native-marker-records/source.pdf"),
            )
            .unwrap(),
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn vertical_source_does_not_require_horizontal_text_moves() {
        let s = original();
        let scene = capture(&s).expect("complete original scene");
        assert_eq!(scene.glyphs, s.page().glyphs.len());
        assert!(s.lineage.coverage_attribute(s.page()).is_none());
        assert!(scene.images.is_empty());
        assert!(!scene.rulings.is_empty());
    }

    #[test]
    fn original_unowned_vertical_ruling_is_proved_by_its_outline() {
        let s = original();
        let scene = capture(&s).unwrap();
        assert!(
            scene.rectangles.iter().any(|b| b
                .iter()
                .zip([416.8624, 87.2159, 417.3584, 95.2479])
                .all(|(a, z)| (*a - z).abs() < 0.000001)),
            "first original outline rectangle"
        );
        assert!(
            scene.unknown.is_empty(),
            "every omitted glyph needs actual outline proof"
        );
    }
    #[test]
    fn image_only_vertical_source_preserves_original_image_bounds() {
        let s = crate::extract_pages_with_aj1_sources(
            &std::fs::read(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../kanpo-parser/tests/data/native-marker-records/image-only.pdf"),
            )
            .unwrap(),
        )
        .unwrap()
        .remove(0);
        let scene = capture(&s).unwrap();
        assert_eq!(scene.images, vec![[10., 20., 40., 60.]]);
    }
    fn pdf(content: &str, extra: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        ];
        let resource = extra
            .iter()
            .enumerate()
            .map(|(i, (name, _))| format!("/{name} {} 0 R", 5 + i))
            .collect::<Vec<_>>()
            .join(" ");
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /XObject << {resource} >> >> /Contents 4 0 R >>").into_bytes());
        let stream = |bytes: &[u8], dict: &str| {
            let mut s = format!("<< /Length {} {dict} >>\nstream\n", bytes.len()).into_bytes();
            s.extend(bytes);
            s.extend(b"\nendstream");
            s
        };
        objects.push(stream(content.as_bytes(), ""));
        objects.extend(extra.iter().map(|(_, bytes)| bytes.clone()));
        let mut result = b"%PDF-1.7\n".to_vec();
        let mut offsets = vec![0];
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(result.len());
            result.extend(format!("{} 0 obj\n", i + 1).as_bytes());
            result.extend(obj);
            result.extend(b"\nendobj\n");
        }
        let start = result.len();
        result.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
        for n in &offsets[1..] {
            result.extend(format!("{n:010} 00000 n \n").as_bytes());
        }
        result.extend(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
                offsets.len()
            )
            .as_bytes(),
        );
        result
    }
    fn from_content(c: &str) -> Option<Scene> {
        capture(
            &crate::extract_pages_with_aj1_sources(&pdf(c, &[]))
                .unwrap()
                .remove(0),
        )
    }
    #[test]
    fn unknown_paint_curves_and_restored_unknown_context_do_not_certify_an_empty_scene() {
        for content in [
            "/Unknown sh",
            "10 10 m 20 20 30 20 40 10 c S",
            "q UnrecognizedPaint Q",
            "q 1 0 cm Q",
            "Q",
            "BT /Missing 8 Tf (opaque) Tj ET",
            "BI /W 1 /H 1 /BPC 8 /CS /G ID x EI",
        ] {
            assert!(from_content(content).is_none(), "{content}");
        }
    }
    #[test]
    fn no_move_path_keeps_ordinary_fills_and_strokes_as_original_paint() {
        let scene = from_content("10 20 30 40 re f 2 w 5 10 m 5 50 l S").unwrap();
        assert_eq!(scene.fills, vec![[10., 40., 40., 80.]]);
        assert_eq!(scene.lines, vec![([5., 50., 5., 90.], [4., 49., 6., 91.])]);
    }
    #[test]
    fn a_form_cannot_silently_certify_its_hidden_contents() {
        let form=b"<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length 0 >>\nstream\n\nendstream".to_vec();
        let src = crate::extract_pages_with_aj1_sources(&pdf("/F Do", &[("F", form)]))
            .unwrap()
            .remove(0);
        assert!(capture(&src).is_none());
    }
    #[test]
    fn invalid_image_transform_never_becomes_an_empty_scene() {
        let image=b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Length 1 >>\nstream\nx\nendstream".to_vec();
        for matrix in ["1 0 cm", "0 0 0 0 0 0 cm"] {
            let src = crate::extract_pages_with_aj1_sources(&pdf(
                &format!("q {matrix} /I Do Q"),
                &[("I", image.clone())],
            ))
            .unwrap()
            .remove(0);
            assert!(capture(&src).is_none(), "{matrix}");
        }
    }
}
