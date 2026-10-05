//! LawXML → 安定 JSON 正規化レイヤ。
//!
//! e-Gov の LawXML は深くネストするため、Phase 1.5 では以下の要素のみを抽出する:
//!   - `LawNum`, `LawTitle`, `PromulgationDate`
//!   - `Article` (`Num` 属性 → `article_id`)
//!   - `ArticleTitle`, `ArticleCaption`
//!   - `Paragraph` / `ParagraphNum`
//!   - 各段落配下の `Sentence` / `ParagraphSentence` (textを連結)
//!   - 各段落配下の `Item` / `Subitem1..10` (`Num` + `ItemTitle` + `ItemSentence` 配下の
//!     `Sentence`/`Column`)。`Paragraph.items` に構造化して持ち、`text` にも
//!     「十五　社外取締役　…」の形で番号付きの行として追記する
//!
//! `Chapter`, `Section`, `Subsection`, `Division` は構造上の階層を保つだけで、
//! `Article` 抽出には影響させない (`MainProvision` 配下のどこにあっても拾う)。
//!
//! 別表 (`AppdxTable`) は条文とは別に `appendix_tables` へ抽出する (表形式・号列挙形式の
//! 両方)。別記・様式・書式・別図・付録 (`AppdxNote` / `AppdxStyle` / `AppdxFormat` /
//! `AppdxFig` / `Appdx`) は書式見本や図が主で平文化しても意味が薄いため、`TOC` と
//! 合わせて引き続き配信対象外とする。

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
    /// 本則の別表 (`<AppdxTable>`)。文書順に `appdx_{index}` で発番する。
    /// 後方互換のため、空なら配信 JSON からも省略される。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appendix_tables: Vec<AppendixTable>,
    pub source: SourceMeta,
}

/// 別表 (`<AppdxTable>`)。
///
/// e-Gov XML では表 (`<TableStruct>`) で書かれるものと、号 (`<Item>`) の列挙で
/// 書かれるものの 2 形があり、それぞれ `rows` / `items` に入る (通常どちらか一方)。
/// 別記・様式・書式・別図・付録 (`AppdxNote` / `AppdxStyle` / `AppdxFormat` /
/// `AppdxFig` / `Appdx`)、附則別表 (`SupplProvisionAppdxTable`) と `TOC` は対象外。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendixTable {
    /// `appdx_{index}`。search.db の `article_id` や SPA のアンカーにもこの値を使う。
    pub appdx_id: String,
    /// 法令内での別表の通し番号 (1 始まり)。
    pub index: u32,
    /// `<AppdxTableTitle>` (e.g. "別表" / "別表第一")。
    pub title: Option<String>,
    /// `<RelatedArticleNum>` (e.g. "（第二条関係）")。
    pub related_article_num: Option<String>,
    /// `<TableStruct>` の行。ヘッダ行 (`<TableHeaderRow>`) も含む。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Vec<AppendixCell>>,
    /// 表ではなく号 (`<Item>`) で列挙された別表の各号。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<AppendixItem>,
    /// `<Remarks>` (備考) の各文。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remarks: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppendixCell {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rowspan: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colspan: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppendixItem {
    /// `<ItemTitle>` (e.g. "一")。
    pub title: Option<String>,
    /// 号の本文。細分 (`Subitem1` 等) は "イ　…" の形で改行区切りで続く。
    pub text: String,
}

impl AppendixTable {
    /// 見出し・表・号・備考を連結した平文。検索索引などに使う。
    pub fn plain_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let head = [self.title.as_deref(), self.related_article_num.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("");
        if !head.is_empty() {
            lines.push(head);
        }
        for row in &self.rows {
            lines.push(
                row.iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("　"),
            );
        }
        for it in &self.items {
            match it.title.as_deref() {
                Some(t) => lines.push(format!("{t}　{}", it.text)),
                None => lines.push(it.text.clone()),
            }
        }
        lines.extend(self.remarks.iter().cloned());
        lines.join("\n")
    }
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Paragraph {
    /// `<ParagraphNum>` の表示テキスト (e.g. "２")。第 1 項や旧様式の法令では空要素
    /// なので None になる。項の特定には `paragraph_num` を使うこと。
    pub paragraph_no: Option<String>,
    /// `<Paragraph Num="...">` 属性 (e.g. "1", "2")。`<ParagraphNum/>` が空でも入る。
    /// 後方互換のため、無ければ配信 JSON からも省略される。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph_num: Option<String>,
    /// 段落本文。項の文に続けて、号・号の細分を「十五　社外取締役　…」の形で
    /// 1 行ずつ番号付きで連結したもの (検索/FTS・表示用)。
    pub text: String,
    /// 段落配下の号 (`<Item>`)。構造化した番号・本文を持つ。空なら省略される。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
}

/// 号 (`<Item>`) と号の細分 (`<Subitem1>` 〜 `<Subitem10>`)。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// `Num` 属性 (e.g. "15", "1_2")。
    pub num: Option<String>,
    /// `<ItemTitle>` / `<SubitemNTitle>` (e.g. "十五", "一の二", "イ", "（１）")。
    pub title: Option<String>,
    /// 号の本文。定義規定などの `<Column>` は全角スペースで連結する
    /// (e.g. "社外取締役　株式会社の取締役であって、…")。
    pub text: String,
    /// 一段下の細分 (号 → イロハ → (1)(2) …)。空なら省略される。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subitems: Vec<Item>,
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

fn attr_value(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

/// `<AppdxTable>` を読んでいる間の組み立て状態。
///
/// `Sentence` の行き先は「表のセル > 備考 > 号」の優先順で決める
/// (セル内の Item や備考内の Item は、号ではなくセル/備考の文として扱う)。
struct AppdxBuilder {
    table: AppendixTable,
    row: Option<Vec<AppendixCell>>,
    cell: Option<AppendixCell>,
    item: Option<AppendixItem>,
    in_remarks: bool,
    /// `Subitem1Title` 等、次の文の頭に付ける見出し。
    pending_title: Option<String>,
    /// 直前に `</Column>` が閉じた = 次の文は同じ行の別カラム (全角空白で繋ぐ)。
    column_sep: bool,
}

impl AppdxBuilder {
    fn new(index: u32) -> Self {
        Self {
            table: AppendixTable {
                appdx_id: format!("appdx_{index}"),
                index,
                title: None,
                related_article_num: None,
                rows: Vec::new(),
                items: Vec::new(),
                remarks: Vec::new(),
            },
            row: None,
            cell: None,
            item: None,
            in_remarks: false,
            pending_title: None,
            column_sep: false,
        }
    }

    fn start(&mut self, name: &str, e: &quick_xml::events::BytesStart) {
        if name != "Column" && name != "Sentence" {
            self.column_sep = false;
        }
        match name {
            "TableRow" | "TableHeaderRow" => self.row = Some(Vec::new()),
            "TableColumn" | "TableHeaderColumn" => {
                let span = |k: &[u8]| attr_value(e, k).and_then(|v| v.parse().ok());
                self.cell = Some(AppendixCell {
                    text: String::new(),
                    header: name == "TableHeaderColumn",
                    rowspan: span(b"rowspan"),
                    colspan: span(b"colspan"),
                });
            }
            "Remarks" => self.in_remarks = true,
            "Item" if self.cell.is_none() && !self.in_remarks => {
                self.item = Some(AppendixItem::default());
            }
            _ => {}
        }
    }

    fn end(&mut self, name: &str, text: &str) {
        match name {
            "AppdxTableTitle" if !text.is_empty() => self.table.title = Some(text.to_string()),
            "RelatedArticleNum" if !text.is_empty() => {
                self.table.related_article_num = Some(text.to_string())
            }
            "ItemTitle" | "Subitem1Title" | "Subitem2Title" | "Subitem3Title"
                if !text.is_empty() =>
            {
                match self.item.as_mut() {
                    Some(it)
                        if name == "ItemTitle"
                            && it.title.is_none()
                            && self.cell.is_none()
                            && !self.in_remarks =>
                    {
                        it.title = Some(text.to_string())
                    }
                    _ => self.pending_title = Some(text.to_string()),
                }
            }
            "Sentence" => self.push_sentence(text),
            "Column" => self.column_sep = true,
            "TableColumn" | "TableHeaderColumn" => {
                if let Some(mut c) = self.cell.take() {
                    // TableHeaderColumn は Sentence を挟まず直接テキストを持つ。
                    if c.text.is_empty() {
                        c.text = text.to_string();
                    }
                    if let Some(r) = self.row.as_mut() {
                        r.push(c);
                    }
                }
            }
            "TableRow" | "TableHeaderRow" => {
                if let Some(r) = self.row.take() {
                    if !r.is_empty() {
                        self.table.rows.push(r);
                    }
                }
            }
            "Remarks" => self.in_remarks = false,
            "Item" if self.cell.is_none() && !self.in_remarks => {
                if let Some(it) = self.item.take() {
                    self.table.items.push(it);
                }
            }
            _ => {}
        }
    }

    fn push_sentence(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let piece = match self.pending_title.take() {
            Some(t) => format!("{t}　{text}"),
            None => text.to_string(),
        };
        let sep = if std::mem::take(&mut self.column_sep) { "　" } else { "\n" };
        let target = if let Some(c) = self.cell.as_mut() {
            &mut c.text
        } else if self.in_remarks {
            self.table.remarks.push(piece);
            return;
        } else if let Some(it) = self.item.as_mut() {
            &mut it.text
        } else {
            return;
        };
        if !target.is_empty() {
            target.push_str(sep);
        }
        target.push_str(&piece);
    }
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

/// `Item` / `Subitem1` 〜 `Subitem10` に `suffix` ("", "Title", "Sentence") を付けた
/// 要素名か。
fn is_item_element(name: &str, suffix: &str) -> bool {
    let Some(base) = name.strip_suffix(suffix) else {
        return false;
    };
    base == "Item"
        || base
            .strip_prefix("Subitem")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

/// 号を `text` 用の 1 行 ("十五　社外取締役　…") に整形する。番号は ItemTitle を優先し、
/// 無ければ Num 属性で代用する。
fn render_item_line(item: &Item) -> String {
    match item.title.as_deref().or(item.num.as_deref()) {
        Some(label) if item.text.is_empty() => label.to_string(),
        Some(label) => format!("{label}\u{3000}{}", item.text),
        None => item.text.clone(),
    }
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
    // 開いている号・号の細分 (Item → Subitem1 → Subitem2 …)。
    let mut item_stack: Vec<Item> = Vec::new();
    // ItemSentence / SubitemNSentence の内側か。内側の Sentence は号の本文に集める。
    let mut in_item_sentence = false;
    // MainProvision に属さない top-level Paragraph (旧太政官布告等) を救う。
    let mut orphan_paragraphs: Vec<Paragraph> = Vec::new();

    let mut text_buf = String::new();

    // スコープスタック。Article は `current_scope()` の値で行き先を決める。
    let mut scope_stack: Vec<Scope> = vec![Scope::None];
    let mut appendix_tables: Vec<AppendixTable> = Vec::new();
    let mut appdx: Option<AppdxBuilder> = None;

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
                            paragraph_num: attr(&e, b"Num"),
                            ..Paragraph::default()
                        });
                        item_stack.clear();
                        in_item_sentence = false;
                    }
                    "Column" if in_item_sentence => {
                        // 定義規定の「用語」と「定義」の列は全角スペースで区切る。
                        if let Some(item) = item_stack.last_mut() {
                            if !item.text.is_empty() {
                                item.text.push('\u{3000}');
                            }
                        }
                    }
                    n if is_item_element(n, "") => {
                        item_stack.push(Item {
                            num: attr(&e, b"Num"),
                            ..Item::default()
                        });
                    }
                    n if is_item_element(n, "Sentence") => {
                        in_item_sentence = true;
                    }
                    // 配信対象外の構造ブロック (附則ではない別表・別紙系)。
                    // 配下の Article は articles/suppl どちらにも入れたくない。
                    "AppdxTable" | "AppdxNote" | "AppdxStyle" | "AppdxFig" | "AppdxFormat"
                    | "Appdx" | "TOC" => {
                        scope_stack.push(Scope::Other);
                        // 別表だけは配下を条文ではなく AppendixTable として拾う。
                        if name == "AppdxTable" {
                            appdx = Some(AppdxBuilder::new(appendix_tables.len() as u32 + 1));
                        }
                    }
                    _ => {}
                }
                if let Some(b) = appdx.as_mut() {
                    b.start(&name, &e);
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
                if let Some(b) = appdx.as_mut() {
                    b.end(&name, trimmed);
                    if name == "AppdxTable" {
                        if let Some(b) = appdx.take() {
                            appendix_tables.push(b.table);
                        }
                    }
                }
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
                    n if is_item_element(n, "Title") => {
                        if let Some(item) = item_stack.last_mut() {
                            if !trimmed.is_empty() {
                                item.title = Some(trimmed.to_string());
                            }
                        }
                    }
                    "Sentence" if in_item_sentence => {
                        // 号の本文 (本文 + ただし書 等) は 1 行に連結する。
                        if let Some(item) = item_stack.last_mut() {
                            item.text.push_str(trimmed);
                        }
                    }
                    n if is_item_element(n, "Sentence") => {
                        in_item_sentence = false;
                        if let Some(item) = item_stack.last_mut() {
                            // `<ItemSentence>本文</ItemSentence>` のように直接テキストを持つ形。
                            item.text.push_str(trimmed);
                            if let Some(p) = current_paragraph.as_mut() {
                                let line = render_item_line(item);
                                if !line.is_empty() {
                                    if !p.text.is_empty() {
                                        p.text.push('\n');
                                    }
                                    p.text.push_str(&line);
                                }
                            }
                        }
                    }
                    n if is_item_element(n, "") => {
                        if let Some(item) = item_stack.pop() {
                            if let Some(parent) = item_stack.last_mut() {
                                parent.subitems.push(item);
                            } else if let Some(p) = current_paragraph.as_mut() {
                                p.items.push(item);
                            }
                        }
                    }
                    "ParagraphSentence" | "Sentence" => {
                        if let Some(p) = current_paragraph.as_mut() {
                            if !trimmed.is_empty() {
                                if !p.text.is_empty() {
                                    p.text.push('\n');
                                }
                                p.text.push_str(trimmed);
                            }
                        }
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
        appendix_tables,
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
    fn parses_appdx_table_with_table_struct() {
        // 公益通報者保護法 別表 を TableStruct 形式で書いた縮約版。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>公益通報者保護法</LawTitle>
    <MainProvision>
      <Article Num="2">
        <ArticleTitle>第二条</ArticleTitle>
        <Paragraph><ParagraphSentence><Sentence>別表に掲げるもの</Sentence></ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
    <AppdxTable>
      <AppdxTableTitle>別表</AppdxTableTitle>
      <RelatedArticleNum>（第二条関係）</RelatedArticleNum>
      <TableStruct>
        <Table>
          <TableHeaderRow>
            <TableHeaderColumn>号</TableHeaderColumn>
            <TableHeaderColumn>法律</TableHeaderColumn>
          </TableHeaderRow>
          <TableRow>
            <TableColumn><Sentence>一</Sentence></TableColumn>
            <TableColumn><Sentence>刑法（明治四十年法律第四十五号）</Sentence></TableColumn>
          </TableRow>
          <TableRow>
            <TableColumn rowspan="2"><Sentence>二</Sentence></TableColumn>
            <TableColumn><Sentence>食品衛生法（昭和二十二年法律第二百三十三号）</Sentence><Sentence>（抄）</Sentence></TableColumn>
          </TableRow>
          <TableRow>
            <TableColumn colspan="2"><Sentence>政令で定めるもの</Sentence></TableColumn>
          </TableRow>
        </Table>
      </TableStruct>
      <Remarks>
        <RemarksLabel>備考</RemarksLabel>
        <Sentence>この表の解釈は政令で定める。</Sentence>
      </Remarks>
    </AppdxTable>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "416AC0000000122").unwrap();
        // 本則には混ざらない。
        assert_eq!(doc.articles.len(), 1);
        assert_eq!(doc.appendix_tables.len(), 1);
        let t = &doc.appendix_tables[0];
        assert_eq!(t.appdx_id, "appdx_1");
        assert_eq!(t.index, 1);
        assert_eq!(t.title.as_deref(), Some("別表"));
        assert_eq!(t.related_article_num.as_deref(), Some("（第二条関係）"));
        assert!(t.items.is_empty());
        assert_eq!(t.rows.len(), 4);
        assert!(t.rows[0].iter().all(|c| c.header));
        assert_eq!(t.rows[0][1].text, "法律");
        assert_eq!(t.rows[1][0].text, "一");
        assert_eq!(t.rows[1][1].text, "刑法（明治四十年法律第四十五号）");
        assert!(!t.rows[1][1].header);
        assert_eq!(t.rows[2][0].rowspan, Some(2));
        assert_eq!(
            t.rows[2][1].text,
            "食品衛生法（昭和二十二年法律第二百三十三号）\n（抄）"
        );
        assert_eq!(t.rows[3][0].colspan, Some(2));
        assert_eq!(t.remarks, vec!["この表の解釈は政令で定める。".to_string()]);
        let text = t.plain_text();
        assert!(text.contains("刑法（明治四十年法律第四十五号）"));
        assert!(text.contains("この表の解釈"));
    }

    #[test]
    fn parses_appdx_table_with_items() {
        // 実 e-Gov XML (416AC0000000122) の別表は TableStruct ではなく Item 列。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>公益通報者保護法</LawTitle>
    <MainProvision>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph><ParagraphSentence><Sentence>本則。</Sentence></ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
    <AppdxTable>
      <AppdxTableTitle WritingMode="vertical">別表</AppdxTableTitle>
      <RelatedArticleNum>（第二条関係）</RelatedArticleNum>
      <Item Num="1">
        <ItemTitle>一</ItemTitle>
        <ItemSentence><Sentence Num="1">刑法（明治四十年法律第四十五号）</Sentence></ItemSentence>
      </Item>
      <Item Num="8">
        <ItemTitle>八</ItemTitle>
        <ItemSentence><Sentence Num="1">前各号に掲げるもののほか、政令で定めるもの</Sentence></ItemSentence>
        <Subitem1 Num="1">
          <Subitem1Title>イ</Subitem1Title>
          <Subitem1Sentence><Sentence>細目。</Sentence></Subitem1Sentence>
        </Subitem1>
      </Item>
    </AppdxTable>
    <AppdxTable>
      <AppdxTableTitle>別表第二</AppdxTableTitle>
      <TableStruct><Table><TableRow><TableColumn><Sentence>x</Sentence></TableColumn></TableRow></Table></TableStruct>
    </AppdxTable>
    <AppdxStyle>
      <AppdxStyleTitle>様式第一</AppdxStyleTitle>
      <StyleStruct><Style><Sentence>様式は拾わない。</Sentence></Style></StyleStruct>
    </AppdxStyle>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "416AC0000000122").unwrap();
        assert_eq!(doc.articles.len(), 1);
        assert_eq!(doc.articles[0].paragraphs[0].text, "本則。");
        assert_eq!(doc.appendix_tables.len(), 2);
        let t = &doc.appendix_tables[0];
        assert_eq!(t.appdx_id, "appdx_1");
        assert!(t.rows.is_empty());
        assert_eq!(t.items.len(), 2);
        assert_eq!(t.items[0].title.as_deref(), Some("一"));
        assert_eq!(t.items[0].text, "刑法（明治四十年法律第四十五号）");
        assert_eq!(t.items[1].title.as_deref(), Some("八"));
        assert_eq!(t.items[1].text, "前各号に掲げるもののほか、政令で定めるもの\nイ　細目。");
        assert!(t.plain_text().contains("一　刑法（明治四十年法律第四十五号）"));

        let t2 = &doc.appendix_tables[1];
        assert_eq!(t2.appdx_id, "appdx_2");
        assert_eq!(t2.index, 2);
        assert_eq!(t2.title.as_deref(), Some("別表第二"));
        assert_eq!(t2.related_article_num, None);
        assert_eq!(t2.rows[0][0].text, "x");

        // 様式 (AppdxStyle) は対象外。
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("様式は拾わない"));
    }

    #[test]
    fn omits_appendix_tables_from_json_when_empty() {
        let xml = r#"<Law><LawBody><LawTitle>x</LawTitle><MainProvision/></LawBody></Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "x").unwrap();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("appendix_tables"));
        // 旧 JSON (フィールド無し) も読める。
        let back: LawDocument = serde_json::from_str(&json).unwrap();
        assert!(back.appendix_tables.is_empty());
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
        // ItemTitle が無い場合は Num 属性で番号付けする。
        assert!(p.text.contains("1　一つ目。"));
        assert!(p.text.contains("2　二つ目。"));
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[0].num.as_deref(), Some("1"));
        assert_eq!(p.items[0].text, "一つ目。");
    }

    #[test]
    fn numbers_items_with_item_title_and_nested_sentence() {
        // 実 e-Gov XML では ItemSentence の中に Sentence があり、ItemTitle が号番号を持つ
        // (労働基準法施行規則 第五条 の構造)。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>労働基準法施行規則</LawTitle>
    <MainProvision>
      <Article Num="5">
        <ArticleTitle>第五条</ArticleTitle>
        <Paragraph Num="1">
          <ParagraphNum/>
          <ParagraphSentence>
            <Sentence Function="main" Num="1">明示しなければならない労働条件は、次に掲げるものとする。</Sentence>
          </ParagraphSentence>
          <Item Num="1">
            <ItemTitle>一</ItemTitle>
            <ItemSentence>
              <Sentence Num="1">労働契約の期間に関する事項</Sentence>
            </ItemSentence>
          </Item>
          <Item Num="1_2">
            <ItemTitle>一の二</ItemTitle>
            <ItemSentence>
              <Sentence Function="main" Num="1">有期労働契約を更新する場合の基準に関する事項</Sentence>
              <Sentence Function="proviso" Num="2">ただし、上限を含む。</Sentence>
            </ItemSentence>
          </Item>
        </Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "322M40000100023").unwrap();
        let p = &doc.articles[0].paragraphs[0];
        assert_eq!(
            p.text,
            "明示しなければならない労働条件は、次に掲げるものとする。\n\
             一　労働契約の期間に関する事項\n\
             一の二　有期労働契約を更新する場合の基準に関する事項ただし、上限を含む。"
        );
        assert_eq!(
            p.items,
            vec![
                Item {
                    num: Some("1".into()),
                    title: Some("一".into()),
                    text: "労働契約の期間に関する事項".into(),
                    subitems: vec![],
                },
                Item {
                    num: Some("1_2".into()),
                    title: Some("一の二".into()),
                    text: "有期労働契約を更新する場合の基準に関する事項ただし、上限を含む。".into(),
                    subitems: vec![],
                },
            ]
        );
    }

    #[test]
    fn numbers_column_definition_items_and_subitems() {
        // 会社法 第二条 (定義) の構造: ItemSentence が Column (用語 / 定義) の組を持ち、
        // Subitem1 (イロハ) / Subitem2 ((1)(2)) が続く。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>会社法</LawTitle>
    <MainProvision>
      <Article Num="2">
        <ArticleCaption>（定義）</ArticleCaption>
        <ArticleTitle>第二条</ArticleTitle>
        <Paragraph Num="1">
          <ParagraphNum/>
          <ParagraphSentence>
            <Sentence Num="1">この法律において、次の各号に掲げる用語の意義は、当該各号に定めるところによる。</Sentence>
          </ParagraphSentence>
          <Item Num="14">
            <ItemTitle>十四</ItemTitle>
            <ItemSentence>
              <Column Num="1"><Sentence Num="1">種類株主総会</Sentence></Column>
              <Column Num="2"><Sentence Num="1">種類株主の総会をいう。</Sentence></Column>
            </ItemSentence>
          </Item>
          <Item Num="15">
            <ItemTitle>十五</ItemTitle>
            <ItemSentence>
              <Column Num="1"><Sentence Num="1">社外取締役</Sentence></Column>
              <Column Num="2"><Sentence Num="1">株式会社の取締役であって、次に掲げる要件のいずれにも該当するものをいう。</Sentence></Column>
            </ItemSentence>
            <Subitem1 Num="1">
              <Subitem1Title>イ</Subitem1Title>
              <Subitem1Sentence><Sentence Num="1">業務執行取締役等でないこと。</Sentence></Subitem1Sentence>
            </Subitem1>
            <Subitem1 Num="2">
              <Subitem1Title>ロ</Subitem1Title>
              <Subitem1Sentence><Sentence Num="1">次のいずれかに該当しないこと。</Sentence></Subitem1Sentence>
              <Subitem2 Num="1">
                <Subitem2Title>（１）</Subitem2Title>
                <Subitem2Sentence><Sentence Num="1">親会社等</Sentence></Subitem2Sentence>
              </Subitem2>
            </Subitem1>
          </Item>
        </Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "417AC0000000086").unwrap();
        let p = &doc.articles[0].paragraphs[0];
        assert_eq!(
            p.text,
            "この法律において、次の各号に掲げる用語の意義は、当該各号に定めるところによる。\n\
             十四　種類株主総会　種類株主の総会をいう。\n\
             十五　社外取締役　株式会社の取締役であって、次に掲げる要件のいずれにも該当するものをいう。\n\
             イ　業務執行取締役等でないこと。\n\
             ロ　次のいずれかに該当しないこと。\n\
             （１）　親会社等"
        );

        assert_eq!(p.items.len(), 2);
        let item15 = p.items.iter().find(|i| i.num.as_deref() == Some("15")).unwrap();
        assert_eq!(item15.title.as_deref(), Some("十五"));
        assert_eq!(
            item15.text,
            "社外取締役　株式会社の取締役であって、次に掲げる要件のいずれにも該当するものをいう。"
        );
        assert_eq!(item15.subitems.len(), 2);
        assert_eq!(item15.subitems[0].num.as_deref(), Some("1"));
        assert_eq!(item15.subitems[0].title.as_deref(), Some("イ"));
        assert_eq!(item15.subitems[0].text, "業務執行取締役等でないこと。");
        assert_eq!(item15.subitems[1].title.as_deref(), Some("ロ"));
        assert_eq!(
            item15.subitems[1].subitems,
            vec![Item {
                num: Some("1".into()),
                title: Some("（１）".into()),
                text: "親会社等".into(),
                subitems: vec![],
            }]
        );
    }

    #[test]
    fn reads_paragraph_num_attribute_for_empty_paragraph_num() {
        // 旧様式の省令 (労働基準法施行規則等) は <ParagraphNum/> が空で、番号は
        // Num 属性にしかない。第 1 項も同様に空。
        let xml = r#"<?xml version="1.0"?>
<Law>
  <LawBody>
    <LawTitle>テスト規則</LawTitle>
    <MainProvision>
      <Article Num="5">
        <ArticleTitle>第五条</ArticleTitle>
        <Paragraph Num="1">
          <ParagraphNum/>
          <ParagraphSentence><Sentence Num="1">第一項。</Sentence></ParagraphSentence>
        </Paragraph>
        <Paragraph Num="2" OldNum="true" OldStyle="false">
          <ParagraphNum/>
          <ParagraphSentence><Sentence Num="1">第二項。</Sentence></ParagraphSentence>
        </Paragraph>
        <Paragraph Num="3">
          <ParagraphNum>３</ParagraphNum>
          <ParagraphSentence><Sentence Num="1">第三項。</Sentence></ParagraphSentence>
        </Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;
        let doc = parse_law_xml(xml.as_bytes(), "322M40000100023").unwrap();
        let ps = &doc.articles[0].paragraphs;
        assert_eq!(ps.len(), 3);
        let nums: Vec<Option<&str>> = ps.iter().map(|p| p.paragraph_num.as_deref()).collect();
        assert_eq!(nums, vec![Some("1"), Some("2"), Some("3")]);
        // paragraph_no は従来どおり ParagraphNum の表示テキスト。
        let nos: Vec<Option<&str>> = ps.iter().map(|p| p.paragraph_no.as_deref()).collect();
        assert_eq!(nos, vec![None, None, Some("３")]);
        assert_eq!(ps[1].text, "第二項。");
    }

    #[test]
    fn paragraph_json_stays_backward_compatible() {
        // 旧 JSON (paragraph_num / items 無し) をそのまま読めること。
        let old: Paragraph =
            serde_json::from_str(r#"{"paragraph_no":null,"text":"本文。"}"#).unwrap();
        assert_eq!(old.paragraph_num, None);
        assert!(old.items.is_empty());

        // 号の無い段落は items キーを出さない (既存 JSON とキー集合が増えるだけ)。
        let p = Paragraph {
            paragraph_no: None,
            paragraph_num: Some("1".into()),
            text: "本文。".into(),
            items: vec![],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"paragraph_no": null, "paragraph_num": "1", "text": "本文。"})
        );
    }
}
