//! 官報の PDF を DOCX にする。
//!
//! 1. [`page_images`] でページに置かれた画像（スキャンした本文）を取り出す。
//! 2. 呼出元が画像を OCR し（ndlocr-lite-rs。CLI はネイティブ、ブラウザは WASM）、画像の画素の枠で認識領域を返す。
//!    [`ocr_page`] がページの画素の枠（[`kanpo_parser::OcrPage`]）に写す。NDLOCR の trace JSON は
//!    [`kanpo_parser::OcrPage::from_ndlocr_trace`] で読める。
//! 3. [`convert`] が PDF の字と OCR の結果を合わせて解析し（[`kanpo_parser::analyze_item_with_ocr`]）、記事ごとに
//!    段落・表・新旧対照表を DOCX に書く。OCR を渡さなければ PDF の字だけで書く。
//!
//! I/O を持たないので WASM でも使える（`kanpo-docx-wasm`）。

pub mod docx;
pub mod images;

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
    let mut page = OcrPage { width: first.page_width * s, height: first.page_height * s, regions: Vec::new() };
    for (image, regions) in images {
        let k = s / image.scale;
        for r in regions {
            let [x0, y0, x1, y1] = r.bbox;
            let bbox = [image.left * s + x0 * k, image.top * s + y0 * k, image.left * s + x1 * k, image.top * s + y1 * k];
            page.regions.push(OcrRegion { text: r.text.clone(), bbox, vertical: r.vertical });
        }
    }
    Some(page)
}

/// PDF のページ数。
pub fn page_count(pdf: &[u8]) -> Result<usize> {
    Ok(kanpo_pdf::Document::load(pdf.to_vec())?.pages().len())
}

/// PDF のページ数分の OCR 結果を、`engine` で画像を認識して作る。字のある画像の無いページは `None`。
pub fn recognize_pages(pdf: &[u8], engine: &mut dyn OcrEngine) -> Result<Vec<Option<OcrPage>>> {
    let images = page_images(pdf)?;
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
    docx::write(&analyze(pdf, ocr)?)
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
        PageImage { page: 0, image: GrayImage { width: w, height: h, pixels: vec![0; (w * h) as usize] }, left, top, scale, page_width: 595.0, page_height: 842.0 }
    }

    #[test]
    fn a_full_page_image_keeps_its_pixel_boxes() {
        // 2480 画素の幅の画像が A4 のページ全体に置かれる（300dpi 相当）。
        let s = 2480.0 / 595.0;
        let img = image(2480, 3509, 0.0, 0.0, s);
        let page = ocr_page(&[(&img, vec![OcrRegion { text: "官報".into(), bbox: [100.0, 200.0, 300.0, 240.0], vertical: false }])]).unwrap();
        assert!((page.width - 2480.0).abs() < 1e-6 && (page.height - 842.0 * s).abs() < 1e-6);
        assert_eq!(page.regions[0].bbox, [100.0, 200.0, 300.0, 240.0]);
    }

    #[test]
    fn a_partial_image_is_moved_to_its_place_on_the_page() {
        // 右上（x=297.5pt、y=0）に置いた、1pt あたり 2 画素の画像。最初の画像（1pt あたり 4 画素）の解像度に揃える。
        let full = image(2380, 3368, 0.0, 0.0, 4.0);
        let part = image(595, 842, 297.5, 0.0, 2.0);
        let page = ocr_page(&[(&full, vec![]), (&part, vec![OcrRegion { text: "x".into(), bbox: [0.0, 10.0, 100.0, 20.0], vertical: true }])]).unwrap();
        assert_eq!(page.regions[0].bbox, [1190.0, 20.0, 1390.0, 40.0]);
        assert!(page.regions[0].vertical);
    }
}
