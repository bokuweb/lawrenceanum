//! ベンダ別パーサが出す「ブロック列」から条・項・号の階層を組み立てる。
//!
//! ぎょうせい (Reiki-Base) は項・号が条 (`div.article`) の外の兄弟要素として並び、
//! 第一法規 (d1w) は 1 行 1 div のフラットな列になる。どちらも文書順のブロック列に
//! 落としてから、ここで同じ規則で組み立てる。

use crate::text::{article_key, classify_line, LineKind};
use crate::{ReikiArticle, ReikiItem, ReikiParagraph, ReikiSupplementary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Title(String),
    Date(String),
    Number(String),
    /// 章・節などの見出し
    Heading(String),
    /// 条・項の見出し「(趣旨)」の中身
    Caption(String),
    Article {
        num: String,
        text: String,
    },
    Paragraph {
        num: Option<String>,
        text: String,
    },
    Item {
        num: String,
        level: u8,
        text: String,
    },
    /// 表は行をタブ区切り・改行区切りに平坦化したテキスト
    Table(String),
    /// 附則見出し（日付・番号・「抄」を含む全体）
    Supplementary(String),
    /// 改正履歴などの注記（本文には含めない）
    Note(String),
    /// 分類できない本文行（前文・制定文など）
    Text(String),
}

/// 1 行テキストを行頭記号でブロックにする（class を持たない HTML 用）。
pub fn block_from_line(line: &str) -> Option<Block> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (kind, body) = classify_line(line);
    Some(match kind {
        LineKind::Article(num) => Block::Article { num, text: body },
        LineKind::Heading => Block::Heading(body),
        LineKind::Caption(c) => Block::Caption(c),
        LineKind::Supplementary => Block::Supplementary(body),
        LineKind::Paragraph(num) => Block::Paragraph {
            num: Some(num),
            text: body,
        },
        LineKind::Item(num, level) => Block::Item {
            num,
            level,
            text: body,
        },
        LineKind::Text => Block::Text(body),
    })
}

#[derive(Debug, Default)]
pub struct Structured {
    pub title: Option<String>,
    pub date: Option<String>,
    pub number: Option<String>,
    /// 第1条より前の本文（制定文・前文・目次など）
    pub preamble: Vec<String>,
    pub articles: Vec<ReikiArticle>,
    pub supplementary: Vec<ReikiSupplementary>,
}

#[derive(Default)]
struct Builder {
    out: Structured,
    pending_caption: Option<String>,
    heading: Option<String>,
    in_suppl: bool,
}

impl Builder {
    fn articles_mut(&mut self) -> &mut Vec<ReikiArticle> {
        if self.in_suppl {
            &mut self
                .out
                .supplementary
                .last_mut()
                .expect("suppl exists")
                .articles
        } else {
            &mut self.out.articles
        }
    }

    fn new_article(&mut self, num: String) {
        let caption = self.pending_caption.take();
        let heading = if self.in_suppl {
            None
        } else {
            self.heading.clone()
        };
        let prefix = if self.in_suppl {
            format!("s{}_", self.out.supplementary.len())
        } else {
            String::new()
        };
        let idx = self.articles_mut().len();
        let key = if num.is_empty() {
            format!("{prefix}p{}", idx + 1)
        } else {
            format!(
                "{prefix}art_{}",
                article_key(&num).unwrap_or_else(|| (idx + 1).to_string())
            )
        };
        self.articles_mut().push(ReikiArticle {
            article_no: num,
            article_id: key,
            caption,
            heading,
            paragraphs: Vec::new(),
        });
    }

    fn current_article(&mut self) -> &mut ReikiArticle {
        if self.articles_mut().is_empty() {
            self.new_article(String::new());
        }
        self.articles_mut().last_mut().unwrap()
    }

    fn push_paragraph(&mut self, num: Option<String>, text: String) {
        // 附則の「1 この条例は…」のように条を持たない項は、無番号の条に入れる。
        let caption = self.pending_caption.take();
        let art = self.current_article();
        art.paragraphs.push(ReikiParagraph {
            num,
            caption,
            text,
            items: Vec::new(),
        });
    }

    fn current_paragraph(&mut self) -> &mut ReikiParagraph {
        let art = self.current_article();
        if art.paragraphs.is_empty() {
            art.paragraphs.push(ReikiParagraph::default());
        }
        art.paragraphs.last_mut().unwrap()
    }

    fn push_item(&mut self, num: String, level: u8, text: String) {
        let para = self.current_paragraph();
        let item = ReikiItem {
            num,
            text,
            subitems: Vec::new(),
        };
        // level 2/3 は直前の号（さらにその細分）にぶら下げる。親が無ければ同列に置く。
        let mut list = &mut para.items;
        for _ in 1..level {
            if list.is_empty() {
                break;
            }
            list = &mut list.last_mut().unwrap().subitems;
        }
        list.push(item);
    }

    /// 表・分類不能行。条の中なら直近の項（号）に追記し、条の前なら前文。
    fn push_text(&mut self, text: String) {
        let has_article = if self.in_suppl {
            self.out
                .supplementary
                .last()
                .is_some_and(|s| !s.articles.is_empty())
        } else {
            !self.out.articles.is_empty()
        };
        if !has_article {
            if self.in_suppl {
                // 附則直下の無番号本文（「この条例は、公布の日から施行する。」）
                self.push_paragraph(None, text);
            } else {
                self.out.preamble.push(text);
            }
            return;
        }
        let para = self.current_paragraph();
        if let Some(item) = last_item_mut(&mut para.items) {
            append_line(&mut item.text, &text);
        } else {
            append_line(&mut para.text, &text);
        }
    }

    fn feed(&mut self, b: Block) {
        match b {
            Block::Title(t) => {
                if self.out.title.is_none() {
                    self.out.title = Some(t.trim_start_matches('○').trim().to_string());
                }
            }
            Block::Date(d) => {
                if self.out.date.is_none() {
                    self.out.date = Some(d);
                }
            }
            Block::Number(n) => {
                if self.out.number.is_none() {
                    self.out.number = Some(n);
                }
            }
            Block::Heading(h) => {
                if !self.in_suppl {
                    self.heading = Some(h);
                }
            }
            Block::Caption(c) => self.pending_caption = Some(c),
            Block::Article { num, text } => {
                self.new_article(num);
                self.push_paragraph(None, text);
            }
            Block::Paragraph { num, text } => self.push_paragraph(num, text),
            Block::Item { num, level, text } => self.push_item(num, level, text),
            Block::Table(t) | Block::Text(t) => self.push_text(t),
            Block::Supplementary(title) => {
                self.in_suppl = true;
                self.pending_caption = None;
                self.out.supplementary.push(ReikiSupplementary {
                    title,
                    articles: Vec::new(),
                });
            }
            Block::Note(_) => {}
        }
    }
}

fn last_item_mut(items: &mut [ReikiItem]) -> Option<&mut ReikiItem> {
    let last = items.last_mut()?;
    if last.subitems.is_empty() {
        Some(last)
    } else {
        last_item_mut(&mut last.subitems).map(|i| i as _)
    }
}

fn append_line(dst: &mut String, line: &str) {
    if !dst.is_empty() {
        dst.push('\n');
    }
    dst.push_str(line);
}

fn render_items(items: &[ReikiItem], out: &mut String) {
    for it in items {
        out.push('\n');
        out.push_str(&it.num);
        out.push('\u{3000}');
        out.push_str(&it.text);
        render_items(&it.subitems, out);
    }
}

/// 条の平坦テキスト（検索・旧クライアント用）。番号は元表記のまま。
pub fn render_article(a: &ReikiArticle) -> String {
    let mut s = String::new();
    if let Some(c) = &a.caption {
        s.push('（');
        s.push_str(c);
        s.push_str("）\n");
    }
    for (i, p) in a.paragraphs.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        if let Some(c) = &p.caption {
            s.push('（');
            s.push_str(c);
            s.push_str("）\n");
        }
        match (i, &p.num) {
            (0, _) if !a.article_no.is_empty() => {
                s.push_str(&a.article_no);
                s.push('\u{3000}');
            }
            (_, Some(n)) => {
                s.push_str(n);
                s.push('\u{3000}');
            }
            _ => {}
        }
        s.push_str(&p.text);
        render_items(&p.items, &mut s);
    }
    s
}

pub fn build(blocks: impl IntoIterator<Item = Block>) -> Structured {
    let mut b = Builder::default();
    for blk in blocks {
        b.feed(blk);
    }
    b.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_article_paragraph_item_and_suppl() {
        let blocks = vec![
            Block::Title("○テスト条例".into()),
            Block::Date("平成16年9月29日".into()),
            Block::Number("条例第34号".into()),
            Block::Text("テスト条例を次のように定める。".into()),
            Block::Heading("第1章　総則".into()),
            Block::Caption("趣旨".into()),
            Block::Article {
                num: "第1条".into(),
                text: "この条例は…".into(),
            },
            Block::Paragraph {
                num: Some("2".into()),
                text: "市長は、次に掲げる".into(),
            },
            Block::Item {
                num: "(1)".into(),
                level: 1,
                text: "甲".into(),
            },
            Block::Item {
                num: "ア".into(),
                level: 2,
                text: "甲の細目".into(),
            },
            Block::Item {
                num: "(2)".into(),
                level: 1,
                text: "乙".into(),
            },
            Block::Table("名称\t位置".into()),
            Block::Note("(平成24条例21・一部改正)".into()),
            Block::Article {
                num: "第8条の2".into(),
                text: "削除".into(),
            },
            Block::Supplementary("附則".into()),
            Block::Text("この条例は、公布の日から施行する。".into()),
            Block::Supplementary("附則(平成22年3月23日条例第7号)抄".into()),
            Block::Caption("施行期日".into()),
            Block::Paragraph {
                num: Some("1".into()),
                text: "平成22年4月1日から施行する。".into(),
            },
        ];
        let s = build(blocks);
        assert_eq!(s.title.as_deref(), Some("テスト条例"));
        assert_eq!(
            s.preamble,
            vec!["テスト条例を次のように定める。".to_string()]
        );
        assert_eq!(s.articles.len(), 2);
        let a1 = &s.articles[0];
        assert_eq!(a1.article_id, "art_1");
        assert_eq!(a1.caption.as_deref(), Some("趣旨"));
        assert_eq!(a1.heading.as_deref(), Some("第1章　総則"));
        assert_eq!(a1.paragraphs.len(), 2);
        assert_eq!(a1.paragraphs[1].num.as_deref(), Some("2"));
        assert_eq!(a1.paragraphs[1].items.len(), 2);
        assert_eq!(a1.paragraphs[1].items[0].subitems[0].text, "甲の細目");
        // 表は直近の号に追記される
        assert!(a1.paragraphs[1].items[1].text.contains("名称\t位置"));
        assert!(a1
            .text()
            .starts_with("（趣旨）\n第1条\u{3000}この条例は…\n2\u{3000}市長は"));
        assert!(a1.text().contains("\nア\u{3000}甲の細目"));
        assert_eq!(s.articles[1].article_id, "art_8_2");

        assert_eq!(s.supplementary.len(), 2);
        assert_eq!(
            s.supplementary[0].articles[0].paragraphs[0].text,
            "この条例は、公布の日から施行する。"
        );
        assert_eq!(s.supplementary[0].articles[0].article_id, "s1_p1");
        let p = &s.supplementary[1].articles[0].paragraphs[0];
        assert_eq!(p.caption.as_deref(), Some("施行期日"));
        assert_eq!(p.num.as_deref(), Some("1"));
    }

    #[test]
    fn line_blocks() {
        assert_eq!(
            block_from_line("第１条　この条例は"),
            Some(Block::Article {
                num: "第１条".into(),
                text: "この条例は".into()
            })
        );
        assert_eq!(block_from_line("  "), None);
    }
}
