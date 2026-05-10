//! LawXML → 安定 JSON 正規化レイヤ。
//!
//! e-Gov の LawXML は深くネストするため、Phase 1.5 では以下の要素のみを抽出する:
//!   - `LawNum`, `LawTitle`, `PromulgationDate`
//!   - `Article` (`Num` 属性 → `article_id`)
//!   - `ArticleTitle`, `ArticleCaption`
//!   - `Paragraph` / `ParagraphNum`
//!   - 各段落配下の `Sentence` / `ParagraphSentence` (textを連結)
//!   - 各段落配下の `Item` (`Num` + `ItemSentence`/`Sentence` text を `text` に追記)
//!
//! `Chapter`, `Section`, `Subsection`, `Division` は構造上の階層を保つだけで、
//! `Article` 抽出には影響させない (`MainProvision` 配下のどこにあっても拾う)。

use anyhow::{Context, Result};
use chrono::Utc;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawSummary {
    pub law_id: String,
    pub law_num: Option<String>,
    pub title: String,
    pub current: String,
    pub timeline: String,
    pub versions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    pub provider: String,
    pub raw_xml_sha256: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawDocument {
    pub schema_version: u32,
    pub law_id: String,
    pub law_num: Option<String>,
    pub title: String,
    pub revision_id: Option<String>,
    pub promulgation_date: Option<String>,
    pub effective_date: Option<String>,
    pub status: String,
    pub articles: Vec<Article>,
    pub source: SourceMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub article_id: String,
    pub article_no: String,
    pub caption: Option<String>,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paragraph {
    pub paragraph_no: Option<String>,
    pub text: String,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn parse_law_xml(xml: &[u8], law_id: &str) -> Result<LawDocument> {
    let raw_sha = sha256_hex(xml);
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut law_num: Option<String> = None;
    let mut title: Option<String> = None;
    let mut promulgation_date: Option<String> = None;
    let mut articles: Vec<Article> = Vec::new();

    let mut current_article: Option<Article> = None;
    let mut current_paragraph: Option<Paragraph> = None;
    let mut current_item_num: Option<String> = None;
    // Article 配下に居ない Paragraph (= 太政官布告など旧法の素のParagraph) は
    // ここに溜め、最後に synthetic な「本則」article として吐き出す。
    let mut orphan_paragraphs: Vec<Paragraph> = Vec::new();

    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Article" => {
                        let num = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"Num")
                            .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                        current_article = Some(Article {
                            article_id: format!(
                                "art_{}",
                                num.clone().unwrap_or_else(|| (articles.len() + 1).to_string())
                            ),
                            article_no: String::new(),
                            caption: None,
                            paragraphs: Vec::new(),
                        });
                    }
                    "Paragraph" => {
                        current_paragraph = Some(Paragraph {
                            paragraph_no: None,
                            text: String::new(),
                        });
                    }
                    "Item" => {
                        current_item_num = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"Num")
                            .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                    }
                    _ => {}
                }
                text_buf.clear();
            }
            Ok(Event::Text(t)) => {
                let s = t.unescape().unwrap_or_default().into_owned();
                text_buf.push_str(&s);
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let collected = std::mem::take(&mut text_buf);
                let trimmed = collected.trim();
                match name.as_str() {
                    "LawNum" => law_num = Some(trimmed.to_string()),
                    "LawTitle" => title = Some(trimmed.to_string()),
                    "PromulgationDate" => promulgation_date = Some(trimmed.to_string()),
                    "ArticleTitle" => {
                        if let Some(a) = current_article.as_mut() {
                            a.article_no = trimmed.to_string();
                        }
                    }
                    "ArticleCaption" => {
                        if let Some(a) = current_article.as_mut() {
                            if !trimmed.is_empty() {
                                a.caption = Some(trimmed.to_string());
                            }
                        }
                    }
                    "ParagraphNum" => {
                        if let Some(p) = current_paragraph.as_mut() {
                            if !trimmed.is_empty() {
                                p.paragraph_no = Some(trimmed.to_string());
                            }
                        }
                    }
                    "ParagraphSentence" | "Sentence" | "ItemSentence" | "Subitem1Sentence"
                    | "Subitem2Sentence" => {
                        if let Some(p) = current_paragraph.as_mut() {
                            if !trimmed.is_empty() {
                                if !p.text.is_empty() {
                                    p.text.push('\n');
                                }
                                if name == "ItemSentence" || name == "Subitem1Sentence" || name == "Subitem2Sentence" {
                                    if let Some(num) = current_item_num.as_deref() {
                                        p.text.push_str(num);
                                        p.text.push(' ');
                                    }
                                }
                                p.text.push_str(trimmed);
                            }
                        }
                    }
                    "Item" => {
                        current_item_num = None;
                    }
                    "Paragraph" => {
                        if let Some(p) = current_paragraph.take() {
                            if let Some(a) = current_article.as_mut() {
                                a.paragraphs.push(p);
                            } else if !p.text.trim().is_empty() {
                                // Article に属さない top-level Paragraph (例: <MainProvision>
                                // 直下の Paragraph、AppdxNote 配下の Paragraph 等) は
                                // 後でまとめて synthetic article として救う。
                                orphan_paragraphs.push(p);
                            }
                        }
                    }
                    "Article" => {
                        if let Some(mut a) = current_article.take() {
                            if a.article_id.is_empty() {
                                a.article_id = format!("art_{}", articles.len() + 1);
                            }
                            articles.push(a);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e).context("xml parse"),
            _ => {}
        }
        buf.clear();
    }

    // 真に Article を持たない法令 (旧法の太政官布告など) は orphan で本則を救う。
    // Article が 1 件でもあれば orphan は無視 (本文外の付録扱い)。
    if articles.is_empty() && !orphan_paragraphs.is_empty() {
        articles.push(Article {
            article_id: "art_preamble".to_string(),
            article_no: "本則".to_string(),
            caption: None,
            paragraphs: orphan_paragraphs,
        });
    }

    Ok(LawDocument {
        schema_version: SCHEMA_VERSION,
        law_id: law_id.to_string(),
        law_num,
        title: title.unwrap_or_else(|| law_id.to_string()),
        revision_id: None,
        promulgation_date,
        effective_date: None,
        status: "current".to_string(),
        articles,
        source: SourceMeta {
            provider: "egov".to_string(),
            raw_xml_sha256: Some(raw_sha),
            fetched_at: Utc::now().to_rfc3339(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_lawxml() {
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawNum>明治二十九年法律第八十九号</LawNum>
  <LawBody>
    <LawTitle>民法</LawTitle>
    <MainProvision>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <ArticleCaption>基本原則</ArticleCaption>
        <Paragraph>
          <ParagraphNum>1</ParagraphNum>
          <ParagraphSentence>私権は、公共の福祉に適合しなければならない。</ParagraphSentence>
        </Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "129AC0000000089").unwrap();
        assert_eq!(doc.title, "民法");
        assert_eq!(doc.law_num.as_deref(), Some("明治二十九年法律第八十九号"));
        assert_eq!(doc.articles.len(), 1);
        assert_eq!(doc.articles[0].article_no, "第一条");
        assert_eq!(doc.articles[0].caption.as_deref(), Some("基本原則"));
        assert_eq!(doc.articles[0].paragraphs.len(), 1);
        assert!(doc.articles[0].paragraphs[0].text.contains("公共の福祉"));
    }

    #[test]
    fn parses_egov_dataroot_wrapped_law() {
        // 実 e-Gov v2 の `lawdata` エンドポイントは DataRoot/ApplData 包みで返す。
        // 内側の <Law> 配下の要素を拾えるか確認する。
        let xml = r#"<?xml version="1.0"?>
<DataRoot>
  <Result><Code>0</Code><Message></Message></Result>
  <ApplData>
    <LawId>129AC0000000089</LawId>
    <LawNum>明治二十九年法律第八十九号</LawNum>
    <LawFullText>
      <Law>
        <LawNum>明治二十九年法律第八十九号</LawNum>
        <PromulgationDate>1896-04-27</PromulgationDate>
        <LawBody>
          <LawTitle>民法</LawTitle>
          <MainProvision>
            <Article Num="1">
              <ArticleTitle>第一条</ArticleTitle>
              <ArticleCaption>基本原則</ArticleCaption>
              <Paragraph>
                <ParagraphNum>1</ParagraphNum>
                <ParagraphSentence>私権は、公共の福祉に適合しなければならない。</ParagraphSentence>
              </Paragraph>
            </Article>
          </MainProvision>
        </LawBody>
      </Law>
    </LawFullText>
  </ApplData>
</DataRoot>"#;
        let doc = parse_law_xml(xml.as_bytes(), "129AC0000000089").unwrap();
        assert_eq!(doc.title, "民法");
        assert_eq!(doc.law_num.as_deref(), Some("明治二十九年法律第八十九号"));
        assert_eq!(doc.promulgation_date.as_deref(), Some("1896-04-27"));
        assert_eq!(doc.articles.len(), 1);
    }

    #[test]
    fn synthesizes_preamble_for_law_without_articles() {
        // 105DF0000000337 (改暦ノ布告) のような <MainProvision><Paragraph> 直下構造を救う。
        let xml = r#"<?xml version="1.0"?>
<DataRoot>
  <Result><Code>0</Code></Result>
  <ApplData>
    <LawFullText>
      <Law>
        <LawNum>明治五年太政官布告第三百三十七号</LawNum>
        <LawBody>
          <LawTitle>明治五年太政官布告第三百三十七号（改暦ノ布告）</LawTitle>
          <MainProvision>
            <Paragraph Num="1">
              <ParagraphNum/>
              <ParagraphSentence>
                <Sentence>今般改暦ノ儀別紙詔書ノ通被仰出候条此旨相達候事</Sentence>
              </ParagraphSentence>
            </Paragraph>
          </MainProvision>
        </LawBody>
      </Law>
    </LawFullText>
  </ApplData>
</DataRoot>"#;
        let doc = parse_law_xml(xml.as_bytes(), "105DF0000000337").unwrap();
        assert_eq!(doc.articles.len(), 1);
        assert_eq!(doc.articles[0].article_id, "art_preamble");
        assert_eq!(doc.articles[0].article_no, "本則");
        assert!(doc.articles[0].paragraphs[0].text.contains("改暦"));
    }

    #[test]
    fn parses_chapters_and_items() {
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawNum>令和五年法律第一号</LawNum>
  <LawBody>
    <LawTitle>テスト法</LawTitle>
    <MainProvision>
      <Chapter Num="1">
        <ChapterTitle>第一章 総則</ChapterTitle>
        <Article Num="1">
          <ArticleTitle>第一条</ArticleTitle>
          <Paragraph>
            <ParagraphNum>1</ParagraphNum>
            <ParagraphSentence>本則。</ParagraphSentence>
            <Item Num="1">
              <ItemSentence>一つ目。</ItemSentence>
            </Item>
            <Item Num="2">
              <ItemSentence>二つ目。</ItemSentence>
            </Item>
          </Paragraph>
        </Article>
      </Chapter>
    </MainProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "L1").unwrap();
        assert_eq!(doc.articles.len(), 1);
        let p = &doc.articles[0].paragraphs[0];
        assert!(p.text.contains("本則。"));
        assert!(p.text.contains("1 一つ目。"));
        assert!(p.text.contains("2 二つ目。"));
    }
}
