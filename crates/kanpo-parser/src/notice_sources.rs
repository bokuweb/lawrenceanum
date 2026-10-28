//! Source references for independently signed procedures sharing financial statements.
//! Indices are zero based; end_line is exclusive. Referenced material remains in PageTrace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceSpan {
    pub page: usize,
    pub region: usize,
    pub start_line: usize,
    pub end_line: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StatementReference {
    pub source: Vec<SourceSpan>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NoticeScope {
    pub source: Vec<SourceSpan>,
    pub financial_statements: Vec<StatementReference>,
}

use crate::{lines::PageRules, trace::PageTrace};
use std::collections::BTreeSet;
pub(crate) struct Certificate {
    scope: NoticeScope,
    words: BTreeSet<(usize, usize)>,
}
pub(crate) fn certificates(pages: &[PageTrace], rules: &[Option<PageRules>]) -> Vec<Certificate> {
    let mut out = vec![];
    for (pi, page) in pages.iter().enumerate() {
        let Some(rule) = rules.get(pi).and_then(Option::as_ref) else {
            continue;
        };
        for scope in crate::financial_enclosures::notice_scopes(pi, page, rule, &pages[..pi]) {
            let mut words = BTreeSet::new();
            let mut valid = true;
            for span in &scope.source {
                for line in &page.regions[span.region].lines[span.start_line..span.end_line] {
                    if line.table.is_some() || line.words.is_empty() {
                        valid = false;
                    }
                    for &id in &line.words {
                        if !words.insert((pi, id)) {
                            valid = false;
                        }
                    }
                }
            }
            if valid && !words.is_empty() {
                out.push(Certificate { scope, words });
            }
        }
    }
    out
}
pub(crate) fn for_sources(
    certificates: &[Certificate],
    sources: impl IntoIterator<Item = (usize, usize)>,
) -> Vec<NoticeScope> {
    let sources = sources.into_iter().collect::<Vec<_>>();
    let unique = sources.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != sources.len() {
        return vec![];
    }
    certificates
        .iter()
        .filter(|c| c.words.is_subset(&unique))
        .map(|c| c.scope.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cert() -> Certificate {
        Certificate {
            scope: NoticeScope {
                source: vec![SourceSpan {
                    page: 0,
                    region: 0,
                    start_line: 0,
                    end_line: 2,
                }],
                financial_statements: vec![StatementReference {
                    source: vec![SourceSpan {
                        page: 0,
                        region: 0,
                        start_line: 2,
                        end_line: 4,
                    }],
                }],
            },
            words: BTreeSet::from([(0, 7), (0, 8)]),
        }
    }
    #[test]
    fn article_assignment_uses_complete_original_word_identity() {
        let c = cert();
        let want = c.scope.clone();
        assert_eq!(for_sources(&[c], [(0, 9), (0, 8), (0, 7)]), vec![want]);
    }
    #[test]
    fn an_incomplete_body_cannot_borrow_another_articles_attribution() {
        assert!(for_sources(&[cert()], [(0, 7)]).is_empty());
    }
    #[test]
    fn equal_values_on_other_pages_are_distinct_sources() {
        assert!(for_sources(&[cert()], [(1, 7), (1, 8)]).is_empty());
    }
    #[test]
    fn duplicate_word_ownership_cannot_authorize_a_source_graph() {
        assert!(for_sources(&[cert()], [(0, 7), (0, 8), (0, 7)]).is_empty());
    }
    #[test]
    fn unavailable_article_provenance_does_not_fall_back_to_string_matching() {
        assert!(for_sources(&[cert()], []).is_empty());
    }
}
