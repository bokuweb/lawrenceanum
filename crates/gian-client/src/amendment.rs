//! 法律案本文の改め文 (「第○条中「A」を「B」に改める。」) を条単位の改正操作に構造化する。
//!
//! 衆議院の提出時法律案 (`bill_text`) は改正対象法令ごとに
//! `X法（昭和○年法律第○号）の一部を次のように改正する。` の柱書きで始まり、
//! 以降の各文が改正操作を並べる。ここではそれを
//!
//! - 改正対象法令 ([`AmendedLaw`]: 題名・法令番号)
//! - 改め文 1 文 ([`AmendmentSentence`]: 原文と、続く新規条文などの挿入内容)
//! - 操作 ([`AmendmentOp`]: 位置 (条・項・号) と種別、置換前後の文字列)
//!
//! に分解する。位置は改正前の条番号 (`9_2` = 第九条の二) で持ち、e-Gov の
//! `article_id` (`art_9_2`) と突き合わせられる。読めなかった句は `unparsed` に残して
//! 件数で追えるようにし、推測で操作を作らない。LLM は使わない。

use serde::{Deserialize, Serialize};

pub const AMENDMENT_SCHEMA_VERSION: u32 = 1;

/// 1 法案の改め文の構造化結果。`public/gian/amendments/{session}/{bill_id}.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillAmendments {
    pub schema_version: u32,
    pub bill_id: String,
    pub session: u32,
    pub title: String,
    pub bill_type: Option<String>,
    /// 公布された法律の法令番号 (例: 「令和八年法律第十四号」)。未成立なら None。
    pub law_num: Option<String>,
    /// e-Gov の改正法令 ID (例: `508AC0000000014`)。versions.json の `amendment_law_id` と一致する。
    pub amendment_law_id: Option<String>,
    pub promulgation_date: Option<String>,
    /// 改め文を読んだ提出時法律案の出典。
    pub source_url: String,
    pub source_sha256: String,
    pub targets: Vec<AmendedLaw>,
    pub stats: ParseStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParseStats {
    pub targets: usize,
    pub sentences: usize,
    pub ops: usize,
    /// 操作として読めなかった句の数。
    pub unparsed_clauses: usize,
}

/// 改正対象法令 1 件分 (柱書き 1 つ分) の改め文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendedLaw {
    /// 法案内の位置 (例: 「第一条」「附則第六条」)。単一法令の改正法案なら None。
    pub provision: Option<String>,
    pub law_title: String,
    /// 柱書きの括弧書きの法令番号。同じ法案内で再出の場合は省略され None になる。
    pub law_num: Option<String>,
    /// 法令 ID。リンカが `public/laws/index.json` から解決する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub law_id: Option<String>,
    pub sentences: Vec<AmendmentSentence>,
    /// リンカが付ける law-diff との対応結果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<TargetLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentSentence {
    /// 改め文 1 文の原文。複数行にわたる括弧内は改行を保つ。
    pub text: String,
    pub ops: Vec<AmendmentOp>,
    /// 「次のように改める」「次の一条を加える」に続く新規条文など。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 操作として読めなかった句。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unparsed: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// 「A」を「B」に改める。
    Replace,
    /// 「A」の下に「B」を加える。
    InsertAfter,
    /// 「A」の上に「B」を加える。
    InsertBefore,
    /// 「A」を削る。
    DeleteText,
    /// 第○条 (第○項) を削る。
    DeleteProvision,
    /// 第○条を次のように改める。
    Rewrite,
    /// 第○条の次に次の一条を加える / 同項に次の一号を加える / 見出しを付する。
    Add,
    /// 第○条を第△条とする。
    Renumber,
    /// 第○条から第△条までを一条ずつ繰り下げる。
    Shift,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationPart {
    /// 本則の条 (章・節を含む)。
    Main,
    Suppl,
    Appendix,
    Toc,
    Title,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArticleRef {
    /// e-Gov の条番号 (`Num` 属性) 形式。第九条の二 → `9_2`。
    pub article: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    /// 位置の原文 (例: 「第三条第一項」「同条第二項」「別表第一」)。
    pub label: String,
    pub part: LocationPart,
    /// 位置が指す条。同条・同項は直前の条に解決済み。
    pub articles: Vec<ArticleRef>,
    /// 見出しへの操作か。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub caption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentOp {
    pub kind: OpKind,
    pub location: Location,
    /// 置換前・挿入位置・削除される文字列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// 置換後・挿入される文字列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// 改正後に現れる条 (改める・加える内容の条名、とする の移動先)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_articles: Vec<String>,
    /// リンカが付ける law-diff との対応結果。
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "match")]
    pub matched: Option<OpMatch>,
}

/// 改正対象法令単位の law-diff 対応結果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetLink {
    /// `linked` (改正前後の版が揃った) / `pending` (改正法令の版はすべて未施行で本文未取得) /
    /// `law_unresolved` / `revision_not_found` / `not_promulgated`。
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_revision_id: Option<String>,
    /// この改正法令で生じた版 (段階施行なら複数)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to_revision_ids: Vec<String>,
    /// この改正法令で生じるが、未施行で本文がまだ無い版。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_revision_ids: Vec<String>,
    /// law-diff 上で変化した条 (`art_*`)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_articles: Vec<String>,
    /// そのうち改め文のいずれかの操作が指していた条。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explained_articles: Vec<String>,
}

/// 操作単位の law-diff 対応結果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpMatch {
    /// `verified` (条が変化し文字列も確認) / `article` (条の変化のみ確認) /
    /// `unmatched` (指す条が変化していない) / `pending` (変化していないが同じ改正法令の未施行の版が残る) /
    /// `skipped` (附則・別表・目次など本則の条でない)。
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub article_ids: Vec<String>,
}

// ── 公開関数 ──────────────────────────────────────────────────────

/// 議案から改め文の構造化結果を作る。提出時法律案が無い、または改正対象が無い (制定法で
/// 附則にも改正が無い) 場合は None。
pub fn parse_bill_amendments(bill: &crate::Bill) -> Option<BillAmendments> {
    let doc = bill.documents.iter().find(|d| d.kind == "bill_text")?;
    let targets = parse_amendment_text(&doc.text);
    if targets.is_empty() {
        return None;
    }
    let stats = stats_of(&targets);
    let promulgation = bill
        .promulgation_date
        .as_deref()
        .and_then(crate::wareki_to_iso);
    let (law_num, amendment_law_id) =
        match (bill.promulgation_date.as_deref(), bill.law_num.as_deref()) {
            (Some(date), Some(num)) => promulgated_law_ids(date, num).unzip(),
            _ => (None, None),
        };
    Some(BillAmendments {
        schema_version: AMENDMENT_SCHEMA_VERSION,
        bill_id: bill.bill_id.clone(),
        session: bill.session,
        title: bill.title.clone(),
        bill_type: bill.bill_type.clone(),
        law_num,
        amendment_law_id,
        promulgation_date: promulgation,
        source_url: doc.url.clone(),
        source_sha256: doc.sha256.clone(),
        targets,
        stats,
    })
}

pub fn stats_of(targets: &[AmendedLaw]) -> ParseStats {
    let mut s = ParseStats {
        targets: targets.len(),
        ..Default::default()
    };
    for t in targets {
        s.sentences += t.sentences.len();
        for sen in &t.sentences {
            s.ops += sen.ops.len();
            s.unparsed_clauses += sen.unparsed.len();
        }
    }
    s
}

/// 「令和 8年 5月 7日」と法律番号「14」から (「令和八年法律第十四号」, `508AC0000000014`)。
pub fn promulgated_law_ids(promulgation_date: &str, law_number: &str) -> Option<(String, String)> {
    let s: String = promulgation_date
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let eras = [
        ("令和", 5),
        ("平成", 4),
        ("昭和", 3),
        ("大正", 2),
        ("明治", 1),
    ];
    let (rest, era_name, code) = eras
        .iter()
        .find_map(|(e, c)| s.strip_prefix(e).map(|r| (r, *e, *c)))?;
    let year_str = &rest[..rest.find('年')?];
    let year = if year_str == "元" {
        1
    } else {
        ascii_digits(year_str)?
    };
    let number = ascii_digits(law_number)?;
    let year_kanji = if year == 1 {
        "元".to_string()
    } else {
        kanji_number(year)
    };
    Some((
        format!("{era_name}{year_kanji}年法律第{}号", kanji_number(number)),
        format!("{code}{year:02}AC{number:010}"),
    ))
}

/// 本文全体から改正対象法令ごとの改め文を取り出す。
pub fn parse_amendment_text(text: &str) -> Vec<AmendedLaw> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut targets: Vec<AmendedLaw> = Vec::new();
    let mut current: Option<TargetState> = None;
    let mut in_suppl = false;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if is_reason_marker(line) {
            break;
        }
        if is_suppl_marker(line) {
            finish(&mut current, &mut targets);
            in_suppl = true;
            i += 1;
            continue;
        }
        if let Some((provision, title, law_num)) = parse_target_header(line) {
            finish(&mut current, &mut targets);
            let provision = provision.map(|p| if in_suppl { format!("附則{p}") } else { p });
            // 同じ法案内の再出は法令番号を省略するので、先に出た同名の柱書きから補う。
            let law_num = law_num.or_else(|| {
                targets
                    .iter()
                    .rev()
                    .find(|t| t.law_title == title)
                    .and_then(|t| t.law_num.clone())
            });
            current = Some(TargetState::new(AmendedLaw {
                provision,
                law_title: title,
                law_num,
                law_id: None,
                sentences: Vec::new(),
                link: None,
            }));
            i += 1;
            continue;
        }
        let Some(state) = current.as_mut() else {
            i += 1;
            continue;
        };
        // 内容モード外で「第二条 この法律は…」「２ …」が来たら、法案自身の次の条項
        // (施行期日・経過措置など) に移った。改正対象の改め文はここで終わる。
        if !state.content_mode && is_content_line(line) && !is_caption_line(line) {
            finish(&mut current, &mut targets);
            i += 1;
            continue;
        }
        if state.content_mode && is_content_line(line) {
            state.push_content(line);
            i += 1;
            continue;
        }
        if state.content_mode && !looks_like_directive(line) {
            state.push_content(line);
            i += 1;
            continue;
        }
        // 改め文 1 文: 括弧が閉じ「。」で終わるまで後続行を結合する。
        let (sentence, next) = join_sentence(&lines, i);
        i = next;
        if is_caption_line(&sentence) {
            // 「（○○法の一部改正）」等の見出し。
            state.content_mode = false;
            continue;
        }
        state.push_sentence(&sentence);
    }
    finish(&mut current, &mut targets);
    targets
}

// ── 行の分類 ──────────────────────────────────────────────────────

fn is_reason_marker(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "理由"
}

fn is_suppl_marker(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "附則"
}

fn is_caption_line(line: &str) -> bool {
    line.starts_with('（') && line.ends_with('）') && !line.contains('「')
}

/// `[第一条 ]X法（昭和○年法律第○号）の一部を次のように改正する。`
fn parse_target_header(line: &str) -> Option<(Option<String>, String, Option<String>)> {
    const TAIL: &str = "の一部を次のように改正する。";
    let body = line.strip_suffix(TAIL)?;
    if body.contains('「') {
        return None;
    }
    let (provision, body) = match split_provision_label(body) {
        Some((label, rest)) => (Some(label.to_string()), rest),
        None => (None, body),
    };
    let (title, law_num) = match body.strip_suffix('）') {
        Some(inner) => {
            let open = inner.rfind('（')?;
            let num = &inner[open + '（'.len_utf8()..];
            if num.ends_with('号') && ERAS.iter().any(|e| num.starts_with(e)) {
                (inner[..open].to_string(), Some(num.to_string()))
            } else {
                (body.to_string(), None)
            }
        }
        None => (body.to_string(), None),
    };
    if title.is_empty() || title.contains('。') {
        return None;
    }
    Some((provision, title, law_num))
}

const ERAS: [&str; 5] = ["明治", "大正", "昭和", "平成", "令和"];

/// 行頭の「第一条 」「１ 」など番号+空白を切り出す。
fn split_provision_label(s: &str) -> Option<(&str, &str)> {
    let idx = s.find([' ', '　'])?;
    let label = &s[..idx];
    let rest = s[idx..].trim_start();
    let numeric_label = label.starts_with('第')
        && label.ends_with('条')
        && label.chars().all(|c| {
            c == '第'
                || c == '条'
                || c == 'の'
                || kanji_digit(c).is_some()
                || matches!(c, '十' | '百' | '千')
        });
    let paragraph_label = label
        .chars()
        .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
    (numeric_label || paragraph_label).then_some((label, rest))
}

/// 新規条文・号・見出しなど「次のように」の内容行か。
fn is_content_line(line: &str) -> bool {
    if is_caption_line(line) {
        return true;
    }
    let Some(idx) = line.find([' ', '　']) else {
        return false;
    };
    let head = &line[..idx];
    if head.is_empty() {
        return false;
    }
    // 第○条 / 第○章 / 第○条及び第○条 / 第○条から第○条まで / 別表第一 等の見出し番号。
    let structural = head.chars().all(|c| {
        kanji_digit(c).is_some()
            || matches!(
                c,
                '十' | '百'
                    | '千'
                    | '第'
                    | '条'
                    | '章'
                    | '節'
                    | '款'
                    | '目'
                    | '編'
                    | 'の'
                    | '及'
                    | 'び'
                    | 'か'
                    | 'ら'
                    | 'ま'
                    | 'で'
                    | '並'
                    | 'に'
                    | '・'
                    | '別'
                    | '表'
                    | '附'
                    | '則'
            )
    });
    // 項番号 (全角/半角数字)、号 (漢数字)、イロハ、（１）。
    let numbered = head
        .chars()
        .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        || head
            .chars()
            .all(|c| kanji_digit(c).is_some() || matches!(c, '十' | '百'))
        || (head.chars().count() == 1 && IROHA.contains(head.chars().next().unwrap_or(' ')))
        || (head.starts_with(['（', '(']) && head.ends_with(['）', ')']));
    structural || numbered
}

const IROHA: &str = "イロハニホヘトチリヌルヲワカヨタレソツネナラムウヰノオクヤマケフコエテアサキユメミシヱヒモセス";

/// 内容モード中に、内容ではなく次の改め文が始まったとみなせるか。
fn looks_like_directive(line: &str) -> bool {
    let starts = [
        "第", "同", "附則", "別表", "目次", "本則", "題名", "備考", "様式", "別記", "次の",
    ];
    if !starts.iter().any(|s| line.starts_with(s)) {
        return false;
    }
    // 新規条文は「…は、」の主語を持つ文になる。改め文は括弧外に主語を持たない。
    let skeleton = skeletonize(line).skeleton;
    if skeleton.contains("は、") || skeleton.contains("ときは") {
        return false;
    }
    skeleton.contains(&format!("中{PH}"))
        || [
            "改める。",
            "加える。",
            "削る。",
            "とする。",
            "削除する。",
            "繰り下げる。",
            "繰り上げる。",
            "付する。",
        ]
        .iter()
        .any(|end| skeleton.ends_with(end))
}

/// `lines[start]` から、鉤括弧が閉じて「。」で終わるまでを 1 文に結合する。
fn join_sentence(lines: &[&str], start: usize) -> (String, usize) {
    let mut out = String::from(lines[start]);
    let mut i = start + 1;
    while i < lines.len() {
        let open = skeletonize(&out).open;
        if !open && out.ends_with('。') {
            break;
        }
        let next = lines[i];
        if !open {
            // 括弧外で文が切れている: 「を」「に、」などの続きだけ結合する。
            let continues = next.starts_with(['「', '」', 'に', 'を', '、', 'と', 'の']);
            if !continues {
                break;
            }
        }
        if open || next.starts_with('「') || out.ends_with('「') {
            out.push('\n');
        }
        out.push_str(next);
        i += 1;
    }
    (out, i)
}

// ── 法令単位の状態 ────────────────────────────────────────────────

struct TargetState {
    law: AmendedLaw,
    content_mode: bool,
    /// 「同条」が指す直前の条。
    last_article: Option<ArticleRef>,
}

impl TargetState {
    fn new(law: AmendedLaw) -> Self {
        Self {
            law,
            content_mode: false,
            last_article: None,
        }
    }

    fn push_content(&mut self, line: &str) {
        let Some(sentence) = self.law.sentences.last_mut() else {
            return;
        };
        let content = sentence.content.get_or_insert_with(String::new);
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(line);
        // 新規条文の条名を、内容を求めた操作の改正後の条として記録する。
        if let Some(article) = leading_article(line) {
            if let Some(op) = sentence
                .ops
                .iter_mut()
                .rev()
                .find(|o| matches!(o.kind, OpKind::Add | OpKind::Rewrite))
            {
                for a in article {
                    if !op.new_articles.contains(&a) {
                        op.new_articles.push(a);
                    }
                }
            }
        }
    }

    fn push_sentence(&mut self, text: &str) {
        let (ops, unparsed, wants_content) = parse_sentence(text, &mut self.last_article);
        self.content_mode = wants_content;
        self.law.sentences.push(AmendmentSentence {
            text: text.to_string(),
            ops,
            content: None,
            unparsed,
        });
    }
}

fn finish(current: &mut Option<TargetState>, targets: &mut Vec<AmendedLaw>) {
    if let Some(state) = current.take() {
        targets.push(state.law);
    }
}

/// 内容行の行頭の条名 (「第十六条及び第十七条 削除」なら 16, 17)。
fn leading_article(line: &str) -> Option<Vec<String>> {
    let idx = line.find([' ', '　'])?;
    let head = &line[..idx];
    if !head.starts_with('第') || !head.contains('条') || head.contains(['章', '節', '款']) {
        return None;
    }
    let refs = scan_articles(head);
    // 「第十六条から第十八条まで」は間も含めず両端だけ (間の条は改正前後で同名)。
    (!refs.is_empty()).then_some(refs)
}

// ── 1 文のパース ──────────────────────────────────────────────────

/// 鉤括弧の文字列をプレースホルダに置き換えた骨格と、括弧内文字列の列。
const PH: char = '\u{FFFC}';

struct Skeleton {
    /// 括弧内を [`PH`] に置き換え、括弧外の改行を除いた骨格。
    skeleton: String,
    /// 括弧内の文字列 (出現順)。
    quotes: Vec<String>,
    /// 閉じていない「 が残っているか (文が次の行に続く)。
    open: bool,
}

/// 「 の直後から対応する 」 を探す。改め文の括弧内は改正対象の文字列の断片なので、
/// 「内閣府」」を のように括弧の釣り合わない文字列も現れる。そこで
/// 「を」「に」「の下に」等の接続語が続く 」 を閉じ括弧の候補とし、
/// 中身の括弧が釣り合う最初の候補 → 無ければ最初の候補 → 無ければ深さで対応する 」 の順に採る。
fn find_closer(chars: &[char], open: usize) -> Option<usize> {
    let mut balance = 0i32;
    let mut first_candidate = None;
    let mut depth_closer = None;
    for j in open + 1..chars.len() {
        match chars[j] {
            '「' => balance += 1,
            '」' => {
                if balance == 0 && depth_closer.is_none() {
                    depth_closer = Some(j);
                }
                if followed_by_connector(chars, j + 1) {
                    if balance == 0 {
                        return Some(j);
                    }
                    first_candidate.get_or_insert(j);
                }
                balance -= 1;
            }
            _ => {}
        }
    }
    first_candidate.or(depth_closer)
}

fn followed_by_connector(chars: &[char], mut k: usize) -> bool {
    while chars.get(k) == Some(&'\n') {
        k += 1;
    }
    if k >= chars.len() {
        return true;
    }
    let rest: String = chars[k..chars.len().min(k + 6)].iter().collect();
    [
        "を",
        "に",
        "の下に",
        "の上に",
        "の項",
        "の欄",
        "及び",
        "並びに",
        "、",
        "。",
        "とあるのは",
        "と、",
        "とし",
        "とする",
        "中",
    ]
    .iter()
    .any(|c| rest.starts_with(c))
}

fn skeletonize(text: &str) -> Skeleton {
    let chars: Vec<char> = text.chars().collect();
    let mut skeleton = String::new();
    let mut quotes = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '「' => match find_closer(&chars, i) {
                Some(close) => {
                    let inner: String = chars[i + 1..close].iter().collect();
                    quotes.push(inner.trim_matches('\n').to_string());
                    skeleton.push(PH);
                    i = close + 1;
                }
                None => {
                    return Skeleton {
                        skeleton,
                        quotes,
                        open: true,
                    };
                }
            },
            '\n' => i += 1,
            c => {
                skeleton.push(c);
                i += 1;
            }
        }
    }
    Skeleton {
        skeleton,
        quotes,
        open: false,
    }
}

/// 1 文を操作列に。戻り値の bool は「次のように…」で内容行が続くか。
fn parse_sentence(
    text: &str,
    last_article: &mut Option<ArticleRef>,
) -> (Vec<AmendmentOp>, Vec<String>, bool) {
    let Skeleton {
        skeleton, quotes, ..
    } = skeletonize(text);
    let body = skeleton.trim_end_matches('。');
    // 「、」で句に分ける。操作の動詞で終わらない句 (位置の列挙「第二条第一項、第三条…」や
    // 「「A」、「B」及び「C」を削る」の括弧の列挙) は次の句とつなぐ。
    let mut clauses: Vec<String> = Vec::new();
    let mut pending = String::new();
    for part in body.split('、') {
        if !pending.is_empty() {
            pending.push('、');
        }
        pending.push_str(part);
        if clause_complete(&pending) {
            clauses.push(std::mem::take(&mut pending));
        }
    }
    if !pending.is_empty() {
        clauses.push(pending);
    }

    let mut ops = Vec::new();
    let mut unparsed = Vec::new();
    let mut wants_content = false;
    let mut current_loc: Option<Location> = None;
    let mut qi = 0usize;
    for clause in &clauses {
        let n = clause.chars().filter(|&c| c == PH).count();
        let end = (qi + n).min(quotes.len());
        let mut ctx = ClauseCtx {
            quotes: &quotes[qi.min(end)..end],
            current_loc: &mut current_loc,
            last_article,
        };
        match parse_clause(clause, &mut ctx) {
            Some(parsed) => {
                for (op, wants) in parsed {
                    wants_content |= wants;
                    ops.push(op);
                }
            }
            None => unparsed.push(restore(clause, &quotes[qi.min(end)..end])),
        }
        qi = end;
    }
    (ops, unparsed, wants_content)
}

/// 操作の動詞で終わる、または動詞を後続と共有する「AをBに」「Aの下にBを」で終わるか。
fn clause_complete(s: &str) -> bool {
    let verbs = [
        "改め",
        "改める",
        "加え",
        "加える",
        "削り",
        "削る",
        "とし",
        "とする",
        "削除する",
        "繰り下げ",
        "繰り下げる",
        "繰り上げ",
        "繰り上げる",
        "付し",
        "付する",
    ];
    if verbs.iter().any(|v| s.ends_with(v)) {
        return true;
    }
    s.ends_with(&format!("{PH}を{PH}に"))
        || s.ends_with(&format!("の下に{PH}を"))
        || s.ends_with(&format!("の上に{PH}を"))
}

/// 骨格の句を元の文字列に戻す (位置ラベル・未解析句の記録用)。
fn restore(s: &str, quotes: &[String]) -> String {
    let mut out = String::new();
    let mut qi = 0usize;
    for c in s.chars() {
        if c == PH {
            out.push('「');
            out.push_str(quotes.get(qi).map(String::as_str).unwrap_or(""));
            out.push('」');
            qi += 1;
        } else {
            out.push(c);
        }
    }
    out
}

struct ClauseCtx<'a> {
    /// この句に含まれる括弧内文字列 (出現順)。
    quotes: &'a [String],
    /// 同じ文で直前に出た位置。位置の無い句はこれを引き継ぐ。
    current_loc: &'a mut Option<Location>,
    /// 「同条」が指す直前の条。
    last_article: &'a mut Option<ArticleRef>,
}

impl ClauseCtx<'_> {
    /// 句の先頭 `prefix` を位置として読み、同じ文の現在位置にする。
    fn location(&mut self, prefix: &str) -> Location {
        let label = restore(prefix, self.quotes);
        let loc = parse_location(&label, prefix, self.current_loc.as_ref(), self.last_article);
        *self.current_loc = Some(loc.clone());
        loc
    }
}

fn op_of(
    kind: OpKind,
    location: Location,
    from: Option<String>,
    to: Option<String>,
) -> AmendmentOp {
    AmendmentOp {
        kind,
        location,
        from,
        to,
        new_articles: Vec::new(),
        matched: None,
    }
}

/// 1 句を操作に。戻り値の bool はその操作が内容行 (「次のように」) を求めるか。
fn parse_clause(clause: &str, ctx: &mut ClauseCtx<'_>) -> Option<Vec<(AmendmentOp, bool)>> {
    // 1) 位置中「…」の文字列操作。位置が無い句は同じ文の直前の位置を引き継ぐ。
    if let Some((loc_str, rest)) = split_text_op(clause) {
        let loc = if loc_str.is_empty() {
            ctx.current_loc.clone()?
        } else {
            ctx.location(loc_str)
        };
        let skip = loc_str.chars().filter(|&c| c == PH).count();
        let ops = parse_text_ops(rest, ctx.quotes.get(skip..)?, &loc)?;
        return Some(ops.into_iter().map(|op| (op, false)).collect());
    }

    // 2) 加える・付する: 「第○条の次に次の一条を加える」「同項に後段として次のように加える」
    //    「同条の前に見出しとして「…」を付する」。
    let adds = ["加え", "加える", "付し", "付する"];
    if adds.iter().any(|v| clause.ends_with(v)) {
        // 題名の無い法令に「次の題名を付する」。
        if clause.starts_with("次の題名") {
            let mut loc = ctx.location("題名");
            loc.part = LocationPart::Title;
            return Some(vec![(op_of(OpKind::Add, loc, None, None), true)]);
        }
        for sep in ["の次に", "の前に", "に後段として", "に"] {
            let Some(idx) = clause.find(sep) else {
                continue;
            };
            let (anchor, tail) = (&clause[..idx], &clause[idx + sep.len()..]);
            let caption = tail.contains("見出し");
            // 「同条に第一項として次の一項を加える」「第十八条第一項に第一号として次の一号を加える」
            let as_numbered = tail.contains("として次の");
            if anchor.is_empty()
                || !(tail.starts_with("次の") || sep == "に後段として" || caption || as_numbered)
            {
                continue;
            }
            let mut loc = ctx.location(anchor);
            loc.caption |= caption;
            let to = (caption && tail.contains(PH))
                .then(|| ctx.quotes.last().cloned())
                .flatten();
            let wants = clause.contains("次の");
            return Some(vec![(op_of(OpKind::Add, loc, None, to), wants)]);
        }
        return None;
    }

    // 3) 構造の操作: 位置 + を + 述部。
    let (target, verb) = split_structural(clause)?;
    let loc = ctx.location(target);
    let op = match verb {
        StructVerb::Delete => (op_of(OpKind::DeleteProvision, loc, None, None), false),
        StructVerb::Rewrite => (op_of(OpKind::Rewrite, loc, None, None), true),
        // 第○条の見出しを「…」に改める。
        StructVerb::ReplaceWhole => (
            op_of(OpKind::Replace, loc, None, ctx.quotes.last().cloned()),
            false,
        ),
        StructVerb::Renumber(dest) => {
            let mut dest_last = ctx.last_article.clone();
            let dest_loc = parse_location(dest, dest, Some(&loc), &mut dest_last);
            let new_articles = if dest.contains('条') {
                dest_loc
                    .articles
                    .iter()
                    .map(|a| a.article.clone())
                    .collect()
            } else {
                Vec::new()
            };
            let mut op = op_of(OpKind::Renumber, loc, None, Some(dest.to_string()));
            op.new_articles = new_articles;
            (op, false)
        }
        StructVerb::Shift(amount) => {
            // 「第十条から第十二条までを二条ずつ繰り下げ」→ 改正後は第十二条から第十四条。
            let chars: Vec<char> = amount.chars().collect();
            let step = take_kanji_number(&chars)
                .filter(|(_, len)| chars.get(*len) == Some(&'条'))
                .map(|(n, _)| n as i64);
            let sign = if clause.contains("繰り上げ") {
                -1
            } else {
                1
            };
            let new_articles = match step {
                Some(n) => loc
                    .articles
                    .iter()
                    .filter_map(|a| a.article.parse::<i64>().ok())
                    .map(|a| a + sign * n)
                    .filter(|a| *a > 0)
                    .map(|a| a.to_string())
                    .collect(),
                None => Vec::new(),
            };
            let mut op = op_of(OpKind::Shift, loc, None, Some(amount.to_string()));
            op.new_articles = new_articles;
            (op, false)
        }
    };
    Some(vec![op])
}

enum StructVerb<'a> {
    Delete,
    Rewrite,
    ReplaceWhole,
    Renumber(&'a str),
    Shift(&'a str),
}

fn split_structural(clause: &str) -> Option<(&str, StructVerb<'_>)> {
    for end in ["を削る", "を削り", "を削除する"] {
        if let Some(t) = clause.strip_suffix(end) {
            return (!t.contains(PH)).then_some((t, StructVerb::Delete));
        }
    }
    for end in ["を次のように改める", "を次のように改め"] {
        if let Some(t) = clause.strip_suffix(end) {
            return Some((t, StructVerb::Rewrite));
        }
    }
    for end in [format!("を{PH}に改める"), format!("を{PH}に改め")] {
        if let Some(t) = clause.strip_suffix(end.as_str()) {
            return (!t.contains(PH)).then_some((t, StructVerb::ReplaceWhole));
        }
    }
    for end in [
        "ずつ繰り下げる",
        "ずつ繰り下げ",
        "ずつ繰り上げる",
        "ずつ繰り上げ",
    ] {
        if let Some(t) = clause.strip_suffix(end) {
            let idx = t.rfind('を')?;
            return Some((&t[..idx], StructVerb::Shift(&t[idx + 'を'.len_utf8()..])));
        }
    }
    for end in ["とする", "とし"] {
        if let Some(t) = clause.strip_suffix(end) {
            if t.contains(PH) {
                return None;
            }
            // 「第二十条を第二十一条とし」「本則を本則第一項とし」「ホをニとし」
            // 「同表中七十三の七の項を七十三の八の項とし」。
            let idx = t.rfind('を')?;
            let dest = &t[idx + 'を'.len_utf8()..];
            if dest.is_empty() || dest.chars().count() > 30 || idx == 0 {
                return None;
            }
            return Some((&t[..idx], StructVerb::Renumber(dest)));
        }
    }
    None
}

/// 「位置中「A」…」を (位置, 「A」…) に分ける。位置が無い句 (「A」を「B」に) は ("", 句)。
/// 位置自体が括弧を含む (別表第一「…」の項中「A」を…) ので、後ろが文字列操作になる「中「」を探す。
fn split_text_op(clause: &str) -> Option<(&str, &str)> {
    if clause.starts_with(PH) {
        return Some(("", clause));
    }
    let marker = format!("中{PH}");
    let mut search = 0usize;
    while let Some(rel) = clause[search..].find(&marker) {
        let idx = search + rel;
        let rest = &clause[idx + '中'.len_utf8()..];
        if parse_text_ops(
            rest,
            &vec![String::new(); rest.matches(PH).count()],
            &dummy_location(),
        )
        .is_some()
        {
            return Some((&clause[..idx], rest));
        }
        search = idx + marker.len();
    }
    None
}

fn dummy_location() -> Location {
    Location {
        label: String::new(),
        part: LocationPart::Main,
        articles: Vec::new(),
        caption: false,
    }
}

/// 位置を除いた文字列操作の列 (「A」を「B」に、「C」の下に「D」を加え …)。
/// 対象の文字列は「「A」及び「B」を「C」に改め」のように並べて共有されることがある。
fn parse_text_ops(rest: &str, q: &[String], loc: &Location) -> Option<Vec<AmendmentOp>> {
    let mut out = Vec::new();
    let mut s = rest;
    let mut qi = 0usize;
    let get = |i: usize| q.get(i).cloned();
    while !s.is_empty() {
        s = s.strip_prefix('、').unwrap_or(s);
        // 先頭の「A」(及び「B」…) の並び。
        let mut n = 0usize;
        while let Some(r) = s.strip_prefix(PH) {
            n += 1;
            s = r;
            // 「、」の後が「C」を「D」に のように新しい操作なら並びはここまで。
            let next = ["及び", "並びに", "、"].iter().find_map(|sep| {
                s.strip_prefix(sep)
                    .filter(|r| r.starts_with(PH) && (*sep != "、" || !starts_with_pair(r)))
            });
            match next {
                Some(r) => s = r,
                None => break,
            }
        }
        if n == 0 {
            return None;
        }
        let targets: Vec<Option<String>> = (0..n).map(|i| get(qi + i)).collect();
        qi += n;
        let (kind, rest, verbs): (OpKind, &str, &[&str]) =
            if let Some(r) = s.strip_prefix(&format!("を{PH}に")) {
                (OpKind::Replace, r, &["改める", "改め"])
            } else if let Some(r) = s.strip_prefix(&format!("の下に{PH}を")) {
                (OpKind::InsertAfter, r, &["加える", "加え"])
            } else if let Some(r) = s.strip_prefix(&format!("の上に{PH}を")) {
                (OpKind::InsertBefore, r, &["加える", "加え"])
            } else if s == "を削り" || s == "を削る" {
                for from in targets {
                    out.push(op_of(OpKind::DeleteText, loc.clone(), from, None));
                }
                break;
            } else {
                return None;
            };
        let to = get(qi);
        qi += 1;
        for from in targets {
            out.push(op_of(kind, loc.clone(), from, to.clone()));
        }
        s = strip_verb(rest, verbs);
        if !s.is_empty() && !s.starts_with('、') {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// 「「A」を「B」に、「C」を…」の「、」の後が新しい操作 (「C」を「D」に 等) で始まるか。
fn starts_with_pair(s: &str) -> bool {
    let Some(r) = s.strip_prefix(PH) else {
        return false;
    };
    r.starts_with(&format!("を{PH}に"))
        || r.starts_with(&format!("の下に{PH}を"))
        || r.starts_with(&format!("の上に{PH}を"))
}

fn strip_verb<'a>(s: &'a str, verbs: &[&str]) -> &'a str {
    verbs.iter().find_map(|v| s.strip_prefix(v)).unwrap_or(s)
}
// ── 位置 ──────────────────────────────────────────────────────────

fn parse_location(
    label: &str,
    skeleton: &str,
    current: Option<&Location>,
    last_article: &mut Option<ArticleRef>,
) -> Location {
    let s = skeleton;
    let part = if s.starts_with("附則") {
        LocationPart::Suppl
    } else if s.starts_with("別表") || s.starts_with("別記") || s.starts_with("様式") {
        LocationPart::Appendix
    } else if s.starts_with("目次") {
        LocationPart::Toc
    } else if s.starts_with("題名") {
        LocationPart::Title
    } else if s.starts_with('同') {
        // 「同条」「同項」「同表」は直前の位置 (附則・別表を含む) の続き。
        current.map_or(LocationPart::Main, |c| c.part)
    } else {
        LocationPart::Main
    };
    let caption = s.contains("見出し");
    let mut articles: Vec<ArticleRef> = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    // 「第十六条から第十八条まで」の始点 (articles の添字) と、直前の条参照の終端位置。
    let mut range_from: Option<usize> = None;
    let mut last_article_end: Option<usize> = None;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("から") {
            if last_article_end == Some(i) && !articles.is_empty() {
                range_from = Some(articles.len() - 1);
            }
            i += 2;
            continue;
        }
        if rest.starts_with("同条") {
            if let Some(a) = last_article.clone() {
                articles.push(ArticleRef {
                    paragraph: None,
                    item: None,
                    ..a
                });
            }
            i += 2;
            continue;
        }
        if rest.starts_with("同項") {
            if let Some(a) = last_article.clone() {
                articles.push(ArticleRef { item: None, ..a });
            }
            i += 2;
            continue;
        }
        if rest.starts_with("同号") {
            if let Some(a) = last_article.clone() {
                articles.push(a);
            }
            i += 2;
            continue;
        }
        if chars[i] == '第' {
            if let Some((n, len)) = take_kanji_number(&chars[i + 1..]) {
                let after = chars.get(i + 1 + len).copied();
                match after {
                    Some('条') => {
                        let mut id = n.to_string();
                        let mut j = i + 2 + len;
                        // 「の二」「の三の二」
                        while chars.get(j) == Some(&'の') {
                            match take_kanji_number(&chars[j + 1..]) {
                                Some((m, l)) => {
                                    id.push_str(&format!("_{m}"));
                                    j += 1 + l;
                                }
                                None => break,
                            }
                        }
                        articles.push(ArticleRef {
                            article: id,
                            paragraph: None,
                            item: None,
                        });
                        if let Some(start) = range_from.take() {
                            if chars[j..].starts_with(&['ま', 'で']) {
                                expand_range(&mut articles, start);
                            }
                        }
                        i = j;
                        last_article_end = Some(j);
                        *last_article = articles.last().cloned();
                        continue;
                    }
                    Some('項') => {
                        attach(&mut articles, last_article, current, |a| {
                            a.paragraph = Some(n);
                            a.item = None;
                        });
                        i += 2 + len;
                        continue;
                    }
                    Some('号') => {
                        attach(&mut articles, last_article, current, |a| a.item = Some(n));
                        i += 2 + len;
                        continue;
                    }
                    _ => {
                        i += 1 + len;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    // 位置に条が無い (「同表第四十号」「第四号から第七号まで」) なら同じ文の直前の位置の条。
    if articles.is_empty() {
        if let Some(c) = current {
            if c.part == part {
                articles = c.articles.clone();
            }
        }
    }
    if let Some(last) = articles.last() {
        *last_article = Some(last.clone());
    }
    dedup_refs(&mut articles);
    Location {
        label: label.to_string(),
        part,
        articles,
        caption,
    }
}

/// 「第十六条から第十九条まで」の両端 (`articles[start]` と末尾) の間の条を補う。
/// 枝番の条 (第九条の二) を含む範囲は間を列挙できないので両端のままにする。
fn expand_range(articles: &mut Vec<ArticleRef>, start: usize) {
    let (Some(a), Some(b)) = (
        articles
            .get(start)
            .and_then(|r| r.article.parse::<u32>().ok()),
        articles.last().and_then(|r| r.article.parse::<u32>().ok()),
    ) else {
        return;
    };
    if b <= a + 1 || b - a > 200 {
        return;
    }
    let end = articles.pop().expect("checked above");
    for n in a + 1..b {
        articles.push(ArticleRef {
            article: n.to_string(),
            paragraph: None,
            item: None,
        });
    }
    articles.push(end);
}

/// 項・号を直前の条参照に付ける。条参照が無ければ「同条」相当 (直前の条) から作る。
fn attach(
    articles: &mut Vec<ArticleRef>,
    last_article: &mut Option<ArticleRef>,
    current: Option<&Location>,
    f: impl Fn(&mut ArticleRef),
) {
    match articles.last_mut() {
        Some(a) => {
            // 既に項が付いている条に別の項が来たら (第三項及び第五項)、同じ条の参照を足す。
            let mut b = a.clone();
            f(&mut b);
            if a.paragraph.is_some() && b.paragraph != a.paragraph {
                articles.push(b);
            } else {
                *a = b;
            }
        }
        None => {
            let base = last_article
                .clone()
                .or_else(|| current.and_then(|c| c.articles.last().cloned()));
            if let Some(mut a) = base {
                f(&mut a);
                articles.push(a);
            }
        }
    }
    if let Some(a) = articles.last() {
        *last_article = Some(a.clone());
    }
}

fn dedup_refs(refs: &mut Vec<ArticleRef>) {
    let mut seen: Vec<ArticleRef> = Vec::new();
    refs.retain(|r| {
        if seen.contains(r) {
            false
        } else {
            seen.push(r.clone());
            true
        }
    });
}

/// 「第十六条及び第十七条」→ ["16", "17"]。
fn scan_articles(s: &str) -> Vec<String> {
    let mut last = None;
    parse_location(s, s, None, &mut last)
        .articles
        .into_iter()
        .map(|a| a.article)
        .collect()
}

// ── 数字 ──────────────────────────────────────────────────────────

fn kanji_digit(c: char) -> Option<u32> {
    "〇一二三四五六七八九"
        .chars()
        .position(|d| d == c)
        .map(|p| p as u32)
}

/// 漢数字 (十・百・千を含む) を先頭から読む。戻り値は (値, 文字数)。
fn take_kanji_number(chars: &[char]) -> Option<(u32, usize)> {
    let mut total = 0u32;
    let mut current = 0u32;
    let mut len = 0usize;
    let mut has_digit = false;
    for &c in chars {
        if let Some(d) = kanji_digit(c) {
            current = current * 10 + d;
            has_digit = true;
        } else if let Some(unit) = match c {
            '十' => Some(10),
            '百' => Some(100),
            '千' => Some(1000),
            _ => None,
        } {
            total += if current == 0 { 1 } else { current } * unit;
            current = 0;
            has_digit = true;
        } else {
            break;
        }
        len += 1;
    }
    has_digit.then_some((total + current, len))
}

fn ascii_digits(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .filter_map(|c| {
            if c.is_ascii_digit() {
                Some(c)
            } else if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32)
            } else {
                None
            }
        })
        .collect();
    digits.parse().ok()
}

/// 1..=9999 を法令番号の漢数字 (十・百・千、位取りの「一」は省く) に。
pub fn kanji_number(n: u32) -> String {
    const D: [&str; 10] = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return "〇".to_string();
    }
    let mut out = String::new();
    for (unit, name) in [(1000, "千"), (100, "百"), (10, "十")] {
        let d = (n / unit) % 10;
        if d > 0 {
            if d > 1 {
                out.push_str(D[d as usize]);
            }
            out.push_str(name);
        }
    }
    out.push_str(D[(n % 10) as usize]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_ops(text: &str) -> Vec<AmendmentOp> {
        let targets = parse_amendment_text(text);
        targets[0]
            .sentences
            .iter()
            .flat_map(|s| s.ops.clone())
            .collect()
    }

    #[test]
    fn law_ids_from_promulgation() {
        assert_eq!(
            promulgated_law_ids("令和 8年 5月 7日", "14"),
            Some((
                "令和八年法律第十四号".to_string(),
                "508AC0000000014".to_string()
            ))
        );
        assert_eq!(
            promulgated_law_ids("平成31年 4月26日", "115").unwrap().1,
            "431AC0000000115"
        );
        assert_eq!(kanji_number(2155), "二千百五十五");
    }

    #[test]
    fn kanji_numbers() {
        let c: Vec<char> = "二十七条".chars().collect();
        assert_eq!(take_kanji_number(&c), Some((27, 3)));
        let c: Vec<char> = "百十七条".chars().collect();
        assert_eq!(take_kanji_number(&c), Some((117, 3)));
        let c: Vec<char> = "二千百五十五".chars().collect();
        assert_eq!(take_kanji_number(&c), Some((2155, 6)));
    }

    #[test]
    fn simple_replace() {
        let text = "裁判所職員定員法（昭和二十六年法律第五十三号）の一部を次のように改正する。\n第二条中「二万千六百六十六人」を「二万千五百四十人」に改める。\n附 則\nこの法律は、公布の日から施行する。";
        let targets = parse_amendment_text(text);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].law_title, "裁判所職員定員法");
        assert_eq!(
            targets[0].law_num.as_deref(),
            Some("昭和二十六年法律第五十三号")
        );
        let ops = first_ops(text);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, OpKind::Replace);
        assert_eq!(ops[0].location.articles[0].article, "2");
        assert_eq!(ops[0].from.as_deref(), Some("二万千六百六十六人"));
        assert_eq!(ops[0].to.as_deref(), Some("二万千五百四十人"));
    }

    #[test]
    fn chained_clauses_and_anaphora() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n第五条第二項を削り、同条第三項中「第一項」を「前項」に改め、「地方公共団体の設置する」を削り、同項を同条第二項とする。";
        let ops = first_ops(text);
        let kinds: Vec<OpKind> = ops.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            vec![
                OpKind::DeleteProvision,
                OpKind::Replace,
                OpKind::DeleteText,
                OpKind::Renumber
            ]
        );
        assert_eq!(
            ops[1].location.articles[0],
            ArticleRef {
                article: "5".into(),
                paragraph: Some(3),
                item: None
            }
        );
        assert_eq!(ops[2].location.articles[0].paragraph, Some(3));
        assert_eq!(ops[3].to.as_deref(), Some("同条第二項"));
    }

    #[test]
    fn shared_verb_and_insert() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n第十五条第一項中「国は、」の下に「毎年度、」を加え、「全額」を「四分の三」に、「金額を」を「額を」に改め、同条第二項中「範囲内で、」の下に「A」を加える。";
        let ops = first_ops(text);
        let kinds: Vec<OpKind> = ops.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            vec![
                OpKind::InsertAfter,
                OpKind::Replace,
                OpKind::Replace,
                OpKind::InsertAfter
            ]
        );
        assert_eq!(ops[0].from.as_deref(), Some("国は、"));
        assert_eq!(ops[0].to.as_deref(), Some("毎年度、"));
        assert_eq!(ops[2].to.as_deref(), Some("額を"));
        assert_eq!(ops[3].location.articles[0].paragraph, Some(2));
        assert_eq!(ops[3].location.articles[0].article, "15");
    }

    #[test]
    fn add_article_records_new_article_and_content() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n本則に次の一条を加える。\n（行財政改革の徹底）\n第五条 政府は、改革を徹底するものとする。\n２ 政府は、検討を加えるものとする。\n第九条を次のように改める。\n第九条 削除\n第十条中「A」を「B」に改める。";
        let targets = parse_amendment_text(text);
        let s = &targets[0].sentences;
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].ops[0].kind, OpKind::Add);
        assert_eq!(s[0].ops[0].new_articles, vec!["5".to_string()]);
        assert!(s[0].content.as_deref().unwrap().contains("改革を徹底"));
        assert_eq!(s[1].ops[0].kind, OpKind::Rewrite);
        assert_eq!(s[1].ops[0].location.articles[0].article, "9");
        assert_eq!(s[2].ops[0].location.articles[0].article, "10");
    }

    #[test]
    fn multiline_quotes_join_into_one_sentence() {
        let text = "電気事業法（昭和三十九年法律第百七十号）の一部を次のように改正する。\n目次中「第三款 会計」を\n「\n第三款 会計\n第四款 認定\n」\nに、「第二十七条の二十九の六」を「第二十七条の二十九の八」に改める。\n第二条の九第二項中「前項の場合」を「前二項」に改め、同項を同条第三項とし、同条第一項の次に次の一項を加える。\n２ 経済産業大臣は、取り消すことができる。";
        let targets = parse_amendment_text(text);
        let s = &targets[0].sentences;
        assert_eq!(s.len(), 2, "{s:#?}");
        assert_eq!(s[0].ops.len(), 2);
        assert_eq!(s[0].ops[0].location.part, LocationPart::Toc);
        assert_eq!(s[0].ops[0].to.as_deref(), Some("第三款 会計\n第四款 認定"));
        assert_eq!(s[1].ops[0].location.articles[0].article, "2_9");
        assert_eq!(s[1].ops[2].kind, OpKind::Add);
        assert!(s[1]
            .content
            .as_deref()
            .unwrap()
            .starts_with("２ 経済産業大臣"));
    }

    #[test]
    fn multi_law_bill_and_suppl_amendments() {
        let text = "第二二一回\n閣第四二号\nA法及びB法の一部を改正する法律案\n（A法の一部改正）\n第一条 A法（平成二十五年法律第八十六号）の一部を次のように改正する。\n第六条第一項中「第三号」を「第五号」に改める。\n（B法の一部改正）\n第二条 B法（昭和三十五年法律第百五号）の一部を次のように改正する。\n第百十七条の二第一項第一号中「状態（」の下に「X」を加える。\n附 則\n（施行期日）\n第一条 この法律は、公布の日から施行する。\n（C法の一部改正）\n第二条 C法（昭和二十二年法律第六十七号）の一部を次のように改正する。\n別表第一中「、第十七条」を削る。\n理 由\n必要がある。";
        let targets = parse_amendment_text(text);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].provision.as_deref(), Some("第一条"));
        assert_eq!(targets[1].law_title, "B法");
        let op = &targets[1].sentences[0].ops[0];
        assert_eq!(
            op.location.articles[0],
            ArticleRef {
                article: "117_2".into(),
                paragraph: Some(1),
                item: Some(1)
            }
        );
        assert_eq!(targets[2].provision.as_deref(), Some("附則第二条"));
        assert_eq!(
            targets[2].sentences.len(),
            1,
            "附則の施行期日は改め文に含めない"
        );
        assert_eq!(
            targets[2].sentences[0].ops[0].location.part,
            LocationPart::Appendix
        );
    }

    #[test]
    fn renumber_shift_and_caption() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n第二条中第八号を第十一号とし、第四号から第七号までを三号ずつ繰り下げ、第三号を第五号とし、同号の次に次の一号を加える。\n六 自動車を走行させる行為\n第九十五条の見出しを削り、同条の前に見出しとして「（商業用レコードの二次使用）」を付し、同条第一項中「第四項まで」の下に「、次条第一項」を加える。\n第九十五条の三第四項中「A」を「B」に改め、第四章第二節中同条を第九十五条の五とし、第九十五条の次に次の二条を加える。\n第九十五条の二 実演が録音されている。\n２ 前項の規定は、適用しない。\n第九十五条の三 商業用レコード。";
        let targets = parse_amendment_text(text);
        let s = &targets[0].sentences;
        assert_eq!(s.len(), 3, "{s:#?}");
        let kinds: Vec<OpKind> = s[0].ops.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            vec![
                OpKind::Renumber,
                OpKind::Shift,
                OpKind::Renumber,
                OpKind::Add
            ]
        );
        assert!(s[0]
            .ops
            .iter()
            .all(|o| o.location.articles[0].article == "2"));
        assert!(s[0].unparsed.is_empty());
        let kinds: Vec<OpKind> = s[1].ops.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            vec![OpKind::DeleteProvision, OpKind::Add, OpKind::InsertAfter]
        );
        assert!(s[1].ops[0].location.caption);
        let renumber = &s[2].ops[1];
        assert_eq!(renumber.kind, OpKind::Renumber);
        assert_eq!(renumber.location.articles[0].article, "95_3");
        assert_eq!(renumber.new_articles, vec!["95_5".to_string()]);
        assert_eq!(
            s[2].ops[2].new_articles,
            vec!["95_2".to_string(), "95_3".to_string()]
        );
    }

    #[test]
    fn content_line_ending_with_directive_verb_stays_content() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n第三条第一項中「A」を「B」に改め、同項に後段として次のように加える。\n薬物の影響により人を死傷させた者も、同様とする。\n第六条第一項中「第三号」を「第五号」に改める。";
        let targets = parse_amendment_text(text);
        let s = &targets[0].sentences;
        assert_eq!(s.len(), 2);
        assert!(s[0].content.as_deref().unwrap().contains("同様とする"));
        assert_eq!(s[0].ops[1].kind, OpKind::Add);
    }

    #[test]
    fn article_ranges_expand_and_shift() {
        let text = "X法（令和元年法律第一号）の一部を次のように改正する。\n第十六条から第十八条までを削る。\n第二十条から第二十二条までを二条ずつ繰り下げ、第十九条の次に次の二条を加える。\n第二十条 A。\n第二十一条 B。";
        let t = parse_amendment_text(text);
        let s = &t[0].sentences;
        assert_eq!(articles(&s[0].ops[0]), vec!["16", "17", "18"]);
        assert_eq!(s[1].ops[0].kind, OpKind::Shift);
        assert_eq!(articles(&s[1].ops[0]), vec!["20", "21", "22"]);
        assert_eq!(s[1].ops[0].new_articles, vec!["22", "23", "24"]);
        assert_eq!(s[1].ops[1].new_articles, vec!["20", "21"]);
    }

    #[test]
    fn omitted_law_number_reuses_earlier_header() {
        let text = "第一条 地方税法（昭和二十五年法律第二百二十六号）の一部を次のように改正する。\n第一条中「A」を「B」に改める。\n第二条 地方税法の一部を次のように改正する。\n第二条中「C」を「D」に改める。";
        let targets = parse_amendment_text(text);
        assert_eq!(targets.len(), 2);
        assert_eq!(
            targets[1].law_num.as_deref(),
            Some("昭和二十五年法律第二百二十六号")
        );
    }

    // ── 実ページ fixture (衆議院 提出時法律案, 第221回) ──────────────

    fn fixture(name: &str) -> Vec<AmendedLaw> {
        let html = match name {
            "g22109001" => include_str!("../tests/fixtures/shugiin_g22109001.htm"),
            "g22109008" => include_str!("../tests/fixtures/shugiin_g22109008.htm"),
            "g22109019" => include_str!("../tests/fixtures/shugiin_g22109019.htm"),
            "g22109036" => include_str!("../tests/fixtures/shugiin_g22109036.htm"),
            "g22109042" => include_str!("../tests/fixtures/shugiin_g22109042.htm"),
            "g22109062" => include_str!("../tests/fixtures/shugiin_g22109062.htm"),
            _ => unreachable!(),
        };
        parse_amendment_text(&crate::extract_document_text(html))
    }

    fn kinds(s: &AmendmentSentence) -> Vec<OpKind> {
        s.ops.iter().map(|o| o.kind).collect()
    }

    fn articles(op: &AmendmentOp) -> Vec<&str> {
        op.location
            .articles
            .iter()
            .map(|a| a.article.as_str())
            .collect()
    }

    #[test]
    fn real_court_staff_bill() {
        // 閣法第19号 裁判所職員定員法の一部を改正する法律案。附則・理由は改め文に含めない。
        let t = fixture("g22109019");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].sentences.len(), 1);
        let op = &t[0].sentences[0].ops[0];
        assert_eq!((op.kind, articles(op)), (OpKind::Replace, vec!["2"]));
        assert_eq!(op.to.as_deref(), Some("二万千五百四十人"));
    }

    #[test]
    fn real_public_bond_bill_adds_article_to_main_provision() {
        // 閣法第1号 公債特例法改正: 「第一条並びに第三条の見出し及び同条第一項中」と本則への条の追加。
        let t = fixture("g22109001");
        let s = &t[0].sentences;
        assert_eq!(s.len(), 2);
        assert_eq!(articles(&s[0].ops[0]), vec!["1", "3", "3"]);
        assert!(s[0].ops[0].location.caption);
        assert_eq!(kinds(&s[1]), vec![OpKind::Add]);
        assert_eq!(s[1].ops[0].new_articles, vec!["5"]);
        assert!(s[1]
            .content
            .as_deref()
            .unwrap()
            .contains("（行財政改革の徹底）"));
    }

    #[test]
    fn real_tuition_support_bill() {
        // 閣法第8号 高等学校等就学支援金法改正: 削除・繰上げ・「次のように改める」と附則の他法改正。
        let t = fixture("g22109008");
        assert_eq!(t.len(), 2);
        let s = &t[0].sentences;
        assert_eq!(s.len(), 9);
        assert!(s.iter().all(|s| s.unparsed.is_empty()));
        assert_eq!(
            kinds(&s[3]),
            vec![
                OpKind::DeleteProvision,
                OpKind::Replace,
                OpKind::DeleteText,
                OpKind::Renumber
            ]
        );
        assert_eq!(kinds(&s[6]), vec![OpKind::Rewrite]);
        assert_eq!(s[6].ops[0].new_articles, vec!["16", "17"]);
        assert_eq!(t[1].provision.as_deref(), Some("附則第六条"));
        assert_eq!(t[1].law_title, "地方自治法");
        assert_eq!(
            t[1].sentences[0].ops[0].location.part,
            LocationPart::Appendix
        );
    }

    #[test]
    fn real_electricity_bill_multiline_toc_and_new_articles() {
        // 閣法第36号 電気事業法改正: 目次の複数行の置換、款・条の追加。
        let t = fixture("g22109036");
        assert_eq!(t[0].law_num.as_deref(), Some("昭和三十九年法律第百七十号"));
        let s = &t[0].sentences;
        assert_eq!(s[0].ops.len(), 4);
        assert!(s[0]
            .ops
            .iter()
            .all(|o| o.location.part == LocationPart::Toc));
        assert!(s[0].ops[0]
            .to
            .as_deref()
            .unwrap()
            .contains("第四款 基幹送変電設備整備等計画の認定等"));
        let add = s
            .iter()
            .flat_map(|s| &s.ops)
            .find(|o| o.new_articles.contains(&"27_3_2".to_string()));
        assert_eq!(add.unwrap().new_articles, vec!["27_3_2", "27_3_3"]);
        let stats = stats_of(&t);
        assert!(stats.ops > 150, "{stats:?}");
        assert!(stats.unparsed_clauses <= 2, "{stats:?}");
    }

    #[test]
    fn real_dangerous_driving_bill_two_laws() {
        // 閣法第42号: 本則第一条・第二条で 2 法令を改正。号の繰下げ・後段の追加。
        let t = fixture("g22109042");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].law_title, "道路交通法");
        let s = &t[0].sentences;
        assert_eq!(
            kinds(&s[2]),
            vec![
                OpKind::Renumber,
                OpKind::Shift,
                OpKind::Renumber,
                OpKind::Add
            ]
        );
        assert!(s[2]
            .content
            .as_deref()
            .unwrap()
            .starts_with("六 殊更にタイヤ"));
        assert_eq!(
            kinds(&s[5]),
            vec![OpKind::Replace, OpKind::Replace, OpKind::Add]
        );
        assert!(t
            .iter()
            .flat_map(|t| &t.sentences)
            .all(|s| s.unparsed.is_empty()));
    }

    #[test]
    fn real_copyright_bill_renumbers_and_suppl_targets() {
        // 閣法第62号 著作権法改正: 条の移動 (同条を第九十五条の五とし) と附則での多数の他法改正。
        let t = fixture("g22109062");
        let s = &t[0].sentences;
        let renumber: Vec<&AmendmentOp> = s
            .iter()
            .flat_map(|s| &s.ops)
            .filter(|o| o.kind == OpKind::Renumber)
            .collect();
        assert_eq!(renumber[0].new_articles, vec!["95_5"]);
        assert_eq!(renumber[1].new_articles, vec!["95_4"]);
        let suppl = s.last().unwrap();
        assert!(
            suppl
                .ops
                .iter()
                .all(|o| o.location.part == LocationPart::Suppl),
            "同条も附則の続き"
        );
        assert!(t.len() >= 9);
        assert!(t.iter().any(|t| t.law_title == "著作権等管理事業法"));
        assert_eq!(stats_of(&t).unparsed_clauses, 0);
    }
}
