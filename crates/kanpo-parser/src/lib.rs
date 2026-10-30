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

/// 目次の標題（例: 「○○規則の一部を改正する省令（総務七七）」）に最も合う記事の添字を返す。
///
/// 本文ヘッダには対象法令名のあとに法令番号「（…）」が挿入されるため、標題そのものの
/// 完全一致では拾えない。対象法令名（「の一部を」「を廃止」より前）を鍵に、まず完全一致、
/// 次に前方一致を縮めながら探し、制定文らしい（「次のように」「改める」等を含む）長い記事を採る。
///
/// 本文は行の途中で折り返されるので空白・改行を除いて比べる。対象法令名が短い（「種苗法」）
/// ときは制定機関略号を除いた標題全体を鍵にする。それでも見つからなければ、数字（漢数字・
/// 算用数字）とダッシュを除いて比べ直す（標題「人事院規則二―四」・本文「人事院規則２－４」）。
pub fn pick_article(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
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
fn title_number(title: &str) -> Option<(Vec<&str>, u64)> {
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

/// 記事の見出しの号数が `n` で、標題の機関名をすべて含むか。
fn has_number(a: &ArticleAnalysis, agencies: &[&str], n: u64) -> bool {
    let head = a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    heading_number(head) == Some(n) && agencies.iter().all(|ag| head.contains(ag))
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
fn heading_number(head: &str) -> Option<u64> {
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
