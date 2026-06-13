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
    /// 本則 (`<MainProvision>`) 配下の条文のみ。`article_id = art_{Num}` で安定。
    pub articles: Vec<Article>,
    /// 附則 (`<SupplProvision>`) の集合。各 SupplProvision は別ブロックとして保持し、
    /// 本則の条文番号と衝突しないよう独立スコープの article_id を持つ。
    /// 後方互換のため、空なら配信 JSON からも省略される。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppl_provisions: Vec<SupplProvision>,
    pub source: SourceMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplProvision {
    /// 附則の通し番号 (1 始まり)。同じ法令内で複数 `<SupplProvision>` がある場合に分離する。
    pub index: u32,
    /// `<SupplProvision AmendLawNum="...">` の AmendLawNum 属性 (= 改正法令番号)。
    /// 制定時の元 SupplProvision には付かないことが多い。
    pub amend_law_num: Option<String>,
    /// 附則の見出し (e.g. "附則" / "附 則" / "附則（令和五年六月一四日法律第五十三号）")。
    pub label: Option<String>,
    /// 附則内の条文。article_id は本則と衝突しないよう `suppl{index}_art_{Num}` で発番。
    pub articles: Vec<Article>,
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

/// 元号 → 西暦元年 (元号1年に対応する西暦)。
fn era_start_year(era: &str) -> Option<i32> {
    match era {
        "Meiji" => Some(1868),
        "Taisho" => Some(1912),
        "Showa" => Some(1926),
        "Heisei" => Some(1989),
        "Reiwa" => Some(2019),
        _ => None,
    }
}

/// `<Law Era="Meiji" Year="29" PromulgateMonth="04" PromulgateDay="27">` から
/// "1896-04-27" を組み立てる。1 要素でも欠けたら None。
fn promulgation_date_from_law_attrs(e: &quick_xml::events::BytesStart) -> Option<String> {
    let mut era: Option<String> = None;
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    for a in e.attributes().flatten() {
        let v = String::from_utf8(a.value.into_owned()).ok()?;
        match a.key.as_ref() {
            b"Era" => era = Some(v),
            b"Year" => year = v.parse().ok(),
            b"PromulgateMonth" => month = v.parse().ok(),
            b"PromulgateDay" => day = v.parse().ok(),
            _ => {}
        }
    }
    let start = era_start_year(era.as_deref()?)?;
    let y = year?;
    let m = month?;
    let d = day?;
    Some(format!("{:04}-{:02}-{:02}", start + y - 1, m, d))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 条文を「どこに帰属させるか」を表す書き先。
///
/// XML を上から舐めながら、`<MainProvision>` に入ったら `Main`、
/// `<SupplProvision>` に入ったら `Suppl(idx)`、それ以外 (AppdxTable 等) は
/// `Other` にする。`Other` 配下の Article は配信対象から外す。
#[derive(Debug, Clone, Copy, PartialEq)]
enum Scope {
    None,
    Main,
    Suppl(u32),
    Other,
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
    let mut suppl_provisions: Vec<SupplProvision> = Vec::new();
    let mut suppl_count: u32 = 0;

    let mut current_article: Option<Article> = None;
    let mut current_paragraph: Option<Paragraph> = None;
    let mut current_item_num: Option<String> = None;
    // MainProvision に属さない top-level Paragraph (旧太政官布告等) を救う。
    let mut orphan_paragraphs: Vec<Paragraph> = Vec::new();

    let mut text_buf = String::new();

    // スコープスタック。Article は `current_scope()` の値で行き先を決める。
    let mut scope_stack: Vec<Scope> = vec![Scope::None];

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "Law" && promulgation_date.is_none() {
                    if let Some(d) = promulgation_date_from_law_attrs(&e) {
                        promulgation_date = Some(d);
                    }
                }
                match name.as_str() {
                    "MainProvision" => {
                        scope_stack.push(Scope::Main);
                    }
                    "SupplProvision" => {
                        suppl_count += 1;
                        let amend_law_num = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"AmendLawNum")
                            .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                        suppl_provisions.push(SupplProvision {
                            index: suppl_count,
                            amend_law_num,
                            label: None,
                            articles: Vec::new(),
                        });
                        scope_stack.push(Scope::Suppl(suppl_count));
                    }
                    "Article" => {
                        let num = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"Num")
                            .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                        let scope = *scope_stack.last().unwrap_or(&Scope::None);
                        let id = match scope {
                            Scope::Main | Scope::None => format!(
                                "art_{}",
                                num.clone().unwrap_or_else(|| (articles.len() + 1).to_string())
                            ),
                            Scope::Suppl(idx) => format!(
                                "suppl{}_art_{}",
                                idx,
                                num.clone().unwrap_or_else(|| {
                                    let n = suppl_provisions
                                        .last()
                                        .map(|s| s.articles.len() + 1)
                                        .unwrap_or(1);
                                    n.to_string()
                                })
                            ),
                            // Other (AppdxTable 等) 配下は配信対象外なので id は何でも良い。
                            Scope::Other => format!("ignored_{}", articles.len() + 1),
                        };
                        current_article = Some(Article {
                            article_id: id,
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
                    // 配信対象外の構造ブロック (附則ではない別表・別紙系)。
                    // 配下の Article は articles/suppl どちらにも入れたくない。
                    "AppdxTable" | "AppdxNote" | "AppdxStyle" | "AppdxFig" | "AppdxFormat"
                    | "Appdx" | "TOC" => {
                        scope_stack.push(Scope::Other);
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
                    "SupplProvisionLabel" => {
                        if let Some(sp) = suppl_provisions.last_mut() {
                            if !trimmed.is_empty() {
                                sp.label = Some(trimmed.to_string());
                            }
                        }
                    }
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
                                let scope = *scope_stack.last().unwrap_or(&Scope::None);
                                if matches!(scope, Scope::Main | Scope::None) {
                                    // 旧法 (太政官布告等) の MainProvision 直下 Paragraph。
                                    orphan_paragraphs.push(p);
                                }
                                // Suppl/Other 配下の orphan Paragraph は捨てる
                                // (現状ユースケース無し)。
                            }
                        }
                    }
                    "Article" => {
                        if let Some(mut a) = current_article.take() {
                            let scope = *scope_stack.last().unwrap_or(&Scope::None);
                            match scope {
                                Scope::Main | Scope::None => {
                                    if a.article_id.is_empty() {
                                        a.article_id = format!("art_{}", articles.len() + 1);
                                    }
                                    articles.push(a);
                                }
                                Scope::Suppl(_) => {
                                    if let Some(sp) = suppl_provisions.last_mut() {
                                        if a.article_id.is_empty() {
                                            a.article_id =
                                                format!("art_{}", sp.articles.len() + 1);
                                        }
                                        sp.articles.push(a);
                                    }
                                }
                                Scope::Other => {
                                    // 別表・別紙系は捨てる。
                                }
                            }
                        }
                    }
                    "MainProvision" | "SupplProvision" | "AppdxTable" | "AppdxNote"
                    | "AppdxStyle" | "AppdxFig" | "AppdxFormat" | "Appdx" | "TOC" => {
                        // 開きで push したスコープを閉じる。
                        if scope_stack.len() > 1 {
                            scope_stack.pop();
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
        suppl_provisions,
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
    fn parses_promulgation_date_from_law_attrs() {
        // e-Gov v1 API 実 XML は <PromulgationDate> 要素ではなく <Law> 属性に
        // Era / Year / PromulgateMonth / PromulgateDay を持つ。これを西暦に変換できること。
        let xml = r#"<?xml version="1.0"?>
<Law Era="Meiji" Year="29" PromulgateMonth="04" PromulgateDay="27">
  <LawBody><LawTitle>民法</LawTitle><MainProvision/></LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "129AC0000000089").unwrap();
        assert_eq!(doc.promulgation_date.as_deref(), Some("1896-04-27"));

        let xml2 = r#"<?xml version="1.0"?>
<Law Era="Reiwa" Year="5" PromulgateMonth="06" PromulgateDay="14">
  <LawBody><LawTitle>x</LawTitle><MainProvision/></LawBody>
</Law>"#;
        let doc2 = parse_law_xml(xml2.as_bytes(), "x").unwrap();
        assert_eq!(doc2.promulgation_date.as_deref(), Some("2023-06-14"));
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
    fn isolates_main_and_suppl_articles() {
        // 民法のように MainProvision と複数の SupplProvision が同じ Num="1" を
        // 持つケースで、article_id が衝突せず本則・附則が分離されることを確認する。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawNum>明治二十九年法律第八十九号</LawNum>
  <LawBody>
    <LawTitle>民法</LawTitle>
    <MainProvision>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <ArticleCaption>（基本原則）</ArticleCaption>
        <Paragraph>
          <ParagraphNum>1</ParagraphNum>
          <ParagraphSentence>私権は、公共の福祉に適合しなければならない。</ParagraphSentence>
        </Paragraph>
      </Article>
      <Article Num="2">
        <ArticleTitle>第二条</ArticleTitle>
        <Paragraph>
          <ParagraphSentence>本則の二条本文。</ParagraphSentence>
        </Paragraph>
      </Article>
    </MainProvision>
    <SupplProvision>
      <SupplProvisionLabel>附則</SupplProvisionLabel>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph>
          <ParagraphSentence>この法律は、公布の日から起算して施行する。</ParagraphSentence>
        </Paragraph>
      </Article>
    </SupplProvision>
    <SupplProvision AmendLawNum="令和五年法律第五十三号">
      <SupplProvisionLabel>附則（令和五年六月一四日法律第五十三号）</SupplProvisionLabel>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph>
          <ParagraphSentence>改正法附則の一条本文。</ParagraphSentence>
        </Paragraph>
      </Article>
    </SupplProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "129AC0000000089").unwrap();
        // 本則は 2 条のみ
        assert_eq!(doc.articles.len(), 2);
        assert_eq!(doc.articles[0].article_id, "art_1");
        assert_eq!(doc.articles[1].article_id, "art_2");
        assert!(doc.articles[0].paragraphs[0]
            .text
            .contains("公共の福祉"));
        // 附則 2 本が独立
        assert_eq!(doc.suppl_provisions.len(), 2);
        assert_eq!(doc.suppl_provisions[0].index, 1);
        assert_eq!(doc.suppl_provisions[0].articles[0].article_id, "suppl1_art_1");
        assert!(doc.suppl_provisions[0].articles[0].paragraphs[0]
            .text
            .contains("公布の日"));
        assert_eq!(doc.suppl_provisions[1].index, 2);
        assert_eq!(
            doc.suppl_provisions[1].amend_law_num.as_deref(),
            Some("令和五年法律第五十三号")
        );
        assert_eq!(doc.suppl_provisions[1].articles[0].article_id, "suppl2_art_1");
        assert!(doc.suppl_provisions[1].articles[0].paragraphs[0]
            .text
            .contains("改正法附則"));
    }

    #[test]
    fn excludes_appdx_articles() {
        // AppdxTable などの別表/別記配下に <Article> がある場合は articles に含めない。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>テスト法</LawTitle>
    <MainProvision>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph><ParagraphSentence>本則。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
    <AppdxTable>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph><ParagraphSentence>別表条文 (拾わない)。</ParagraphSentence></Paragraph>
      </Article>
    </AppdxTable>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "L").unwrap();
        assert_eq!(doc.articles.len(), 1);
        assert!(doc.articles[0].paragraphs[0].text.contains("本則"));
        assert_eq!(doc.suppl_provisions.len(), 0);
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
