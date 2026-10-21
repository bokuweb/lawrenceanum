/// Consumer validation only. The PDF producer separately proves the real CFF,
/// source font, unresolved Identity route, and each original Glyph occurrence.
pub(crate) fn candidate(raw: &str, value: &str) -> Option<String> {
    let r: Vec<_> = raw.chars().collect();
    let v: Vec<_> = value.chars().collect();
    if r.is_empty() || r.len() != v.len() {
        return None;
    }
    // A verified source-specific decoder is already resolved. The standard
    // collection is supplemental evidence for scalar fallback only; a mixed
    // Word containing any explicitly decoded source retains its entire legacy
    // route and cannot acquire partial/competing source ownership.
    // Preserve whole-Word decoders (including multi-Glyph compounds) before
    // consulting individual scalar fallbacks. A resolved component also keeps
    // the complete legacy Word route rather than accepting a partial override.
    if crate::normalize::private_cid_glyph(raw, 1., 1.).is_some()
        || r.iter()
            .any(|c| crate::normalize::private_cid_glyph(&c.to_string(), 1., 1.).is_some())
    {
        return None;
    }
    for (c, u) in r.iter().zip(&v) {
        let cid = (*c as u32).checked_sub(kanpo_pdf::bbox::IDENTITY_CID_BASE)?;
        if cid > 0xFFFD
            || kanpo_pdf::font::aj1(cid) != Some(*u)
            || u.is_whitespace()
            || u.is_control()
            || matches!(*u as u32,0xE000..=0xF8FF|0xF0000..=0xFFFFD|0x100000..=0x10FFFD)
        {
            return None;
        }
    }
    Some(value.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw(cid: u32) -> String {
        char::from_u32(0xF0000 + cid).unwrap().to_string()
    }
    #[test]
    fn verified_standard_singleton() {
        assert_eq!(candidate(&raw(17518), "嬴"), Some("嬴".into()));
    }
    #[test]
    fn all_source_glyphs_match_in_order() {
        assert_eq!(
            candidate(&(raw(18000) + &raw(18813)), "瀀釬"),
            Some("瀀釬".into())
        );
    }
    #[test]
    fn ordinary_unicode_never_uses_the_candidate() {
        for s in ["救", "嬴", "䑮"] {
            assert!(candidate(s, "嬴").is_none());
        }
    }
    #[test]
    fn wrong_CID_cannot_claim_another_character() {
        assert!(candidate(&raw(17518), "瀀").is_none());
    }
    #[test]
    fn unknown_out_of_table_CID_is_unresolved() {
        assert!(candidate(&raw(25937), "（チ）").is_none());
    }
    #[test]
    fn full_word_cardinality_and_empty_are_refused() {
        for (r, s) in [
            (String::new(), ""),
            (raw(17518), "嬴外"),
            (raw(17518) + &raw(18000), "嬴"),
        ] {
            assert!(candidate(&r, s).is_none());
        }
    }
    #[test]
    fn partial_known_word_never_passes() {
        assert!(candidate(&(raw(17518) + &raw(65535)), "嬴外").is_none());
    }
    #[test]
    fn reversed_source_text_is_refused() {
        assert!(candidate(&(raw(17518) + &raw(18000)), "瀀嬴").is_none());
    }
    #[test]
    fn ordinary_PUA_and_multicodepoint_are_not_source_tags() {
        assert!(candidate("\u{E9F5}", "嬴").is_none());
    }
}
