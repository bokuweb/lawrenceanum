//! PDF ファイルの読み込み: 相互参照（xref 表・xref ストリーム・オブジェクトストリーム）、
//! 間接オブジェクトの取得と復号、ページツリー（ISO 32000-1 7.5・7.7.3）。
//!
//! xref が壊れているファイルは、本体を走査して `n g obj` を拾い直す。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::crypt::Decryptor;
use crate::filter;
use crate::lexer::{Parser, Token};
use crate::object::{Dict, ObjRef, Object, Stream};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy)]
enum Entry {
    Offset(usize),
    InStream { stream: u32, index: u32 },
}

/// 読み込んだ PDF 文書。
pub struct Document {
    data: Vec<u8>,
    xref: HashMap<u32, Entry>,
    trailer: Dict,
    crypt: Option<Decryptor>,
    cache: RefCell<HashMap<u32, Object>>,
}

/// ページ（継承される属性を解決済み）。
#[derive(Debug, Clone)]
pub struct PageRef {
    pub dict: Dict,
    pub resources: Dict,
    /// `[x0 y0 x1 y1]`（正規化済み）。
    pub media_box: [f64; 4],
    pub crop_box: [f64; 4],
    /// 0・90・180・270。
    pub rotate: i32,
}

const MAX_RESOLVE: usize = 32;

impl Document {
    pub fn load(data: Vec<u8>) -> Result<Self> {
        let mut doc = Document {
            data,
            xref: HashMap::new(),
            trailer: Dict::new(),
            crypt: None,
            cache: RefCell::new(HashMap::new()),
        };
        let mut object_streams = Vec::new();
        if doc.read_xref_chain().is_err() || !doc.trailer.contains_key(&b"Root"[..]) {
            object_streams = doc.reconstruct();
        }
        if !doc.trailer.contains_key(&b"Root"[..]) {
            return Err(Error::Malformed("no document catalog".into()));
        }
        if let Some(enc) = doc.trailer.get(&b"Encrypt"[..]).cloned() {
            let enc = doc.resolve(&enc);
            let id0 = doc
                .trailer
                .get(&b"ID"[..])
                .and_then(Object::as_array)
                .and_then(|a| a.first())
                .and_then(Object::as_str)
                .map(|s| s.to_vec())
                .unwrap_or_default();
            if let Some(d) = enc.as_dict() {
                doc.crypt = Some(Decryptor::new(d, &id0)?);
                // 復号前に読んだオブジェクト（Encrypt 辞書など）は捨てて、読み直させる。
                doc.cache.borrow_mut().clear();
            }
        }
        doc.index_object_streams(&object_streams);
        Ok(doc)
    }

    pub fn trailer(&self) -> &Dict {
        &self.trailer
    }

    // ---- xref ----

    fn read_xref_chain(&mut self) -> Result<()> {
        let tail_start = self.data.len().saturating_sub(2048);
        let tail = &self.data[tail_start..];
        let at = rfind(tail, b"startxref").ok_or_else(|| Error::Malformed("no startxref".into()))?;
        let mut p = Parser::new(&self.data, tail_start + at + b"startxref".len());
        let mut offset = match p.lexer.next_token() {
            Some(Token::Int(i)) if i >= 0 => i as usize,
            _ => return Err(Error::Malformed("bad startxref".into())),
        };
        let mut seen = Vec::new();
        let mut first = true;
        while !seen.contains(&offset) && offset < self.data.len() {
            seen.push(offset);
            let trailer = self.read_xref_section(offset)?;
            if let Some(stm) = trailer.get(&b"XRefStm"[..]).and_then(Object::as_i64) {
                // ハイブリッド: 同じ版の xref ストリームの項目も加える。
                let _ = self.read_xref_section(stm as usize);
            }
            let prev = trailer.get(&b"Prev"[..]).and_then(Object::as_i64);
            if first {
                self.trailer = trailer;
                first = false;
            } else {
                for (k, v) in trailer {
                    self.trailer.entry(k).or_insert(v);
                }
            }
            match prev {
                Some(p) if p >= 0 => offset = p as usize,
                _ => break,
            }
        }
        Ok(())
    }

    /// `offset` の xref 表または xref ストリームを読み、その trailer 辞書を返す。
    /// 既にある項目（より新しい版）は上書きしない。
    fn read_xref_section(&mut self, offset: usize) -> Result<Dict> {
        let mut p = Parser::new(&self.data, offset);
        if p.lexer.next_token() == Some(Token::Keyword(b"xref".to_vec())) {
            loop {
                let save = p.pos();
                match (p.lexer.next_token(), p.lexer.next_token()) {
                    (Some(Token::Int(start)), Some(Token::Int(count))) => {
                        for i in 0..count.max(0) {
                            let (Some(Token::Int(off)), Some(Token::Int(_gen)), Some(Token::Keyword(kind))) =
                                (p.lexer.next_token(), p.lexer.next_token(), p.lexer.next_token())
                            else {
                                return Err(Error::Malformed("bad xref entry".into()));
                            };
                            let num = (start + i) as u32;
                            if kind == b"n" && off > 0 {
                                self.xref.entry(num).or_insert(Entry::Offset(off as usize));
                            } else {
                                // 空き項目も「この版で消えた」として古い版の項目を隠す。
                                self.xref.entry(num).or_insert(Entry::Offset(0));
                            }
                        }
                    }
                    _ => {
                        p.lexer.pos = save;
                        break;
                    }
                }
            }
            if p.lexer.next_token() != Some(Token::Keyword(b"trailer".to_vec())) {
                return Err(Error::Malformed("no trailer".into()));
            }
            return match p.object() {
                Some(Object::Dict(d)) => Ok(d),
                _ => Err(Error::Malformed("bad trailer".into())),
            };
        }
        let (_, obj) = self.parse_indirect_at(offset, false)?;
        let Object::Stream(s) = obj else {
            return Err(Error::Malformed("xref is not a stream".into()));
        };
        let data = self.decode_stream(&s)?;
        let w: Vec<usize> = s
            .dict
            .get(&b"W"[..])
            .and_then(Object::as_array)
            .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0).max(0) as usize).collect())
            .unwrap_or_default();
        if w.len() < 3 {
            return Err(Error::Malformed("bad /W".into()));
        }
        let size = s.dict.get(&b"Size"[..]).and_then(Object::as_i64).unwrap_or(0);
        let index: Vec<i64> = s
            .dict
            .get(&b"Index"[..])
            .and_then(Object::as_array)
            .map(|a| a.iter().filter_map(Object::as_i64).collect())
            .unwrap_or_else(|| vec![0, size]);
        let row = w[0] + w[1] + w[2];
        let mut pos = 0;
        let field = |bytes: &[u8]| bytes.iter().fold(0u64, |acc, &b| acc << 8 | b as u64);
        for pair in index.chunks(2) {
            let (start, count) = (pair[0], pair.get(1).copied().unwrap_or(0));
            for i in 0..count.max(0) {
                if pos + row > data.len() {
                    break;
                }
                let r = &data[pos..pos + row];
                pos += row;
                let kind = if w[0] == 0 { 1 } else { field(&r[..w[0]]) };
                let a = field(&r[w[0]..w[0] + w[1]]);
                let b = field(&r[w[0] + w[1]..]);
                let num = (start + i) as u32;
                let entry = match kind {
                    1 => Entry::Offset(a as usize),
                    2 => Entry::InStream {
                        stream: a as u32,
                        index: b as u32,
                    },
                    _ => Entry::Offset(0),
                };
                self.xref.entry(num).or_insert(entry);
            }
        }
        Ok(s.dict)
    }

    /// xref が使えないとき、本体から `n g obj` を拾って表を作り直す。
    /// 作り直した後に中身を索引するオブジェクトストリームの番号を返す（暗号化された文書では
    /// 鍵ができてからでないと中身が読めないので、[`Self::index_object_streams`] は後で呼ぶ）。
    fn reconstruct(&mut self) -> Vec<u32> {
        self.xref.clear();
        let mut object_streams = Vec::new();
        let data = &self.data;
        let mut i = 0;
        let mut trailer = Dict::new();
        while let Some(at) = find(&data[i..], b"obj") {
            let end = i + at;
            i = end + 3;
            // `obj` の直前の `num gen ` を後ろ向きに読む。
            let mut j = end;
            let skip_ws = |j: &mut usize| {
                while *j > 0 && crate::lexer::is_whitespace(data[*j - 1]) {
                    *j -= 1;
                }
            };
            let digits = |j: &mut usize| {
                let e = *j;
                while *j > 0 && data[*j - 1].is_ascii_digit() {
                    *j -= 1;
                }
                (*j < e)
                    .then(|| std::str::from_utf8(&data[*j..e]).ok()?.parse::<u32>().ok())
                    .flatten()
            };
            skip_ws(&mut j);
            let Some(_gen) = digits(&mut j) else { continue };
            skip_ws(&mut j);
            let Some(num) = digits(&mut j) else { continue };
            if j > 0 && !crate::lexer::is_whitespace(data[j - 1]) && !crate::lexer::is_delimiter(data[j - 1]) {
                continue;
            }
            self.xref.insert(num, Entry::Offset(j));
        }
        let entries: Vec<(u32, Entry)> = self.xref.iter().map(|(k, v)| (*k, *v)).collect();
        // オブジェクトストリームの中身と、カタログ・暗号化辞書を拾う。
        for (num, e) in entries {
            let Entry::Offset(off) = e else { continue };
            let Ok((r, obj)) = self.parse_indirect_at(off, false) else {
                continue;
            };
            let Some(d) = obj.as_dict() else { continue };
            match d.get(&b"Type"[..]).and_then(Object::as_name) {
                Some(b"XRef") => {
                    for k in [&b"Root"[..], b"Encrypt", b"ID", b"Info"] {
                        if let Some(v) = d.get(k) {
                            trailer.insert(k.to_vec(), v.clone());
                        }
                    }
                }
                Some(b"Catalog") => {
                    trailer.entry(b"Root".to_vec()).or_insert(Object::Ref(r));
                }
                Some(b"ObjStm") => object_streams.push(num),
                _ => {}
            }
        }
        // 古典的な trailer 辞書もあれば使う。
        if let Some(at) = rfind(&self.data, b"trailer") {
            if let Some(Object::Dict(d)) = Parser::new(&self.data, at + 7).object() {
                for (k, v) in d {
                    trailer.entry(k).or_insert(v);
                }
            }
        }
        self.trailer = trailer;
        self.cache.borrow_mut().clear();
        object_streams
    }

    /// オブジェクトストリームの中のオブジェクトを xref に加える（xref を作り直したとき）。
    fn index_object_streams(&mut self, streams: &[u32]) {
        for &num in streams {
            let Some(Object::Stream(s)) = self.get(ObjRef { num, gen: 0 }) else { continue };
            let Ok(body) = self.decode_stream(&s) else { continue };
            let n = s.dict.get(&b"N"[..]).and_then(Object::as_i64).unwrap_or(0);
            let mut p = Parser::new(&body, 0);
            for index in 0..n.max(0) as u32 {
                match (p.lexer.next_token(), p.lexer.next_token()) {
                    (Some(Token::Int(obj_num)), Some(Token::Int(_))) => {
                        self.xref.entry(obj_num as u32).or_insert(Entry::InStream { stream: num, index });
                    }
                    _ => break,
                }
            }
        }
        self.cache.borrow_mut().clear();
    }

    // ---- objects ----

    /// `offset` から `num gen obj ... endobj` を読む。`decrypt` なら文字列とストリームを復号する。
    fn parse_indirect_at(&self, offset: usize, decrypt: bool) -> Result<(ObjRef, Object)> {
        let mut p = Parser::new(&self.data, offset);
        let (num, gen) = match (p.lexer.next_token(), p.lexer.next_token(), p.lexer.next_token()) {
            (Some(Token::Int(n)), Some(Token::Int(g)), Some(Token::Keyword(k))) if k == b"obj" => (n as u32, g as u16),
            _ => return Err(Error::Malformed(format!("no object at {offset}"))),
        };
        let r = ObjRef { num, gen };
        let mut obj = p.object().unwrap_or(Object::Null);
        if let Object::Dict(dict) = &obj {
            if p.lexer.next_token() == Some(Token::Keyword(b"stream".to_vec())) {
                let mut start = p.pos();
                if self.data.get(start) == Some(&b'\r') {
                    start += 1;
                }
                if self.data.get(start) == Some(&b'\n') {
                    start += 1;
                }
                let declared = dict.get(&b"Length"[..]).and_then(|l| match l {
                    Object::Ref(lr) if lr.num != num => self.get(*lr).and_then(|o| o.as_i64()),
                    other => other.as_i64(),
                });
                let end = declared
                    .map(|l| start + l.max(0) as usize)
                    .filter(|&e| e <= self.data.len() && self.endstream_follows(e))
                    .or_else(|| {
                        let at = find(&self.data[start..], b"endstream")?;
                        let mut e = start + at;
                        if e > start && self.data[e - 1] == b'\n' {
                            e -= 1;
                        }
                        if e > start && self.data[e - 1] == b'\r' {
                            e -= 1;
                        }
                        Some(e)
                    })
                    .unwrap_or(self.data.len());
                obj = Object::Stream(Stream {
                    dict: dict.clone(),
                    data: self.data[start..end].to_vec(),
                });
            }
        }
        if decrypt {
            if let Some(c) = &self.crypt {
                obj = decrypt_object(c, r, obj);
            }
        }
        Ok((r, obj))
    }

    fn endstream_follows(&self, end: usize) -> bool {
        let mut p = Parser::new(&self.data, end);
        p.lexer.skip_whitespace();
        self.data[p.pos()..].starts_with(b"endstream")
    }

    /// 間接オブジェクトを取る。
    pub fn get(&self, r: ObjRef) -> Option<Object> {
        if let Some(o) = self.cache.borrow().get(&r.num) {
            return Some(o.clone());
        }
        let obj = match *self.xref.get(&r.num)? {
            Entry::Offset(0) => return None,
            Entry::Offset(off) => {
                let (got, obj) = self.parse_indirect_at(off, true).ok()?;
                if got.num != r.num {
                    return None;
                }
                obj
            }
            Entry::InStream { stream, index } => self.read_object_stream(stream, index, r.num)?,
        };
        self.cache.borrow_mut().insert(r.num, obj.clone());
        Some(obj)
    }

    fn read_object_stream(&self, stream: u32, index: u32, num: u32) -> Option<Object> {
        if stream == num {
            return None;
        }
        let s = self.get(ObjRef { num: stream, gen: 0 })?;
        let s = s.as_stream()?;
        let first = s.dict.get(&b"First"[..]).and_then(Object::as_i64)? as usize;
        let n = s.dict.get(&b"N"[..]).and_then(Object::as_i64).unwrap_or(0) as u32;
        let body = self.decode_stream(s).ok()?;
        let mut p = Parser::new(&body, 0);
        let mut offsets = Vec::new();
        for _ in 0..n {
            match (p.lexer.next_token(), p.lexer.next_token()) {
                (Some(Token::Int(o)), Some(Token::Int(off))) => offsets.push((o as u32, off as usize)),
                _ => break,
            }
        }
        // 同じストリームの他のオブジェクトも（xref がこのストリームを指していれば）置いておく。
        let mut found = None;
        for (i, (o, off)) in offsets.iter().enumerate() {
            let mut q = Parser::new(&body, first + off);
            let obj = q.object().unwrap_or(Object::Null);
            if *o == num && (i as u32 == index || found.is_none()) {
                found = Some(obj.clone());
            }
            if matches!(self.xref.get(o), Some(Entry::InStream { stream: st, .. }) if *st == stream) {
                self.cache.borrow_mut().entry(*o).or_insert(obj);
            }
        }
        found
    }

    /// 参照なら辿って中身を返す（参照でなければそのまま）。
    pub fn resolve(&self, o: &Object) -> Object {
        let mut cur = o.clone();
        for _ in 0..MAX_RESOLVE {
            match cur {
                Object::Ref(r) => cur = self.get(r).unwrap_or(Object::Null),
                other => return other,
            }
        }
        Object::Null
    }

    /// 辞書の値を参照を辿って取る。
    pub fn dict_get(&self, d: &Dict, key: &[u8]) -> Option<Object> {
        d.get(key).map(|o| self.resolve(o)).filter(|o| *o != Object::Null)
    }

    /// ストリームのフィルタを解いた中身。
    pub fn decode_stream(&self, s: &Stream) -> Result<Vec<u8>> {
        let filters: Vec<Vec<u8>> = match s.dict.get(&b"Filter"[..]).map(|f| self.resolve(f)) {
            Some(Object::Name(n)) => vec![n],
            Some(Object::Array(a)) => a
                .iter()
                .filter_map(|x| self.resolve(x).as_name().map(<[u8]>::to_vec))
                .collect(),
            _ => Vec::new(),
        };
        let parms_obj = s
            .dict
            .get(&b"DecodeParms"[..])
            .or_else(|| s.dict.get(&b"DP"[..]))
            .map(|p| self.resolve(p));
        let parms: Vec<Option<Dict>> = match parms_obj {
            Some(Object::Dict(d)) => vec![Some(d)],
            Some(Object::Array(a)) => a.iter().map(|x| self.resolve(x).as_dict().cloned()).collect(),
            _ => Vec::new(),
        };
        let parms: Vec<Option<&Dict>> = parms.iter().map(Option::as_ref).collect();
        filter::decode(s.data.clone(), &filters, &parms)
    }

    // ---- pages ----

    /// ページを文書順に並べる。
    pub fn pages(&self) -> Vec<PageRef> {
        let mut out = Vec::new();
        let Some(root) = self.trailer.get(&b"Root"[..]).map(|r| self.resolve(r)) else {
            return out;
        };
        let Some(pages) = root.as_dict().and_then(|d| self.dict_get(d, b"Pages")) else {
            return out;
        };
        let inherited = Inherited {
            resources: None,
            media_box: None,
            crop_box: None,
            rotate: None,
        };
        let mut visited = Vec::new();
        self.walk_pages(&pages, &inherited, &mut out, &mut visited, 0);
        out
    }

    fn walk_pages(&self, node: &Object, inh: &Inherited, out: &mut Vec<PageRef>, visited: &mut Vec<u32>, depth: usize) {
        let Some(d) = node.as_dict() else { return };
        if depth > 64 {
            return;
        }
        let inh = Inherited {
            resources: self
                .dict_get(d, b"Resources")
                .and_then(|o| o.as_dict().cloned())
                .or_else(|| inh.resources.clone()),
            media_box: self.rect(d, b"MediaBox").or(inh.media_box),
            crop_box: self.rect(d, b"CropBox").or(inh.crop_box),
            rotate: self
                .dict_get(d, b"Rotate")
                .and_then(|o| o.as_i64())
                .map(|r| r as i32)
                .or(inh.rotate),
        };
        let kids = self.dict_get(d, b"Kids");
        let is_page = d.get(&b"Type"[..]).and_then(Object::as_name) == Some(b"Page") || kids.is_none();
        if is_page {
            let media_box = inh.media_box.unwrap_or([0.0, 0.0, 612.0, 792.0]);
            let crop_box = inh.crop_box.map(|c| intersect(c, media_box)).unwrap_or(media_box);
            out.push(PageRef {
                dict: d.clone(),
                resources: inh.resources.unwrap_or_default(),
                media_box,
                crop_box,
                rotate: inh.rotate.unwrap_or(0).rem_euclid(360) / 90 * 90,
            });
            return;
        }
        for kid in kids.as_ref().and_then(Object::as_array).unwrap_or(&[]) {
            if let Object::Ref(r) = kid {
                if visited.contains(&r.num) {
                    continue;
                }
                visited.push(r.num);
            }
            let k = self.resolve(kid);
            self.walk_pages(&k, &inh, out, visited, depth + 1);
        }
    }

    fn rect(&self, d: &Dict, key: &[u8]) -> Option<[f64; 4]> {
        let a = self.dict_get(d, key)?;
        let v: Vec<f64> = a.as_array()?.iter().filter_map(|x| self.resolve(x).as_f64()).collect();
        if v.len() != 4 {
            return None;
        }
        Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
    }
}

struct Inherited {
    resources: Option<Dict>,
    media_box: Option<[f64; 4]>,
    crop_box: Option<[f64; 4]>,
    rotate: Option<i32>,
}

fn intersect(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    if r[0] < r[2] && r[1] < r[3] {
        r
    } else {
        b
    }
}

fn decrypt_object(c: &Decryptor, r: ObjRef, obj: Object) -> Object {
    match obj {
        Object::String(s) => Object::String(c.decrypt_string(r, &s)),
        Object::Array(a) => Object::Array(a.into_iter().map(|o| decrypt_object(c, r, o)).collect()),
        Object::Dict(d) => Object::Dict(d.into_iter().map(|(k, v)| (k, decrypt_object(c, r, v))).collect()),
        Object::Stream(s) => {
            let is_xref = s.dict.get(&b"Type"[..]).and_then(Object::as_name) == Some(b"XRef");
            let dict: Dict = s.dict.into_iter().map(|(k, v)| (k, decrypt_object(c, r, v))).collect();
            let data = if is_xref { s.data } else { c.decrypt_stream(r, &s.data) };
            Object::Stream(Stream { dict, data })
        }
        other => other,
    }
}

pub(crate) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}
