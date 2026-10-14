//! PDF の字句解析と、トークン列からのオブジェクトの組み立て（ISO 32000-1 7.2・7.3）。
//!
//! ファイル本体とコンテンツストリームの両方で使う。コンテンツストリームでは演算子が
//! [`Token::Keyword`] として出てくる。

use crate::object::{Dict, ObjRef, Object};

pub(crate) fn is_whitespace(b: u8) -> bool {
    matches!(b, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

pub(crate) fn is_delimiter(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn is_regular(b: u8) -> bool {
    !is_whitespace(b) && !is_delimiter(b)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Int(i64),
    Real(f64),
    Name(Vec<u8>),
    String(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
    /// `true`・`R`・`obj`・`stream`・コンテンツストリームの演算子など、区切りの無い語。
    Keyword(Vec<u8>),
}

pub(crate) struct Lexer<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize) -> Self {
        Lexer { data, pos }
    }

    pub(crate) fn skip_whitespace(&mut self) {
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if is_whitespace(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self.pos < self.data.len() && !matches!(self.data[self.pos], b'\r' | b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub(crate) fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        let b = *self.data.get(self.pos)?;
        match b {
            b'[' => {
                self.pos += 1;
                Some(Token::ArrayStart)
            }
            b']' => {
                self.pos += 1;
                Some(Token::ArrayEnd)
            }
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                Some(Token::DictStart)
            }
            b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Some(Token::DictEnd)
            }
            b'<' => Some(Token::String(self.hex_string())),
            b'(' => Some(Token::String(self.literal_string())),
            b'/' => Some(Token::Name(self.name())),
            // PostScript 関数の `{ }` や壊れた `)` `>` は 1 文字の語として読み飛ばせるようにする。
            b'{' | b'}' | b')' | b'>' => {
                self.pos += 1;
                Some(Token::Keyword(vec![b]))
            }
            _ => {
                let start = self.pos;
                while self.pos < self.data.len() && is_regular(self.data[self.pos]) {
                    self.pos += 1;
                }
                let word = &self.data[start..self.pos];
                Some(parse_number(word).unwrap_or_else(|| Token::Keyword(word.to_vec())))
            }
        }
    }

    fn name(&mut self) -> Vec<u8> {
        self.pos += 1; // '/'
        let mut out = Vec::new();
        while self.pos < self.data.len() && is_regular(self.data[self.pos]) {
            let b = self.data[self.pos];
            if b == b'#' {
                let digit = |i: usize| self.data.get(self.pos + i).and_then(|c| hex_val(*c));
                if let (Some(h), Some(l)) = (digit(1), digit(2)) {
                    out.push(h << 4 | l);
                    self.pos += 3;
                    continue;
                }
            }
            out.push(b);
            self.pos += 1;
        }
        out
    }

    fn hex_string(&mut self) -> Vec<u8> {
        self.pos += 1; // '<'
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_val(b) {
                match hi.take() {
                    Some(h) => out.push(h << 4 | v),
                    None => hi = Some(v),
                }
            }
        }
        if let Some(h) = hi {
            out.push(h << 4);
        }
        out
    }

    fn literal_string(&mut self) -> Vec<u8> {
        self.pos += 1; // '('
        let mut out = Vec::new();
        let mut depth = 1usize;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(&e) = self.data.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.data.get(self.pos) {
                                    Some(&d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        // 行末の `\` は改行ごと無視する。
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
        out
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_number(word: &[u8]) -> Option<Token> {
    let first = *word.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    let s = std::str::from_utf8(word).ok()?;
    if !s.contains('.') {
        if let Ok(i) = s.parse::<i64>() {
            return Some(Token::Int(i));
        }
    }
    // `--5`・`4.`・`-.5` のような崩れた実数も読む。
    let trimmed = s.trim_start_matches(['+', '-']);
    let neg = s.len() != trimmed.len() && s.starts_with('-');
    let body = trimmed.strip_suffix('.').unwrap_or(trimmed);
    let v: f64 = if body.is_empty() || body == "." {
        0.0
    } else {
        body.parse().ok()?
    };
    Some(Token::Real(if neg { -v } else { v }))
}

/// トークン列から PDF オブジェクトを組み立てる。
pub(crate) struct Parser<'a> {
    pub(crate) lexer: Lexer<'a>,
    /// `n g R` を参照として読むか（コンテンツストリームでは読まない）。
    refs: bool,
}

const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize) -> Self {
        Parser {
            lexer: Lexer::new(data, pos),
            refs: true,
        }
    }

    pub(crate) fn content(data: &'a [u8]) -> Self {
        Parser {
            lexer: Lexer::new(data, 0),
            refs: false,
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.lexer.pos
    }

    /// 次のオブジェクト。語（演算子など）が来たら `Err(語)` を返す。
    pub(crate) fn next(&mut self) -> Option<Result<Object, Vec<u8>>> {
        let tok = self.lexer.next_token()?;
        Some(self.parse_token(tok, 0))
    }

    pub(crate) fn object(&mut self) -> Option<Object> {
        self.next()?.ok()
    }

    fn parse_token(&mut self, tok: Token, depth: usize) -> Result<Object, Vec<u8>> {
        if depth > MAX_DEPTH {
            return Ok(Object::Null);
        }
        Ok(match tok {
            Token::Int(i) => {
                if self.refs && i >= 0 {
                    if let Some(r) = self.try_ref(i) {
                        return Ok(Object::Ref(r));
                    }
                }
                Object::Int(i)
            }
            Token::Real(r) => Object::Real(r),
            Token::Name(n) => Object::Name(n),
            Token::String(s) => Object::String(s),
            Token::ArrayStart => {
                let mut items = Vec::new();
                loop {
                    match self.lexer.next_token() {
                        None | Some(Token::ArrayEnd) => break,
                        Some(t) => match self.parse_token(t, depth + 1) {
                            Ok(o) => items.push(o),
                            Err(k) => items.push(keyword_object(&k)),
                        },
                    }
                }
                Object::Array(items)
            }
            Token::DictStart => Object::Dict(self.dict_body(depth)),
            Token::Keyword(k) => match k.as_slice() {
                b"true" => Object::Bool(true),
                b"false" => Object::Bool(false),
                b"null" => Object::Null,
                _ => return Err(k),
            },
            Token::ArrayEnd | Token::DictEnd => Object::Null,
        })
    }

    fn dict_body(&mut self, depth: usize) -> Dict {
        let mut dict = Dict::new();
        loop {
            match self.lexer.next_token() {
                None | Some(Token::DictEnd) => break,
                Some(Token::Name(key)) => {
                    let Some(tok) = self.lexer.next_token() else { break };
                    if tok == Token::DictEnd {
                        dict.insert(key, Object::Null);
                        break;
                    }
                    let value = self.parse_token(tok, depth + 1).unwrap_or_else(|k| keyword_object(&k));
                    dict.insert(key, value);
                }
                // キーでないものは読み飛ばす（壊れた辞書への耐性）。
                Some(_) => {}
            }
        }
        dict
    }

    /// `num` の後に `gen R` が続けば参照として読む。続かなければ位置を戻す。
    fn try_ref(&mut self, num: i64) -> Option<ObjRef> {
        let save = self.lexer.pos;
        if let Some(Token::Int(gen)) = self.lexer.next_token() {
            if let Some(Token::Keyword(k)) = self.lexer.next_token() {
                if k == b"R" && (0..=u16::MAX as i64).contains(&gen) && num <= u32::MAX as i64 {
                    return Some(ObjRef {
                        num: num as u32,
                        gen: gen as u16,
                    });
                }
            }
        }
        self.lexer.pos = save;
        None
    }
}

fn keyword_object(k: &[u8]) -> Object {
    match k {
        b"true" => Object::Bool(true),
        b"false" => Object::Bool(false),
        _ => Object::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Object {
        Parser::new(s.as_bytes(), 0).object().unwrap()
    }

    #[test]
    fn parses_nested_objects_and_refs() {
        let o = parse("<< /Type /Page /Kids [1 0 R 2 0 R] /Num -.5 /S (a\\(b\\)c\\101) /H <4142 3> /N#20x 3 >>");
        let d = o.as_dict().unwrap();
        assert_eq!(d[&b"Type"[..]], Object::Name(b"Page".to_vec()));
        assert_eq!(
            d[&b"Kids"[..]],
            Object::Array(vec![
                Object::Ref(ObjRef { num: 1, gen: 0 }),
                Object::Ref(ObjRef { num: 2, gen: 0 })
            ])
        );
        assert_eq!(d[&b"Num"[..]], Object::Real(-0.5));
        assert_eq!(d[&b"S"[..]], Object::String(b"a(b)cA".to_vec()));
        assert_eq!(d[&b"H"[..]], Object::String(vec![0x41, 0x42, 0x30]));
        assert_eq!(d[&b"N x"[..]], Object::Int(3));
    }

    #[test]
    fn content_parser_does_not_read_refs() {
        let mut p = Parser::content(b"1 0 0 1 0 0 cm [(a) -120 (b)] TJ");
        let mut ops = Vec::new();
        let mut operands = Vec::new();
        while let Some(r) = p.next() {
            match r {
                Ok(o) => operands.push(o),
                Err(k) => {
                    ops.push((k, std::mem::take(&mut operands)));
                }
            }
        }
        assert_eq!(ops[0].0, b"cm");
        assert_eq!(ops[0].1.len(), 6);
        assert_eq!(ops[1].0, b"TJ");
        assert_eq!(ops[1].1[0].as_array().unwrap().len(), 3);
    }
}
