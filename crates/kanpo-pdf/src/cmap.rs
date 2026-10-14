//! CMap（ISO 32000-1 9.7.5・9.10.3）。埋め込みの文字コード → CID 対応と、ToUnicode を読む。

use std::collections::HashMap;

use crate::lexer::{Lexer, Token};

#[derive(Debug, Clone, Default)]
pub(crate) struct CMap {
    /// 文字コードの範囲（バイト長ごと）。`(長さ, 下限, 上限)`、各バイトごとに範囲判定する。
    codespace: Vec<(usize, Vec<u8>, Vec<u8>)>,
    /// `(長さ, 下限, 上限, 先頭の CID)`。
    cid_ranges: Vec<(usize, u32, u32, u32)>,
    /// `(長さ, 符号) → Unicode 文字列`。
    unicode: HashMap<(usize, u32), String>,
    /// 先行する CMap（`usecmap`）が Identity。
    pub(crate) identity: bool,
}

const MAX_RANGE: u32 = 0x1_0000;

impl CMap {
    pub(crate) fn identity() -> Self {
        CMap {
            codespace: vec![(2, vec![0, 0], vec![0xff, 0xff])],
            identity: true,
            ..Default::default()
        }
    }

    pub(crate) fn parse(data: &[u8]) -> Self {
        let mut cmap = CMap::default();
        let mut lexer = Lexer::new(data, 0);
        let mut operands: Vec<Token> = Vec::new();
        let mut section: Option<Vec<u8>> = None;
        while let Some(tok) = lexer.next_token() {
            match tok {
                Token::Keyword(k) => {
                    match k.as_slice() {
                        b"begincodespacerange"
                        | b"beginbfchar"
                        | b"beginbfrange"
                        | b"begincidchar"
                        | b"begincidrange"
                        | b"beginnotdefrange"
                        | b"beginnotdefchar" => {
                            section = Some(k);
                            operands.clear();
                        }
                        b"endcodespacerange" => {
                            for pair in operands.chunks(2) {
                                if let [Token::String(lo), Token::String(hi)] = pair {
                                    if lo.len() == hi.len() && !lo.is_empty() {
                                        cmap.codespace.push((lo.len(), lo.clone(), hi.clone()));
                                    }
                                }
                            }
                            section = None;
                        }
                        b"endbfchar" => {
                            for pair in operands.chunks(2) {
                                if let [Token::String(src), dst] = pair {
                                    if let Some(s) = dst_string(dst) {
                                        cmap.unicode.insert((src.len(), be(src)), s);
                                    }
                                }
                            }
                            section = None;
                        }
                        b"endbfrange" => {
                            for triple in operands.chunks(3) {
                                if let [Token::String(lo), Token::String(hi), dst] = triple {
                                    cmap.add_bfrange(lo, hi, dst);
                                }
                            }
                            section = None;
                        }
                        b"endcidchar" => {
                            for pair in operands.chunks(2) {
                                if let [Token::String(src), Token::Int(cid)] = pair {
                                    let c = be(src);
                                    cmap.cid_ranges.push((src.len(), c, c, *cid as u32));
                                }
                            }
                            section = None;
                        }
                        b"endcidrange" => {
                            for triple in operands.chunks(3) {
                                if let [Token::String(lo), Token::String(hi), Token::Int(cid)] = triple {
                                    cmap.cid_ranges.push((lo.len(), be(lo), be(hi), *cid as u32));
                                }
                            }
                            section = None;
                        }
                        b"endnotdefrange" | b"endnotdefchar" => section = None,
                        b"usecmap" => {
                            if let Some(Token::Name(n)) = operands.last() {
                                if n.starts_with(b"Identity") {
                                    cmap.identity = true;
                                }
                            }
                        }
                        _ => {}
                    }
                    if section.is_none() {
                        operands.clear();
                    }
                }
                Token::ArrayStart if section.as_deref() == Some(b"beginbfrange") => {
                    // bfrange の宛先の配列: [<...> <...> ...]
                    let mut items = Vec::new();
                    while let Some(t) = lexer.next_token() {
                        match t {
                            Token::ArrayEnd => break,
                            Token::String(s) => items.push(s),
                            _ => {}
                        }
                    }
                    operands.push(Token::Keyword(encode_array(&items)));
                }
                other => operands.push(other),
            }
        }
        if cmap.codespace.is_empty() && cmap.identity {
            cmap.codespace = CMap::identity().codespace;
        }
        cmap
    }

    fn add_bfrange(&mut self, lo: &[u8], hi: &[u8], dst: &Token) {
        let (l, h) = (be(lo), be(hi));
        if h < l || h - l > MAX_RANGE {
            return;
        }
        match dst {
            Token::String(start) => {
                let units = utf16_units(start);
                for (i, code) in (l..=h).enumerate() {
                    let mut u = units.clone();
                    if let Some(last) = u.last_mut() {
                        *last = last.wrapping_add(i as u16);
                    }
                    self.unicode.insert((lo.len(), code), String::from_utf16_lossy(&u));
                }
            }
            Token::Keyword(encoded) => {
                for (i, item) in decode_array(encoded).into_iter().enumerate() {
                    let code = l + i as u32;
                    if code > h {
                        break;
                    }
                    self.unicode
                        .insert((lo.len(), code), String::from_utf16_lossy(&utf16_units(&item)));
                }
            }
            _ => {}
        }
    }

    /// 文字列を文字コードに切り分ける。`(符号, バイト長)` の並び。
    pub(crate) fn split(&self, bytes: &[u8]) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let len = self.code_len(&bytes[i..]);
            let end = (i + len).min(bytes.len());
            out.push((be(&bytes[i..end]), end - i));
            i = end;
        }
        out
    }

    fn code_len(&self, rest: &[u8]) -> usize {
        if self.codespace.is_empty() {
            return 1;
        }
        for len in 1..=4 {
            if rest.len() < len {
                break;
            }
            let hit = self
                .codespace
                .iter()
                .any(|(l, lo, hi)| *l == len && (0..len).all(|k| lo[k] <= rest[k] && rest[k] <= hi[k]));
            if hit {
                return len;
            }
        }
        // どの範囲にも入らなければ、最短の範囲の長さで進める。
        self.codespace.iter().map(|(l, _, _)| *l).min().unwrap_or(1)
    }

    pub(crate) fn cid(&self, code: u32, len: usize) -> u32 {
        for &(l, lo, hi, cid) in self.cid_ranges.iter().rev() {
            if l == len && lo <= code && code <= hi {
                return cid + (code - lo);
            }
        }
        if self.identity {
            code
        } else {
            0
        }
    }

    pub(crate) fn unicode(&self, code: u32, len: usize) -> Option<&str> {
        self.unicode.get(&(len, code)).map(String::as_str)
    }

    pub(crate) fn has_unicode(&self) -> bool {
        !self.unicode.is_empty()
    }
}

fn be(b: &[u8]) -> u32 {
    b.iter().take(4).fold(0u32, |acc, &x| acc << 8 | x as u32)
}

fn utf16_units(b: &[u8]) -> Vec<u16> {
    if b.len() == 1 {
        return vec![b[0] as u16];
    }
    b.chunks(2)
        .map(|c| {
            if c.len() == 2 {
                (c[0] as u16) << 8 | c[1] as u16
            } else {
                c[0] as u16
            }
        })
        .collect()
}

fn dst_string(t: &Token) -> Option<String> {
    match t {
        Token::String(s) => Some(String::from_utf16_lossy(&utf16_units(s))),
        // `/space` のような字形名は扱わない。
        _ => None,
    }
}

// bfrange の配列は Token に入れにくいので長さ付きで詰めて一時的に運ぶ。
fn encode_array(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for it in items {
        out.extend_from_slice(&(it.len() as u32).to_be_bytes());
        out.extend_from_slice(it);
    }
    out
}

fn decode_array(mut b: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while b.len() >= 4 {
        let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
        b = &b[4..];
        let n = n.min(b.len());
        out.push(b[..n].to_vec());
        b = &b[n..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_to_unicode() {
        let src = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap
            1 begincodespacerange <0000> <FFFF> endcodespacerange
            2 beginbfchar <0003> <0020> <0010> <D840DC0B> endbfchar
            2 beginbfrange <0020> <0022> <4E00> <0030> <0031> [<0041> <00420043>] endbfrange
            endcmap";
        let c = CMap::parse(src);
        assert_eq!(c.split(&[0, 3, 0, 0x21]), vec![(3, 2), (0x21, 2)]);
        assert_eq!(c.unicode(3, 2), Some(" "));
        assert_eq!(c.unicode(0x10, 2), Some("\u{2000B}"));
        assert_eq!(c.unicode(0x22, 2), Some("\u{4E02}"));
        assert_eq!(c.unicode(0x31, 2), Some("BC"));
    }

    #[test]
    fn parses_cid_ranges() {
        let src = b"1 begincodespacerange <00> <80> endcodespacerange
            1 begincodespacerange <8140> <FCFC> endcodespacerange
            1 begincidrange <8140> <817E> 633 endcidrange
            1 begincidchar <20> 1 endcidchar";
        let c = CMap::parse(src);
        assert_eq!(c.split(&[0x20, 0x81, 0x41]), vec![(0x20, 1), (0x8141, 2)]);
        assert_eq!(c.cid(0x8141, 2), 634);
        assert_eq!(c.cid(0x20, 1), 1);
    }
}
