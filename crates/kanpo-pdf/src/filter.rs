//! ストリームのフィルタ（ISO 32000-1 7.4）。テキストと罫線に要るものだけを持つ。
//! 画像用のフィルタ（DCT・JPX・CCITT・JBIG2）は扱わない。

use crate::object::{Dict, Object};
use crate::{Error, Result};

/// `filters` を順に掛けて解く。`parms` は各フィルタの `DecodeParms`。
pub(crate) fn decode(mut data: Vec<u8>, filters: &[Vec<u8>], parms: &[Option<&Dict>]) -> Result<Vec<u8>> {
    for (i, f) in filters.iter().enumerate() {
        let parm = parms.get(i).copied().flatten();
        data = match f.as_slice() {
            b"FlateDecode" | b"Fl" => predict(inflate(&data), parm)?,
            b"LZWDecode" | b"LZW" => {
                let early = parm
                    .and_then(|p| p.get(&b"EarlyChange"[..]))
                    .and_then(Object::as_i64)
                    .unwrap_or(1);
                predict(lzw(&data, early != 0), parm)?
            }
            b"ASCIIHexDecode" | b"AHx" => ascii_hex(&data),
            b"ASCII85Decode" | b"A85" => ascii85(&data),
            b"RunLengthDecode" | b"RL" => run_length(&data),
            b"Crypt" => data,
            other => return Err(Error::Unsupported(format!("filter {}", String::from_utf8_lossy(other)))),
        };
    }
    Ok(data)
}

/// zlib を解く。壊れていても解けたところまでは返す（途中で切れたストリームへの耐性）。
fn inflate(data: &[u8]) -> Vec<u8> {
    match miniz_oxide::inflate::decompress_to_vec_zlib(data) {
        Ok(v) => v,
        Err(e) if !e.output.is_empty() => e.output,
        // zlib ヘッダの無い生 deflate のこともある。
        Err(_) => miniz_oxide::inflate::decompress_to_vec(data).unwrap_or_else(|e| e.output),
    }
}

/// PNG（10〜15）・TIFF（2）予測子を戻す。
fn predict(data: Vec<u8>, parm: Option<&Dict>) -> Result<Vec<u8>> {
    let Some(parm) = parm else { return Ok(data) };
    let get = |k: &[u8], d: i64| parm.get(k).and_then(Object::as_i64).unwrap_or(d);
    let predictor = get(b"Predictor", 1);
    if predictor < 2 {
        return Ok(data);
    }
    let colors = get(b"Colors", 1).max(1) as usize;
    let bpc = get(b"BitsPerComponent", 8).max(1) as usize;
    let columns = get(b"Columns", 1).max(1) as usize;
    let bpp = (colors * bpc).div_ceil(8);
    let row_len = (colors * bpc * columns).div_ceil(8);
    if predictor == 2 {
        if bpc != 8 {
            return Err(Error::Unsupported(format!("TIFF predictor with {bpc} bits")));
        }
        let mut out = data;
        for row in out.chunks_mut(row_len) {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
        return Ok(out);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row_len];
    for chunk in data.chunks(row_len + 1) {
        let (kind, src) = (chunk[0], &chunk[1..]);
        let mut row = vec![0u8; row_len];
        for i in 0..src.len() {
            let left = if i >= bpp { row[i - bpp] } else { 0 };
            let up = prev[i];
            let up_left = if i >= bpp { prev[i - bpp] } else { 0 };
            row[i] = match kind {
                0 => src[i],
                1 => src[i].wrapping_add(left),
                2 => src[i].wrapping_add(up),
                3 => src[i].wrapping_add(((left as u16 + up as u16) / 2) as u8),
                4 => src[i].wrapping_add(paeth(left, up, up_left)),
                _ => src[i],
            };
        }
        out.extend_from_slice(&row[..src.len()]);
        prev = row;
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut hi: Option<u8> = None;
    for &b in data {
        if b == b'>' {
            break;
        }
        let v = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        match hi.take() {
            Some(h) => out.push(h << 4 | v),
            None => hi = Some(v),
        }
    }
    if let Some(h) = hi {
        out.push(h << 4);
    }
    out
}

fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut group = [0u32; 5];
    let mut n = 0;
    let mut i = 0;
    if data.starts_with(b"<~") {
        i = 2;
    }
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group[n] = (b - b'!') as u32;
                n += 1;
                if n == 5 {
                    let v = group.iter().fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
                    out.extend_from_slice(&v.to_be_bytes());
                    n = 0;
                }
            }
            _ => {}
        }
    }
    if n > 1 {
        for g in group.iter_mut().skip(n) {
            *g = 84;
        }
        let v = group.iter().fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
        out.extend_from_slice(&v.to_be_bytes()[..n - 1]);
    }
    out
}

fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let len = data[i];
        i += 1;
        match len {
            128 => break,
            0..=127 => {
                let end = (i + len as usize + 1).min(data.len());
                out.extend_from_slice(&data[i..end]);
                i = end;
            }
            _ => {
                if let Some(&b) = data.get(i) {
                    out.extend(std::iter::repeat_n(b, 257 - len as usize));
                }
                i += 1;
            }
        }
    }
    out
}

fn lzw(data: &[u8], early: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table: Vec<Vec<u8>> = (0..=255u16).map(|b| vec![b as u8]).collect();
    table.push(Vec::new()); // 256: clear
    table.push(Vec::new()); // 257: EOD
    let (mut bit_pos, mut width) = (0usize, 9usize);
    let mut prev: Option<Vec<u8>> = None;
    loop {
        if bit_pos + width > data.len() * 8 {
            break;
        }
        let mut code = 0usize;
        for k in 0..width {
            let bit = (data[(bit_pos + k) / 8] >> (7 - (bit_pos + k) % 8)) & 1;
            code = code << 1 | bit as usize;
        }
        bit_pos += width;
        match code {
            256 => {
                table.truncate(258);
                width = 9;
                prev = None;
                continue;
            }
            257 => break,
            _ => {}
        }
        let entry = if code < table.len() {
            table[code].clone()
        } else if let Some(p) = &prev {
            let mut e = p.clone();
            e.push(p[0]);
            e
        } else {
            break;
        };
        out.extend_from_slice(&entry);
        if let Some(mut p) = prev.take() {
            p.push(entry[0]);
            table.push(p);
        }
        prev = Some(entry);
        let next = table.len() + early as usize;
        width = match next {
            0..=511 => 9,
            512..=1023 => 10,
            1024..=2047 => 11,
            _ => 12,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_up_predictor() {
        // 2 列 × 2 行、各行の先頭が予測子（2 = Up）。
        let raw = vec![2, 1, 2, 2, 1, 1];
        let mut parm = Dict::new();
        parm.insert(b"Predictor".to_vec(), Object::Int(12));
        parm.insert(b"Columns".to_vec(), Object::Int(2));
        assert_eq!(predict(raw, Some(&parm)).unwrap(), vec![1, 2, 2, 3]);
    }

    #[test]
    fn ascii85_roundtrip() {
        assert_eq!(ascii85(b"87cURD]i,\"Ebo80~>"), b"Hello World!".to_vec());
    }

    #[test]
    fn inflate_zlib() {
        let z = miniz_oxide::deflate::compress_to_vec_zlib(b"BT /F1 12 Tf ET", 6);
        assert_eq!(inflate(&z), b"BT /F1 12 Tf ET".to_vec());
    }
}
