//! Exact boundaries for a limited unfiltered inline-image subset.
//! Failure leaves the original parser untouched for the legacy scanner.
use crate::{
    lexer::{is_whitespace, Parser},
    Dict, Document, Object,
};

pub(crate) fn skip_raw(parser: &mut Parser<'_>, doc: &Document, resources: &Dict) -> bool {
    // Device default replacements/resource color spaces are outside this subset.
    if doc.dict_get(resources, b"ColorSpace").is_some_and(|o| {
        o.as_dict().is_none_or(|d| {
            [b"DefaultGray".as_slice(), b"DefaultRGB", b"DefaultCMYK"]
                .iter()
                .any(|key| d.contains_key(*key))
        })
    }) {
        return false;
    }
    let Some(end) = boundary_end(parser.lexer.data, parser.pos()) else {
        return false;
    };
    parser.lexer.pos = end;
    true
}

fn boundary_end(data: &[u8], header: usize) -> Option<usize> {
    // Look ahead independently: no partially parsed header may move the caller.
    let bytes = data.get(header..)?;
    let mut p = Parser::content(bytes);
    let (mut width, mut height, mut depth, mut components) = (None, None, None, None);
    let mut seen = 0u8;
    loop {
        let key = match p.next()? {
            Err(op) if op == b"ID" => break,
            Ok(Object::Name(key)) => key,
            _ => return None,
        };
        let bit = match key.as_slice() {
            b"W" | b"Width" => 1,
            b"H" | b"Height" => 2,
            b"BPC" | b"BitsPerComponent" => 4,
            b"CS" | b"ColorSpace" => 8,
            b"IM" | b"ImageMask" => 16,
            b"I" | b"Interpolate" => 32,
            _ => return None,
        };
        if seen & bit != 0 {
            return None;
        }
        seen |= bit;
        match (bit, p.next()?) {
            (1 | 2, Ok(Object::Int(n))) if n > 0 => {
                let value = usize::try_from(n).ok()?;
                if bit == 1 {
                    width = Some(value);
                } else {
                    height = Some(value);
                }
            }
            (4, Ok(Object::Int(n))) if matches!(n, 1 | 2 | 4 | 8 | 16) => depth = Some(n as usize),
            (8, Ok(Object::Name(name))) => {
                components = Some(match name.as_slice() {
                    b"G" | b"DeviceGray" => 1usize,
                    b"RGB" | b"DeviceRGB" => 3,
                    b"CMYK" | b"DeviceCMYK" => 4,
                    _ => return None,
                })
            }
            (16, Ok(Object::Bool(false))) | (32, Ok(Object::Bool(_))) => (),
            _ => return None,
        }
    }
    // PDF sample rows are independently padded to the next whole byte.
    let row_bits = width?.checked_mul(components?)?.checked_mul(depth?)?;
    let count = row_bits
        .checked_add(7)?
        .checked_div(8)?
        .checked_mul(height?)?;
    let id_end = p.pos();
    if !bytes.get(id_end).is_some_and(|b| is_whitespace(*b)) {
        return None;
    }
    // A single whitespace byte is a delimiter, including CR followed by a
    // leading LF sample. Also accept the CRLF-as-EOL compatibility form, but
    // only when exact raw lengths prove the same final EI for every success.
    let crlf = bytes.get(id_end..id_end.checked_add(2)?) == Some(b"\r\n");
    let mut selected = None;
    for delimiter in 1..=if crlf { 2 } else { 1 } {
        let start = id_end.checked_add(delimiter)?;
        if let Some(end) = raw_end(bytes, start, count) {
            if selected.is_some_and(|previous| previous != end) {
                return None;
            }
            selected = Some(end);
        }
    }
    header.checked_add(selected?)
}

fn raw_end(bytes: &[u8], start: usize, count: usize) -> Option<usize> {
    let end = start.checked_add(count)?;
    if end > bytes.len() {
        return None;
    }
    let mut ei = end;
    while bytes.get(ei).is_some_and(|b| is_whitespace(*b)) {
        ei = ei.checked_add(1)?;
    }
    let keyword_end = ei.checked_add(2)?;
    if ei == end
        || bytes.get(ei..keyword_end) != Some(b"EI")
        || !bytes.get(keyword_end).is_none_or(|b| is_whitespace(*b))
    {
        return None;
    }
    Some(keyword_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_padding_is_per_row_not_per_image() {
        let bytes = b"/W 1 /H 3 /BPC 1 /CS /G ID abc\nEI";
        assert_eq!(boundary_end(bytes, 0), Some(bytes.len()));
        assert_eq!(boundary_end(b"/W 1 /H 3 /BPC 1 /CS /G ID a\nEI", 0), None);
    }
    #[test]
    fn exact_id_delimiter_and_leading_samples() {
        for separator in [b" ".as_slice(), b"\r\n"] {
            for first in [0, b' ', b'\n', b'\r'] {
                let mut bytes = b"/W 4 /H 1 /BPC 8 /CS /G ID".to_vec();
                bytes.extend_from_slice(separator);
                bytes.extend_from_slice(&[first, 1, 2, 3]);
                bytes.extend_from_slice(b"\nEI");
                assert_eq!(boundary_end(&bytes, 0), Some(bytes.len()));
            }
        }
    }
    #[test]
    fn single_cr_delimiter_then_lf_pixel_with_one_ei_separator() {
        let bytes = b"/W 4 /H 1 /BPC 8 /CS /G ID\r\nabc\nEI";
        assert_eq!(boundary_end(bytes, 0), Some(bytes.len()));
    }
    #[test]
    fn crlf_compatibility_and_two_valid_starts_keep_one_ei() {
        for bytes in [
            b"/W 4 /H 1 /BPC 8 /CS /G ID\r\nabcd\nEI".as_slice(),
            b"/W 4 /H 1 /BPC 8 /CS /G ID\r\nabc \nEI",
            b"/W 4 /H 1 /BPC 8 /CS /G ID\r\nabc \n\nEI",
        ] {
            assert_eq!(boundary_end(bytes, 0), Some(bytes.len()));
        }
        // A computed end is not an invitation to seek another later EI.
        assert_eq!(
            boundary_end(b"/W 4 /H 1 /BPC 8 /CS /G ID\r\nabcdef\nEI", 0),
            None
        );
    }
    #[test]
    fn malformed_or_unproved_boundaries_do_not_guess() {
        for bytes in [
            b"/W 4 /H 1 /BPC 8 /CS /G ID abcdEI".as_slice(),
            b"/W 4 /H 1 /BPC 8 /CS /G ID abcd\nEIx ",
            b"/W 4 /H 1 /BPC 8 /CS /G ID abcd\nEI/Name",
            b"/W 4 /H 1 /BPC 8 /CS /G ID abcd\n%comment\nEI",
            b"/W 4 /H 1 /BPC 8 /CS /G ID ab\nEI",
        ] {
            assert_eq!(boundary_end(bytes, 0), None);
        }
    }
    #[test]
    fn checked_dimensions_and_aliases_do_not_choose_a_length() {
        for bytes in [
            b"/W 9223372036854775807 /H 2 /BPC 16 /CS /CMYK ID x\nEI".as_slice(),
            b"/W 4 /Width 4 /H 1 /BPC 8 /CS /G ID abcd\nEI",
            b"/W 4.0 /H 1 /BPC 8 /CS /G ID abcd\nEI",
            b"/W 0 /H 1 /BPC 8 /CS /G ID \nEI",
            b"/W 4 /H 1 /BPC 3 /CS /G ID abcd\nEI",
            b"/W 4 /H 1 /BPC 8 /CS /G /F [] ID abcd\nEI",
        ] {
            assert_eq!(boundary_end(bytes, 0), None);
        }
    }
    #[test]
    fn header_offset_is_returned_in_original_coordinate_space() {
        let bytes = b"BI /W 4 /H 1 /BPC 8 /CS /G ID abcd\nEI\nS";
        assert_eq!(boundary_end(bytes, 2), Some(bytes.len() - 2));
        assert_eq!(boundary_end(bytes, bytes.len() + 1), None);
    }
}
