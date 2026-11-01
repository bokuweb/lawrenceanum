//! デジタル官報の項目別 PDF パーサ。縦書きの読み順を復元し、改め文・新旧対照表を構造化する。
//!
//! 使い方は crate 直下の `README.md` を参照。
//!
//! デジタル官報 PDF はテキスト層を持つ（OCR 不要）が、**縦書き・多段組**のため
//! `pdftotext -layout` の素の出力には次の癖がある。
//!
//! 1. 縦書き presentation form の約物（︑ ︒ ﹇ ﹈ ︵ ︶ …）
//! 2. ページ柱（「官」「報」「令和 年 月 日」等の余白テキスト）の混入
//! 3. 新旧対照表での 改正後／改正前 2 カラムの並走（罫線でなく y 座標の帯で表現）
//!
//! 本クレートは座標付きテキスト（`pdftotext -bbox-layout` と同じ形の XHTML）から縦書きの読み順を
//! 復元し、(1)(2) を整形、(3) を改正後/改正前に分離する。PDF からの座標付きテキストと罫線の
//! 取り出しは pure Rust の kanpo-pdf が行う（poppler は要らない）。
//!
//! # モジュール構成
//!
//! - `normalize` — 約物正規化・PUA 除去・ページ柱ノイズ判定
//! - `vertical` — 縦書き読み順の復元（段分割・右→左再構成）
//! - `horizontal` — 回転ページ・横書きページの読み順復元（列分割・左→右再構成）
//! - `shinkyu` — 新旧対照表（改正後=上帯/改正前=下帯）の検出と分離
//! - `segment` — 1 ページ内の複数記事の分割
//! - `format` — 改め文の形式判定（prose / shinkyu / unknown）
//! - `lines` — 罫線（kanpo-pdf で抽出）
//!
//! # 例
//! ```no_run
//! let pdf: &[u8] = b"...";
//! let ex = kanpo_parser::extract(pdf).unwrap();
//! println!("{} ({})", ex.text, ex.format);
//! ```

use anyhow::Result;
mod notice_boundaries;
mod native_quote_scopes;
mod company_document_parts;
mod facility_continuations;
mod framed_band_continuations;
#[cfg(test)]
#[path = "paired_framed_continuation_shape_tests.rs"]
mod paired_framed_continuation_shape_tests;
mod paired_framed_continuations;
mod wrapped_notice_titles;
mod area_units;
mod blank_area_units;
mod chemical_line_end;
mod coordinate_rows;
mod coordinate_symbols;
mod framed_amount_rows;
mod inline_quotes;
mod micro_prefixes;
mod native_triangle_cells;
mod range_endpoints;
mod reference_markers;

mod cell_row_dashes;
mod financial_enclosures;
mod section_context;
pub mod notice_sources;
pub mod format;
mod fractions;
pub mod grid;
#[cfg(test)]
mod headed_data_column_tests;
mod headed_data_columns;
mod headerless_four_bands;
mod horizontal;

pub mod lines;
pub mod model;
mod nested_fraction_geometry;
mod nested_fractions;
pub mod normalize;
pub mod ocr_articles;
mod numeric_grid_orientation;
mod owned_dashes;
mod quantity_tables;
mod record_suffix;
mod ruled_plain_dashes;
pub mod segment;
mod shared_table_labels;
pub mod shinkyu;
pub mod table;
mod table_dashes;
pub mod trace;
mod unruled_road;
pub mod vertical;

pub use format::detect_format_of;
pub use model::{Block, Document, Run, ShinkyuRow};
pub use comparison_labels::ComparisonLabels;
pub use normalize::normalize_text;
pub use ocr_articles::{analyze_item_with_ocr, OcrPage, OcrRegion};
pub use segment::segment_articles;
pub use vertical::reconstruct_vertical;

/// 抽出結果。`text` は整形済み本文、`format` は "prose"/"shinkyu"/"unknown"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub text: String,
    pub format: String,
}

/// PDF バイト列（官報1ページ分）から、縦書きの読み順を復元したページ本文と
/// 形式判定を返す。
pub fn extract(pdf: &[u8]) -> Result<Extracted> {
    let xhtml = bbox_xhtml(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let format = format::detect_format(&text);
    Ok(Extracted { text, format })
}

/// 復元過程まで含めた解析結果（playground・デバッグ用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Analysis {
    /// ページごとの語・領域・行の割り当て。
    pub pages: Vec<trace::PageTrace>,
    /// [`reconstruct_vertical`] と同じ本文。
    pub text: String,
    /// 本文全体の形式判定（"prose"/"shinkyu"/"unknown"）。
    pub format: String,
    /// 記事ごとの本文と構造化結果。
    pub articles: Vec<ArticleAnalysis>,
}

/// 記事 1 件の本文と構造化結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArticleAnalysis {
    pub text: String,
    pub document: Document,
    /// Independently signed notices and their proven source statement references.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<notice_sources::NoticeScope>,
}

/// `pdftotext -bbox-layout` の XHTML を解析し、本文・記事・構造化結果に加えて、各語が
/// どの領域（前文・改正後・改正前・表外・本文）のどの行に組まれたかを返す。
/// poppler を使わないので WASM でも動く。
pub fn analyze(xhtml: &str) -> Analysis {
    analyze_with_rules(xhtml, &[])
}

/// [`analyze`] に各ページの罫線（[`page_rules`]）を与える版。`rules[i]` は i ページ目の罫線。
///
/// 罫線があると、新旧対照表の外枠で表の外の列（附則など）を分け、帯の中の罫線の表
/// （別表など）をセルの結合まで含めて 2D 表として復元し、構造化結果では本文の流れの中の
/// 表（[`model::Run::table`]）として返す。複数ページの XHTML（`<page>` が複数）も渡せる。
pub fn analyze_with_rules(xhtml: &str, rules: &[Option<lines::PageRules>]) -> Analysis {
    let pages = vertical::analyze_pages_with_rules(xhtml, rules);
    let join = |f: &dyn Fn(&trace::PageTrace) -> &str| {
        trace::join_page_texts(&pages, f)
    };
    let text = join(&|p| p.text.as_str());
    let doc_text = join(&|p| p.doc_text.as_str());
    let table_scopes=native_table_scopes::Scopes::for_pages(&pages,rules);
    let tables=table_scopes.tables();
    let mut segments = segment_articles(&doc_text);
    wrapped_notice_titles::repair_wrapped_notice_titles(&mut segments, &pages);
    wrapped_notice_titles::split_typeset_notice_titles(&mut segments, &pages, rules);
    financial_enclosures::repair_segments(&mut segments,&pages,rules);
    let quote_scopes=native_quote_scopes::Scopes::for_pages(xhtml,&pages,rules);
    let groups=if quote_scopes.is_empty(){notice_boundaries::groups(&segments,&doc_text,&pages,rules)}
        else{notice_boundaries::groups_with_scopes(&segments,&doc_text,&pages,rules,&quote_scopes)};
    let certificates=notice_sources::certificates(&pages,rules);
    let mut articles=Vec::new();
    let mut article_sources=Vec::new();
    for (segment,parts)in segments.into_iter().zip(groups){
        let original=Document::from_source_text_with_tables(&segment,&table_scopes);
        if parts.len()>1 {
            let desired:Vec<_>=parts.iter().map(|p|Document::from_source_text_with_tables(&p.text,&table_scopes)).collect();
            let documents=company_document_parts::repartition(&original,&desired)
                .or_else(||source_article_parts::documents_with_scopes(&original,&segment,&parts,&pages,&desired,&table_scopes));
            if let Some(documents)=documents{
                for (p,document) in parts.into_iter().zip(documents) {
                    let notices=notice_sources::for_sources(&certificates,p.sources.iter().copied());
                    article_sources.push(section_context::OwnedSources::from_partial_claims(&p.text,p.sources));
                    articles.push(ArticleAnalysis{text:render_table_markers(&p.text,&tables),document,notices});
                }continue;
            }
        }
        let sources=parts.into_iter().flat_map(|p|p.sources).collect::<Vec<_>>();
        let notices=notice_sources::for_sources(&certificates,sources.iter().copied());
        article_sources.push(section_context::OwnedSources::from_partial_claims(&segment,sources));
        articles.push(ArticleAnalysis{text:render_table_markers(&segment,&tables),document:original,notices});
    }
    section_context::repair(&mut articles,&mut article_sources,&pages,rules);
    Analysis {
        format: format::detect_format(&text),
        pages,
        text,
        articles,
    }
}

/// 項目（告示など）の 1 ページ分の入力。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PageInput {
    /// `pdftotext -bbox-layout` と同じ形の XHTML（1 ページ、[`bbox_xhtml`]）。
    pub xhtml: String,
    /// そのページの罫線（[`page_rules`]）。無ければ表の復元をしない。
    #[serde(default)]
    pub rules: Option<lines::PageRules>,
}

/// 複数ページにまたがる項目をまとめて解析する。ページで切れた新旧対照表は 1 つの表に
/// つなぎ、記事（`〇…号`）ごとに構造化する。
pub fn analyze_item(pages: &[PageInput]) -> Analysis {
    let xhtml = combine_pages(pages.iter().map(|p| p.xhtml.as_str()));
    let rules: Vec<Option<lines::PageRules>> = pages.iter().map(|p| p.rules.clone()).collect();
    analyze_with_rules(&xhtml, &rules)
}

/// [`analyze_item`] に、項目の前の頁（`context`、項目の頁の直前まで）を官庁報告の欄の文脈として足す版。
///
/// 官庁報告の欄（「国家試験」）の中では、公示の標題の行で記事を分ける（目次は官庁報告の公示を 1 件ずつ載せる）。
/// 欄見出しは項目より前の頁にあることがあり（号外第195号は34ページの「国家試験」の下に、41ページの
/// 「あん摩マッサージ指圧師国家試験の施行」）、項目の頁だけでは欄が分からず標題で分けられない。項目の頁に
/// 区分見出し・欄見出しが無く、`context` のうち区分見出し・欄見出しのある最後の頁が官庁報告の見出し（「官庁報告」
/// 「国家試験」）だけを持つなら、その頁から先を足して解析し、項目の頁の最初の記事の最初の行を含む記事から後ろだけを返す（足した頁の
/// 記事は選ばない）。そうでなければ [`analyze_item`] と同じ。公告の区分見出し（「公告」「諸事項」）は見ない。
pub fn analyze_item_in_context(context: &[PageInput], pages: &[PageInput]) -> Analysis {
    let base = analyze_item(pages);
    let words = |p: &PageInput| -> Vec<String> {
        p.xhtml.split("</word>").filter_map(|w| w.rsplit_once('>').map(|(_, t)| t.chars().filter(|c| !c.is_whitespace()).collect())).collect()
    };
    if pages.iter().any(|p| words(p).iter().any(|w| segment::is_section_label(w))) {
        return base;
    }
    // 前の頁のうち区分見出し・欄見出しのある最後の頁が、官庁報告の見出しだけを持つときに限る（公告の区分の後ろの項目には
    // 足さない）。
    let report = |w: &String| w == "官庁報告" || segment::is_report_field(w);
    let Some(from) = context.iter().rposition(|p| words(p).iter().any(|w| segment::is_section_label(w))) else { return base };
    let labels: Vec<String> = words(&context[from]).into_iter().filter(|w| segment::is_section_label(w)).collect();
    if !labels.iter().all(report) {
        return base;
    }
    let Some(first) = base.articles.first().and_then(|a| a.text.lines().find(|l| !l.trim().is_empty())).map(|l| l.trim().to_string()) else {
        return base;
    };
    let mut all = context[from..].to_vec();
    all.extend(pages.iter().cloned());
    let wide = analyze_item(&all);
    let Some(start) = wide.articles.iter().position(|a| a.text.lines().any(|l| l.trim() == first)) else { return base };
    // 足した頁で項目の頁の記事が増えない（欄の文脈で分かれる標題が無い）なら、足した頁の字を記事に入れない。
    if wide.articles.len() - start <= base.articles.len() {
        return base;
    }
    Analysis { articles: wide.articles[start..].to_vec(), ..base }
}

/// 目次の標題（例: 「○○規則の一部を改正する省令（総務七七）」）に最も合う記事の添字を返す。
///
/// 本文ヘッダには対象法令名のあとに法令番号「（…）」が挿入されるため、標題そのものの
/// 完全一致では拾えない。対象法令名（「の一部を」「を廃止」より前）を鍵に、まず完全一致、
/// 次に前方一致を縮めながら探し、制定文らしい（「次のように」「改める」等を含む）長い記事を採る。
///
/// 本文は行の途中で折り返されるので空白・改行を除いて比べる。対象法令名が短い（「種苗法」）
/// ときは制定機関略号を除いた標題全体を鍵にする。それでも見つからなければ、数字（漢数字・
/// 算用数字）とダッシュを除いて比べ直す（標題「人事院規則二―四」・本文「人事院規則２－４」）。
///
/// 標題で選べない「…関係」の項目は [`pick_listed_section`] で選ぶ。
///
/// 号外の 1 ページ目の目次（「破産、免責、再生関係　一」）は、項目の標題をそのまま載せるので標題の鍵に合う。選んだ記事が
/// その標題を目次の項目として載せる記事なら、その記事を除いて選び直す。
pub fn pick_article(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    let pick = |articles: &[ArticleAnalysis]| {
        if title.ends_with("関係") {
            // 目次は欄の公告を順に並べるので、標題の鍵で選んだ記事より前に、小見出しで選べる記事があればそちらから始まる
            // （号外第217号: 「独立行政法人北方領土問題対策協会…財務諸表、国立研究開発法人宇宙航空研究開発機構…関係」）。
            let picked = match (pick_article_by_title(title, articles), pick_listed_section(title, articles)) {
                (Some(t), Some(l)) => Some(t.min(l)),
                (t, l) => t.or(l),
            };
            // 目次が括弧にまとめて並べる名前（「令和七年度共済組合の決算（農林水産省・林野庁・国土交通省）」）の公告は、
            // 語の頭が本文の標題（「令和７年度農林水産省共済組合の決算公告」）と合わない。選んだ記事より前に、まとめた名前を
            // 標題に持つ記事があれば、そこから始まる（号外第162号）。
            picked.map(|i| (0..i).find(|&j| grouped_name_heading(title, &articles[j])).unwrap_or(i))
        } else {
            pick_article_by_title(title, articles).or_else(|| pick_listed_section(title, articles))
        }
    };
    match pick(articles) {
        Some(i) if lists_as_toc_entry(&articles[i], title) => {
            let mut rest = articles.to_vec();
            for a in rest.iter_mut().filter(|a| lists_as_toc_entry(a, title)) {
                a.text.clear();
            }
            pick(&rest)
        }
        picked => picked,
    }
}

/// 記事が目次（行の後ろに字間を空けて頁「　一」「　四〇」を置く行が 2 つ以上）か。
fn lists_toc_entries(a: &ArticleAnalysis) -> bool {
    let paged = |l: &str| {
        l.trim().rsplit_once('\u{3000}').is_some_and(|(head, page)| {
            !head.trim().is_empty() && !page.is_empty() && page.chars().all(|c| "〇一二三四五六七八九十百千".contains(c))
        })
    };
    a.text.lines().filter(|l| paged(l)).count() >= 2
}

/// 記事が、標題を目次の項目（標題の終わりの行の後ろに字間を空けて頁「　一」「　三三」）として載せるか。目次は長い標題を
/// 折り返す（「…一般旅券の返納 / 命令に関する通知関係　一」）ので、頁の前の字が標題の終わりの 2 字以上と合えばよい。
fn lists_as_toc_entry(a: &ArticleAnalysis, title: &str) -> bool {
    let title: String = title.chars().filter(|c| !c.is_whitespace()).collect();
    a.text.lines().any(|l| {
        let l = l.trim();
        let Some((head, page)) = l.rsplit_once('\u{3000}') else { return false };
        let head: String = head.chars().filter(|c| !c.is_whitespace()).collect();
        !page.is_empty()
            && page.chars().all(|c| "〇一二三四五六七八九十百千".contains(c))
            && head.chars().count() >= 2
            && title.ends_with(&head)
    })
}

/// 公告の欄をまとめた目次の項目（「相続、公示催告、失踪、…、所有者不明関係」「破産、免責、再生関係」）は、欄に載る
/// 小見出しの頭の語を「、」で並べ、末尾に「関係」を付ける。本文はその小見出し（「相続財産清算人の選任及び相続権主張の
/// 催告」「破産手続開始及び免責許可申立てに関する意見申述期間」）から始まり、標題そのものは載らない。区分見出し・欄見出し
/// （「公告」「諸事項」）の後の最初の行が、目次の最初の語で始まる記事が 1 つだけなら、それを採る。
fn pick_listed_section(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    for (word, whole) in listed_heading_words(title) {
        let (heads, inner) = listed_lines(&word, whole, articles);
        match (&heads[..], inner.is_empty()) {
            ([], true) => continue,
            ([i], _) => return Some(*i),
            // 小見出しが前の公告の記事の途中にあるなら、[`split_listed_sections`] で分けてから選ぶ。
            _ => return None,
        }
    }
    None
}

/// 語（`word`）で始まる行を持つ記事。区分見出し・欄見出しの後の最初の行がそうである記事（`heads`）と、記事の 2 行目以降
/// （区分見出しの直後を除く）の句点の無い行がそうである記事とその行の位置（`inner`）。`whole` なら、語で始まる行ではなく
/// 語の小見出しの行（語そのもの、または種類を付けた語）だけを比べる。
fn listed_lines(word: &str, whole: bool, articles: &[ArticleAnalysis]) -> (Vec<usize>, Vec<(usize, usize)>) {
    // 目次は漢数字（「農業協同組合法第七十三条第四項」）、本文は算用数字（「第73条第４項」）で組むので、数字を除いて比べる。
    let compact = |s: &str| {
        s.chars().filter(|c| !c.is_whitespace() && !c.is_ascii_digit() && !"０１２３４５６７８９〇一二三四五六七八九十百千".contains(*c)).collect::<String>()
    };
    let word = &compact(word);
    let (mut heads, mut inner) = (Vec::new(), Vec::new());
    for (i, a) in articles.iter().enumerate() {
        let lines: Vec<String> = a.text.split('\n').map(compact).collect();
        // 小見出しは折り返す（「金融商品取引業者に対する行 / 政処分の公告」）ので、語の長さまで次の行を続けて比べる。
        let starts = |j: usize| {
            let mut head = String::new();
            for l in lines[j..].iter().filter(|l| !l.is_empty()) {
                if head.chars().count() >= word.chars().count() {
                    break;
                }
                head.push_str(l);
            }
            head.starts_with(word)
        };
        // 欄の小見出しは、目次の語の前に種類を付けることがある（目次「財団」、小見出し「工場財団」）。句点の無い短い
        // 小見出し（語より 4 字長いまで）が語で終わるなら、語の小見出し。
        let qualified = |c: &String| !c.contains('。') && c.ends_with(word.as_str()) && c.chars().count() <= word.chars().count() + 4;
        let heading = |j: usize| if whole { qualified(&lines[j]) } else { starts(j) || qualified(&lines[j]) };
        let mut prev: Option<&String> = None;
        for (j, c) in lines.iter().enumerate() {
            if c.is_empty() {
                continue;
            }
            match prev {
                None if segment::is_section_label(c) => continue,
                None => {
                    if heading(j) {
                        heads.push(i);
                    }
                }
                Some(p) if !segment::is_section_label(p) && !c.contains('。') && (if whole { qualified(c) } else { starts(j) }) => inner.push((i, j)),
                Some(_) => {}
            }
            prev = Some(c);
        }
    }
    (heads, inner)
}

/// 「…関係」の項目の標題に並ぶ小見出しの頭の語（「相続、公示催告、失踪、…所有者不明関係」→「相続」「公示催告」…）。
/// 欄の小見出しは画像の上に載るなどして本文に無いことがある（裁判所の欄の「相続財産清算人の選任…」が画像で、
/// 「公示催告」から本文）ので、目次の順に、本文に小見出しのある最初の語で選ぶ。2 つ目からの語は、本文の別の行の頭と
/// 紛れないよう 4 字以上のものだけ。
pub(crate) fn listed_words(title: &str) -> Vec<String> {
    listed_heading_words(title).into_iter().filter(|(_, whole)| !whole).map(|(w, _)| w).collect()
}

/// [`listed_words`] に、2 つ目からの 4 字に満たない語（「前払式支払手段発行者の発行保証金に係る配当表、財団関係」の「財団」）
/// も目次の順に加える。短い語（`true`）は本文の別の行の頭（「財団法人…」）と紛れるので、小見出しの行（「工場財団」）とだけ
/// 比べる（[`listed_lines`] の `whole`）。項目の最初の語の公告が画像で本文に無い（本紙第1811号）と、欄はこの語の小見出しから始まる。
fn listed_heading_words(title: &str) -> Vec<(String, bool)> {
    let Some(listed) = title.strip_suffix("関係") else { return vec![] };
    listed
        .split(['、', '・'])
        .map(|w| w.chars().filter(|c| !c.is_whitespace()).collect::<String>())
        .enumerate()
        .filter(|(_, w)| !w.is_empty())
        .map(|(k, w)| {
            let whole = k > 0 && w.chars().count() < 4;
            (w, whole)
        })
        .collect()
}

/// 「…関係」の項目（`titles`）の欄の最初の小見出しが、前の公告の記事の途中から始まるなら、その行で記事を分ける。
///
/// 裁判所の欄の小見出し（「相続財産清算人の選任及び相続権主張の催告」）は、欄見出し「裁判所」が本文に無いと、
/// 前の欄の最後の公告（「令和８年７月16日　広島法務局」で終わる）の記事に続く（本紙第1749号）。目次の項目と記事を
/// 1 対 1 にするため、[`pick_listed_section`] で選べない項目について（標題で選べる記事があるなら、その記事の中だけ）、目次の語（[`listed_words`] の順に、行の見つかる
/// 最初の語）で始まる行（句点の無い行）が、どの記事の先頭にも無く、記事の 2 行目以降にちょうど 1 つあれば、その行から
/// 後ろを別の記事にする。分ける記事は、分ける行より後ろに表を持たないもので、公告の出典の証明を持たないものに限る
/// （[`split_paragraphs`]）。
pub fn split_listed_sections(titles: &[&str], articles: &mut Vec<ArticleAnalysis>) {
    for title in titles {
        // 標題で選んだ記事でも、最初の語の小見出しの行がその記事の途中にあるなら分ける（本紙第1806号: 「企業年金基金変更関係」
        // は鍵の字を含む裁判所の欄の記事を選ぶが、「企業年金基金変更公告」の行はその記事の 4 行目）。
        let picked = pick_article(title, articles);
        if picked.is_some_and(|i| pick_listed_section(title, articles) == Some(i)) {
            continue;
        }
        let mut split = None;
        // 標題で選んだ記事は項目の公告を含むので、2 つ目からの語の行（号外第196号の「令和８年度公害防止管理者等…」）では
        // 分けず、最初の語の行でだけ分ける。
        for (word, whole) in listed_heading_words(title).into_iter().take(if picked.is_some() { 1 } else { usize::MAX }) {
            let (heads, mut inner) = listed_lines(&word, whole, articles);
            // 号外の 1 ページ目の目次（「被害回復給付金裁定表記載関係　一」）は項目の標題を行に並べ、頁の字が次の行に落ちる
            // こともある（号外第170号の「破産、免責、再生関係」）。目次の記事の行は欄の小見出しではない。
            inner.retain(|&(i, _)| !lists_toc_entries(&articles[i]));
            // 語で始まる小見出しが続く（「相続財産清算人の選任及び相続権主張の催告」「相続権主張の催告」）なら、欄の始まりは最初の行。
            // 前の小見出しが 2 行に折り返す（号外第178号の「破産手続開始及び免責許可申 / 立てに関する意見申述期間」）と、間に
            // 句点の無い折り返しの行が 1 つ入る。
            let after_blank = |&(i, j): &(usize, usize), &(pi, pj): &(usize, usize)| {
                let between: Vec<&str> = articles[i].text.split('\n').skip(pj + 1).take(j.saturating_sub(pj + 1)).map(str::trim).filter(|l| !l.is_empty()).collect();
                i == pi && pj < j && (between.is_empty() || matches!(between[..], [l] if !l.contains('。')))
            };
            // 続く行どうしで比べる（3 行以上続いても、最初の行だけを残す）。
            let all = inner.clone();
            inner = all.iter().enumerate().filter(|&(k, at)| k == 0 || !after_blank(at, &all[k - 1])).map(|(_, at)| *at).collect();
            if heads.is_empty() && inner.is_empty() {
                continue;
            }
            if let ([], [at]) = (&heads[..], &inner[..]) {
                split = Some(*at);
            }
            break;
        }
        let Some((i, j)) = split.filter(|&(i, _)| picked.is_none_or(|p| p == i)) else { continue };
        let a = &articles[i];
        if !a.notices.is_empty() {
            continue;
        }
        let lines: Vec<&str> = a.text.split('\n').collect();
        let (head, tail) = (lines[..j].join("\n"), lines[j..].join("\n"));
        let Some((head_doc, tail_doc)) = split_paragraphs(&a.document, &tail) else { continue };
        articles.splice(
            i..=i,
            [
                ArticleAnalysis { text: head, document: head_doc, notices: vec![] },
                ArticleAnalysis { text: tail, document: tail_doc, notices: vec![] },
            ],
        );
    }
}

/// 文書（`doc`）を、本文の後ろの行（`tail`）の始まりで 2 つに分ける。後ろは `tail` から作り直し、前は `doc` のブロックを、
/// 作り直した後ろの字の手前で切る（区分見出し「公告」を別の段落にした構造や、前の公告の表を保つ）。作り直した後ろが段落
/// だけでない、後ろの字から後ろに表がある、作り直した後ろの字が `doc` の段落の字の終わりと合わない、のどれかなら分けない。
fn split_paragraphs(doc: &Document, tail: &str) -> Option<(Document, Document)> {
    let back = Document::from_text(tail);
    let text = |d: &Document| -> String {
        d.blocks.iter().filter_map(|b| if let Block::Paragraph { runs } = b { Some(runs) } else { None }).flatten().map(|r| r.text.as_str()).collect()
    };
    let plain = |b: &Block| matches!(b, Block::Paragraph { runs } if runs.iter().all(|r| r.table.is_none()));
    let (all, rest) = (text(doc), text(&back));
    if !back.blocks.iter().all(plain) || rest.is_empty() || rest.len() >= all.len() || !all.ends_with(&rest) {
        return None;
    }
    // 後ろの字より前（字の無い表を含む）は前に残す。後ろの字から後ろは、作り直した後ろと同じ段落の字だけ。
    let mut left = all.len() - rest.len();
    let mut started = false;
    let mut blocks = Vec::new();
    for b in &doc.blocks {
        if started {
            if !plain(b) {
                return None;
            }
            continue;
        }
        let Block::Paragraph { runs } = b else {
            blocks.push(b.clone());
            continue;
        };
        let mut kept = Vec::new();
        for r in runs {
            if started || (left == 0 && !r.text.is_empty()) {
                started = true;
                if r.table.is_some() {
                    return None;
                }
                continue;
            }
            let n = r.text.len().min(left);
            left -= n;
            started = n < r.text.len();
            let text = r.text[..n].trim_end_matches('\n');
            if !text.is_empty() || r.table.is_some() {
                kept.push(Run { text: text.to_string(), ..r.clone() });
            }
        }
        if !kept.is_empty() {
            blocks.push(Block::Paragraph { runs: kept });
        }
    }
    Some((Document { format: doc.format.clone(), blocks }, back))
}

fn pick_article_by_title(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    // ダッシュ類は標題（―）と本文（－）で字が違うことがあるので揃えて比べる。
    let unify = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| if matches!(c, '―' | '—' | '‐' | '‑' | '–' | '-') { '－' } else { c })
            .collect()
    };
    let loose = |s: &str| -> String {
        unify(s).chars().filter(|c| !c.is_ascii_digit() && !"０１２３４５６７８９〇一二三四五六七八九十百千－".contains(*c)).collect()
    };
    // 短い目次名（「内閣」）は本文中の部分一致には使えないが、独立した冒頭見出しなら照合できる。
    // 官庁報告の標題は、目次の標題から末尾の機関名（「（国土交通省）」）を除いたもの。
    let exact = unify(title);
    let bare = unify(title.rfind('（').filter(|&p| title.ends_with('）') && p > 0).map_or(title, |p| &title[..p]));
    let is_title = |l: &str| unify(l) == exact || unify(l) == bare;
    let headings: Vec<usize> = articles.iter().enumerate().filter(|(_, a)| {
        a.text.lines().filter(|l| !l.trim().is_empty()).take_while(|l| {
            segment::is_section_label(l) || is_title(l)
        }).any(|l| is_title(l))
    }).map(|(i, _)| i).collect();
    if headings.len() == 1 {
        return Some(headings[0]);
    }
    // 見出しも号数も無い規則（官庁報告の内閣官房の規則）は、制定文と日付の後に標題だけの行を載せる。目次の標題にも
    // 番号が無い（「（内閣官房）」「（同）」）。標題と同じ行を持つ記事が 1 つだけなら、それを採る（標題の前の方の字だけが
    // 合う別の規則の附則「…調査室組織規則…は、廃止する。」ではなく）。番号のある標題は見出しの号数で選ぶ（読み順の乱れで
    // 前の記事に入った標題の行には従わない）。
    let titled: Vec<usize> = articles.iter().enumerate().filter(|(_, a)| a.text.lines().any(|l| is_title(l))).map(|(i, _)| i).collect();
    if headings.is_empty() && titled.len() == 1 && title_number(title).is_none() && title_code(title).is_none() {
        return Some(titled[0]);
    }
    // 公告の標題が「…清算人就任」＋「公告」と折り返し、「公告」の行が区分見出しとして記事を分けると、標題の頭の行は
    // 前の記事の最後に残る（本紙第1761号31ページ）。目次の標題（「…関係」を除く）と同じ行で終わる記事の次の記事が
    // 「公告」「公示」の行から始まるなら、その記事を採る。
    let wrapped_head = unify(title.strip_suffix("関係").unwrap_or(title));
    if wrapped_head.chars().count() >= 6 {
        let follows: Vec<usize> = (1..articles.len())
            .filter(|&i| {
                articles[i - 1].text.lines().rev().find(|l| !l.trim().is_empty()).is_some_and(|l| unify(l) == wrapped_head)
                    && articles[i].text.lines().find(|l| !l.trim().is_empty()).is_some_and(|l| matches!(l.trim(), "公告" | "公示"))
            })
            .collect();
        if follows.len() == 1 {
            return Some(follows[0]);
        }
    }
    // 人事異動の目次名は欄に載る機関を空白で並べる（「内閣 財務省 最高裁判所」）。先頭の機関が冒頭見出しで、
    // どの機関も独立した行（長い機関名は 2 行に折り返す: 「特定複合観光施設区域整備推進本」「部事務局」）として載る記事を採る。
    let agencies: Vec<String> = title.split_whitespace().map(unify).collect();
    if agencies.len() > 1 {
        let listed: Vec<usize> = articles.iter().enumerate().filter(|(_, a)| {
            let lines: Vec<String> = a.text.lines().map(unify).filter(|l| !l.is_empty()).collect();
            let listed = |ag: &String| (0..lines.len()).any(|i| lines[i] == *ag || lines.get(i + 1).is_some_and(|next| format!("{}{next}", lines[i]) == *ag));
            lines.iter().take_while(|l| segment::is_section_label(l) || **l == agencies[0]).any(|l| *l == agencies[0])
                && agencies[1..].iter().all(listed)
        }).map(|(i, _)| i).collect();
        if listed.len() == 1 {
            return Some(listed[0]);
        }
    }
    let mut core = unify(&title_core(title));
    if core.chars().count() < 4 {
        core = unify(title.rfind('（').map_or(title, |p| &title[..p]));
        if core.chars().count() < 6 {
            return None;
        }
    }
    // 前置き（「…の規定に基づき、」「…の規定により、」）の付いた標題は、その後ろでも照合する。
    let mut cores = vec![core.clone()];
    for marker in ["規定に基づき、", "規定により、"] {
        if let Some(p) = core.rfind(marker) {
            let rest = core[p + marker.len()..].to_string();
            if rest.chars().count() >= 6 {
                cores.push(rest);
            }
        }
    }
    let keys_of = |cores: &[String]| -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for c in cores {
            keys.push(c.clone());
            let chars: Vec<char> = c.chars().collect();
            let mut len = chars.len().saturating_sub(2);
            while len >= 6 {
                keys.push(chars[..len].iter().collect());
                len = len.saturating_sub(4);
            }
        }
        keys.retain(|k| k.chars().count() >= 4);
        keys
    };
    let body_score = |s: &str| -> usize {
        let markers = ["の規定に基づき", "次のように", "に改める", "を加える", "定める"];
        markers.iter().filter(|m| s.contains(*m)).count() * 1000 + s.chars().count()
    };
    // 似た標題の告示（「保安林の指定をする件（農林水産一〇〇一）」「保安林の指定を解除する件（同一〇〇二）」）は
    // 同じ鍵に合うので、鍵に合う記事のうち見出しの号数が標題の番号と合うものがあれば、それを採る。
    let number = title_number(title);
    let code = title_code(title);
    let best = |keys: &[String], texts: &[String]| {
        let matched: Vec<(usize, &ArticleAnalysis)> = articles
            .iter()
            .enumerate()
            .filter(|(i, _)| keys.iter().any(|k| texts[*i].contains(k.as_str())))
            .collect();
        let numbered: Vec<(usize, &ArticleAnalysis)> = matched
            .iter()
            .copied()
            .filter(|(_, a)| {
                number.as_ref().is_some_and(|(agencies, n)| has_number(a, agencies, *n) || has_number_line(a, agencies, *n))
                    || code.as_ref().is_some_and(|code| has_code_line(a, code))
            })
            .collect();
        (if numbered.is_empty() { matched } else { numbered })
            .into_iter()
            .max_by_key(|(_, a)| body_score(&a.text))
            .map(|(i, _)| i)
    };
    let texts: Vec<String> = articles.iter().map(|a| unify(&a.text)).collect();
    let picked = best(&keys_of(&cores), &texts).or_else(|| {
        let cores: Vec<String> = cores.iter().map(|c| loose(c)).filter(|c| c.chars().count() >= 6).collect();
        let texts: Vec<String> = articles.iter().map(|a| loose(&a.text)).collect();
        best(&keys_of(&cores), &texts)
    });
    // 本文に標題を載せない告示（「〇消防庁告示第十四号」の下に本文だけ）は、鍵が号外の 1 ページ目の目次の項目か、似た標題の
    // 別の告示（「熊本県の一部の地域における…を延長する件」が並ぶ号外）にしか合わない。鍵に合った記事が目次の項目か、
    // 見出しに別の号数を持つ記事で、見出しの号数が合う記事が 1 つだけあれば、そちらを採る。見出しに号数の無い記事
    // （区分見出しで見出しと本文が分かれた「告示 / …を改正する告示」）はそのまま。
    let other_number = |a: &ArticleAnalysis, n: u64| heading_number(a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("")).is_some_and(|m| m != n);
    match picked {
        Some(i) if number.as_ref().is_some_and(|(agencies, n)| !has_number(&articles[i], agencies, *n) && !has_number_line(&articles[i], agencies, *n)
            && (is_toc_entry(&articles[i]) || other_number(&articles[i], *n))) => {
            pick_by_number(title, articles).or(picked)
        }
        _ => picked.or_else(|| pick_by_number(title, articles)),
    }
}

/// 標題の末尾の番号（「（同二八五）」「（総務二八五）」）と記事の見出しの号数（「〇総務省告示第二百八十五号」）で
/// 記事を選ぶ。目次の標題と本文の中の告示の名前が違うことがある（目次「…人体にばく露される電波の許容値に係る
/// 総務大臣が別に告示する無線設備を定める件」、本文「…第十四条の二第一項第二号等の規定に基づく総務大臣が
/// 別に告示する無線設備を定める件」）。見出しの号数が合う記事が 1 つだけのときに限る。機関名（「同」でなければ）
/// が標題にあれば、見出しにもあるものだけ。
fn pick_by_number(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    let (agencies, n) = title_number(title)?;
    let matched: Vec<usize> = articles
        .iter()
        .enumerate()
        .filter(|(_, a)| has_number(a, &agencies, n))
        .map(|(i, _)| i)
        .collect();
    (matched.len() == 1).then(|| matched[0])
}

/// 標題の末尾の番号が、ダッシュで区切った規則の番号（「（同九－一二三－四八）」「（人事院九―二四―二四）」）なら、その番号
/// （「九－一二三－四八」）。人事院規則は同じ鍵（「人事院規則九－」）に合う記事が並び、[`title_number`] は最後の組（「四八」）
/// しか読まない。
fn title_code(title: &str) -> Option<String> {
    let open = title.rfind('（')?;
    let inner = title[open + '（'.len_utf8()..].strip_suffix('）')?;
    let digit = |c: char| "〇一二三四五六七八九十百千".contains(c);
    let dash = |c: char| matches!(c, '―' | '—' | '‐' | '‑' | '–' | '-' | '－');
    let start = inner.find(digit)?;
    let code: String = inner[start..].chars().map(|c| if dash(c) { '－' } else { c }).collect();
    (code.contains('－') && code.chars().all(|c| digit(c) || c == '－') && !code.starts_with('－') && !code.ends_with('－')).then_some(code)
}

/// 記事の中に、規則の番号（「九－一二三－四八」）で終わる番号だけの行（「人事院規則九－一二三－四八」）があるか。
fn has_code_line(a: &ArticleAnalysis, code: &str) -> bool {
    a.text.lines().map(str::trim).any(|l| {
        let l: String = l.chars().map(|c| if matches!(c, '―' | '—' | '‐' | '‑' | '–' | '-') { '－' } else { c }).collect();
        l.ends_with(code) && !l.contains(['。', '、', '「', '（']) && !l[..l.len() - code.len()].ends_with(|c: char| "〇一二三四五六七八九十百千－".contains(c))
    })
}

/// 目次の項目か。目次は標題を番号（「（消防庁一四）」「（同一五）」）まで載せ、その後に頁（「　一」）を置く。本文の行は
/// 標題の番号で終わらない。
fn is_toc_entry(a: &ArticleAnalysis) -> bool {
    a.text.lines().any(|l| {
        let l = l.trim();
        let tagged = l.trim_end_matches(|c: char| "〇一二三四五六七八九十百千".contains(c)).trim_end();
        tagged.ends_with('）') && title_number(tagged).is_some_and(|(_, n)| n > 0) && {
            let inner = &tagged[tagged.rfind('（').unwrap() + '（'.len_utf8()..];
            !inner.starts_with(|c: char| "〇一二三四五六七八九".contains(c))
        }
    })
}

/// 標題の末尾の番号の機関名（「同」なら無し）と先頭の号数。
pub(crate) fn title_number(title: &str) -> Option<(Vec<&str>, u64)> {
    let open = title.rfind('（')?;
    let inner = title[open + '（'.len_utf8()..].trim_end_matches('）');
    let digit = |c: char| "〇一二三四五六七八九".find(c).map(|i| i / '〇'.len_utf8());
    // 号数は末尾の漢数字の並び（「防衛二五一～二五三」「同二五四、二五五」のように範囲・列挙のこともあり、
    // そのときは先頭の号数）。機関名が漢数字で始まる（「九州地方整備局九八」）ので、先頭の漢数字では切らない。
    let start = inner
        .char_indices()
        .find(|&(i, c)| digit(c).is_some() && inner[i..].chars().all(|c| digit(c).is_some() || "～〜、・".contains(c)))?
        .0;
    let agency = &inner[..start];
    let n = inner[start..].chars().map_while(digit).fold(0u64, |n, d| n * 10 + d as u64);
    let agencies: Vec<&str> = if agency == "同" { Vec::new() } else { agency.split('・').filter(|a| !a.is_empty()).collect() };
    Some((agencies, n))
}

/// 標題の末尾に並ぶ号数（「（農林水産九九七、九九八）」「（同一〇〇三～一〇一四）」「（同九九一、九九三～九九五）」）を
/// すべて取り出す。号数が 2 つ以上のときだけ返す。範囲（「～」）は両端を含む。
fn title_numbers(title: &str) -> Option<(Vec<&str>, Vec<u64>)> {
    let (agencies, _) = title_number(title)?;
    let open = title.rfind('（')?;
    let inner = title[open + '（'.len_utf8()..].trim_end_matches('）');
    let digit = |c: char| "〇一二三四五六七八九".find(c).map(|i| (i / '〇'.len_utf8()) as u64);
    let start = inner.char_indices().find(|&(i, c)| digit(c).is_some() && inner[i..].chars().all(|c| digit(c).is_some() || "～〜、・".contains(c)))?.0;
    let number = |s: &str| (!s.is_empty()).then(|| s.chars().map(|c| digit(c).unwrap()).fold(0u64, |n, d| n * 10 + d));
    let mut numbers = Vec::new();
    for part in inner[start..].split(['、', '・']) {
        match part.split_once(['～', '〜']) {
            Some((a, b)) => {
                let (a, b) = (number(a)?, number(b)?);
                if b < a || b - a > 1000 {
                    return None;
                }
                numbers.extend(a..=b);
            }
            None => numbers.push(number(part)?),
        }
    }
    (numbers.len() > 1).then_some((agencies, numbers))
}

/// 目次の項目に当たる記事をすべて選ぶ。号数を並べた項目（「肥料を登録した件（農林水産九九七、九九八）」
/// 「保安林の指定をする件（農林水産一〇一九～一〇三四）」）は、告示ごとに `〇` の見出しで記事が分かれるので、
/// [`pick_article`] の選んだ最初の号数の記事から後ろへ、最初の見出しと同じ機関・種類（「〇国土交通省告示」）で
/// 号数が項目の号数に入る見出しの記事を選ぶ。項目の号数をすべて選び終えるか、区分見出し・公布文に来たら止める。
/// - 号数が別の項目と交互に並ぶ（「（同九九一、九九三～九九五）」と「（同九九二、九九六、九九七）」）ので、
///   号数の入らない見出しの記事は飛ばして続ける。読み順で間に入った別の機関の告示も飛ばす。
/// - 選んだ記事に続く見出しの無い記事（頁をまたいだ告示の続き）と、号数の無い `〇` で始まる行の記事
///   （折り返した「〇番三まで」）は、その告示の続きとして含める。
/// 号数が 1 つの項目や、最初の記事が最初の号数の見出しを持たないときは [`pick_article`] の 1 記事だけ。
/// [`pick_article`] がどの記事も選ばない欄の項目（「会社決算公告」）は [`pick_series`] で選ぶ。
pub fn pick_articles(title: &str, articles: &[ArticleAnalysis]) -> Vec<usize> {
    let Some(first) = pick_article(title, articles) else { return pick_series(title, articles) };
    let Some((agencies, numbers)) = title_numbers(title) else { return vec![first] };
    if !has_number(&articles[first], &agencies, numbers[0]) {
        return vec![first];
    }
    let head_of = |a: &ArticleAnalysis| a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    let kind = |head: &str| head.find('第').map(|p| head[..p].to_string());
    let first_kind = kind(&head_of(&articles[first]));
    let mut rest: std::collections::BTreeSet<u64> = numbers[1..].iter().copied().collect();
    let mut picked = vec![first];
    let mut member = true;
    for (j, a) in articles.iter().enumerate().skip(first + 1) {
        if rest.is_empty() {
            break;
        }
        let head = head_of(a);
        if segment::is_section_label(&head) || head.contains("をここに公布する") {
            break;
        }
        match heading_number(&head).filter(|_| segment::is_heading_mark(&head)) {
            Some(n) => {
                member = kind(&head) == first_kind && rest.remove(&n);
                if member {
                    picked.push(j);
                }
            }
            None if member => picked.push(j),
            None => {}
        }
    }
    // 最後の号数の告示の後ろの見出しの無い記事は含めない（号数が 1 つの項目の [`pick_article`] と同じ）。
    picked
}

/// 同じ頁の範囲にある目次の項目（`titles`）に、記事をまとめて割り当てる。各項目は [`pick_articles`] で選ぶ。
/// 欄そのものの項目（目次「会社その他」、本文の欄見出し「会社その他の公告」）は、目次ではその欄の公告すべてなので、
/// 選んだ記事が項目の名前で始まる欄見出しの行から始まるなら、続く記事を、ほかの項目の選ぶ記事・欄見出しだけの記事・
/// 区分見出し・号数のある `〇` の見出しの手前まで含める（欄の後に「会社決算公告」の欄が続けば、その項目の選ぶ決算公告の手前で
/// 止まる）。欄の中の 1 件の公告の項目は、欄見出しを名前に持たないので広げない。
///
/// 欄の小見出しを並べた「…関係」の項目（[`pick_listed_section`] で選んだ記事）も欄の公告すべてなので、同じく続く記事を
/// 含める。欄の公告は小見出しだけの 1 行の記事（「破産管財人変更」「書面による計算報告」）を挟むので、欄見出しだけの記事では
/// 止めず、ほかの項目の選ぶ記事・区分見出し・本文の欄見出し（「会社その他の公告」）・標題が「…公告」「…公示」で終わる記事
/// （項目に並ぶ語で始まるものを除く）・ほかの「…関係」の項目に並ぶ語で始まる記事・号数のある `〇` の見出しの手前で止める。
/// ほかの「…関係」の項目に並ぶ語で始まる行を途中に持つ記事の後ろへは広げない。
pub fn pick_items(titles: &[&str], articles: &[ArticleAnalysis]) -> Vec<Vec<usize>> {
    let mut picks: Vec<Vec<usize>> = titles.iter().map(|t| pick_articles(t, articles)).collect();
    let claimed: std::collections::BTreeSet<usize> = picks.iter().flatten().copied().collect();
    // 同じ種類の公告を並べた欄の項目（「会社決算公告」、[`pick_series`] で選ぶ）の記事。会社その他の公告は決算公告の枠と同じ
    // 頁に組まれ、読み順で決算公告の間に入るので、欄見出しの項目はこの記事を飛ばして続ける。
    let series: std::collections::BTreeSet<usize> = titles
        .iter()
        .zip(&picks)
        .filter(|(t, _)| pick_article(t, articles).is_none())
        .flat_map(|(_, p)| p.iter().copied())
        .collect();
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let head = |a: &ArticleAnalysis| compact(a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or(""));
    // 欄見出しの行: 句読点の無い短い 1 行。
    let label = |h: &str| h.chars().count() <= 12 && !h.contains(['。', '、']);
    let label_only = |a: &ArticleAnalysis| a.text.lines().filter(|l| !l.trim().is_empty()).count() == 1 && label(&head(a));
    for (p, title) in picks.iter_mut().zip(titles) {
        let [i] = p[..] else { continue };
        let h = head(&articles[i]);
        let listed = title.ends_with("関係");
        if !(label(&h) && h.starts_with(&compact(title))) && !listed {
            // 公告の欄見出し（「諸事項」）が段の頭に組まれ、読み順で官庁報告の欄の公示の間に入ると、後ろの公示（本紙第1801号の
            // 「沖縄労働局最低賃金公示第１号」）は欄見出しで始まる別の記事になる。欄見出しの後の標題が目次の標題と 7 字以上
            // 同じ字を持つ（[`names_in_title`]）なら、項目の続き。
            for (j, a) in articles.iter().enumerate().skip(i + 1) {
                let body: Vec<&str> = a.text.lines().skip_while(|l| l.trim().is_empty() || segment::is_section_label(l)).collect();
                if claimed.contains(&j) || !segment::is_section_label(&head(a)) || !names_in_title_text(title, &body.join("\n")) {
                    break;
                }
                p.push(j);
            }
            continue;
        }
        // ほかの「…関係」の項目の小見出しが記事の途中にある（欄見出しが本文に無く、前の欄の最後の記事に続いた）なら、
        // その後ろはその項目の欄（本紙第1793号の「相続財産清算人の選任…」）。
        let runs_into_other = |a: &ArticleAnalysis| {
            titles.iter().filter(|t| *t != title).flat_map(|t| listed_words(t)).any(|w| !listed_lines(&w, false, std::slice::from_ref(a)).1.is_empty())
        };
        if listed && runs_into_other(&articles[i]) {
            continue;
        }
        for (j, a) in articles.iter().enumerate().skip(i + 1) {
            let h = head(a);
            // 号数の無い `〇` で始まる行（住所の折り返し「〇番地三五八」）は公告の続き。号数の見出しは「第…号」で終わる。
            // 財務諸表の注記が法令を引く行（「〇独立行政法人水資源機構の財務及び会計等に関する省令（平成15年国土交通省令
            // 第104号）」）は、号数を括弧の中に持つ公告の続き。
            let numbered = segment::is_heading_mark(&h) && heading_number(&h).is_some() && h.ends_with('号');
            // 欄の小見出し（「特別清算終結」「破産管財人変更」）は「公告」「公示」で終わらない。標題の行（2 行に折り返す）が
            // 「…公告」「…公示」で終わる記事は、欄の後ろに続く別の欄の公告（「…令和７年度財務諸表公告」）。
            let titled = || {
                let lines: Vec<String> = a.text.lines().map(compact).filter(|l| !l.is_empty()).take(2).collect();
                lines.iter().any(|l| l.ends_with("公告") || l.ends_with("公示"))
            };
            // ほかの「…関係」の項目の欄の小見出し（「特別清算終結」）は、その欄の始まり。
            let other_field = || titles.iter().filter(|t| *t != title).flat_map(|t| listed_words(t)).any(|w| h.starts_with(&w));
            // 項目に並ぶ語で始まる記事（「財部町土地改良区の定款変更の認可の公告」）は、標題が「…公告」で終わっても欄の公告。
            let own = || listed_heading_words(title).iter().any(|(w, whole)| !listed_lines(w, *whole, std::slice::from_ref(a)).0.is_empty()) || names_in_title(title, a);
            let field_end = if listed { segment::is_text_field_label(&h) || (titled() && !own()) || other_field() } else { label_only(a) };
            if !listed && series.contains(&j) {
                continue;
            }
            // 標題（「…財務諸表に関する / 公告」）の最後の行「公告」が区分見出しとして記事を分けたなら、前の標題だけの記事の続き。
            let title_only = |a: &ArticleAnalysis| !a.text.contains('。') && a.text.lines().filter(|l| !l.trim().is_empty()).count() <= 2;
            let title_tail = listed && matches!(h.as_str(), "公告" | "公示") && p.last() == Some(&(j - 1)) && title_only(&articles[j - 1]);
            // 標題が「公告」だけの公告（号外第196号の宮城県知事の「公告」）は区分見出しと同じ字になる。本文が項目に並ぶ語の
            // 頭 7 字（数字を除く、「農業協同組合法第六十四条の二の届出」の「農業協同組合法」）を含むなら、欄の公告。
            let titled_own = listed && h == "公告" && {
                let body = compact_digits(&a.text);
                listed_words(title).iter().map(|w| compact_digits(w).chars().take(7).collect::<String>()).any(|w| w.chars().count() == 7 && body.contains(&w))
            };
            // 欄見出しの項目（「会社その他」）の欄の中の、標題が「公告」だけの公告（号外第190号の信託財産の「公告 / 一、信託事務の
            // 状況」）。区分見出しの「公告」は次の行が欄見出し（「諸事項」「会社その他の公告」）なので、次の行が欄見出しの形でなければ
            // 欄の公告として続ける。
            let titled_field = !listed && h == "公告" && a.text.lines().map(compact).filter(|l| !l.is_empty()).nth(1).is_some_and(|l| !label(&l));
            if claimed.contains(&j) || (!title_tail && !titled_own && !titled_field && (field_end || segment::is_section_label(&h))) || numbered || h.contains("をここに公布する") {
                break;
            }
            p.push(j);
            if listed && runs_into_other(a) {
                break;
            }
        }
    }
    // 号数の無い `〇` の見出し（基本計画の中の「〇本邦に在留する全ての外国人について適正な在留管理を実施すること」）
    // で始まる記事は、告示の見出しではなく前の記事の続き。選んだ記事のすぐ後ろに続くなら、ほかの項目の選ぶ記事・号数のある
    // 見出し・区分見出しの手前まで含める。
    let claimed: std::collections::BTreeSet<usize> = picks.iter().flatten().copied().collect();
    for p in picks.iter_mut() {
        let Some(&last) = p.last() else { continue };
        for (j, a) in articles.iter().enumerate().skip(last + 1) {
            let h = head(a);
            if claimed.contains(&j) || !segment::is_heading_mark(&h) || heading_number(&h).is_some() || segment::is_section_label(&h) {
                break;
            }
            p.push(j);
        }
    }
    picks
}

/// 記事の標題（2 行まで）が、目次の標題の括弧にまとめて並べた名前（「（農林水産省・林野庁・国土交通省）」の「林野庁」、3 字以上）
/// を含み、目次の標題と 7 字以上続けて同じ字を持つか（[`names_in_title`]）。
fn grouped_name_heading(title: &str, a: &ArticleAnalysis) -> bool {
    let head: String = a.text.lines().filter(|l| !l.trim().is_empty()).take(2).flat_map(|l| l.chars().filter(|c| !c.is_whitespace())).collect();
    let names: Vec<String> = title
        .split('（')
        .skip(1)
        .filter_map(|g| g.split_once('）').map(|(inner, _)| inner))
        .flat_map(|inner| inner.split('・'))
        .map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>())
        .filter(|n| n.chars().count() >= 3)
        .collect();
    names.iter().any(|n| head.contains(n.as_str())) && names_in_title(title, a)
}

/// 空白と数字（漢数字・算用数字）を除いた字。
fn compact_digits(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !c.is_ascii_digit() && !"０１２３４５６７８９〇一二三四五六七八九十百千".contains(*c)).collect()
}

/// 記事の標題（2 行まで）が、目次の標題と 7 字以上続けて同じ字（数字を除く）を持つか。目次は同じ欄の公告の名前を
/// まとめて並べる（「令和七年度共済組合の決算（内閣・法務省・刑務）」）ので、語の頭が本文の標題（「令和７年度内閣
/// 共済組合の決算公告」）と合わないことがある。
fn names_in_title(title: &str, a: &ArticleAnalysis) -> bool {
    names_in_title_text(title, &a.text)
}

/// [`names_in_title`] を記事の本文の字（`text`）で比べる。
fn names_in_title_text(title: &str, text: &str) -> bool {
    let compact = |s: &str| -> Vec<char> {
        s.chars().filter(|c| !c.is_whitespace() && !c.is_ascii_digit() && !"０１２３４５６７８９〇一二三四五六七八九十百千".contains(*c)).collect()
    };
    let head: Vec<char> = text.lines().filter(|l| !l.trim().is_empty()).take(2).flat_map(|l| compact(l)).collect();
    let title: String = compact(title).into_iter().collect();
    head.windows(7).any(|w| title.contains(&w.iter().collect::<String>()))
}

/// [`pick_items`] が欄見出しの項目（`titles[0]`、「会社その他」）として選んだ記事が、記事の並びの最後まで続いたか。
/// 最後に選んだ記事より後ろが、同じ種類の公告を並べた欄の項目（「会社決算公告」）の記事だけでもよい。
///
/// 会社その他の公告は、後ろの「会社決算公告」の欄の頁に決算公告の枠と一緒に組まれて続く。目次はその欄の開始頁しか
/// 載せないので、項目の頁の範囲（次の項目の開始頁まで）で欄の公告が最後まで続いたら、呼び出し側は次の項目の頁の
/// 範囲まで広げて選び直す。
pub fn field_runs_to_end(titles: &[&str], articles: &[ArticleAnalysis]) -> bool {
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let picks = pick_items(titles, articles);
    let (Some(&first), Some(&last)) = (picks[0].first(), picks[0].last()) else { return false };
    let series: std::collections::BTreeSet<usize> =
        titles.iter().zip(&picks).skip(1).filter(|(t, _)| pick_article(t, articles).is_none()).flat_map(|(_, p)| p.iter().copied()).collect();
    let head = compact(articles[first].text.lines().find(|l| !l.trim().is_empty()).unwrap_or(""));
    head.chars().count() <= 12
        && !head.contains(['。', '、'])
        && head.starts_with(&compact(titles[0]))
        && (last + 1..articles.len()).all(|j| series.contains(&j))
}

/// 同じ種類の公告を並べた欄の目次の項目（「会社決算公告」）は、欄の名前の後ろの方（「決算公告」）で終わる見出し
/// （「第12期決算公告」「令和７年度決算公告」）の記事をすべて選ぶ。見出しの後ろに日付が続く（「第38期決算公告
/// 令和８年６月29日」）ので、見出しの行の最初の語で比べる。[`pick_article`] がどの記事も選ばないときだけ使う。
/// 欄の名前は番号も括弧も無い短い名前で、比べる後ろの方は 4 字以上・名前の半分以上。
fn pick_series(title: &str, articles: &[ArticleAnalysis]) -> Vec<usize> {
    let name: Vec<char> = title.trim().chars().collect();
    if name.len() > 10 || name.iter().any(|c| "（）、・".contains(*c)) || title.ends_with("関係") {
        return Vec::new();
    }
    let heads: Vec<String> = articles
        .iter()
        .map(|a| a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").split_whitespace().next().unwrap_or("").to_string())
        .collect();
    let min = 4.max(name.len().div_ceil(2));
    for len in (min..=name.len()).rev() {
        let tail: String = name[name.len() - len..].iter().collect();
        let picked: Vec<usize> = (0..articles.len()).filter(|&i| heads[i].ends_with(&tail)).collect();
        if !picked.is_empty() {
            return picked;
        }
    }
    Vec::new()
}

/// 記事の見出しの号数が `n` で、標題の機関名をすべて含むか。標題は機関名を略す（「山口県公安委告示配一」、見出し
/// 「山口県公安委員会告示配第一号」）ので、見出しの「第」より前に機関名の字が順に現れればよい。
pub(crate) fn has_number(a: &ArticleAnalysis, agencies: &[&str], n: u64) -> bool {
    let head = a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let name = head.split('第').next().unwrap_or("");
    let names = |ag: &str| head.contains(ag) || {
        let mut rest = name.chars();
        ag.chars().all(|c| rest.any(|h| h == c))
    };
    heading_number(head) == Some(n) && agencies.iter().all(|ag| names(ag))
}

/// 記事の中に号数だけの行（政令の公布文・御名御璽・署名の後の「政令第三百二十三号」）があり、その号数が `n` で、
/// 標題の機関名をすべて含むか。同じ法律の施行期日の政令と施行令の改正の政令（号外第 223 号の「（三二三）」
/// 「（三二四）」）は同じ鍵に合い、公布文が見出しなので [`has_number`] では選べない。
fn has_number_line(a: &ArticleAnalysis, agencies: &[&str], n: u64) -> bool {
    a.text.lines().map(str::trim).any(|l| {
        l.ends_with('号') && !l.contains(['。', '、', '「', '（']) && heading_number(l) == Some(n) && agencies.iter().all(|ag| l.contains(ag))
    })
}

/// 見出しの「第二百八十五号」「第26号」の号数。
pub(crate) fn heading_number(head: &str) -> Option<u64> {
    let start = head.find('第')? + '第'.len_utf8();
    let end = start + head[start..].find('号')?;
    let s = &head[start..end];
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c)) {
        return Some(s.chars().fold(0, |n, c| n * 10 + c.to_digit(10).or_else(|| Some(c as u32 - '０' as u32)).unwrap() as u64));
    }
    // 漢数字（千・百・十の位取り）。
    let (mut total, mut digit) = (0u64, None::<u64>);
    for c in s.chars() {
        match "〇一二三四五六七八九".chars().position(|d| d == c) {
            Some(d) => digit = Some(d as u64),
            None => {
                let unit = match c {
                    '十' => 10,
                    '百' => 100,
                    '千' => 1000,
                    _ => return None,
                };
                total += digit.unwrap_or(1) * unit;
                digit = None;
            }
        }
    }
    let n = total + digit.unwrap_or(0);
    (n > 0).then_some(n)
}

/// 標題から突合の鍵となる「対象法令名」を取り出す（末尾の制定機関略号と「の一部を…」を除く）。
pub fn title_core(title: &str) -> String {
    let mut t = title;
    if let Some(p) = t.rfind('（') {
        t = &t[..p];
    }
    for marker in ["の一部を", "を廃止"] {
        if let Some(p) = t.find(marker) {
            return t[..p].trim().to_string();
        }
    }
    t.trim().to_string()
}

/// 1 ページずつの XHTML の `<page>` 要素を 1 つの文書にまとめる。
fn combine_pages<'a>(xhtmls: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::from("<html><body><doc>");
    for x in xhtmls {
        let mut rest = x;
        while let Some(start) = rest.find("<page") {
            let Some(end) = rest[start..].find("</page>") else { break };
            out.push_str(&rest[start..start + end + "</page>".len()]);
            rest = &rest[start + end + "</page>".len()..];
        }
    }
    out.push_str("</doc></body></html>");
    out
}

/// 構造化用テキストの表の目印を、表を書き下したテキストに戻す。
fn render_table_markers(text: &str, tables: &[trace::Table]) -> String {
    text.lines()
        // 新旧対照表の終わりの目印は構造化用なので落とす。
        .filter(|l| l.trim() != trace::SHINKYU_END)
        .map(|l| match trace::parse_table_marker(l).and_then(|i| tables.get(i)) {
            Some(t) => t.render(),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// PDF バイト列から、`pdftotext -bbox-layout` と同じ形の座標付き XHTML を得る（kanpo-pdf）。
/// [`analyze`] / [`reconstruct_vertical`] の入力になる。
pub fn bbox_xhtml(pdf: &[u8]) -> Result<String> {
    let sources= kanpo_pdf::extract_pages_with_aj1_sources(pdf)?;
    Ok(kanpo_pdf::bbox::bbox_layout_with_native_styles(&sources))
}

/// PDF の各ページを [`analyze_item`] の入力（座標付きテキストと罫線）にする。
pub fn page_inputs(pdf: &[u8]) -> Result<Vec<PageInput>> {
    Ok(kanpo_pdf::extract_pages_with_aj1_sources(pdf)?
        .into_iter()
        .map(|source| PageInput {
            rules: Some(kanpo_pdf::page_rules(source.page())),
            xhtml: kanpo_pdf::bbox::bbox_layout_with_native_styles(std::slice::from_ref(&source)),
        })
        .collect())
}

/// PDF（1 ページ）の罫線（縦線・横線）を抽出する。[`analyze_with_rules`] に渡すと、新旧対照表の
/// 帯の中の罫線表を 2D 表として復元できる。
pub fn page_rules(pdf: &[u8]) -> Result<lines::PageRules> {
    lines::extract_rules(pdf)
}

/// PDF バイト列から構造化した [`Document`] を得る。HTML や表へ変換しやすい形。
/// 改め文をテキストではなく構造で受け取りたい呼び出し側はこちらを使う。
///
/// 新旧対照表中の別表（罫線で区切られた表）は、罫線とテキスト座標から
/// 2D 表として復元し、対応する `別表第○` 行の `after_table`/`before_table` に格納する。
pub fn extract_document(pdf: &[u8]) -> Result<Document> {
    let xhtml = bbox_xhtml(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let mut doc = Document::from_text(&text);
    attach_tables(&mut doc, pdf);
    Ok(doc)
}

/// 既存の [`Document`] に、1 ページ PDF から復元した別表 2D 表を attach する。
///
/// 複数ページにまたがる項目では、各ページの PDF についてこれを呼ぶと、まだ表の付いて
/// いない `別表第○` 行へ読み順に割り当てられる。罫線が取れなければ何もしない（best-effort）。
pub fn attach_tables(doc: &mut Document, pdf: &[u8]) {
    let Ok(rules) = lines::extract_rules(pdf) else {
        return;
    };
    let Ok(xhtml) = bbox_xhtml(pdf) else {
        return;
    };
    attach_bessho_tables(doc, &rules, &xhtml);
}

/// 罫線から別表 2D 表を復元し、Document の未充填の `別表第○` 行に順番に attach する。
fn attach_bessho_tables(doc: &mut Document, rules: &lines::PageRules, xhtml: &str) {
    // 1 ページ目（kanpo の項目別 PDF はページ単位で渡される）。
    let Some((height, cols)) = vertical::parse_page_cols(xhtml).into_iter().next() else {
        return;
    };
    // 改正後/改正前 の境界 y。見出しが無ければ別表 attach はしない。
    let Some((header_x, divider)) = shinkyu::detect_shinkyu_header(&cols) else {
        return;
    };
    let words: Vec<table::PlacedWord> = cols.iter().map(vertical::col_to_word).collect();
    let after_tables = table::reconstruct_tables(rules, &words, vertical::TOP_MARGIN, divider, header_x);
    let before_tables =
        table::reconstruct_tables(rules, &words, divider, height - vertical::BOTTOM_MARGIN, header_x);

    // Document の 別表行（after が「別表」で始まる行）に、読み順で順番に割り当てる。
    let (mut ai, mut bi) = (0usize, 0usize);
    for block in &mut doc.blocks {
        let model::Block::Shinkyu { rows ,..} = block else { continue };
        for row in rows.iter_mut() {
            let is_bessho = |runs: &[Run]| runs.first().map(|r| model::starts_with_spaced(&r.text, "別表")).unwrap_or(false);
            // 既に別表が付いている行（前のページで充填済み）は飛ばす。
            if is_bessho(&row.after) && row.after_table.is_none() && ai < after_tables.len() {
                row.after_table = Some(to_nested(&after_tables[ai]));
                ai += 1;
            }
            if is_bessho(&row.before) && row.before_table.is_none() && bi < before_tables.len() {
                row.before_table = Some(to_nested(&before_tables[bi]));
                bi += 1;
            }
        }
    }
}

/// 復元した 2D 表（セル文字列）を Document の入れ子テーブルへ変換する。
fn to_nested(t: &table::GridTable) -> model::NestedTable {
    model::NestedTable {
        merges: Vec::new(),
        rows: t
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        if cell.is_empty() {
                            Vec::new()
                        } else {
                            vec![Run::plain(cell.clone())]
                        }
                    })
                    .collect()
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arts(texts: &[&str]) -> Vec<ArticleAnalysis> {
        texts.iter().map(|t| ArticleAnalysis { text: t.to_string(), document: model::Document::from_text(t), notices:vec![],
            }).collect()
    }

    #[test]
    fn short_title_requires_an_independent_leading_heading() {
        let a = arts(&["国会事項\n内閣に関する質問", "人事異動\n内閣\n〇財務大臣臨時代理\n指定する",
        ]);
        assert_eq!(pick_article("内閣", &a), Some(1));
        assert_eq!(pick_article("内閣", &a[..1]), None);
    }

    #[test]
    fn picks_article_across_line_breaks() {
        let a = arts(&["〇防衛省令第十八号\n自衛隊法施行規則の一部を次のように改正する。", "〇個人情報保護委員会告示第一号\n…第九条の規定に基づき、金\n融分野における個人情報保護に関するガイドラインの一部を次のように改正し"]);
        assert_eq!(pick_article("金融分野における個人情報保護に関するガイドラインの一部を改正する件（個人情報保護委・金融庁一）", &a), Some(1));
    }

    #[test]
    fn picks_article_with_short_law_name() {
        let a = arts(&["〇農林水産省令第五十号\n種苗法施行規則の一部を次のように改正する。", "種苗法の一部を改正する法律をここに公布する。\n御名御璽",
        ]);
        assert_eq!(pick_article("種苗法の一部を改正する法律（七二）", &a), Some(1));
    }

    #[test]
    fn picks_article_ignoring_numeral_style() {
        let a = arts(&["人事院公示第26号\n人事院は、人事院規則２４（人事院の職員に対する権限の委任）第２項の規定に基づき、昭和38年人事院公示第５号の一部を次のように改正する。"]);
        let title = "人事院規則二―四（人事院の職員に対する権限の委任）第二項の規定に基づき、昭和三十八年人事院公示第五号の一部を改正する件（人事院二六）";
        assert_eq!(pick_article(title, &a), Some(0));
    }

    #[test]
    fn picks_article_by_number_when_toc_title_differs_from_body() {
        // 目次の標題と本文の中の告示の名前が違う（号外第 171 号 29 ページ）。号数で選ぶ。
        let a = arts(&[
            "〇総務省告示第二百八十四号\n八〇〇MHz帯広帯域小電力無線システムの携帯局の無線設備の送信時間制限装置を定める件の一部を次のように改正する。",
            "〇総務省告示第二百八十五号\n無線設備規則第十四条の二第一項第二号等の規定に基づく総務大臣が別に告示する無線設備を定める件の一部を次のように改正する。",
        ]);
        let title = "無線設備規則で規定する人体にばく露される電波の許容値に係る総務大臣が別に告示する無線設備を定める件の一部を改正する件（同二八五）";
        assert_eq!(pick_article(title, &a), Some(1));
        assert_eq!(pick_article(&title.replace("（同二八五）", "（総務二八五）"), &a), Some(1));
        // 機関名が見出しに無ければ選ばない。号数の合う記事が無いときも。
        assert_eq!(pick_article(&title.replace("（同二八五）", "（国土交通二八五）"), &a), None);
        assert_eq!(pick_article(&title.replace("（同二八五）", "（同二八六）"), &a), None);
        assert_eq!(heading_number("〇国土交通省告示第千百八十七号"), Some(1187));
        assert_eq!(heading_number("人事院公示第26号"), Some(26));
    }

    #[test]
    fn similar_titles_pick_the_article_with_their_own_number() {
        // 似た標題の告示は同じ鍵に合う。号数の合う記事を採り、本文の長い隣の告示を採らない。
        let a = arts(&[
            "〇農林水産省告示第千一号\n森林法第二十五条第一項の規定により、次のように保安林の指定をする。\n保安林の所在場所\n指定の目的\n指定施業要件",
            "〇農林水産省告示第千二号\n森林法第二十六条第一項の規定により、次のように保安林の指定を解除する。",
        ]);
        assert_eq!(pick_article("保安林の指定をする件（農林水産一〇〇一）", &a), Some(0));
        assert_eq!(pick_article("保安林の指定を解除する件（同一〇〇二）", &a), Some(1));
        // 号数の合う記事が鍵に合わなければ、従来どおり鍵で選ぶ。
        assert_eq!(pick_article("保安林の指定を解除する件（同一〇〇三）", &a), Some(0));
    }

    #[test]
    fn report_titles_pick_the_article_headed_by_them_without_the_agency() {
        // 官庁報告の標題は目次の標題から機関名を除いたもの（第 1771 号 7 ページ）。隣の公示は鍵を縮めた
        // 「登録低引火点燃料船」に合い、本文が 1 字長くても選ばない。
        let a = arts(&[
            "官庁報告\n官庁事項\n登録低引火点燃料船学科講習実施機関の登録\n船員法施行規則の規定に基づき、下記の者を登録したので公示する。",
            "登録低引火点燃料船安全担当者講習実施機関の登録\n船員労働安全衛生規則の規定に基づき、下記の者を登録したので公示する。\n令和８年８月19日",
        ]);
        assert_eq!(pick_article("登録低引火点燃料船学科講習実施機関の登録（国土交通省）", &a), Some(0));
        assert_eq!(pick_article("登録低引火点燃料船安全担当者講習実施機関の登録（同）", &a), Some(1));
    }

    #[test]
    fn articles_led_by_an_unnumbered_mark_continue_the_picked_article() {
        // 基本計画の中の「〇本邦に在留する…こと」の見出しで記事が分かれても、項目の記事の続き（号外第198号の出入国在留管理基本計画）。
        // 号数のある次の告示の見出しで止まる。
        let a = arts(&[
            "〇法務省告示第七十一号\n出入国管理及び難民認定法第六十一条の十第一項の規定に基づき、出入国在留管理基本計画を次のとおり定めた。",
            "〇受け入れた外国人との秩序ある共生社会の実現に向けた環境整備を推進していくこと\n共生社会の実現を図る。",
            "〇本邦に在留する全ての外国人について適正な在留管理を実施すること\n在留管理を適正に行う。",
            "〇法務省告示第七十二号\n別の告示。",
        ]);
        assert_eq!(pick_items(&["出入国在留管理基本計画を定めた件（法務七一）"], &a), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn untitled_notices_pick_their_number_over_toc_entries_and_similar_notices() {
        // 本文に標題を載せない告示は、鍵が号外の 1 ページ目の目次の項目にしか合わない（号外第 1787 号）。
        let a = arts(&[
            "〇消防法施行規則第三十一条の五第四\n項に規定する登録認定機関の代表者\nの氏名の変更に関する件\n（消防庁一四）　一",
            "〇消防法施行規則第三十一条の七第二\n項に規定する登録講習機関の代表者\nの氏名の変更に関する件（同一五）",
            "〇消防庁告示第十四号\n消防法施行規則第三十一条の五第四項において読み替えて準用する同令の規定に基づき、届出があったので公示する。",
            "〇消防庁告示第十五号\n消防法施行規則第三十一条の七第二項において読み替えて準用する同令の規定に基づき、届出があったので公示する。",
        ]);
        assert_eq!(pick_article("消防法施行規則第三十一条の五第四項に規定する登録認定機関の代表者の氏名の変更に関する件（消防庁一四）", &a), Some(2));
        assert_eq!(pick_article("消防法施行規則第三十一条の七第二項に規定する登録講習機関の代表者の氏名の変更に関する件（同一五）", &a), Some(3));
        // 似た標題の別の告示（号外第 39 号の熊本県の延長の告示）は、見出しの号数で選び直す。
        let a = arts(&[
            "〇国税庁告示第二十一号\n令和八年熊本地震の被害に伴い、熊本県の一部の地域における国税に関する申告期限等を延長する。",
            "〇厚生労働省告示第三百十三号\n熊本県の一部の地域における事業主掛金及び企業型年金加入者掛金の納付の特例を定める件の規定に基づき、次のように定める。熊本県の一部の地域における社会保険料及び労働保険料等に関する納期限等を延長する件",
        ]);
        assert_eq!(pick_article("熊本県の一部の地域における国税に関する申告期限等を延長する件（国税庁二一）", &a), Some(0));
        // 見出しに号数の無い記事（区分見出しで見出しと分かれた本文）は、号数の見出しだけの断片に替えない。
        let a = arts(&[
            "〇文部科学省告示第百八号\nに定める。",
            "告示\n学校法人の寄附行為及び寄附行為の変更の認可に関する審査基準の一部を改正する告示を次のように定める。",
        ]);
        assert_eq!(pick_article("学校法人の寄附行為及び寄附行為の変更の認可に関する審査基準の一部を改正する告示（文部科学一〇八）", &a), Some(1));
    }

    #[test]
    fn personnel_rules_pick_the_article_with_their_dashed_rule_number() {
        // 人事院規則は同じ鍵（「人事院規則九－」）に合う記事が並ぶ（本紙第 217 号）。標題の番号「（同九－一二三－四八）」の
        // 規則の番号だけの行で選ぶ。目次はダッシュを「―」、本文は「－」で組む。
        let a = arts(&[
            "人事院規則九－一二三－四八\n人事院規則九－一二三（本府省業務調整手当）の一部を改正する人事院規則\n人事院規則九－一二三（本府省業務調整手当）の一部を次のように改正する。",
            "人事院規則九－三〇－一一六\n人事院規則九－三〇（特殊勤務手当）の一部を改正する人事院規則\n人事院規則九－三〇（特殊勤務手当）の一部を次のように改正する。人事院規則九－一二三（本府省業務調整手当）の適用を受ける職員を除く。次の表により改める。",
        ]);
        assert_eq!(pick_article("人事院規則九―一二三（本府省業務調整手当）の一部を改正する人事院規則（同九－一二三－四八）", &a), Some(0));
        assert_eq!(pick_article("人事院規則九―三〇（特殊勤務手当）の一部を改正する人事院規則（人事院九－三〇－一一六）", &a), Some(1));
        // 規則の番号の頭が別の番号の続き（「九－一二三－四八」で終わる「一九－一二三－四八」）なら合わない。
        assert!(!has_code_line(&arts(&["人事院規則一九－一二三－四八"])[0], "九－一二三－四八"));
        assert_eq!(title_code("…件（総務二八五）"), None);
    }

    #[test]
    fn listed_numbers_pick_every_notice_of_the_item() {
        // 号数が別の項目と交互に並び（本紙第1754号「（同九九一、九九三～九九五）」「（同九九二、九九六、九九七）」）、
        // 読み順で別の機関の告示が間に入る（本紙第1763号）。頁をまたいだ告示の続きと、折り返した「〇番三まで」は含める。
        let a = arts(&[
            "〇国土交通省告示第九百九十一号\n砂防法第二条の土地の指定を解除する。",
            "〇国土交通省告示第九百九十二号\n砂防法第二条の土地を指定する。",
            "〇国土交通省告示第九百九十三号\n砂防法第二条の土地の指定を解除する。",
            "区域　…まで",
            "〇農林水産省告示第九百九十四号\n保安林の指定をする。",
            "〇国土交通省告示第九百九十四号\n砂防法第二条の土地の指定を解除する。",
            "〇番三まで　最小一二四",
            "〇国土交通省告示第九百九十五号\n砂防法第二条の土地の指定を解除する。",
            "〇国土交通省告示第九百九十六号\n砂防法第二条の土地を指定する。",
            "人事異動\n内閣",
        ]);
        assert_eq!(pick_articles("砂防法第二条の土地の指定を解除する件（国土交通九九一、九九三～九九五）", &a), vec![0, 2, 3, 5, 6, 7]);
        assert_eq!(pick_articles("砂防法第二条の土地を指定する件（同九九二、九九六、九九七）", &a), vec![1, 8]);
        // 号数が 1 つの項目は 1 記事。
        assert_eq!(pick_articles("砂防法第二条の土地を指定する件（国土交通九九二）", &a), vec![1]);
    }

    #[test]
    fn a_label_only_item_takes_the_notices_of_its_section_up_to_the_next_item() {
        // 号外第170号: 「会社その他の公告」の欄見出しの後に会社の公告が並び、「会社決算公告」の欄の決算公告が続く。
        let a = arts(&[
            "行旅死亡人\n本籍・住所・氏名不詳の男性",
            "会社その他の公告",
            "解散公告\n当社は、…解散いたしました。",
            "合併公告\n左記会社は合併して…",
            "第12期決算公告\n貸借対照表の要旨",
        ]);
        let picks = pick_items(&["会社その他", "会社決算公告"], &a);
        assert_eq!(picks, vec![vec![1, 2, 3], vec![4]]);
    }

    #[test]
    fn a_listed_section_item_picks_the_article_led_by_its_first_listed_heading() {
        // 号外第161号80ページ: 目次「破産、免責、再生関係」。本文は公告・諸事項の後に小見出しから始まり、標題は載らない。
        let a = arts(&[
            "解散公告\n当社は、…解散いたしました。",
            "公　告\n諸事項\n破産手続開始及び免責許可申立てに関する意見申述期間\n破産手続開始・破産手続廃止及び免責許可申立てに関する意見申述期間",
            "相続財産清算人の選任及び相続権主張の催告\n公示催告",
        ]);
        assert_eq!(pick_article("破産、免責、再生関係", &a), Some(1));
        assert_eq!(pick_article("相続、公示催告、失踪、除権決定、破産、免責、特別清算、再生、所有者不明関係", &a), Some(2));
        // 最初の語で始まる記事が 2 つ以上あるとき、「関係」で終わらない標題のときは選ばない。
        assert_eq!(pick_article("破産、免責、再生関係", &[a[1].clone(), a[1].clone()]), None);
        assert_eq!(pick_article("破産、免責、再生", &a), None);
    }

    #[test]
    fn a_toc_listing_of_the_title_is_not_picked() {
        // 号外第200号1ページ: 目次が標題と頁（「（経済産業省）　一」）を載せ、本文の公示は後ろにある。
        let a = arts(&[
            "国家試験\n令和八年度情報処理技術者試験合格者\n（経済産業省）　一\n〔公告〕",
            "官庁報告\n国家試験\n令和８年度情報処理技術者試験合格者\n令和８年度情報処理技術者試験（７月実施分）の合格者を次のとおり公示する。",
        ]);
        assert_eq!(pick_article("令和八年度情報処理技術者試験合格者（経済産業省）", &a), Some(1));
        // 号外第159号1ページ: 「破産、免責、再生関係　一」の目次の後に、公告の欄の小見出しが続く。
        let a = arts(&[
            "諸事項\n裁判所\n破産、免責、再生関係　一\n特殊法人等\n会社その他　四八",
            "公告\n破産手続開始\n破産手続開始及び免責許可申立てに関する意見申述期間",
        ]);
        assert_eq!(pick_article("破産、免責、再生関係", &a), Some(1));
        // 目次しか無ければ選ばない。
        assert_eq!(pick_article("破産、免責、再生関係", &a[..1]), None);
    }

    #[test]
    fn a_listed_section_item_takes_the_notices_of_its_field() {
        // 本紙第1795号: 裁判所の欄は小見出しだけの 1 行の記事（「破産管財人変更」）を挟んで特別清算の公告が続き、
        // 「会社その他の公告」の欄見出しで終わる。
        let a = arts(&[
            "公告\n諸事項\n相続財産清算人の選任及び相続権主張の催告\n公示催告",
            "破産管財人変更",
            "特別清算終結\n令和8年（ヒ）第3011号\n清算株式会社　株式会社…",
            "令和８年（ヒ）第２号\n清算株式会社　株式会社…",
            "会社その他の公告",
            "合併公告\n左記会社は合併して…",
        ]);
        let picks = pick_items(&["相続、公示催告、失踪、破産、特別清算、再生、所有者不明関係"], &a);
        assert_eq!(picks, vec![vec![0, 1, 2, 3]]);
        // 欄の後ろに続く別の欄の公告（標題が「…公告」で終わる）の手前で止まる。
        let a = arts(&[
            "公告\n破産手続開始\n破産手続開始及び免責許可申立てに関する意見申述期間",
            "令和８年（フ）第１号\n…",
            "地方公共団体情報システム機構令和７年度財務諸表公告\n貸借対照表",
        ]);
        assert_eq!(pick_items(&["破産、免責、再生関係"], &a), vec![vec![0, 1]]);
    }

    #[test]
    fn a_listed_item_keeps_the_notes_that_cite_a_numbered_ordinance() {
        // 号外第195号: 財務諸表の注記が法令を引く行は `〇` で始まり、号数を括弧の中に持つ。告示の見出しではない。
        let a = arts(&[
            "令和７事業年度独立行政法人水資源機構の財務諸表に関する公告\n貸借対照表",
            "〇独立行政法人水資源機構の財務及び会計等に関する省令（平成15年国土交通省令第104号）\n（補助金等の会計処理）",
            "〇国土交通省告示第百号\n…",
        ]);
        let title = "令和七事業年度独立行政法人水資源機構の財務諸表、独立行政法人日本高速道路保有・債務返済機構車両制限令第三条第四項の規定による道路の指定関係";
        assert_eq!(pick_items(&[title], &a), vec![vec![0, 1]]);
    }

    #[test]
    fn a_label_only_item_skips_the_financial_notices_set_among_its_notices() {
        // 号外第194号: 会社その他の公告は、会社決算公告の欄の頁に決算公告の枠と交互に組まれて続く。
        let a = arts(&[
            "会社その他の公告",
            "解散公告\n当社は、…解散いたしました。",
            "第70期決算公告　令和８年８月31日\n貸借対照表の要旨",
            "合併公告\n左記会社は合併して…",
            "第76期決算公告　令和８年７月31日\n貸借対照表の要旨",
        ]);
        let picks = pick_items(&["会社その他", "会社決算公告"], &a);
        assert_eq!(picks, vec![vec![0, 1, 3], vec![2, 4]]);
        // 欄の公告が最後まで続いた（後ろは決算公告だけ）ので、呼び出し側は次の項目の頁まで広げる。
        assert!(field_runs_to_end(&["会社その他", "会社決算公告"], &a));
        // 欄の公告の後ろに別の欄があれば広げない。
        let mut b = a.clone();
        b.extend(arts(&["官庁報告\n官庁事項"]));
        assert!(!field_runs_to_end(&["会社その他", "会社決算公告"], &b));
        assert!(!field_runs_to_end(&["会社決算公告", "会社その他"], &a));
    }

    #[test]
    fn a_listed_section_starting_inside_the_previous_notice_is_split_off() {
        // 本紙第1749号: 裁判所の欄見出しが本文に無く、欄の最初の小見出しが前の公告の記事の署名の後に続く。
        let mut a = arts(&[
            "公告\n諸事項\n有権者申出方\n…当局に申し出て下さい。\n令和８年７月16日　広島法務局\n相続財産清算人の選任及び相続権主張の催告\n失踪宣告",
            "会社その他の公告",
        ]);
        let titles = ["相続、公示催告、失踪、破産、特別清算、再生、所有者不明関係", "有権者申出方関係"];
        assert_eq!(pick_article(titles[0], &a), None);
        split_listed_sections(&titles, &mut a);
        assert_eq!(a.len(), 3);
        assert!(a[0].text.ends_with("令和８年７月16日　広島法務局"));
        assert_eq!(a[1].text, "相続財産清算人の選任及び相続権主張の催告\n失踪宣告");
        assert_eq!(pick_items(&titles, &a), vec![vec![1], vec![0]]);
        // 既に選べる項目、最初の語で始まる行が 2 つ以上ある項目では分けない。
        let mut b = arts(&["前の公告\n令和８年７月16日　広島法務局\n相続財産清算人の選任\n…申し出て下さい。\n相続財産清算人の選任"]);
        split_listed_sections(&titles, &mut b);
        assert_eq!(b.len(), 1);
        // 本紙第1753号: 語で始まる小見出しが続くなら、最初の行で分ける。
        let mut e = arts(&["前の公告\n令和８年７月23日　法務局\n相続財産清算人の選任及び相続権主張の催告\n相続権主張の催告\n公示催告"]);
        split_listed_sections(&titles, &mut e);
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].text, "相続財産清算人の選任及び相続権主張の催告\n相続権主張の催告\n公示催告");
        // 号外第174号: 3 行以上続いても 1 つと数える。
        let mut h = arts(&["…Tourism.\n破産手続開始及び免責許可申立てに関する意見申述期間\n破産手続開始・破産手続廃止及び免責許可申立てに関する意見申述期間\n破産手続廃止"]);
        split_listed_sections(&["破産、免責、再生関係"], &mut h);
        assert_eq!(h.len(), 2);
        // 本紙第1759号: 区分見出し「公告」を別の段落にした文書は、その構造のまま前を切る。
        let text = "公告\n諸事項\n…公告する。\n令和８年７月31日\n東北農政局長\n相続財産清算人の選任及び相続権主張の催告\n特別清算開始";
        let paragraph = |t: &str| Block::Paragraph { runs: vec![Run::plain(t)] };
        let document = Document {
            format: "prose".into(),
            blocks: vec![paragraph("公告"), paragraph("諸事項…公告する。\n令和８年７月31日東北農政局長相続財産清算人の選任及び相続権主張の催告特別清算開始")],
        };
        let mut c = vec![ArticleAnalysis { text: text.into(), document, notices: vec![] }];
        split_listed_sections(&titles, &mut c);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].document.blocks, vec![paragraph("公告"), paragraph("諸事項…公告する。\n令和８年７月31日東北農政局長")]);
        assert_eq!(c[1].document, Document::from_text("相続財産清算人の選任及び相続権主張の催告\n特別清算開始"));
        // 本紙第1804号: 分ける行より前の表は前の記事に残す。後ろに表があれば分けない。
        let table = |text: &str| Run { text: String::new(), underline: false, table: Some(model::NestedTable { rows: vec![vec![vec![Run::plain(text)]]], merges: vec![] }) };
        let before = Block::Paragraph { runs: vec![table("名称")] };
        let document = Document {
            format: "prose".into(),
            blocks: vec![paragraph("公告"), before.clone(), paragraph("諸事項…公告する。\n令和８年７月31日東北農政局長相続財産清算人の選任及び相続権主張の催告特別清算開始")],
        };
        let mut f = vec![ArticleAnalysis { text: text.into(), document, notices: vec![] }];
        split_listed_sections(&titles, &mut f);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].document.blocks, vec![paragraph("公告"), before, paragraph("諸事項…公告する。\n令和８年７月31日東北農政局長")]);
        // 表が分ける行の直前の段落なら、前に残す。
        let document = Document {
            format: "prose".into(),
            blocks: vec![
                paragraph("公告"),
                paragraph("諸事項…公告する。\n令和８年７月31日東北農政局長"),
                Block::Paragraph { runs: vec![table("名称")] },
                paragraph("相続財産清算人の選任及び相続権主張の催告特別清算開始"),
            ],
        };
        let mut g = vec![ArticleAnalysis { text: text.into(), document, notices: vec![] }];
        split_listed_sections(&titles, &mut g);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].document.blocks.len(), 3);
        let after = Block::Paragraph { runs: vec![Run::plain("諸事項…公告する。\n令和８年７月31日東北農政局長相続財産清算人の選任及び相続権主張の催告特別清算開始"), table("名称")] };
        let mut d = vec![ArticleAnalysis { text: text.into(), document: Document { format: "prose".into(), blocks: vec![paragraph("公告"), after] }, notices: vec![] }];
        split_listed_sections(&titles, &mut d);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn a_listed_section_item_falls_back_to_a_later_listed_word() {
        // 号外第173号: 「教育職員免許状失効」「行旅死亡人」の小見出しは画像の上で本文に無く、「特定空家等の除却命令及び代執行」
        // から本文。
        let a = arts(&["会社決算公告の前の公告\n…", "特定空家等の除却命令及び代執行について\n…", "会社その他の公告"]);
        assert_eq!(pick_article("教育職員免許状失効、行旅死亡人、特定空家等の除却命令及び代執行関係", &a), Some(1));
        // 2 つ目からの語は 4 字以上だけ。
        assert_eq!(listed_words("財団、建設業の許可の取消処分、公示送達関係"), vec!["財団", "建設業の許可の取消処分", "公示送達"]);
        assert_eq!(listed_words("相続、失踪、破産関係"), vec!["相続"]);
        // 目次の漢数字と本文の算用数字は、数字を除いて比べる（号外第208号）。
        let e = arts(&["公告\n第48回（令和８年度）エネルギー管理士試験合格者\n…"]);
        assert_eq!(pick_article("第四十八回（令和八年度）エネルギー管理士試験合格者、日本弁護士連合会懲戒処分関係", &e), Some(0));
        // 前の語の小見出しが前の公告の記事の途中にあるなら、後ろの語では選ばず、分けてから前の語で選ぶ。
        let mut b = arts(&[
            "公告\n…申し出て下さい。\n令和８年７月16日　広島法務局\n相続財産清算人の選任及び相続権主張の催告",
            "公示催告\n…",
        ]);
        let title = "相続、公示催告、失踪、所有者不明関係";
        assert_eq!(pick_article(title, &b), None);
        split_listed_sections(&[title], &mut b);
        assert_eq!(pick_article(title, &b), Some(1));
        // ほかの「…関係」の項目に並ぶ語で始まる記事の手前で止まる。
        let c = arts(&["公告\n諸事項\n財団法人…の解散", "令和８年（フ）第１号", "公示催告\n…", "令和８年（ヒ）第２号"]);
        assert_eq!(pick_items(&["財団関係", "相続、公示催告、失踪関係"], &c), vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn a_listed_section_inside_the_article_its_title_picks_is_split_off() {
        // 本紙第1806号: 「企業年金基金変更関係」は鍵の字を含む裁判所の欄の記事を選ぶが、小見出しの行はその記事の途中にある。
        let mut a = arts(&["所有者不明土地管理命令に関する異議の催告\n所有者不明建物管理命令に関する異議の催告\n企業年金基金変更公告\n…次のように公告する。"]);
        let title = "企業年金基金変更関係";
        assert_eq!(pick_article(title, &a), Some(0));
        split_listed_sections(&[title], &mut a);
        assert_eq!(a.len(), 2);
        assert_eq!(pick_article(title, &a), Some(1));
        // 2 つ目からの語の行では分けない（号外第196号: 項目の公告が続く）。
        let mut b = arts(&["高速道路の料金の額及び徴収期間の変更公告\n西日本高速道路株式会社料金の額及び徴収期間の変更について…公告する。\n令和８年度公害防止管理者等資格認定講習\n…"]);
        let title = "西日本高速道路株式会社料金の額及び徴収期間の変更、令和八年度公害防止管理者等資格認定講習関係";
        assert_eq!(pick_article(title, &b), Some(0));
        split_listed_sections(&[title], &mut b);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn a_listed_item_picked_by_its_title_takes_the_notices_its_title_names() {
        // 号外第178号: 「裁判所共済組合（定款の一部変更・令和七年度決算）、独立行政法人都市再生機構関係」は標題で
        // 「裁判所共済組合定款の一部変更について」を選び、目次がまとめて並べる公告（「令和７年度裁判所共済組合の決算公告」）
        // と、語で始まる公告（「独立行政法人都市再生機構公告」）を続けて含める。
        let a = arts(&[
            "裁判所共済組合定款の一部変更について\n裁判所共済組合定款の一部を次のように変更した…",
            "令和７年度裁判所共済組合の決\n算公告\n…",
            "独立行政法人都市再生機構公告\n…",
            "令和７年度国家公務員共済組合\n連合会の決算公告\n…",
        ]);
        let title = "裁判所共済組合（定款の一部変更・令和七年度決算）、独立行政法人都市再生機構関係";
        assert_eq!(pick_article(title, &a), Some(0));
        assert_eq!(pick_items(&[title], &a), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn a_listed_item_starts_at_the_earlier_of_its_title_and_heading_picks() {
        // 号外第217号: 標題の鍵は後ろの大きい財務諸表の記事にも合うが、目次の最初の語の小見出しで始まる記事が前にある。
        let a = arts(&[
            "独立行政法人北方領土問題対策協会令和７事業年度財務諸表に関する公告\n…貸借対照表…",
            "国立研究開発法人宇宙航空研究開発機構令和７事業年度財務諸表に関する公告\n独立行政法人北方領土問題対策協会…国立研究開発法人宇宙航空研究開発機構令和七事業年度財務諸表…の規定に基づき…貸借対照表…損益計算書…次のように…定める",
        ]);
        let title = "独立行政法人北方領土問題対策協会令和七事業年度財務諸表、国立研究開発法人宇宙航空研究開発機構令和七事業年度財務諸表関係";
        assert_eq!(pick_article_by_title(title, &a), Some(1));
        assert_eq!(pick_article(title, &a), Some(0));
        // 号外第216号: 標題だけの記事の後ろの「公告」の行は、折り返した標題の最後の行。
        let b = arts(&[
            "独立行政法人日本学術振興会令和７事業年度財務諸表に関する",
            "公告\n独立行政法人通則法…公告する。",
            "国立研究開発法人水産研究・教育機構令和７事業年度財務諸表に関する公告\n…",
        ]);
        let title = "独立行政法人日本学術振興会令和七事業年度財務諸表、国立研究開発法人水産研究・教育機構令和七事業年度財務諸表関係";
        assert_eq!(pick_items(&[title], &b), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn a_listed_item_starts_at_the_first_notice_of_a_grouped_name() {
        // 号外第162号: 目次は名前を括弧にまとめ、本文は名前ごとの決算公告を載せる。
        let a = arts(&[
            "令和７年度防衛省共済組合の決\n算公告\n…",
            "令和７年度農林水産省共済組合\nの決算公告\n…",
            "令和７年度林野庁共済組合の決\n算公告\n…",
            "独立行政法人都市再生機構公告\n…",
        ]);
        let title = "令和七年度共済組合の決算（農林水産省・林野庁・国土交通省）、独立行政法人都市再生機構関係";
        assert_eq!(pick_article(title, &a), Some(1));
        assert_eq!(pick_items(&[title], &a), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn a_listed_word_matches_a_wrapped_heading() {
        // 本紙第1757号: 次の語の小見出しは 2 行に折り返す。項目に並ぶ語で始まる記事は、標題が「…公告」で終わっても欄の公告。
        let a = arts(&[
            "官庁報告の記事。",
            "金融商品取引業者に対する行\n政処分の公告\n金融商品取引法…",
            "財部町土地改良区の定款変更\nの認可の公告\n土地改良法…",
            "相続財産清算人の選任及び相続権主張の催告",
        ]);
        let title = "財団、金融商品取引業者に対する行政処分、財部町土地改良区の定款変更の認可関係";
        assert_eq!(pick_article(title, &a), Some(1));
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &a), vec![vec![1, 2], vec![3]]);
        // 「財団」の小見出しは種類を付けた「工場財団」。句点の無い短い小見出しが語で終わるなら、語の小見出し（本紙第1756号）。
        let mut d = a.clone();
        d[0] = arts(&["公告\n諸事項\n工場財団\n…権利を申し出て下さい。"]).remove(0);
        assert_eq!(pick_article(title, &d), Some(0));
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &d), vec![vec![0, 1, 2], vec![3]]);
        // 長い行や文の行は、語で終わっても小見出しではない。
        let e = arts(&["公告\n諸事項\n株式会社中山製鋼所の工場財団", "相続財産清算人の選任及び相続権主張の催告"]);
        assert_eq!(pick_article("財団関係", &e), None);
        // ほかの語の標題が「…公告」で終わる記事では止まる。
        let b = arts(&["金融商品取引業者に対する行\n政処分の公告\n…", "令和７事業年度財務諸表に関\nする公告\n…"]);
        assert_eq!(pick_items(&["金融商品取引業者に対する行政処分関係"], &b), vec![vec![0]]);
        // 本紙第1793号: ほかの「…関係」の項目の小見出しが記事の途中にあれば、その後ろ（裁判所の欄の公告）へは広げない。
        let c = arts(&[
            "公　告\n諸事項\n成田国際空港の安全確保に関\nする緊急措置法第３条第２項\nの規定に基づく公告\n…審査請求をすることができます。\n相続財産清算人の選任及び相続権主張の催告",
            "令和８年（ヒ）第２０２１号\n…",
        ]);
        let title = "公示送達、成田国際空港の安全確保に関する緊急措置法第三条第二項の規定に基づく公告関係";
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &c), vec![vec![0], vec![]]);
    }

    #[test]
    fn a_short_listed_word_after_the_first_matches_only_a_heading_line() {
        // 本紙第1790号: 最初の語の公告は本文に無く、欄は 3 つ目の語「財団」の小見出し（種類を付けた「工場財団」）から始まる。
        let title = "前払式支払手段発行者の発行保証金に係る仮配当表・権利の実行に関する意見聴取会、財団、佐賀東部土地改良区の定款変更の認可関係";
        let a = arts(&[
            "公告\n諸事項\n工場財団\n…権利を申し出て下さい。",
            "佐賀東部土地改良区の定款変更の認可の公告\n土地改良法…",
            "相続財産清算人の選任及び相続権主張の催告",
        ]);
        assert_eq!(pick_article(title, &a), Some(0));
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &a), vec![vec![0, 1], vec![2]]);
        // 短い語は行の頭（「財団法人…」）とは比べない。
        let b = arts(&["公告\n諸事項\n財団法人日本…の解散の公告\n…", "佐賀東部土地改良区の定款変更の認可の公告\n…"]);
        assert_eq!(pick_article(title, &b), Some(1));
        // 本紙第1811号: 小見出しの前に前の欄の表の字（「Ｂ9919」）が残ると、記事の途中の小見出しの行で分ける。
        let title = "前払式支払手段発行者の発行保証金に係る配当表、財団関係";
        let mut c = arts(&["公告\n諸事項\nＢ9919\n工場財団\n…権利を申し出て下さい。\n令和８年10月20日", "相続財産清算人の選任及び相続権主張の催告"]);
        split_listed_sections(&[title, "相続、失踪関係"], &mut c);
        assert_eq!(c.len(), 3);
        assert!(c[1].text.starts_with("工場財団"));
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &c), vec![vec![1], vec![2]]);
    }

    #[test]
    fn a_notice_after_a_stray_section_label_continues_the_item_its_title_names() {
        // 本紙第1801号: 公告の欄見出し「諸事項」が読み順で官庁報告の 2 つの最低賃金の公示の間に入る。
        let title = "最低賃金の改正決定に関する公示（熊本労働局最低賃金公示一、沖縄同一）";
        let a = arts(&[
            "労働\n最低賃金の改正決定に関する公示\n熊本労働局最低賃金公示第１号\n…公示する。",
            "諸事項\n沖縄労働局最低賃金公示第１号\n最低賃金法（昭和34年法律第137号）第12条の\n…公示する。",
            "相続財産清算人の選任及び相続権主張の催告",
        ]);
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &a), vec![vec![0, 1], vec![2]]);
        // 欄見出しの後の公示が標題と字を同じくしないなら、続きではない。
        let mut b = a.clone();
        b[1] = arts(&["諸事項\n工場財団\n…権利を申し出て下さい。"]).remove(0);
        assert_eq!(pick_items(&[title, "相続、失踪関係"], &b), vec![vec![0], vec![2]]);
    }

    #[test]
    fn a_field_item_continues_over_a_notice_titled_only_koukoku() {
        // 号外第190号: 決算公告の頁に組まれた信託財産の「公告」（標題が「公告」だけ）は、会社その他の欄の公告。後ろの合併公告まで続ける。
        let a = arts(&[
            "会社その他の公告",
            "解散公告\n当社は、株主総会において解散することを決議しました。",
            "公告\n一、信託事務の状況\n当期においては振替機関等から通知を受けた。",
            "合併公告\n左記会社は合併して甲は乙の権利義務全部を承継して存続する。",
        ]);
        assert_eq!(pick_items(&["会社その他"], &a)[0], vec![0, 1, 2, 3]);
        // 次の行が欄見出しの「公告」（区分見出し）は止まる。
        let mut b = a.clone();
        b[2] = arts(&["公告\n諸事項\n当局に申し出て下さい。"]).remove(0);
        assert_eq!(pick_items(&["会社その他"], &b)[0], vec![0, 1]);
    }

    #[test]
    fn a_listed_item_takes_a_notice_titled_only_koukoku_that_names_its_word() {
        // 号外第196号: 宮城県知事の「公告」（標題が「公告」だけ）は、本文に項目の語「農業協同組合法…」を含む欄の公告。
        let title = "解散命令、教育職員免許状失効、行旅死亡人、農業協同組合法第六十四条の二の届出関係";
        let a = arts(&[
            "解散命令公告\n…命令した。",
            "公告\n宮城県の区域を地区とする農業協同組合又は農\n…農業協同組合法施行規則（平成17年農林水産省令第27\n号）第208条の２で定めるところにより…届出をされたい。",
            "会社その他の公告",
            "解散公告\n当社は…",
        ]);
        assert_eq!(pick_items(&[title, "会社その他"], &a)[0], vec![0, 1]);
        // 語を含まない「公告」は区分見出しとして止まる。
        let mut b = a.clone();
        b[1] = arts(&["公告\n諸事項\n…権利を申し出て下さい。"]).remove(0);
        assert_eq!(pick_items(&[title, "会社その他"], &b)[0], vec![0]);
    }

    #[test]
    fn stacked_listed_headings_with_a_wrapped_first_line_start_one_section() {
        // 号外第178号: 破産の欄の小見出し（公告は画像）が前の公告の記事の後ろに続き、最初の小見出しは 2 行に折り返す。
        let title = "破産、免責、再生関係";
        let mut a = arts(&[
            "登録政治資金監査人登録公告\n…公告する。\n政治資金適正化委員会委員長野々上尚\n破産手続開始及び免責許可申\n立てに関する意見申述期間\n破産手続開始・破産手続廃止及び免責許可申立てに関する意見申述期間\n破産手続廃止\n免責許可決定",
        ]);
        split_listed_sections(&[title], &mut a);
        assert_eq!(a.len(), 2);
        assert!(a[1].text.starts_with("破産手続開始及び免責許可申\n"));
        assert_eq!(pick_article(title, &a), Some(1));
        // 号外第170号: 1 ページ目の目次の行（頁の字が次の行に落ちた「破産、免責、再生関係」）は欄の小見出しではない。
        let mut b = arts(&[
            "（号外）\n発行　内閣府\n（原稿作成　国立印刷局）\n目次\n〔公告〕",
            "諸事項\n官庁\n被害回復給付金裁定表記載関係　一\n裁判所\n破産、免責、再生関係\n特殊法人等\n令和七年度共済組合の決算（防衛\n省・日本たばこ産業）、独立行政法\n人統計センター令和七年度財務諸表\n関係　四〇\n地方公共団体\n行旅死亡人関係　四六\n会社その他\n会社決算公告　五二",
            "公告\n諸事項\n被害回復給付金裁定表記載公告\n…請求することができます。\n破産手続開始及び免責許可申立てに関する意見申述期間\n破産手続廃止",
        ]);
        split_listed_sections(&[title], &mut b);
        assert_eq!(b.len(), 4);
        assert_eq!(pick_article(title, &b), Some(3));
    }

    #[test]
    fn a_section_of_same_kind_notices_picks_every_notice_ending_with_its_name() {
        // 号外第170号52ページ: 会社その他の公告の後に、会社決算公告の欄の決算公告が続く。
        let a = arts(&[
            "解散公告（第三回）\n当社は、…解散いたしました。",
            "第12期決算公告\n貸借対照表の要旨",
            "第52期決算公告　令和８年５月21日\n貸借対照表の要旨",
            "令和７年度決算公告\n…",
        ]);
        assert_eq!(pick_article("会社決算公告", &a), None);
        assert_eq!(pick_articles("会社決算公告", &a), vec![1, 2, 3]);
        // 「…関係」の項目や長い標題には使わない。
        assert!(pick_articles("行旅死亡人関係", &a).is_empty());
    }

    #[test]
    fn a_wrapped_notice_title_picks_the_body_after_its_lone_label() {
        // 本紙第1761号31ページ: 標題「石炭鉱業年金基金清算人就任」＋「公告」が折り返し、「公告」の行で記事が分かれる。
        let a = arts(&[
            "所有者不明専有部分管理命令\nに関する異議の催告\n石炭鉱業年金基金清算人就任",
            "公告\n当基金は、…解散いたしましたので、…次のとおり公告します。\n理事長菊地靖則",
            "会社その他の公告\n合併公告",
        ]);
        assert_eq!(pick_article("石炭鉱業年金基金清算人就任関係", &a), Some(1));
    }

    #[test]
    fn unnumbered_rules_pick_the_article_carrying_their_title_line() {
        // 号外第 169 号 184 ページ: 内閣官房の規則は見出しも番号も無く、制定文・日付の後に標題の行を載せる。
        // 前の規則の附則（「内閣情報調査室組織規則…は、廃止する。」）にも標題の前の方の字が合う。
        let a = arts(&[
            "官庁報告\n官庁事項\n内閣官房組織令…の規定に基づき、国家情報局に調査官を置く規則を次のように定める。\n令和八年七月二十九日　内閣総理大臣高市早苗\n国家情報局に調査官を置く規則\n附則\n２内閣情報調査室組織規則（昭和五十一年十二月二十三日内閣総理大臣決定）は、廃止する。",
            "内閣官房組織令…の規定に基づき、内閣情報調査室に内閣情報分析官等を置く規則を廃止する規則を次のように定める。\n令和八年七月二十九日　内閣総理大臣高市早苗\n内閣情報調査室に内閣情報分析官等を置く規則を廃止する規則\n…は、廃止する。",
        ]);
        assert_eq!(pick_article("内閣情報調査室に内閣情報分析官等を置く規則を廃止する規則（同）", &a), Some(1));
        assert_eq!(pick_article("国家情報局に調査官を置く規則（内閣官房）", &a), Some(0));
    }

    #[test]
    fn personnel_titles_listing_agencies_pick_the_personnel_section() {
        // 人事異動の目次名は欄に載る機関を空白で並べる（号外第 1753 号「内閣 財務省 最高裁判所 福岡県」）。
        // 長い機関名は 2 行に折り返す（本紙第 190 号）。
        let a = arts(&[
            "国会事項\n内閣に関する質問",
            "人事異動\n内閣\n〇防衛大臣臨時代理\n指定する\n財務省\n任命する\n特定複合観光施設区域整備推進本\n部事務局\n併任する\n福岡県\n〇監査委員選任\n選任された。",
            "皇室事項\n御祝電",
        ]);
        assert_eq!(pick_article("内閣 財務省 福岡県", &a), Some(1));
        assert_eq!(pick_article("内閣 特定複合観光施設区域整備推進本部事務局", &a), Some(1));
        // 欄に載らない機関があれば採らない。
        assert_eq!(pick_article("内閣 最高裁判所", &a), None);
    }

    #[test]
    fn promulgated_cabinet_orders_pick_their_own_number_line() {
        // 同じ法律の施行期日の政令と施行令の改正の政令は同じ鍵に合う（号外第 223 号 2 ページ）。見出しは公布文なので、
        // 御名御璽と署名の後の号数だけの行で選ぶ。
        let a = arts(&[
            "建築士法の一部を改正する法律の一部の施行期日を定める政令をここに公布する。\n御名御璽\n令和八年十月九日\n内閣総理大臣高市早苗\n政令第三百二十三号\n建築士法の一部を改正する法律の一部の施行期日を定める政令\n内閣は、建築士法の一部を改正する法律附則第一条第二号の規定に基づき、この政令を制定する。",
            "建築士法施行令の一部を改正する政令をここに公布する。\n御名御璽\n令和八年十月九日\n内閣総理大臣高市早苗\n政令第三百二十四号\n建築士法施行令の一部を改正する政令\n内閣は、建築士法の規定に基づき、この政令を制定する。\n建築士法施行令の一部を次のように改正する。\n附則\nこの政令は、建築士法の一部を改正する法律附則第一条第二号に掲げる規定の施行の日から施行する。",
        ]);
        assert_eq!(pick_article("建築士法の一部を改正する法律の一部の施行期日を定める政令（三二三）", &a), Some(0));
        assert_eq!(pick_article("建築士法施行令の一部を改正する政令（三二四）", &a), Some(1));
        // 本文の中で引いた号数（「…政令第二百一号）の一部を」）は号数だけの行ではない。
        assert!(!has_number_line(&arts(&["建築士法施行令（昭和二十五年政令第二百一号）の一部を次のように改正する。"])[0], &[], 201));
    }

    #[test]
    fn picks_article_by_number_when_agency_starts_with_numeral() {
        // 機関名が漢数字で始まる（「九州地方整備局九八」）。先頭の「九」を号数と取り違えない。
        let a = arts(&[
            "〇東北地方整備局告示第百一号\n次のように道路の区域を変更した。",
            "〇九州地方整備局告示第九十八号\n都市計画事業を認可したので告示する。",
        ]);
        assert_eq!(pick_article("都市計画に関する件（九州地方整備局九八）", &a), Some(1));
        // 範囲・列挙の号数は先頭の号数で選ぶ。
        let a = arts(&["〇防衛省告示第二百五十一号\n射撃訓練を実施する。", "〇防衛省告示第二百五十四号\n射撃等訓練を実施する。",
        ]);
        assert_eq!(pick_by_number("訓練の件（防衛二五一～二五三）", &a), Some(0));
        assert_eq!(pick_by_number("訓練の件（同二五四、二五五）", &a), Some(1));
    }
}

mod composite_numbers;

mod large_parentheses;

mod financial_flow;

mod circled_numbers;

mod unruled_records;

mod parallel_fraction_geometry;
mod parallel_fractions;

mod numeric_fraction_geometry;
mod numeric_fractions;

#[cfg(test)]
mod ruled_plain_dash_tests;

mod quoted_inline_tables;

mod source_aj1;

#[cfg(test)]
mod qualified_notice_title_tests;
#[cfg(test)]
mod notice_title_scope_tests;
#[cfg(test)]
mod unparsed_notice_title_tests;

#[cfg(test)]
mod native_layout_relations_tests;

#[cfg(test)]
mod native_geometry_originals_tests;

mod native_word_bounds;

#[cfg(test)]
mod notice_native_geometry_tests;

#[cfg(test)]
mod glyph_typography_originals_tests;

#[cfg(test)]
mod glyph_typography_contract_tests;

mod native_content_styles;

#[cfg(test)]
mod common_header_source_order_tests;

mod common_financial_header;

#[cfg(test)]
mod bank_frame_source_control_tests;

mod framed_native_columns;

#[cfg(test)]
mod original_director_frames_tests;

#[cfg(test)]
mod legal_role_original_tests;

#[cfg(test)]
mod closed_frame_continuation_original_tests;

mod native_word_axes;
mod native_role_continuation;

#[cfg(test)]
mod financial_original_source_tests;

#[cfg(test)]mod native_rule_source_order_tests;

mod native_word_rulings;

#[cfg(test)]
mod source_frame_fences_original_tests;

#[cfg(test)]
mod enclosed_warichu_original_tests;

#[cfg(test)]
mod composed_inline_pipeline_tests;

#[cfg(test)]
mod frame_boundary_original_tests;

#[cfg(test)]
mod frame_projection_counterfactual_tests;

#[cfg(test)]
mod source_table_border_original_tests;

#[cfg(test)]
mod source_auxiliary_original_tests;

#[cfg(test)]
mod source_shared_note_original_tests;

#[cfg(test)]
mod source_numeric_sign_original_tests;

#[cfg(test)]
mod ruled_marker_original_tests;

#[cfg(test)]
mod ruled_marker_existing_layout_tests;

mod native_unit_dash;
mod native_word_outlines;
mod native_amount_marks;

#[cfg(test)]
mod source_unit_dash_original_tests;

#[cfg(test)]
mod orthogonal_gutters_original_tests;

mod ruled_cell_flow;

#[cfg(test)]
mod native_role_contracts;
#[cfg(test)]
mod native_role_caption_contracts;

mod native_sentence_cells;

#[cfg(test)]
mod native_sentence_contracts;

mod native_word_flow;
mod native_advance_band;

mod native_paragraph_scope;

mod amendment_quotes;

mod native_column_flow;

mod quoted_references;

mod partial_rule_columns;

#[cfg(test)] mod partial_rule_column_tests;

mod native_grid_fragments;

mod native_atomic_column;

mod native_caption_tiers;

mod native_parallel_edits;

mod native_cell_text;
mod native_latin_wraps;

mod native_cell_origin;


mod comparison_labels;

mod source_article_parts;

mod native_numeric_frames;

mod native_terminal_glyph;

mod source_font_alias;

mod native_table_scopes;

mod native_cell_lineage;

#[cfg(test)]
mod native_caption_scope_tests;

#[cfg(test)]
mod native_compound_caption_tests;

mod native_source_scene;

mod legacy_comparison_scope;

mod native_wrapped_amounts;
