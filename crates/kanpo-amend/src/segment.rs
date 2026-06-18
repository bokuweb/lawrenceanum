//! 1 ページ内の複数記事（法令/告示）の分割。

/// 官報1ページ分の本文を「記事」単位に分割する。
///
/// 1ページに複数の法令/告示が詰まることがあるので、記事の先頭マーカで区切る。
/// - 省令・告示・規則: `〇` 見出し（例: 「〇総務省令第七十七号」）。
/// - 法律・政令の公布: 「{件名}をここに公布する。」で始まるブロック。
///   （これが無いと複数の政令が1記事に混ざり、呼び出し側の標題突合が誤った巨大ブロックを返す。）
pub fn segment_articles(text: &str) -> Vec<String> {
    let mut articles: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.lines() {
        if is_article_boundary(line) && !cur.is_empty() {
            articles.push(cur.join("\n").trim().to_string());
            cur = Vec::new();
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        let s = cur.join("\n").trim().to_string();
        if !s.is_empty() {
            articles.push(s);
        }
    }
    articles
}

/// その行が新しい記事の先頭か。
///
/// - 省令・告示・規則の `〇` 見出し（例: 「〇総務省令第七十七号」）
/// - 法律・政令の公布行（「…をここに公布する」）
/// - 「告示」「公告」「公示」の独立見出し行（新旧対照表ページに同居する別記事の境界。
///   例: 郵便法施行規則の表の下に並ぶ自転車の型式認定告示）
fn is_article_boundary(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('〇') || t.contains("をここに公布する") || matches!(t, "告示" | "公告" | "公示")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_articles_on_circle_heading() {
        let text = "〇総務省令第七十七号\n電波法の一部を改正する省令\n〇農林水産省告示第七百六十四号\n規格を廃止する件";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("電波法"));
        assert!(arts[1].contains("規格を廃止"));
    }

    #[test]
    fn segments_articles_on_kokuji_heading() {
        // 新旧対照表の下に同居する別記事(告示)を「告示」独立見出しで分離する。
        let text = "改正前\n第十六条〔同上〕\nこの省令は…から施行する。\n告示\n型式認定番号…\n交Ｎ2626…";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("第十六条") && !arts[0].contains("型式認定"));
        assert!(arts[1].starts_with("告示") && arts[1].contains("型式認定"));
    }

    #[test]
    fn segments_articles_on_promulgation_boundary() {
        // 同一ページに複数の政令公布が並ぶケース。「をここに公布する」で記事を分ける。
        let text = "労働組合法施行令の一部を改正する政令をここに公布する。\n御名御璽\n労働組合法施行令の一部を次のように改正する。\n美容師法施行令の一部を改正する政令をここに公布する。\n御名御璽\n美容師法施行令の一部を次のように改正する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("労働組合法施行令") && !arts[0].contains("美容師"));
        assert!(arts[1].contains("美容師法施行令") && !arts[1].contains("労働組合"));
    }
}
