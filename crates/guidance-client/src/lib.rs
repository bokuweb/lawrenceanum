//! 所管省庁の政府解釈 (ノーアクションレター・Q&A・ガイドライン等) のクライアント。
//!
//! 国税庁の法令解釈通達 (`tsutatsu-client`) を補完する、行政による法令解釈の一次資料を集める。
//! 省庁ごとに HTML の作りが違うので、取得とパースは省庁アダプタ ([`fsa`] など) に分け、
//! 出力は共通スキーマ [`GuidanceDoc`] (発出日・発出元・関連法令・本文/添付) に揃える。
//!
//! ## 収集対象 (2026-10 時点)
//!
//! - 金融庁 ([`fsa`]): 法令適用事前確認手続 (ノーアクションレター) の照会書・回答書、
//!   一般的な法令解釈に係る書面照会、広く共有することが有効な相談事例、
//!   「告示・ガイドライン・Q&A・法令解釈事例集一覧」に載る Q&A・ガイドラインの PDF/HTML。
//! - 金融庁のパブリックコメント回答 (「コメントの概要及びそれに対する金融庁の考え方」) は
//!   e-Gov パブリックコメントに結果公示として載るため `pubcomment-client` が集める。
//!   ここでは重複して取らない。
//!
//! ## ライセンス
//!
//! 金融庁ウェブサイトのコンテンツは、権利表記が無い限り「公共データ利用規約（第1.0版）」
//! (PDL1.0, CC BY 4.0 互換) に準拠して利用でき、商用利用・再配布も可。条件は出典の記載
//! (「出典：金融庁ウェブサイト（当該ページの URL）」) と、編集・加工した場合はその旨の記載、
//! 国が作成したかのような態様で公表しないこと
//! (<https://www.fsa.go.jp/rules/index.html>, PDL1.0:
//! <https://www.digital.go.jp/resources/open_data/public_data_license_v1.0>)。
//! 本文は PDF からの機械抽出 (= 加工) なので、各文書に [`GuidanceDoc::attribution`] として
//! 出典 URL と加工の旨を必ず持たせる。
//! robots.txt は不在 (`https://www.fsa.go.jp/robots.txt` は 404, 2026-10-20 確認)。
//! リクエスト間隔は HttpProvider が空ける (既定 1 秒、CI ではさらに長く取る)。
//!
//! index 時に LLM は呼ばない。表の列・見出し・本文の定型句だけで正規化する。

use anyhow::Result;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};

pub mod fsa;

pub const SCHEMA_VERSION: u32 = 1;

// ── 共通スキーマ ──────────────────────────────────────────────────

/// 政府解釈の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceKind {
    /// 法令適用事前確認手続 (ノーアクションレター) の照会と回答。
    NoActionLetter,
    /// 一般的な法令解釈に係る書面照会手続の照会と回答。
    WrittenInquiry,
    /// 広く共有することが有効な相談事例 (相談事例集)。
    ConsultationCases,
    /// Q&A / FAQ。
    Qa,
    /// ガイドライン・留意事項・原則など。
    Guideline,
    /// 法令解釈事例集。
    InterpretationCases,
    Other,
}

impl GuidanceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoActionLetter => "no_action_letter",
            Self::WrittenInquiry => "written_inquiry",
            Self::ConsultationCases => "consultation_cases",
            Self::Qa => "qa",
            Self::Guideline => "guideline",
            Self::InterpretationCases => "interpretation_cases",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoActionLetter => "ノーアクションレター",
            Self::WrittenInquiry => "書面照会への回答",
            Self::ConsultationCases => "相談事例",
            Self::Qa => "Q&A",
            Self::Guideline => "ガイドライン",
            Self::InterpretationCases => "法令解釈事例集",
            Self::Other => "その他",
        }
    }

    /// 文書 ID の種別部分。
    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::NoActionLetter => "nal",
            Self::WrittenInquiry => "inquiry",
            Self::ConsultationCases => "cases",
            Self::Qa => "qa",
            Self::Guideline => "guideline",
            Self::InterpretationCases => "jirei",
            Self::Other => "doc",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::NoActionLetter,
            Self::WrittenInquiry,
            Self::ConsultationCases,
            Self::Qa,
            Self::Guideline,
            Self::InterpretationCases,
            Self::Other,
        ]
        .into_iter()
        .find(|k| k.as_str() == s || k.id_prefix() == s)
    }

    /// 一覧の文書名から種別を推定する (Q&A > 事例集 > ガイドライン系 > その他)。
    pub fn classify(title: &str) -> Self {
        let t: String = title
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
            .map(|c| match c {
                'Ｑ' => 'Q',
                'Ａ' => 'A',
                '＆' => '&',
                c => c,
            })
            .collect::<String>()
            .to_ascii_uppercase();
        if t.contains("Q&A") || t.contains("FAQ") || t.contains("よくあるご質問") {
            Self::Qa
        } else if t.contains("事例集") {
            Self::InterpretationCases
        } else if ["ガイドライン", "留意事項", "指針", "原則", "着眼点"]
            .iter()
            .any(|k| t.contains(k))
        {
            Self::Guideline
        } else {
            Self::Other
        }
    }
}

/// 文書が触れる法令。`name` と `provisions` は原文のまま、`law_id`/`articles` は
/// 法令索引で解決できたときだけ入る (build-json 時に `linking` が埋める)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawRef {
    /// 法令名 (例: "銀行法")。旧法名の括弧書きは落とす。
    pub name: String,
    /// 該当条文の原文 (例: "第10条第２項、第12条")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provisions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub law_id: Option<String>,
    /// 解決した条番号 (例: "10", "52_36")。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub articles: Vec<String>,
}

impl LawRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provisions: None,
            law_id: None,
            articles: Vec::new(),
        }
    }
}

/// 文書を構成するファイルの役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocRole {
    /// 照会書。
    Inquiry,
    /// 回答書。
    Answer,
    /// 文書本体 (Q&A・ガイドライン・事例集)。
    Document,
}

impl DocRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inquiry => "照会書",
            Self::Answer => "回答書",
            Self::Document => "本文",
        }
    }
}

/// 一覧に載っているファイルへのリンク。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocLink {
    pub role: DocRole,
    /// 一覧上のリンク文字列 (例: "平成22年3月25日", "インサイダー取引規制に関するQ&A")。
    pub label: String,
    pub url: String,
}

/// 一覧ページから読み取った 1 文書分のメタ。ファイル本体の取得前の状態。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// `{ministry}-{kind}-{key}` (例: `fsa-nal-ginkou-10`, `fsa-qa-1a2b3c4d5e`)。
    pub id: String,
    /// 省庁スラッグ (例: "fsa")。
    pub ministry: String,
    pub ministry_name: String,
    pub kind: GuidanceKind,
    /// 一覧上の整理番号 (ノーアクションレター等)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    pub title: String,
    /// 発出日 (回答日 / 更新日 / 公表日)。ISO 8601。
    pub issued_on: Option<String>,
    /// 照会日 (ノーアクションレター・書面照会のみ)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquired_on: Option<String>,
    pub related_laws: Vec<LawRef>,
    /// 一覧上の分類見出し (例: "保険業法関連")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 一覧の備考 (例: "※金融商品取引法に係る同手続整理番号４と同内容")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub links: Vec<DocLink>,
    /// 一覧ページの URL。
    pub source_url: String,
}

/// 取得したファイル。原本は content-addressed (SHA-256) で保存し、抽出テキストを持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub role: DocRole,
    pub label: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// キャッシュ内の原本の相対パス (`guidance-assets/{ministry}/{sha256}.{ext}`)。配信 JSON には出さない。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_path: Option<String>,
    /// 抽出テキスト。配信 JSON では [`GuidanceDoc::body_text`] にまとめて、ここは落とす。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    /// 取得失敗・スキャン PDF などで本文が取れなかった理由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

impl Attachment {
    pub fn from_link(link: &DocLink) -> Self {
        Self {
            role: link.role,
            label: link.label.clone(),
            url: link.url.clone(),
            media_type: None,
            sha256: None,
            bytes: None,
            raw_path: None,
            text: None,
            extraction_method: None,
            extraction_error: None,
            fetched_at: None,
        }
    }
}

/// 正規化済みの政府解釈 1 件。`.cache/guidance/{ministry}/{id}.json` と
/// `public/guidance/{id}.json` の形。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuidanceDoc {
    pub schema_version: u32,
    pub id: String,
    pub ministry: String,
    pub ministry_name: String,
    pub kind: GuidanceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    pub title: String,
    pub issued_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquired_on: Option<String>,
    /// 発出元 (例: "金融庁監督局 銀行第一課長")。回答書から読めなければ省庁名。
    pub issuer: String,
    /// 回答の要旨 (回答書の「１．回答」の冒頭)。読めなければ None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub related_laws: Vec<LawRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// 添付から抽出した本文 (役割ごとに 【照会書】【回答書】 の見出しを付けて連結)。
    pub body_text: Option<String>,
    pub attachments: Vec<Attachment>,
    pub source_url: String,
    /// 利用条件 (例: "PDL1.0")。
    pub license: String,
    /// 出典と加工の旨の表記。
    pub attribution: String,
    pub fetched_at: String,
}

impl GuidanceDoc {
    /// 配信用にキャッシュ専用の項目 (添付ごとの本文・原本パス) を落とす。
    pub fn for_public(&self) -> Self {
        let mut d = self.clone();
        for a in &mut d.attachments {
            a.text = None;
            a.raw_path = None;
        }
        d
    }

    /// 一覧の行と比べてファイルを取り直す必要があるか (リンク先が変わった・取得に失敗していた)。
    pub fn needs_refetch(&self, listing: &Listing) -> bool {
        listing.links.iter().any(|l| {
            !self
                .attachments
                .iter()
                .any(|a| a.url == l.url && a.sha256.is_some())
        })
    }
}

/// 省庁ごとの利用条件。
#[derive(Debug, Clone, Copy)]
pub struct Ministry {
    pub slug: &'static str,
    pub name: &'static str,
    pub license: &'static str,
    /// 出典表記の書式。`{url}` を出典 URL に置き換える。
    pub attribution: &'static str,
}

/// 一覧と添付から正規化済みの文書を作る。発出元と要旨は回答書の本文から読む。
pub fn compose(
    ministry: &Ministry,
    listing: &Listing,
    attachments: Vec<Attachment>,
    fetched_at: &str,
) -> GuidanceDoc {
    let mut body = Vec::new();
    for a in &attachments {
        if let Some(t) = a.text.as_deref().filter(|t| !t.trim().is_empty()) {
            body.push(format!("【{}】\n{}", a.role.label(), t.trim()));
        }
    }
    let answer = attachments
        .iter()
        .find(|a| a.role == DocRole::Answer)
        .and_then(|a| a.text.as_deref());
    let issuer = answer
        .and_then(|t| find_issuer(t, ministry.name))
        .unwrap_or_else(|| ministry.name.to_string());
    let summary = answer.and_then(answer_summary);
    GuidanceDoc {
        schema_version: SCHEMA_VERSION,
        id: listing.id.clone(),
        ministry: listing.ministry.clone(),
        ministry_name: listing.ministry_name.clone(),
        kind: listing.kind,
        number: listing.number,
        title: listing.title.clone(),
        issued_on: listing.issued_on.clone(),
        inquired_on: listing.inquired_on.clone(),
        issuer,
        summary,
        related_laws: listing.related_laws.clone(),
        category: listing.category.clone(),
        note: listing.note.clone(),
        body_text: if body.is_empty() {
            None
        } else {
            Some(body.join("\n\n"))
        },
        attachments,
        source_url: listing.source_url.clone(),
        license: ministry.license.to_string(),
        attribution: ministry.attribution.replace("{url}", &listing.source_url),
        fetched_at: fetched_at.to_string(),
    }
}

// ── Provider ──────────────────────────────────────────────────────

/// 取得したファイル本体。
#[derive(Debug, Clone)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    /// Content-Type (パラメータ付きのまま)。
    pub content_type: Option<String>,
    pub fetched_at: String,
}

impl Fetched {
    /// パラメータを除いた MIME type。無ければ中身から推定する。
    pub fn media_type(&self) -> Option<String> {
        let ct = self
            .content_type
            .as_deref()
            .map(|c| {
                c.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
            .filter(|c| !c.is_empty() && c != "application/octet-stream");
        ct.or_else(|| {
            if self.bytes.starts_with(b"%PDF-") {
                Some("application/pdf".to_string())
            } else if looks_like_html(&self.bytes) {
                Some("text/html".to_string())
            } else {
                None
            }
        })
    }
}

/// 省庁アダプタの共通インタフェース。
pub trait GuidanceProvider: Send + Sync {
    fn ministry(&self) -> &'static Ministry;
    /// 一覧ページ群を巡回して文書メタを返す。個々の一覧ページの失敗は skip する。
    fn list(&self) -> Result<Vec<Listing>>;
    /// ファイル本体を取得する。
    fn fetch(&self, url: &str) -> Result<Fetched>;
    /// HTML 文書から、本文として一緒に取るべきファイル (Q&A の PDF 版など) を返す。
    fn follow_links(&self, _page_url: &str, _html: &str) -> Vec<DocLink> {
        Vec::new()
    }
}

/// オフライン用。ノーアクションレター 1 件 (照会書・回答書とも HTML) を返す。
pub struct MockProvider;

const MOCK_INQUIRY: &str = "<html><body><div id=\"main\"><p>照会者は、銀行法第10条第２項に規定する\
その他の銀行業に付随する業務に該当するか照会する。</p></div></body></html>";
const MOCK_ANSWER: &str = "<html><body><div id=\"main\"><p>金融庁監督局　銀行第一課長</p>\
<p>１．回答</p><p>照会書にある事実を前提とすると、銀行法第10条第２項柱書きに規定する業務に該当する。</p>\
<p>２．見解及び根拠</p><p>同法第12条の他業禁止規定には抵触しない。</p></div></body></html>";

impl GuidanceProvider for MockProvider {
    fn ministry(&self) -> &'static Ministry {
        &fsa::MINISTRY
    }

    fn list(&self) -> Result<Vec<Listing>> {
        let base = "https://www.fsa.go.jp/common/noact/kaitou_2/ginkou/index.html";
        Ok(vec![Listing {
            id: "fsa-nal-ginkou-10".to_string(),
            ministry: fsa::MINISTRY.slug.to_string(),
            ministry_name: fsa::MINISTRY.name.to_string(),
            kind: GuidanceKind::NoActionLetter,
            number: Some(10),
            title: "銀行法 第10条第２項、第12条 に関するノーアクションレター（整理番号10）"
                .to_string(),
            issued_on: Some("2010-03-25".to_string()),
            inquired_on: Some("2010-03-04".to_string()),
            related_laws: vec![LawRef {
                provisions: Some("第10条第２項、第12条".to_string()),
                ..LawRef::named("銀行法")
            }],
            category: None,
            note: None,
            links: vec![
                DocLink {
                    role: DocRole::Inquiry,
                    label: "平成22年3月4日".to_string(),
                    url: "https://www.fsa.go.jp/mock/inquiry.html".to_string(),
                },
                DocLink {
                    role: DocRole::Answer,
                    label: "平成22年3月25日".to_string(),
                    url: "https://www.fsa.go.jp/mock/answer.html".to_string(),
                },
            ],
            source_url: base.to_string(),
        }])
    }

    fn fetch(&self, url: &str) -> Result<Fetched> {
        let body = if url.contains("inquiry") {
            MOCK_INQUIRY
        } else {
            MOCK_ANSWER
        };
        Ok(Fetched {
            bytes: body.as_bytes().to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
            fetched_at: "2026-10-20T00:00:00Z".to_string(),
        })
    }
}

// ── テキスト処理の共通部品 ────────────────────────────────────────

pub(crate) fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}

/// 空白 (全角・NBSP 含む) を 1 つの半角空白に詰める。
pub(crate) fn squash(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn text_of(e: ElementRef) -> String {
    squash(&e.text().collect::<String>())
}

/// 全角数字を半角に。
pub(crate) fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            c => c,
        })
        .collect()
}

pub(crate) fn abs_url(page_url: &str, href: &str) -> String {
    reqwest::Url::parse(page_url)
        .and_then(|u| u.join(href.trim()))
        .map(|mut u| {
            u.set_fragment(None);
            u.to_string()
        })
        .unwrap_or_else(|_| href.to_string())
}

/// 短い安定 ID 片 (SHA-256 の先頭 10 桁)。
pub(crate) fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(s.as_bytes()))[..10].to_string()
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let head: Vec<u8> = bytes
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .take(64)
        .map(|b| b.to_ascii_lowercase())
        .collect();
    head.starts_with(b"<!doctype html") || head.starts_with(b"<html")
}

/// Content-Type の charset → `<meta charset>` → UTF-8 の順で文字コードを決めて復号する。
pub fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    let label_of = |s: &str| -> Option<String> {
        let lower = s.to_ascii_lowercase();
        let rest = &lower[lower.find("charset=")? + "charset=".len()..];
        let label: String = rest
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        (!label.is_empty()).then_some(label)
    };
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let enc = content_type
        .and_then(label_of)
        .or_else(|| label_of(&head))
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(bytes).0.into_owned()
}

/// HTML ページの本文テキスト。`#main` (省庁サイト共通の本文枠) → `main` → `body` の順に探し、
/// script/style・機械翻訳の注記・SNS ボタンは除く。ブロック要素と `<br>` で改行する。
pub fn html_main_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let root = ["#main .inner", "#main", "main", "body"]
        .iter()
        .find_map(|s| doc.select(&sel(s)).next())
        .unwrap_or_else(|| doc.root_element());
    let mut out = String::new();
    walk_text(root, &mut out);
    let mut lines: Vec<String> = Vec::new();
    for line in out.lines() {
        let l = line.trim_matches(|c: char| c.is_whitespace() || c == '\u{a0}');
        if l.is_empty() {
            continue;
        }
        lines.push(l.to_string());
    }
    lines.join("\n")
}

fn walk_text(el: ElementRef, out: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(t) => out.push_str(&t.replace(['\r', '\n'], "")),
            Node::Element(e) => {
                let name = e.name();
                if matches!(name, "script" | "style" | "noscript") {
                    continue;
                }
                let class = e.attr("class").unwrap_or("");
                if class.contains("MT_note") || class.contains("share-button") {
                    continue;
                }
                let block = matches!(
                    name,
                    "p" | "div"
                        | "br"
                        | "li"
                        | "tr"
                        | "table"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "dt"
                        | "dd"
                        | "ul"
                        | "ol"
                        | "center"
                );
                if block {
                    out.push('\n');
                }
                if name == "td" || name == "th" {
                    out.push(' ');
                }
                if let Some(child_el) = ElementRef::wrap(child) {
                    walk_text(child_el, out);
                }
                if block {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
}

/// 和暦 (例: `令和７年10月30日`, `平成 22 年３月 25 日`, `平成元年5月1日`) → `YYYY-MM-DD`。
/// 文字列中の最初の和暦日付を読む。
pub fn wareki_to_iso(s: &str) -> Option<String> {
    let s: String = ascii_digits(s)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect();
    for (era, base) in [("令和", 2018), ("平成", 1988), ("昭和", 1925)] {
        let mut from = 0;
        while let Some(pos) = s[from..].find(era) {
            let start = from + pos + era.len();
            from = start;
            let rest = &s[start..];
            let Some((y, rest)) = rest.split_once('年') else {
                continue;
            };
            let y: i32 = if y == "元" {
                1
            } else {
                match y.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                }
            };
            let Some((m, rest)) = rest.split_once('月') else {
                continue;
            };
            let Some((d, _)) = rest.split_once('日') else {
                continue;
            };
            let (Ok(m), Ok(d)) = (m.parse::<u32>(), d.parse::<u32>()) else {
                continue;
            };
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(base + y, m, d) {
                return Some(date.format("%Y-%m-%d").to_string());
            }
        }
    }
    None
}

/// 回答書本文から発出元 (「金融庁監督局 銀行第一課長」など) を読む。氏名は落とす。
pub fn find_issuer(text: &str, ministry_name: &str) -> Option<String> {
    const ROLES: &[&str] = &[
        "課長",
        "室長",
        "局長",
        "部長",
        "長官",
        "参事官",
        "審議官",
        "企画官",
        "監理官",
    ];
    for line in text.lines() {
        let Some(start) = line.find(ministry_name) else {
            continue;
        };
        let tail = &line[start..];
        // 「金融庁における法令適用事前確認手続（回答書）」のような表題は除く。
        let Some(end) = ROLES
            .iter()
            .filter_map(|r| tail.find(r).map(|i| i + r.len()))
            .min()
        else {
            continue;
        };
        let issuer = squash(&tail[..end]);
        if issuer.chars().count() <= 40 {
            return Some(issuer);
        }
    }
    None
}

/// 回答書の回答部分の冒頭を要旨として返す (最大 200 字)。
/// 回答の始まりは「１．回答」節 → 「記」の行 → 「…照会文書に対する回答）」「下記のとおり回答します。」
/// の行の直後、の順に探す (年代によって回答書の体裁が違うため)。
pub fn answer_summary(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let norm = |l: &str| -> String {
        ascii_digits(l)
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
            .collect()
    };
    let is_heading = |l: &str, n: char| {
        let l = norm(l);
        let mut cs = l.chars();
        cs.next() == Some(n) && matches!(cs.next(), Some('.' | '．' | '、' | ')' | '）'))
    };
    let after = |pred: &dyn Fn(&str) -> bool| lines.iter().position(|l| pred(l)).map(|i| i + 1);
    let start = after(&|l| is_heading(l, '1') && norm(l).contains("回答"))
        .or_else(|| after(&|l| norm(l) == "記"))
        .or_else(|| {
            after(&|l| {
                let l = norm(l);
                l.ends_with("に対する回答）") || l.ends_with("回答します。")
            })
        })?;
    let mut buf = String::new();
    for l in &lines[start..] {
        let n = norm(l);
        if is_heading(l, '2') || n.starts_with("（注") || n == "以上" {
            break;
        }
        // 「下記のとおり回答します。」の後に「記」が続く体裁では「記」を飛ばす。
        if buf.is_empty() && (n.is_empty() || n == "記") {
            continue;
        }
        // PDF の行折り返しを詰めて 1 段落にする (数字は原文の全角/半角のまま)。
        buf.extend(l.chars().filter(|c| !c.is_whitespace() && *c != '\u{a0}'));
        if buf.chars().count() > 400 {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }
    let mut out: String = buf.chars().take(200).collect();
    if buf.chars().count() > 200 {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wareki() {
        assert_eq!(
            wareki_to_iso("令和７年10月30日").as_deref(),
            Some("2025-10-30")
        );
        assert_eq!(
            wareki_to_iso("平成 22 年３月 25 日").as_deref(),
            Some("2010-03-25")
        );
        assert_eq!(
            wareki_to_iso("平成16年6月１日").as_deref(),
            Some("2004-06-01")
        );
        assert_eq!(
            wareki_to_iso("銀行法関係（令和４年７月15日更新）").as_deref(),
            Some("2022-07-15")
        );
        assert_eq!(
            wareki_to_iso("平成元年5月1日").as_deref(),
            Some("1989-05-01")
        );
        assert_eq!(wareki_to_iso("令和7年2月30日"), None);
        assert_eq!(wareki_to_iso("日付なし"), None);
    }

    #[test]
    fn classify_titles() {
        use GuidanceKind::*;
        assert_eq!(
            GuidanceKind::classify("インサイダー取引規制に関するQ&A"),
            Qa
        );
        assert_eq!(GuidanceKind::classify("保険業該当性に関するQ\u{a0}& A"), Qa);
        assert_eq!(GuidanceKind::classify("貸金業法に関するＱ＆Ａ"), Qa);
        assert_eq!(
            GuidanceKind::classify(
                "マネロン・テロ資金供与対策ガイドラインに関するよくあるご質問（FAQ）"
            ),
            Qa
        );
        assert_eq!(
            GuidanceKind::classify("証券法令解釈事例集"),
            InterpretationCases
        );
        assert_eq!(
            GuidanceKind::classify("犯罪収益移転防止法の留意事項"),
            Guideline
        );
        assert_eq!(
            GuidanceKind::classify("モデル・リスク管理に関する原則"),
            Guideline
        );
        assert_eq!(GuidanceKind::classify("所管告示一覧"), Other);
        assert_eq!(GuidanceKind::parse("nal"), Some(NoActionLetter));
        assert_eq!(GuidanceKind::parse("qa"), Some(Qa));
        assert_eq!(GuidanceKind::parse("x"), None);
    }

    #[test]
    fn issuer_and_summary_from_answer() {
        // 001_10b.pdf (銀行法 整理番号10) の pdftotext -layout 出力の抜粋。
        let text = "          金融庁における法令適用事前確認手続（回答書）\n\
                                 平成 22 年３月 25 日\n\
  （照会者）   殿\n\
                        金融庁監督局   銀行第一課長\n\
                    記\n\
１．回答\n\
   照会書にある事実を前提とすると、「本件不動産デリバティブ取引」は銀行法第\n\
 10 条第２項柱書きに規定する「その他の銀行業に付随する業務」に該当し、\n\
２．当該事実が照会法令の適用対象となることに関する見解及び根拠\n\
  本件不動産デリバティブ取引は…";
        assert_eq!(
            find_issuer(text, "金融庁").as_deref(),
            Some("金融庁監督局 銀行第一課長")
        );
        let s = answer_summary(text).unwrap();
        assert!(s.starts_with("照会書にある事実を前提とすると"), "{s}");
        assert!(s.contains("銀行法第10条第２項柱書き"), "{s}");
        assert!(!s.contains("見解及び根拠"));
    }

    #[test]
    fn issuer_drops_person_name() {
        let text = "金融庁総務企画局市場課長　大森 泰人\n記\n金融先物取引法は…できません。";
        assert_eq!(
            find_issuer(text, "金融庁").as_deref(),
            Some("金融庁総務企画局市場課長")
        );
        assert_eq!(
            answer_summary(text).as_deref(),
            Some("金融先物取引法は…できません。")
        );
    }

    #[test]
    fn summary_after_reply_heading() {
        // 036_01b.pdf (保険業法 整理番号１) の体裁: 「記」も節番号も無い。
        let text = "平成１６年４月１２日\n\n代表取締役 某   殿\n\n        金融庁監督局保険課長\n          某\n\n\
   「保険業法」に関する法令適用事前確認手続に係る照会について\n\
      （平成１６年３月１９日付照会文書に対する回答）\n\n\n\
 「保険業」を定義した保険業法第２条第１項にいう「不特定の者を相手方\n\
として」に該当するか否かは、総合的に判断することとなる。\n\
（注）本回答は、照会対象法令（条項）を所管する立場から…";
        assert_eq!(
            answer_summary(text).as_deref(),
            Some("「保険業」を定義した保険業法第２条第１項にいう「不特定の者を相手方として」に該当するか否かは、総合的に判断することとなる。")
        );
        // 「下記のとおり回答します。」の後に本文が続く体裁 (令和の回答書)。
        let text = "金融庁企画市場局市場課長\n照会のあった事例について、下記のとおり回答します。\n\n記\n\n 金融商品取引法第 167 条第１項に規定する…\n（注）本回答は";
        assert_eq!(
            answer_summary(text).as_deref(),
            Some("金融商品取引法第167条第１項に規定する…")
        );
    }

    #[test]
    fn main_text_skips_chrome() {
        let html = r#"<html><body><nav>メニュー</nav><div id="main">
          <p class="MT_note">Note: machine translated</p>
          <p class="share-button"><a>Tweet</a><script>var x = 1;</script></p>
          <div class="inner"><h1>貸金業法に関するQ&amp;A</h1><p>問１<br>答え</p></div>
        </div></body></html>"#;
        assert_eq!(html_main_text(html), "貸金業法に関するQ&A\n問１\n答え");
    }

    #[test]
    fn compose_builds_body_and_issuer() {
        let p = MockProvider;
        let listing = p.list().unwrap().remove(0);
        let atts: Vec<Attachment> = listing
            .links
            .iter()
            .map(|l| {
                let f = p.fetch(&l.url).unwrap();
                Attachment {
                    text: Some(html_main_text(&decode_html(
                        &f.bytes,
                        f.content_type.as_deref(),
                    ))),
                    sha256: Some("x".into()),
                    ..Attachment::from_link(l)
                }
            })
            .collect();
        let doc = compose(p.ministry(), &listing, atts, "2026-10-20T00:00:00Z");
        assert_eq!(doc.issuer, "金融庁監督局 銀行第一課長");
        assert!(doc
            .summary
            .as_deref()
            .unwrap()
            .starts_with("照会書にある事実"));
        let body = doc.body_text.as_deref().unwrap();
        assert!(body.starts_with("【照会書】"));
        assert!(body.contains("【回答書】"));
        assert!(doc.attribution.contains(&listing.source_url));
        assert!(!doc.needs_refetch(&listing));
        let public = doc.for_public();
        assert!(public.attachments.iter().all(|a| a.text.is_none()));
    }
}
