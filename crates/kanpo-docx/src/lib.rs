//! 官報の PDF を DOCX にする。
//!
//! 1. [`ocr_images`] でページに置かれた画像から認識対象を取り出す。
//! 2. 呼出元が画像を OCR し（ndlocr-lite-rs。CLI はネイティブ、ブラウザは WASM）、画像の画素の枠で認識領域を返す。
//!    [`ocr_page`] がページの画素の枠（[`kanpo_parser::OcrPage`]）に写す。NDLOCR の trace JSON は
//!    [`kanpo_parser::OcrPage::from_ndlocr_trace`] で読める。
//! 3. [`convert`] が PDF の字と OCR の結果を合わせて解析し（[`kanpo_parser::analyze_item_with_ocr`]）、記事ごとに
//!    段落・表・新旧対照表を DOCX に書く。OCR を渡さなければ PDF の字と元画像で書く。
//!
//! I/O を持たないので WASM でも使える（`kanpo-docx-wasm`）。

mod annotations;
mod borders;
mod continuations;
mod decorations;
pub mod docx;
mod formalities;
mod indents;
pub mod images;
mod layout;
mod pictures;
mod comparison_geometry;
pub(crate) mod painted;
mod paragraph_gaps;
mod spacing;

pub use images::{page_images, GrayImage, PageImage};
pub use kanpo_parser::{OcrPage, OcrRegion};

use anyhow::Result;

/// 画像を OCR する手段。認識領域の枠は画像の画素（左上原点、`[x0, y0, x1, y1]`）。
pub trait OcrEngine {
    fn recognize(&mut self, image: &PageImage) -> Result<Vec<OcrRegion>>;
}

/// 画像ごとの認識領域（画像の画素の枠）を、ページの画素の枠（最初の画像の解像度で、表示したページ全体を描いた画素）に
/// 写して 1 ページの [`OcrPage`] にまとめる。`images` は同じページの画像。
pub fn ocr_page(images: &[(&PageImage, Vec<OcrRegion>)]) -> Option<OcrPage> {
    let (first, _) = images.first()?;
    let s = first.scale;
    if s <= 0.0 {
        return None;
    }
    let mut page = OcrPage {
        width: first.page_width * s,
        height: first.page_height * s,
        regions: Vec::new(),
    };
    for (image, regions) in images {
        let k = s / image.scale;
        for r in regions {
            let [x0, y0, x1, y1] = r.bbox;
            let bbox = [
                image.left * s + x0 * k,
                image.top * s + y0 * k,
                image.left * s + x1 * k,
                image.top * s + y1 * k,
            ];
            page.regions.push(OcrRegion {
                text: r.text.clone(),
                bbox,
                vertical: r.vertical,
            });
        }
    }
    Some(page)
}

/// PDF のページ数。
pub fn page_count(pdf: &[u8]) -> Result<usize> {
    Ok(kanpo_pdf::Document::load(pdf.to_vec())?.pages().len())
}

/// OCRにかける画像。原本のセルに一意に対応する図を除き、OCR用の解像度に縮小する。
pub fn ocr_images(pdf: &[u8]) -> Result<Vec<PageImage>> {
    let originals = page_images(pdf)?;
    // A figure bound to one original table cell is retained as a figure in
    // both modes. Incidental lettering inside a diagram or coin must not
    // replace that cell's original picture with running OCR prose.
    let figures = if originals.is_empty() {
        Vec::new()
    } else {
        pictures::table_figures(pdf)?
    };
    let images: Vec<PageImage> = originals
        .iter()
        .filter(|image| {
            !figures
                .iter()
                .any(|(page, b)| pictures::same_image(image, *page, *b))
        })
        .map(PageImage::for_ocr)
        .collect();
    Ok(images)
}

/// Recognize text-bearing page images; figures in certified table cells remain
/// original pictures in both native and browser conversions.
pub fn recognize_pages(pdf: &[u8], engine: &mut dyn OcrEngine) -> Result<Vec<Option<OcrPage>>> {
    let images = ocr_images(pdf)?;
    let pages = page_count(pdf)?;
    let mut per_page: Vec<Vec<(&PageImage, Vec<OcrRegion>)>> = vec![Vec::new(); pages];
    for image in &images {
        let regions = engine.recognize(image)?;
        if let Some(slot) = per_page.get_mut(image.page) {
            slot.push((image, regions));
        }
    }
    Ok(per_page.iter().map(|imgs| ocr_page(imgs)).collect())
}

/// PDF の字と、ページごとの OCR 結果（`ocr[i]` は 0 始まりの `i` ページ目、短ければ残りは `None`）を合わせて解析する。
pub fn analyze(pdf: &[u8], ocr: &[Option<OcrPage>]) -> Result<kanpo_parser::Analysis> {
    let pages = kanpo_parser::page_inputs(pdf)?;
    let mut ocr = ocr.to_vec();
    ocr.resize(pages.len(), None);
    Ok(kanpo_parser::analyze_item_with_ocr(&pages, &ocr))
}

/// PDF を DOCX のバイト列にする。`ocr` は [`analyze`] と同じ。
pub fn convert(pdf: &[u8], ocr: &[Option<OcrPage>]) -> Result<Vec<u8>> {
    let mut analysis = analyze(pdf, ocr)?;
    let source = kanpo_parser::media::analyze_pdf(pdf)?;
    let fields = [
        kanpo_parser::media::Field::Paragraph,
        kanpo_parser::media::Field::After,
        kanpo_parser::media::Field::Before,
    ];
    // The source layout removes only proven nontext drawing glyphs. OCR may
    // add text; in that case transfer only complete unchanged source fields.
    if analysis.articles.len() == source.native.articles.len()
        && fields.iter().all(|&f| {
            kanpo_parser::source_underlines::field_text(&analysis, f)
                == kanpo_parser::source_underlines::field_text(&source.native, f)
        })
    {
        analysis = source.source_layout.clone();
    } else {
        for field in fields {
            kanpo_parser::source_underlines::transfer_field(
                &source.source_layout,
                &mut analysis,
                field,
            );
        }
    }
    let (documents, pictures) = pictures::prepare_source(pdf, &analysis, &source)?;
    let mut styles = decorations::SourceStyles::from_media(&source, pdf)?;
    styles.rebind_fonts(pdf, &analysis, &documents, &source)?;
    let painted = painted_insertions(pdf, &documents);
    docx::write_documents(&analysis, &documents, &pictures, &styles, &painted)
}

/// Readings of characters painted as shapes (`painted`), bound to the source
/// line seam between their two native Words by `painted_text::analyze_pdf`,
/// and placed at that seam's unique position in the document field. A reading
/// that cannot be placed exactly once is dropped; the source image of the
/// paint is not part of the Word output either way.
fn painted_insertions(
    pdf: &[u8],
    documents: &[kanpo_parser::media::Document],
) -> Vec<(kanpo_parser::media::Field, usize, String)> {
    use kanpo_parser::{media::Field, trace::Role};
    let Ok(readings) = painted::readings(pdf) else { return Vec::new() };
    if readings.is_empty() {
        return Vec::new();
    }
    let Some(digest) = kanpo_pdf::extract_pages_with_xobject_sources(pdf)
        .ok()
        .and_then(|s| s.first().map(|p| p.document_sha256().iter().map(|b| format!("{b:02x}")).collect::<String>()))
    else {
        return Vec::new();
    };
    let report = serde_json::json!({
        "schema": 1,
        "kind": "source_font_compound_observations",
        "source": {"document_sha256": digest, "document_bytes": pdf.len()},
        "observations": readings.iter().map(|r| serde_json::json!({
            "page": r.page,
            "before_glyph": r.gap.before_glyph,
            "after_glyph": r.gap.after_glyph,
            "gap": r.gap.gap,
            "path_ids": r.gap.path_ids,
            "text": r.text,
            "status": "candidate_unverified",
        })).collect::<Vec<_>>(),
    });
    let Ok(bound) = kanpo_parser::painted_text::analyze_pdf(pdf, report) else { return Vec::new() };
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let mut out = Vec::new();
    for b in &bound.bindings {
        let Some(region) = bound.native.pages.get(b.page).and_then(|p| p.regions.get(b.region)) else { continue };
        let Some(line) = region.lines.get(b.line) else { continue };
        let field = match region.role {
            Role::After => Field::After,
            Role::Before => Field::Before,
            _ => Field::Paragraph,
        };
        let Some(head) = line.text.get(..b.byte_offset) else { continue };
        let offset = compact(head).chars().count();
        let whole = compact(&line.text).chars().collect::<Vec<_>>();
        if whole.is_empty() {
            continue;
        }
        let stream = documents.iter().map(|d| d.field_text(field)).collect::<String>().chars().collect::<Vec<_>>();
        let starts = (0..stream.len().saturating_sub(whole.len() - 1))
            .filter(|&i| stream[i..i + whole.len()] == whole[..])
            .collect::<Vec<_>>();
        if let [start] = starts[..] {
            out.push((field, start + offset, readings[b.observation].text.clone()));
        }
    }
    out
}

/// `engine` で画像を OCR して PDF を DOCX にする（[`recognize_pages`] + [`convert`]）。
pub fn convert_with(pdf: &[u8], engine: &mut dyn OcrEngine) -> Result<Vec<u8>> {
    let ocr = recognize_pages(pdf, engine)?;
    convert(pdf, &ocr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, left: f64, top: f64, scale: f64) -> PageImage {
        PageImage {
            page: 0,
            image: GrayImage {
                width: w,
                height: h,
                pixels: vec![0; (w * h) as usize],
            },
            left,
            top,
            scale,
            page_width: 595.0,
            page_height: 842.0,
        }
    }

    #[test]
    fn a_full_page_image_keeps_its_pixel_boxes() {
        // 2480 画素の幅の画像が A4 のページ全体に置かれる（300dpi 相当）。
        let s = 2480.0 / 595.0;
        let img = image(2480, 3509, 0.0, 0.0, s);
        let page = ocr_page(&[(
            &img,
            vec![OcrRegion {
                text: "官報".into(),
                bbox: [100.0, 200.0, 300.0, 240.0],
                vertical: false,
            }],
        )])
        .unwrap();
        assert!((page.width - 2480.0).abs() < 1e-6 && (page.height - 842.0 * s).abs() < 1e-6);
        assert_eq!(page.regions[0].bbox, [100.0, 200.0, 300.0, 240.0]);
    }

    #[test]
    fn a_partial_image_is_moved_to_its_place_on_the_page() {
        // 右上（x=297.5pt、y=0）に置いた、1pt あたり 2 画素の画像。最初の画像（1pt あたり 4 画素）の解像度に揃える。
        let full = image(2380, 3368, 0.0, 0.0, 4.0);
        let part = image(595, 842, 297.5, 0.0, 2.0);
        let page = ocr_page(&[
            (&full, vec![]),
            (
                &part,
                vec![OcrRegion {
                    text: "x".into(),
                    bbox: [0.0, 10.0, 100.0, 20.0],
                    vertical: true,
                }],
            ),
        ])
        .unwrap();
        assert_eq!(page.regions[0].bbox, [1190.0, 20.0, 1390.0, 40.0]);
        assert!(page.regions[0].vertical);
    }
}
