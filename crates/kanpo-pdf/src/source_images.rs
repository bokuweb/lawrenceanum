//! Source observations for named Image XObjects visited by the native interpreter.
//! The owned bundle keeps original native font evidence and image bytes together.
//! Gaps identify known traversal/geometry omissions. An empty gap list does not
//! certify all raster/body mechanisms, pixels, clipping, compositing or visibility.
use crate::{Dict, Document, ObjRef, Page, Result, SubPath};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    InlineImage,
    MissingResource,
    RecursiveForm,
    FormDepthLimit,
    FormDecodeFailure,
    PageContentFailure,
    InvalidFormMatrix,
    InvalidFormBox,
    InvalidImageDimensions,
    InvalidContentMatrix,
    StateStackUnderflow,
    UnknownXObject,
    PathGeometryIncomplete,
    InvalidPageGeometry,
    OpenPathAtForm,
    UnrepresentableGeometry,
    InvalidFormResources,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureGap {
    pub kind: GapKind,
    pub resource_name: Vec<u8>,
    pub form_path: Vec<ObjRef>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipOrigin {
    Path,
    FormBox,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipRule {
    NonZero,
    EvenOdd,
}
#[derive(Debug, Clone)]
pub struct SourceClip {
    pub origin: ClipOrigin,
    pub rule: ClipRule,
    pub subpaths: Vec<SubPath>,
    pub form_box: Option<[f64; 4]>,
    pub page_quad: Option<[[f64; 2]; 4]>,
    pub geometry_complete: bool,
}

#[derive(Debug, Clone)]
pub struct ImageDraw {
    pub clips: Vec<SourceClip>,
    pub source_geometry_complete: bool,
    pub graphics_states: Vec<Vec<u8>>,
    pub(crate) payload: Vec<u8>,
    pub(crate) declared_dict: Dict,
    pub resource_name: Vec<u8>,
    pub object_ref: Option<ObjRef>,
    pub sequence: usize,
    pub form_path: Vec<ObjRef>,
    pub pixel_width: Option<i64>,
    pub pixel_height: Option<i64>,
    pub image_to_user: [f64; 6],
    /// Existing interpreter clip bounds in display-page coordinates; not full clip geometry/visibility.
    pub page_clip_bounds: Option<[f64; 4]>,
}
impl ImageDraw {
    /// Bytes retain stream filters and have already passed Document decryption.
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.payload
    }
    pub fn encoded_sha256(&self) -> [u8; 32] {
        Sha256::digest(&self.payload).into()
    }
    /// Original stream dictionary; indirect entries remain declarations, not resolved pixels.
    pub fn declared_dictionary(&self) -> &Dict {
        &self.declared_dict
    }
}
#[derive(Debug, Clone)]
pub struct XObjectPageSources {
    pub(crate) gaps: Vec<CaptureGap>,
    pub(crate) document_digest: [u8; 32],
    pub(crate) media: [f64; 4],
    pub(crate) native: crate::Aj1PageSources,
    pub(crate) crop: [f64; 4],
    pub(crate) draws: Vec<ImageDraw>,
}
impl XObjectPageSources {
    pub fn capture_gaps(&self) -> &[CaptureGap] {
        &self.gaps
    }
    pub fn document_sha256(&self) -> [u8; 32] {
        self.document_digest
    }
    pub fn media_box(&self) -> [f64; 4] {
        self.media
    }
    /// This observer never claims final rendered visibility or an OCR/native assignment.
    pub fn visibility_verified(&self) -> bool {
        false
    }
    /// Existing source-verified native font evidence belongs to this same original page.
    pub fn native_sources(&self) -> &crate::Aj1PageSources {
        &self.native
    }
    pub fn page(&self) -> &Page {
        self.native.page()
    }
    pub fn crop_box(&self) -> [f64; 4] {
        self.crop
    }
    pub fn images(&self) -> &[ImageDraw] {
        &self.draws
    }
}
pub fn extract(pdf: &[u8]) -> Result<Vec<XObjectPageSources>> {
    let snapshot = pdf.to_vec();
    let digest: [u8; 32] = Sha256::digest(&snapshot).into();
    let d = Document::load(snapshot)?;
    Ok(d.pages()
        .iter()
        .map(|p| {
            let mut out = crate::page::extract_page_source_images(&d, p);
            out.document_digest = digest;
            out
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(pdf: &[u8]) -> Vec<XObjectPageSources> {
        let old = crate::extract_pages(pdf).unwrap();
        let new = extract(pdf).unwrap();
        assert_eq!(
            serde_json::to_string(&old).unwrap(),
            serde_json::to_string(&new.iter().map(|p| p.page()).collect::<Vec<_>>()).unwrap(),
            "all old native values exact"
        );
        new
    }
    #[test]
    fn active_partial_image_sources_keep_their_actual_matrix_and_clip() {
        let pdf = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/image-source-fixtures/20260930g002150251.pdf"),
        )
        .unwrap();
        let a = check(&pdf);
        let p = &a[0];
        assert_eq!(p.crop_box(), [0., 0., 595., 842.]);
        assert_eq!(p.images().len(), 3);
        assert_eq!(p.images()[0].resource_name, b"Im0");
        for (i, y) in [(1, 421.), (2, 53.)] {
            let g = &p.images()[i];
            assert_eq!((g.pixel_width, g.pixel_height), (Some(4192), Some(3000)));
            assert_eq!(g.image_to_user, [503.04, 0., 0., 360., 46., y]);
        }
        assert_eq!(p.images()[1].page_clip_bounds, Some([46., 61., 549., 421.]));
        assert_eq!(
            p.images()[2].page_clip_bounds,
            Some([46., 429., 549., 789.])
        );
    }
    #[test]
    fn rotated_mixed_body_stays_with_the_same_original_page() {
        let pdf = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/image-source-fixtures/20260925h017950022.pdf"),
        )
        .unwrap();
        let a = check(&pdf);
        let p = &a[0];
        assert_eq!(
            (p.page().width, p.page().height, p.page().rotate),
            (842., 595., 270)
        );
        let original = serde_json::to_string(p.page()).unwrap();
        let mut copy = p.page().clone();
        copy.glyphs.clear();
        assert_eq!(serde_json::to_string(p.page()).unwrap(), original);
        assert_eq!(p.images().len(), 1);
        let g = &p.images()[0];
        assert_eq!(g.image_to_user, [595., 0., 0., 842., 0., 0.]);
        assert_eq!((g.pixel_width, g.pixel_height), (Some(2480), Some(3509)));
    }
    pub(super) fn stream(body: &[u8], extra: &str) -> Vec<u8> {
        let mut v = format!("<< /Length {} {} >>\nstream\n", body.len(), extra).into_bytes();
        v.extend_from_slice(body);
        v.extend_from_slice(b"\nendstream");
        v
    }
    pub(super) fn pdf(objects: Vec<Vec<u8>>) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0];
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let start = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
        );
        for o in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                offsets.len(),
                start
            )
            .as_bytes(),
        );
        out
    }
    pub(super) fn objects(main: &[u8]) -> Vec<Vec<u8>> {
        vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R /Resources << /XObject << /F 5 0 R /Pic 6 0 R >> >> >>".to_vec(),stream(main,""),stream(b"q 1 0 0 1 2 3 cm /Pic Do Q","/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix [2 0 0 3 10 20] /Resources << /XObject << /Pic 6 0 R >> >>"),stream(&[255;6],"/Type /XObject /Subtype /Image /Width 2 /Height 3 /ColorSpace /DeviceGray /BitsPerComponent 8")]
    }
    #[test]
    fn repeated_same_image_has_distinct_draw_occurrences_and_q_restores_placement() {
        let b = pdf(objects(
            b"q 2 0 0 3 5 7 cm /Pic Do Q q 4 0 0 5 11 13 cm /Pic Do Q",
        ));
        let a = check(&b);
        let ds = a[0].images();
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].object_ref, ds[1].object_ref);
        assert_eq!((ds[0].sequence, ds[1].sequence), (0, 1));
        assert_eq!(ds[0].image_to_user, [2., 0., 0., 3., 5., 7.]);
        assert_eq!(ds[1].image_to_user, [4., 0., 0., 5., 11., 13.]);
    }
    #[test]
    fn form_local_transform_and_resource_scope_do_not_leak_into_later_image() {
        let b = pdf(objects(
            b"q 4 0 0 5 7 11 cm /F Do Q q 1 0 0 1 100 200 cm /Pic Do Q",
        ));
        let a = check(&b);
        let ds = a[0].images();
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].image_to_user, [8., 0., 0., 15., 63., 156.]);
        assert_eq!(ds[1].image_to_user, [1., 0., 0., 1., 100., 200.]);
        assert_eq!(ds[0].form_path.len(), 1);
        assert!(ds[1].form_path.is_empty());
    }
}

#[cfg(test)]
#[path = "source_images_evidence_tests.rs"]
mod evidence_tests;

pub(crate) fn dimension(value: Option<crate::Object>) -> Option<i64> {
    match value {
        Some(crate::Object::Int(n)) if n > 0 => Some(n),
        _ => None,
    }
}
