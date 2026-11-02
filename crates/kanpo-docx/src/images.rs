//! ページに置かれた画像（スキャンした本文）を取り出し、8 bit グレースケールに復号する。
//!
//! 官報の PDF の画像は、FlateDecode の 8 bit・1 bit グレースケール、CCITTFaxDecode（G4）の 1 bit、
//! DCTDecode（JPEG）のグレースケールで組まれる（原本 3000 冊分を調べた）。どのページにも、ほぼ白い
//! 2480×3509 の画像が 1 枚置かれるので、字の無い画像（黒い画素が [`MIN_INK`] 未満）は除く。

use anyhow::{anyhow, bail, Context, Result};
use kanpo_pdf::{Dict, Object};

/// 8 bit グレースケールの画像。左上原点、1 画素 1 バイト（0 が黒、255 が白）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// ページに置かれた画像を、ページの表示の向き（`/Rotate` を適用した後）に起こしたものと、その置き場所。
/// 横に寝かせて組んだ画像（`/Rotate 270` のページなど）も、字が正立するように回してある。
#[derive(Debug, Clone)]
pub struct PageImage {
    /// 0 始まりの PDF のページ番号。
    pub page: usize,
    /// 表示の向きの画像。
    pub image: GrayImage,
    /// 画像の左上の、表示したページでの位置（pt、左上原点）。
    pub left: f64,
    pub top: f64,
    /// 1 pt あたりの画像の画素数。
    pub scale: f64,
    /// 表示したページの幅・高さ（pt）。
    pub page_width: f64,
    pub page_height: f64,
}

/// 字のある画像とみなす、黒い画素（128 未満）の割合の下限。
pub const MIN_INK: f64 = 0.0005;

/// PDF の全ページから、字のある画像を取り出す。復号できない形式の画像は飛ばす。
pub fn page_images(pdf: &[u8]) -> Result<Vec<PageImage>> {
    let mut out = Vec::new();
    for (page, sources) in kanpo_pdf::extract_pages_with_xobject_sources(pdf)?.iter().enumerate() {
        let device = device_matrix(sources.page().rotate, sources.crop_box());
        for draw in sources.images() {
            let Ok(image) = decode(draw.declared_dictionary(), draw.encoded_bytes(), draw.pixel_width, draw.pixel_height) else { continue };
            if ink(&image) < MIN_INK {
                continue;
            }
            let Some(upright) = upright(&image, concat(draw.image_to_user, device)) else { continue };
            out.push(PageImage { page, page_width: sources.page().width, page_height: sources.page().height, ..upright });
        }
    }
    Ok(out)
}

/// 利用者空間から表示したページ（左上原点、y は下向き）への変換。kanpo-pdf のページ座標と同じ。
fn device_matrix(rotate: i32, crop: [f64; 4]) -> [f64; 6] {
    let [x0, y0, x1, y1] = crop;
    match rotate {
        90 => [0.0, 1.0, 1.0, 0.0, -y0, -x0],
        180 => [-1.0, 0.0, 0.0, 1.0, x1, -y0],
        270 => [0.0, -1.0, -1.0, 0.0, y1, x1],
        _ => [1.0, 0.0, 0.0, -1.0, -x0, y1],
    }
}

/// `m` の後に `n` を掛けた変換（PDF の行列の並び `[a b c d e f]`）。
fn concat(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

/// 画像の単位正方形から表示したページへの変換 `m` で、画像を表示の向きに起こす（最近傍）。画素の大きさは、
/// 画像の横の向きの 1 画素の長さを保つ。回転・反転だけでなく斜めに置いた画像も、外接する枠に描く。
fn upright(image: &GrayImage, m: [f64; 6]) -> Option<PageImage> {
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 {
        return None;
    }
    let at = |u: f64, v: f64| (m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5]);
    let corners = [at(0.0, 0.0), at(1.0, 0.0), at(0.0, 1.0), at(1.0, 1.0)];
    let (left, top) = (corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min), corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min));
    let (right, bottom) = (corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max), corners.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max));
    // 画像の横の 1 画素の長さ（pt）。
    let scale = w / m[0].hypot(m[1]);
    let (ow, oh) = (((right - left) * scale).round() as usize, ((bottom - top) * scale).round() as usize);
    if ow == 0 || oh == 0 || ow * oh > 200_000_000 {
        return None;
    }
    let mut pixels = vec![255u8; ow * oh];
    for oy in 0..oh {
        for ox in 0..ow {
            // 出力の画素の中心 → 表示したページ → 画像の単位正方形（v は上向き）→ 画像の画素（左上原点）。
            let (x, y) = (left + (ox as f64 + 0.5) / scale - m[4], top + (oy as f64 + 0.5) / scale - m[5]);
            let u = (m[3] * x - m[2] * y) / det;
            let v = (-m[1] * x + m[0] * y) / det;
            let (px, py) = ((u * w).floor(), ((1.0 - v) * h).floor());
            if px >= 0.0 && py >= 0.0 && px < w && py < h {
                pixels[oy * ow + ox] = image.pixels[py as usize * image.width as usize + px as usize];
            }
        }
    }
    Some(PageImage { page: 0, image: GrayImage { width: ow as u32, height: oh as u32, pixels }, left, top, scale, page_width: 0.0, page_height: 0.0 })
}

/// 黒い画素（128 未満）の割合。
pub fn ink(image: &GrayImage) -> f64 {
    if image.pixels.is_empty() {
        return 0.0;
    }
    image.pixels.iter().filter(|&&p| p < 128).count() as f64 / image.pixels.len() as f64
}

fn name(o: &Object) -> Option<&[u8]> {
    o.as_name()
}

/// 画像の XObject の辞書と、フィルタを掛けたままのバイト列を 8 bit グレースケールにする。
pub fn decode(dict: &Dict, data: &[u8], width: Option<i64>, height: Option<i64>) -> Result<GrayImage> {
    let int = |key: &[u8]| dict.get(key).and_then(Object::as_i64);
    let width = width.or_else(|| int(b"Width")).ok_or_else(|| anyhow!("image without Width"))? as usize;
    let height = height.or_else(|| int(b"Height")).ok_or_else(|| anyhow!("image without Height"))? as usize;
    if width == 0 || height == 0 || width * height > 200_000_000 {
        bail!("unsupported image size {width}x{height}");
    }
    let filters: Vec<&[u8]> = match dict.get(&b"Filter"[..]) {
        Some(Object::Array(a)) => a.iter().filter_map(name).collect(),
        Some(o) => name(o).into_iter().collect(),
        None => vec![],
    };
    let parms: Vec<Option<&Dict>> = match dict.get(&b"DecodeParms"[..]) {
        Some(Object::Array(a)) => a.iter().map(Object::as_dict).collect(),
        Some(o) => vec![o.as_dict()],
        None => vec![],
    };
    let image_mask = matches!(dict.get(&b"ImageMask"[..]), Some(Object::Bool(true)));
    let bpc = if image_mask { 1 } else { int(b"BitsPerComponent").unwrap_or(8) as usize };
    let components = match dict.get(&b"ColorSpace"[..]).and_then(name) {
        Some(b"DeviceRGB") => 3,
        Some(b"DeviceCMYK") => 4,
        _ => 1,
    };
    // Decode [1 0]（とマスク画像）は 0 と 1 を入れ替える。
    let inverted = image_mask || matches!(dict.get(&b"Decode"[..]), Some(Object::Array(d)) if d.first().and_then(Object::as_f64) == Some(1.0));

    let mut bytes = data.to_vec();
    for (k, filter) in filters.iter().enumerate() {
        let parm = parms.get(k).copied().flatten();
        match *filter {
            b"FlateDecode" | b"Fl" => {
                bytes = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes).map_err(|e| anyhow!("inflate: {e:?}"))?;
                bytes = predict(bytes, parm, components, bpc, width)?;
            }
            b"DCTDecode" | b"DCT" => return jpeg(&bytes, width, height),
            b"CCITTFaxDecode" | b"CCF" => return ccitt(&bytes, parm, width, height, inverted),
            other => bail!("unsupported image filter {}", String::from_utf8_lossy(other)),
        }
    }
    samples(&bytes, width, height, components, bpc, inverted)
}

/// 生の標本（`components` 色、`bpc` bit）をグレースケールにする。
fn samples(bytes: &[u8], width: usize, height: usize, components: usize, bpc: usize, inverted: bool) -> Result<GrayImage> {
    let row = (width * components * bpc).div_ceil(8);
    if bytes.len() < row * height {
        bail!("image data too short: {} < {}", bytes.len(), row * height);
    }
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        let line = &bytes[y * row..(y + 1) * row];
        for x in 0..width {
            let v = match (bpc, components) {
                (8, 1) => line[x],
                (8, 3) => {
                    let p = &line[x * 3..x * 3 + 3];
                    ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8
                }
                (8, 4) => {
                    let p = &line[x * 4..x * 4 + 4];
                    let k = u32::from(p[3]);
                    let c = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
                    (255 - (c + k).min(255)) as u8
                }
                (1, 1) => {
                    if line[x / 8] >> (7 - x % 8) & 1 == 1 {
                        255
                    } else {
                        0
                    }
                }
                _ => bail!("unsupported image format: {components} components x {bpc} bit"),
            };
            pixels.push(if inverted { 255 - v } else { v });
        }
    }
    Ok(GrayImage { width: width as u32, height: height as u32, pixels })
}

/// FlateDecode の予測子（PNG の 10〜15、TIFF の 2）を戻す。
fn predict(data: Vec<u8>, parm: Option<&Dict>, components: usize, bpc: usize, width: usize) -> Result<Vec<u8>> {
    let get = |key: &[u8], default: i64| parm.and_then(|p| p.get(key)).and_then(Object::as_i64).unwrap_or(default);
    let predictor = get(b"Predictor", 1);
    if predictor < 2 {
        return Ok(data);
    }
    let colors = get(b"Colors", components as i64).max(1) as usize;
    let bits = get(b"BitsPerComponent", bpc as i64).max(1) as usize;
    let columns = get(b"Columns", width as i64).max(1) as usize;
    let bpp = (colors * bits).div_ceil(8).max(1);
    let row = (colors * bits * columns).div_ceil(8);
    if predictor == 2 {
        if bits != 8 {
            bail!("TIFF predictor with {bits} bit samples");
        }
        let mut out = data;
        for line in out.chunks_mut(row) {
            for i in bpp..line.len() {
                line[i] = line[i].wrapping_add(line[i - bpp]);
            }
        }
        return Ok(out);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        let (kind, src) = (chunk[0], &chunk[1..]);
        let mut line = vec![0u8; row];
        for i in 0..src.len().min(row) {
            let a = if i >= bpp { line[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            line[i] = src[i].wrapping_add(match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => {
                    let p = i16::from(a) + i16::from(b) - i16::from(c);
                    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                k => bail!("unknown PNG predictor {k}"),
            });
        }
        out.extend_from_slice(&line);
        prev = line;
    }
    Ok(out)
}

fn jpeg(data: &[u8], width: usize, height: usize) -> Result<GrayImage> {
    let options = zune_jpeg::zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::Luma);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(data, options);
    let pixels = decoder.decode().map_err(|e| anyhow!("jpeg: {e:?}"))?;
    let (w, h) = decoder.dimensions().map(|(w, h)| (w, h)).unwrap_or((width, height));
    if pixels.len() != w * h {
        bail!("jpeg: {} bytes for {w}x{h}", pixels.len());
    }
    Ok(GrayImage { width: w as u32, height: h as u32, pixels })
}

/// CCITTFaxDecode（G4 と G3）を戻す。`BlackIs1` が偽（既定）なら 0 の bit が黒。
fn ccitt(data: &[u8], parm: Option<&Dict>, width: usize, height: usize, inverted: bool) -> Result<GrayImage> {
    let get = |key: &[u8], default: i64| parm.and_then(|p| p.get(key)).and_then(Object::as_i64).unwrap_or(default);
    let k = get(b"K", 0);
    let columns = get(b"Columns", 1728).max(1) as usize;
    let rows = get(b"Rows", height as i64).max(0) as usize;
    let black_is_1 = matches!(parm.and_then(|p| p.get(&b"BlackIs1"[..])), Some(Object::Bool(true)));
    if k >= 0 {
        bail!("CCITT G3 (K={k}) is not supported");
    }
    let width32 = u32::try_from(columns).context("CCITT columns")?;
    let mut pixels = Vec::with_capacity(columns * rows.max(height));
    fax::decoder::decode_g4(data.iter().copied(), width32, Some(rows.max(1) as u32), |transitions| {
        let mut color = fax::Color::White;
        let mut x = 0usize;
        let mut line = vec![255u8; columns];
        for &t in transitions {
            let t = (t as usize).min(columns);
            if color == fax::Color::Black {
                line[x..t].fill(0);
            }
            x = t;
            color = !color;
        }
        if color == fax::Color::Black {
            line[x..].fill(0);
        }
        pixels.extend_from_slice(&line);
    })
    .ok_or_else(|| anyhow!("CCITT G4 decode failed"))?;
    // BlackIs1 と Decode [1 0] は、復号した白黒を入れ替える。
    if black_is_1 != inverted {
        for p in &mut pixels {
            *p = 255 - *p;
        }
    }
    let h = pixels.len() / columns;
    if h == 0 {
        bail!("CCITT image has no rows");
    }
    let _ = width;
    Ok(GrayImage { width: columns as u32, height: h as u32, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(entries: &[(&str, Object)]) -> Dict {
        entries.iter().map(|(k, v)| (k.as_bytes().to_vec(), v.clone())).collect()
    }

    #[test]
    fn flate_gray_with_png_up_predictor_is_restored() {
        // 2x2: 1 行目 [10, 20]、2 行目は Up 予測子で差 [1, 1] → [11, 21]。
        let raw = [2u8, 10, 20, 2, 1, 1];
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let parms = dict(&[("Predictor", Object::Int(12)), ("Columns", Object::Int(2)), ("Colors", Object::Int(1))]);
        let d = dict(&[
            ("Width", Object::Int(2)),
            ("Height", Object::Int(2)),
            ("BitsPerComponent", Object::Int(8)),
            ("ColorSpace", Object::Name(b"DeviceGray".to_vec())),
            ("Filter", Object::Name(b"FlateDecode".to_vec())),
            ("DecodeParms", Object::Dict(parms)),
        ]);
        let img = decode(&d, &z, None, None).unwrap();
        assert_eq!(img.pixels, vec![10, 20, 11, 21]);
    }

    #[test]
    fn one_bit_gray_maps_zero_to_black_unless_decode_inverts() {
        let raw = [0b1000_0000u8];
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let base = vec![
            ("Width", Object::Int(2)),
            ("Height", Object::Int(1)),
            ("BitsPerComponent", Object::Int(1)),
            ("ColorSpace", Object::Name(b"DeviceGray".to_vec())),
            ("Filter", Object::Name(b"FlateDecode".to_vec())),
        ];
        assert_eq!(decode(&dict(&base), &z, None, None).unwrap().pixels, vec![255, 0]);
        let mut inv = base.clone();
        inv.push(("Decode", Object::Array(vec![Object::Int(1), Object::Int(0)])));
        assert_eq!(decode(&dict(&inv), &z, None, None).unwrap().pixels, vec![0, 255]);
    }

    #[test]
    fn a_page_rotated_270_turns_a_sideways_image_upright() {
        // 2×1 の画像（左が黒、右が白）を、595×842 のページ全体に置き、ページは /Rotate 270。
        let image = GrayImage { width: 2, height: 1, pixels: vec![0, 255] };
        let m = concat([595.0, 0.0, 0.0, 842.0, 0.0, 0.0], device_matrix(270, [0.0, 0.0, 595.0, 842.0]));
        let up = upright(&image, m).unwrap();
        // 表示したページは横長（842×595）。画像の左（利用者空間の左）は表示の下に来る。
        assert_eq!((up.image.width, up.image.height), (3, 2), "scale 2/595 px per pt");
        assert_eq!((up.left, up.top), (0.0, 0.0));
        let px = |x: usize, y: usize| up.image.pixels[y * 3 + x];
        assert_eq!((px(1, 0), px(1, 1)), (255, 0), "{:?}", up.image.pixels);
    }

    #[test]
    fn nearly_white_images_have_no_ink() {
        let img = GrayImage { width: 100, height: 100, pixels: vec![255; 10_000] };
        assert!(ink(&img) < MIN_INK);
    }
}
