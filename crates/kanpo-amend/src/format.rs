//! 改め文の形式判定（prose / shinkyu / unknown）。

/// 記事本文の形式を判定する。判定できない場合は `None`（呼び出し側でページ全体の
/// 判定にフォールバックできるよう Option を返す）。
pub fn detect_format_of(text: &str) -> Option<String> {
    match detect_format(text).as_str() {
        "unknown" => None,
        other => Some(other.to_string()),
    }
}

/// 改め文の形式を判定する。
///
/// 新旧対照表(shinkyu)は「改正後」「改正前」が**欄見出し（独立行）**として現れるもの
/// だけに限定する。単なる本文中の部分一致（廃止文や隣接記事の混入で出てくる「改正後／
/// 改正前」）を新旧対照表と誤判定しないため。これによりフロントの表組み可否（独立見出し
/// 行を要求する parseShinkyu）と判定が一致する。
pub(crate) fn detect_format(text: &str) -> String {
    let has_header = |label: &str| text.lines().any(|l| l.trim() == label);
    if has_header("改正後") && has_header("改正前") {
        return "shinkyu".to_string();
    }
    if text.contains("に改める")
        || text.contains("次のように改正する")
        || text.contains("を加える")
        || text.contains("を削る")
        || text.contains("廃止する")
        || text.contains("定める")
    {
        return "prose".to_string();
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_shinkyu() {
        // 「改正後」「改正前」が独立見出し行のときだけ shinkyu。
        assert_eq!(detect_format("改正後\n（見出し）\n改正前\n（見出し）"), "shinkyu");
        assert_eq!(detect_format("第一条中「甲」を「乙」に改める。"), "prose");
    }

    #[test]
    fn substring_kaiseigo_is_not_shinkyu() {
        // 本文中に部分一致で「改正後／改正前」が出ても（廃止文・隣接記事の混入など）
        // 独立見出し行でなければ新旧対照表とはしない。
        let repeal = "次に掲げる府令は、廃止する。\n改正後の規定は…改正前の例による。";
        assert_eq!(detect_format(repeal), "prose");
    }

    #[test]
    fn unknown_when_no_marker() {
        assert_eq!(detect_format("単なる本文。"), "unknown");
        assert_eq!(detect_format_of("単なる本文。"), None);
        assert_eq!(detect_format_of("廃止する。"), Some("prose".to_string()));
    }
}
