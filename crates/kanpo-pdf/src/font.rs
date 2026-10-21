//! フォント（ISO 32000-1 9.6〜9.10）。字形は描かないので、文字コードの切り分け・CID・
//! 字幅（横書き・縦書き）・Unicode への対応だけを持つ。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::cmap::CMap;
use crate::document::Document;
use crate::object::{Dict, Object};

/// Adobe-Japan1 の CID → Unicode（`data/aj1-unicode.bin`、`examples/gen_aj1.rs` で生成）。
static AJ1_BYTES: &[u8] = include_bytes!("../data/aj1-unicode.bin");

pub fn aj1(cid: u32) -> Option<char> {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        AJ1_BYTES
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    });
    t.get(cid as usize)
        .copied()
        .filter(|&u| u != 0)
        .and_then(char::from_u32)
}

/// CID と Unicode の対応の取り方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidCollection {
    /// Adobe-Japan1: 対応表で引く。
    Japan1,
    /// 対応表を持たない文字集合（`Identity`・独自の `Npb1` など）。ToUnicode が無ければ
    /// CID の値をそのまま符号位置とみなす（poppler と同じふるまい）。
    Other,
}

#[derive(Debug, Clone)]
pub struct Font {
    pub base_font: String,
    /// 縦書き（WMode 1）。
    pub vertical: bool,
    /// CID フォント（Type0）か。
    pub composite: bool,
    pub collection: Option<CidCollection>,
    pub ordering: Option<String>,
    /// ToUnicode を持つか。
    pub has_to_unicode: bool,
    /// em を 1 とした字面の上端・下端（FontDescriptor の Ascent / Descent）。
    pub ascent: f64,
    pub descent: f64,
    encoding: CMap,
    to_unicode: Option<CMap>,
    /// 単純フォントの字形名による対応（Encoding の Differences）。
    simple_names: HashMap<u32, String>,
    widths: HashMap<u32, f64>,
    default_width: f64,
    /// 縦書きの字送り（w1y）と、横書きの原点から縦書きの原点への位置（vx, vy）。1/1000 em。
    vmetrics: HashMap<u32, (f64, f64, f64)>,
    default_vmetrics: (f64, f64),
    /// Type3 の FontMatrix の x 倍率（他は 0.001）。
    scale: f64,
    // Private source evidence; never changes Glyph/FontInfo or unicode priority.
    identity_encoding: bool,
    declared_to_unicode: bool,
    source_cff: Option<crate::object::Stream>,
    source_charset: OnceLock<Option<Vec<u16>>>,
}

/// 文字列中の 1 文字。
#[derive(Debug, Clone)]
pub(crate) struct CharCode {
    pub code: u32,
    pub len: usize,
    pub cid: u32,
}

impl Font {
    pub(crate) fn load(doc: &Document, dict: &Dict) -> Font {
        let name = |d: &Dict, k: &[u8]| {
            doc.dict_get(d, k)
                .and_then(|o| o.as_name().map(|n| String::from_utf8_lossy(n).into_owned()))
        };
        let subtype = name(dict, b"Subtype").unwrap_or_default();
        let base_font = name(dict, b"BaseFont").unwrap_or_default();
        let to_unicode = doc
            .dict_get(dict, b"ToUnicode")
            .and_then(|o| o.as_stream().and_then(|s| doc.decode_stream(s).ok()))
            .map(|data| CMap::parse(&data))
            .filter(CMap::has_unicode);
        let mut font = Font {
            base_font,
            vertical: false,
            composite: false,
            collection: None,
            ordering: None,
            has_to_unicode: false,
            ascent: 0.88,
            descent: -0.12,
            encoding: CMap::default(),
            to_unicode,
            simple_names: HashMap::new(),
            widths: HashMap::new(),
            default_width: 1000.0,
            vmetrics: HashMap::new(),
            default_vmetrics: (880.0, -1000.0),
            scale: 0.001,
            identity_encoding: false,
            declared_to_unicode: dict.contains_key(&b"ToUnicode"[..]),
            source_cff: None,
            source_charset: OnceLock::new(),
        };
        if subtype == "Type0" {
            font.composite = true;
            match doc.dict_get(dict, b"Encoding") {
                Some(Object::Name(n)) => {
                    font.identity_encoding = matches!(n.as_slice(), b"Identity-H" | b"Identity-V");
                    font.encoding = CMap::identity();
                    font.vertical = n.ends_with(b"-V");
                    // Identity 以外の定義済み CMap（UniJIS-UCS2-H など）は持っていないので、
                    // 2 バイトの Identity として読む。
                }
                Some(Object::Stream(s)) => {
                    font.encoding = doc
                        .decode_stream(&s)
                        .map(|d| CMap::parse(&d))
                        .unwrap_or_else(|_| CMap::identity());
                    font.vertical = doc.dict_get(&s.dict, b"WMode").and_then(|o| o.as_i64()) == Some(1);
                }
                _ => font.encoding = CMap::identity(),
            }
            let desc = doc
                .dict_get(dict, b"DescendantFonts")
                .and_then(|a| a.as_array().and_then(|a| a.first().map(|x| doc.resolve(x))));
            if let Some(Object::Dict(cid_font)) = desc {
                font.load_cid_metrics(doc, &cid_font);
                if let Some(csi) = doc.dict_get(&cid_font, b"CIDSystemInfo") {
                    let ordering = csi
                        .as_dict()
                        .and_then(|d| doc.dict_get(d, b"Ordering"))
                        .and_then(|o| o.as_str().map(|s| String::from_utf8_lossy(s).into_owned()));
                    font.collection = Some(match ordering.as_deref() {
                        Some("Japan1") => CidCollection::Japan1,
                        _ => CidCollection::Other,
                    });
                    font.ordering = ordering;
                }
                if let Some(fd) = doc.dict_get(&cid_font, b"FontDescriptor") {
                    font.load_descriptor(doc, fd.as_dict());
                }
            }
        } else {
            font.encoding = CMap::default();
            let first = doc.dict_get(dict, b"FirstChar").and_then(|o| o.as_i64()).unwrap_or(0);
            if let Some(Object::Array(ws)) = doc.dict_get(dict, b"Widths") {
                for (i, w) in ws.iter().enumerate() {
                    if let Some(w) = doc.resolve(w).as_f64() {
                        font.widths.insert((first + i as i64) as u32, w);
                    }
                }
            }
            let fd = doc.dict_get(dict, b"FontDescriptor");
            font.default_width = fd
                .as_ref()
                .and_then(|fd| fd.as_dict().and_then(|d| doc.dict_get(d, b"MissingWidth")))
                .and_then(|o| o.as_f64())
                .unwrap_or(if font.widths.is_empty() { 500.0 } else { 0.0 });
            if let Some(fd) = fd {
                font.load_descriptor(doc, fd.as_dict());
            }
            if subtype == "Type3" {
                if let Some(Object::Array(m)) = doc.dict_get(dict, b"FontMatrix") {
                    font.scale = m.first().and_then(|x| doc.resolve(x).as_f64()).unwrap_or(0.001);
                }
            }
            if let Some(Object::Dict(enc)) = doc.dict_get(dict, b"Encoding") {
                if let Some(Object::Array(diffs)) = doc.dict_get(&enc, b"Differences") {
                    let mut code = 0u32;
                    for d in diffs {
                        match doc.resolve(&d) {
                            Object::Int(i) => code = i as u32,
                            Object::Name(n) => {
                                font.simple_names.insert(code, String::from_utf8_lossy(&n).into_owned());
                                code += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        font.has_to_unicode = font.to_unicode.is_some();
        font
    }

    fn load_descriptor(&mut self, doc: &Document, fd: Option<&Dict>) {
        let Some(fd) = fd else { return };
        if let Some(Object::Stream(stream)) = doc.dict_get(fd, b"FontFile3") {
            if doc
                .dict_get(&stream.dict, b"Subtype")
                .and_then(|o| o.as_name().map(Vec::from))
                .as_deref()
                == Some(b"CIDFontType0C")
            {
                self.source_cff = Some(stream);
            }
        }
        let num = |k: &[u8]| doc.dict_get(fd, k).and_then(|o| o.as_f64());
        if let (Some(a), Some(d)) = (num(b"Ascent"), num(b"Descent")) {
            // 0 や上下逆の壊れた値は使わない。
            if a > 0.0 && d <= 0.0 && a - d > 100.0 {
                self.ascent = a / 1000.0;
                self.descent = d / 1000.0;
            }
        }
    }

    fn load_cid_metrics(&mut self, doc: &Document, cid_font: &Dict) {
        if let Some(dw) = doc.dict_get(cid_font, b"DW").and_then(|o| o.as_f64()) {
            self.default_width = dw;
        }
        if let Some(Object::Array(w)) = doc.dict_get(cid_font, b"W") {
            let w: Vec<Object> = w.iter().map(|x| doc.resolve(x)).collect();
            let mut i = 0;
            while i < w.len() {
                let Some(first) = w[i].as_i64() else { break };
                match w.get(i + 1) {
                    Some(Object::Array(list)) => {
                        for (k, x) in list.iter().enumerate() {
                            if let Some(v) = doc.resolve(x).as_f64() {
                                self.widths.insert(first as u32 + k as u32, v);
                            }
                        }
                        i += 2;
                    }
                    Some(last) => {
                        let (Some(last), Some(v)) = (last.as_i64(), w.get(i + 2).and_then(Object::as_f64)) else {
                            break;
                        };
                        if last >= first && last - first <= 0x1_0000 {
                            for c in first..=last {
                                self.widths.insert(c as u32, v);
                            }
                        }
                        i += 3;
                    }
                    None => break,
                }
            }
        }
        if let Some(Object::Array(dw2)) = doc.dict_get(cid_font, b"DW2") {
            if let (Some(vy), Some(w1)) = (
                dw2.first().and_then(Object::as_f64),
                dw2.get(1).and_then(Object::as_f64),
            ) {
                self.default_vmetrics = (vy, w1);
            }
        }
        if let Some(Object::Array(w2)) = doc.dict_get(cid_font, b"W2") {
            let w2: Vec<Object> = w2.iter().map(|x| doc.resolve(x)).collect();
            let mut i = 0;
            while i < w2.len() {
                let Some(first) = w2[i].as_i64() else { break };
                match w2.get(i + 1) {
                    Some(Object::Array(list)) => {
                        let nums: Vec<f64> = list.iter().filter_map(|x| doc.resolve(x).as_f64()).collect();
                        for (k, t) in nums.chunks(3).enumerate() {
                            if t.len() == 3 {
                                self.vmetrics.insert(first as u32 + k as u32, (t[0], t[1], t[2]));
                            }
                        }
                        i += 2;
                    }
                    Some(last) => {
                        let nums: Vec<f64> = (2..5).filter_map(|k| w2.get(i + k).and_then(Object::as_f64)).collect();
                        let Some(last) = last.as_i64() else { break };
                        if nums.len() == 3 && last >= first && last - first <= 0x1_0000 {
                            for c in first..=last {
                                self.vmetrics.insert(c as u32, (nums[0], nums[1], nums[2]));
                            }
                        }
                        i += 5;
                    }
                    None => break,
                }
            }
        }
    }

    pub(crate) fn decode(&self, bytes: &[u8]) -> Vec<CharCode> {
        if !self.composite {
            return bytes
                .iter()
                .map(|&b| CharCode {
                    code: b as u32,
                    len: 1,
                    cid: b as u32,
                })
                .collect();
        }
        self.encoding
            .split(bytes)
            .into_iter()
            .map(|(code, len)| CharCode {
                code,
                len,
                cid: self.encoding.cid(code, len),
            })
            .collect()
    }

    /// 横書きの字幅（em 単位）。
    pub(crate) fn width(&self, c: &CharCode) -> f64 {
        let key = if self.composite { c.cid } else { c.code };
        self.widths.get(&key).copied().unwrap_or(self.default_width) * self.scale
    }

    /// 縦書きの字送り w1y と、原点の位置 (vx, vy)（em 単位）。
    pub(crate) fn vertical_metrics(&self, c: &CharCode) -> (f64, f64, f64) {
        match self.vmetrics.get(&c.cid) {
            Some(&(w1, vx, vy)) => (w1 / 1000.0, vx / 1000.0, vy / 1000.0),
            None => (
                self.default_vmetrics.1 / 1000.0,
                self.width(c) / 2.0,
                self.default_vmetrics.0 / 1000.0,
            ),
        }
    }

    /// A separate candidate only for a real unresolved Identity encoding.
    /// Explicit/partial/broken ToUnicode declarations always keep priority.
    pub(crate) fn source_aj1_fallback(&self, doc: &Document, code: u32, cid: u32) -> Option<char> {
        if !self.composite
            || self.ordering.as_deref() != Some("Identity")
            || !self.identity_encoding
            || self.declared_to_unicode
            || self.has_to_unicode
            || self.to_unicode.is_some()
            || code != cid
        {
            return None;
        }
        let c = aj1(cid)?;
        if c.is_control()
            || c.is_whitespace()
            || matches!(c as u32,0xE000..=0xF8FF|0xF0000..=0xFFFFD|0x100000..=0x10FFFD)
        {
            return None;
        }
        let set = self
            .source_charset
            .get_or_init(|| {
                let bytes = doc.decode_stream(self.source_cff.as_ref()?).ok()?;
                crate::cff_aj1::standard_cids(&bytes)
            })
            .as_ref()?;
        set.binary_search(&u16::try_from(cid).ok()?).ok()?;
        Some(c)
    }

    /// 1 文字の Unicode。対応が分からなければ空文字列。
    pub(crate) fn unicode(&self, c: &CharCode) -> String {
        if let Some(s) = self.to_unicode.as_ref().and_then(|m| m.unicode(c.code, c.len)) {
            return s.to_string();
        }
        if self.composite {
            let ch = match self.collection {
                Some(CidCollection::Japan1) => aj1(c.cid),
                _ => char::from_u32(c.cid),
            };
            return ch.map(String::from).unwrap_or_default();
        }
        if let Some(name) = self.simple_names.get(&c.code) {
            if let Some(ch) = glyph_name_char(name) {
                return ch.to_string();
            }
        }
        match c.code {
            0x20..=0x7e | 0xa0..=0xff => {
                char::from_u32(c.code).map(String::from).unwrap_or_default()
            }
            _ => String::new(),
        }
    }
}

/// 字形名から字へ（`uniXXXX`・`uXXXX[XX]`・ASCII の主な名前）。
fn glyph_name_char(name: &str) -> Option<char> {
    let name = name.split('.').next().unwrap_or(name);
    if let Some(hex) = name.strip_prefix("uni") {
        if hex.len() == 4 {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }
    if let Some(hex) = name.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return char::from_u32(v);
            }
        }
    }
    if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
        return name.chars().next();
    }
    const NAMES: &[(&str, char)] = &[
        ("space", ' '),
        ("exclam", '!'),
        ("quotedbl", '"'),
        ("numbersign", '#'),
        ("dollar", '$'),
        ("percent", '%'),
        ("ampersand", '&'),
        ("quotesingle", '\''),
        ("quoteright", '\''),
        ("parenleft", '('),
        ("parenright", ')'),
        ("asterisk", '*'),
        ("plus", '+'),
        ("comma", ','),
        ("hyphen", '-'),
        ("period", '.'),
        ("slash", '/'),
        ("zero", '0'),
        ("one", '1'),
        ("two", '2'),
        ("three", '3'),
        ("four", '4'),
        ("five", '5'),
        ("six", '6'),
        ("seven", '7'),
        ("eight", '8'),
        ("nine", '9'),
        ("colon", ':'),
        ("semicolon", ';'),
        ("less", '<'),
        ("equal", '='),
        ("greater", '>'),
        ("question", '?'),
        ("at", '@'),
        ("bracketleft", '['),
        ("backslash", '\\'),
        ("bracketright", ']'),
        ("underscore", '_'),
        ("braceleft", '{'),
        ("bar", '|'),
        ("braceright", '}'),
    ];
    NAMES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aj1_table_maps_common_cids() {
        // CID 1 は UniJIS-UTF32 の先頭の候補の NBSP（poppler も同じ）。
        assert_eq!(aj1(1), Some('\u{A0}'));
        // CID 1200 は「一」（部首の ⼀ ではなく統合漢字）。
        assert_eq!(aj1(1200), Some('一'));
        // 縦書き用の読点は縦書き用の字形の符号になる。
        assert_eq!(aj1(7887), Some('\u{FE11}'));
        // poppler の表に無い縦書き用のコロン（「一：六」）も字にする。
        assert_eq!(aj1(12101), Some('：'));
        // 互換漢字は統合漢字に（CID 7651 は U+FA36 ではなく「喝」U+559D）。
        assert_eq!(aj1(7651), Some('\u{559D}'));
    }
}

#[cfg(test)]
#[path = "source_aj1_font_tests.rs"]
mod source_aj1_font_tests;
