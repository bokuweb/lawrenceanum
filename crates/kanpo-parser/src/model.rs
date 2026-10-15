//! 改め文の構造化表現。`reconstruct_vertical` が出すマーカー付きテキストを、
//! HTML や表など任意の形式に変換しやすい構造化データへ整える。
//!
//! - `Document` — 1 件の改め文。形式（prose/shinkyu）とブロック列。
//! - `Block` — 段落（散文）か、新旧対照表（改正後/改正前）。
//! - `Run` — 連続するテキスト片。傍線（下線）の有無を持つ。
//!
//! 傍線は現状の `pdftotext`/`pdftocairo` 出力からは復元できないため、いまは常に
//! `false`（型としてのみ用意し、将来の傍線抽出で充填できるようにする）。

use serde::Serialize;

use crate::format::detect_format;
use crate::trace::{parse_table_marker, Table, SHINKYU_END};

/// 改め文 1 件の構造化表現。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    /// "prose" | "shinkyu" | "unknown"。
    pub format: String,
    /// 文書を構成するブロック列（出現順）。
    pub blocks: Vec<Block>,
}

/// 文書ブロック。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// 散文（前文・散文改め文など）。
    Paragraph { runs: Vec<Run> },
    /// 新旧対照表の 1 段。条（別表）ごとの行に分け、各行で改正後/改正前を対応させる。
    Shinkyu { rows: Vec<ShinkyuRow> },
}

/// 新旧対照表の 1 行（条・別表など 1 単位）。改正後/改正前のセル。
/// 片側のみ存在する行（新設・削除）はもう一方が空 Vec になる。
/// 別表（罫線で区切られた表）の行では `after_table`/`before_table` に 2D 表が入る。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShinkyuRow {
    pub after: Vec<Run>,
    pub before: Vec<Run>,
    /// 別表の改正後側 2D 表（あれば）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_table: Option<NestedTable>,
    /// 別表の改正前側 2D 表（あれば）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_table: Option<NestedTable>,
}

/// 罫線の表を復元した 2D 表。`rows[r][c]` がセルの Run 列。
///
/// 結合セルは `merges` に左上のマスと広がりを持ち、結合で覆われる残りのマスは空になる。
/// 縦組みの表は「縦の列＝行」に転置している（右の列が 1 行目）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NestedTable {
    pub rows: Vec<Vec<Vec<Run>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merges: Vec<CellMerge>,
}

/// 結合セル（`row`・`col` の位置から `rowspan` × `colspan` マス）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CellMerge {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
}

/// 同一スタイルが続くテキスト片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    /// 傍線（下線）が付いているか。現状は常に false（将来の傍線抽出用の予約フィールド）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    /// 本文の流れの中に置かれた表（あれば `text` は空）。新旧対照表のセルの中の別表など。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<NestedTable>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Run {
    /// 傍線無しのプレーンな Run を作る。
    pub fn plain(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            underline: false,
            table: None,
        }
    }
}

/// テキストの 1 セル（複数行）を 1 つの Run に畳む。空なら空 Vec。
/// 縦書きの列折り返しで途中改行された行は結合し、条・号・文末などの構造的改行のみ残す。
fn cell_runs(lines: &[&str]) -> Vec<Run> {
    let text = join_wrapped_lines(lines);
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Run::plain(text)]
    }
}

/// 縦書きの列折り返し（語の途中での改行）を結合する。
///
/// 各行は復元時の 1 縦列に相当し、列幅で途中改行される。文末（`。`）で終わる行や、
/// 次行が構造マーカー（`第○条` / 号（一・二…/イ・ロ…/数字）/ `（見出し）` / `別表` /
/// `附則`）で始まる箇所だけ改行を残し、それ以外は連結して読みやすくする。
fn join_wrapped_lines(lines: &[&str]) -> String {
    let lines: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        let starts_line = out.is_empty() || out.ends_with('\n');
        out.push_str(line);
        if idx + 1 < lines.len() {
            let ends_sentence = line.ends_with('。') || line.ends_with('！') || line.ends_with('？');
            // 表の目印の行は前後とも改行で区切る（表を本文に連結しない）。
            let around_table = parse_table_marker(line).is_some() || parse_table_marker(lines[idx + 1]).is_some();
            // 省略の印（「〔２・３　略〕」）は 1 列を占めるので、後ろの列もつなげない。前の行につないだ印
            // （「修業〔項を加える。〕年限…」。隣の欄の印が文の途中に紛れ込んだもの）は文の続きを切らない。
            let after_note = starts_line && line.starts_with(['〔', '［']) && is_bracket_note(line);
            // 「…法律第」＋「二十六号）…」のように語の途中で切れた行は、次が号の頭に見えても続ける。
            let mid_word = line.ends_with('第');
            // 漢数字の続き（「十六号）…」）は、前の列が下まで埋まっているときだけ折り返しとみなす。
            // 短い列のあとの数は、1 列に 1 つずつ並べた地番（「四四三番二」「四四五番二」）などの区切り。
            let wrapped_number = fills_column(&lines, idx)
                && (continues_number(line, lines[idx + 1]) || starts_reference(lines[idx + 1]) || continues_parenthetical(line, lines[idx + 1]));
            if ends_sentence || around_table || after_note || (starts_structural_marker(lines[idx + 1]) && !mid_word && !wrapped_number) {
                out.push('\n');
            }
        }
    }
    out.trim().to_string()
}

/// 行頭が条文の構造マーカー（条・号・見出し・別表・附則・省略の印）か。列折り返し結合で改行を残す判定に使う。
fn starts_structural_marker(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some('第') | Some('（') | Some('(') | Some('別') | Some('附') => true,
        // 号: 漢数字・カタカナ列記・全角/半角数字で始まる（語の途中で折り返した数は呼び出し側で除く）。
        Some(c) if "一二三四五六七八九十".contains(c) => true,
        Some(c) if "イロハニホヘトチリヌルヲワカヨタレソツネ".contains(c) => true,
        Some(c) if c.is_ascii_digit() || ('０'..='９').contains(&c) => true,
        // 省略・新設などの印（「〔四・五　略〕」「〔新設〕」「［表略］」）は、本文の後ろに続けず行を分ける。
        Some('〔') | Some('［') => is_bracket_note(line),
        _ => false,
    }
}

/// `lines[idx]` が下まで埋まった列か（近くの行の長さとくらべる。半角の字は 0.5 字で数える）。
///
/// 復元したテキストの 1 行は縦書きの 1 列なので、語の途中で折り返す行は列の下端まで字が詰まっている。
fn fills_column(lines: &[&str], idx: usize) -> bool {
    let width = |l: &str| l.chars().map(|c| if (c as u32) < 0x100 || ('｡'..='ﾟ').contains(&c) { 0.5 } else { 1.0 }).sum::<f32>();
    let lo = idx.saturating_sub(4);
    let hi = (idx + 2).min(lines.len());
    let longest = lines[lo..hi].iter().map(|l| width(l)).fold(0.0, f32::max);
    width(lines[idx]) >= (longest * 0.8).max(18.0)
}

/// 行頭の `〔…〕`・`［…］` が、新旧対照表の省略・新設・削除などの印か（「〔２・３　略〕」「〔同上〕」
/// 「〔新設〕」「〔第二項を加える。〕」）。「〔　〕の記載」のような本文中の括弧は印でない。
///
/// 印は 1 列を占めるので、後ろに同じ行で本文が続くもの（「〔項を加える。〕年限二年以上の…」。
/// 隣の欄の印が文の途中に紛れ込んだもの）は印として扱わない。印が続く（「〔略〕〔２・３　略〕」）のはよい。
fn is_bracket_note(line: &str) -> bool {
    let mut rest = line.trim();
    let mut found = false;
    while let Some(close) = rest.strip_prefix('〔').map(|_| '〕').or_else(|| rest.strip_prefix('［').map(|_| '］')) {
        let body = &rest[rest.chars().next().map_or(0, char::len_utf8)..];
        let Some(end) = body.find(close) else {
            return false;
        };
        let inner = body[..end].trim();
        if !["略", "同上", "同左", "新設", "削る", "削除", "加える。", "削る。", "挿入"].iter().any(|s| inner.ends_with(s)) {
            return false;
        }
        found = true;
        rest = body[end + close.len_utf8()..].trim_start_matches([' ', '　']);
    }
    found && rest.is_empty()
}

/// 行頭の漢数字が号の番号でなく、`prev` の行の末尾から折り返した数の続きか。
///
/// 号の番号ではありえない並び（[`NumberCue::Strong`]）なら続き。それ以外は `prev` が数の途中
/// （「第四十」「四万」「「三〇一、」「（平成」）で終わるときだけ続きとする（「…第四十一条の十」＋
/// 「六の二第一項…」）。位取りの数は地番や表の値を 1 列に 1 つずつ並べたもの（「四四三番二」
/// 「四四五番二」）でもあるので、形だけでは続きとしない。
fn continues_number(prev: &str, line: &str) -> bool {
    let mid_number = prev.ends_with(|c: char| "一二三四五六七八九十百千〇万億、・「（第".contains(c))
        || ["明治", "大正", "昭和", "平成", "令和"].iter().any(|era| prev.ends_with(era));
    match kanji_number_cue(line) {
        NumberCue::Heading => false,
        NumberCue::Strong => true,
        NumberCue::Plain | NumberCue::Positional => mid_number,
    }
}

/// 行頭が号・項を指す語（「第七号並びに…」「第二項の規定により…」）か。条の見出し（「第十条　」）と違い、
/// 号・項は「第…号」「第…項」の形で行頭に立たないので、前の行から折り返した語の続き。
fn starts_reference(line: &str) -> bool {
    const NUMERAL: &str = "一二三四五六七八九十百千〇";
    let Some(rest) = line.strip_prefix('第') else {
        return false;
    };
    let mut chars = rest.chars().peekable();
    let mut digits = 0;
    while let Some(&c) = chars.peek() {
        if NUMERAL.contains(c) {
            digits += 1;
            chars.next();
        } else if c == 'の' && digits > 0 {
            chars.next();
        } else {
            break;
        }
    }
    let kind = chars.next();
    digits > 0 && matches!(kind, Some('号' | '項')) && !chars.next().is_some_and(char::is_whitespace)
}

/// 行頭の `（` が見出し（「（経過措置）」）や番号（「（１）」「（イ）」）でなく、`prev` の行から折り返した
/// 括弧書き（「…に関する法律」＋「（昭和五十七年法律第八十号）第七十条…」）か。1 行で閉じない括弧で、
/// `prev` が文や括弧書きの終わり（「。」「）」）で終わっていないもの。
fn continues_parenthetical(prev: &str, line: &str) -> bool {
    let Some(inner) = line.strip_prefix('（') else {
        return false;
    };
    // 番号の括弧（「（１４）　」「（イ）」「（乙）」）。
    let label: String = inner.chars().take_while(|&c| c != '）').collect();
    let numbered = inner.contains('）')
        && label.chars().count() <= 4
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c.is_numeric() || "一二三四五六七八九十ⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹイロハニホヘトチリヌルヲ甲乙丙丁".contains(c) || ('ａ'..='ｚ').contains(&c) || ('Ａ'..='Ｚ').contains(&c));
    // 1 行で閉じる見出し（「（経過措置）」）。
    let caption = line.ends_with('）') && line.matches('（').count() == line.matches('）').count();
    !numbered && !caption && !prev.ends_with(['。', '）'])
}

/// 行頭の漢数字が前の行からの続きである手がかり。
#[derive(Debug, PartialEq, Eq)]
enum NumberCue {
    /// 漢数字で始まらない行。または数の後ろが空白（「四　」「四一八〇　防府北飛行場」）で、号の番号や表の行の頭。
    Heading,
    /// 号の番号かもしれない（手がかり無し。散文の段落では号の番号の後ろの空白が落ちる）。
    Plain,
    /// 位取りで書いた数（「四、五〇〇」「三・〇五MHz」「一三、五〇〇」）。号の番号は「十四」のように書く。
    Positional,
    /// 号の番号ではありえない並び（「十六号）」「十三年法律第…」）。
    Strong,
}

/// 行頭の漢数字が号の番号か、前の行で折り返した語の続きかの手がかり。
///
/// 号の番号（「四　」「三の二　」「四〜九　」）は空白で本文と区切られる。散文の段落では空白が落ちて
/// 「二前号に規定する…」「三日本郵政株式会社」のようにもなるので、空白が無いだけでは続きとみなさず、
/// 号の番号ではありえない並びのときだけ続きとする:
///
/// - 位取りで書いた数（[`NumberCue::Positional`]）
/// - 数の直後が号・条・項・円・閉じ括弧（「十六号）」「五条の二十三」「一項」「七九〇円」）
/// - 法令番号・日付・期間の途中（「十三年法律第…」「八日）から」「六月以内」「十日まで」）
fn kanji_number_cue(line: &str) -> NumberCue {
    const DIGITS: &str = "一二三四五六七八九";
    const NUMERAL: &str = "一二三四五六七八九十百千万億〇";
    let chars: Vec<char> = line.chars().collect();
    // 数（「三の二」「四・五」「四、五〇〇」の区切りも含める）の終わりまで進む。
    let mut i = 0;
    while i < chars.len() {
        let separator = matches!(chars[i], 'の' | '・' | '、' | '〜') && chars.get(i + 1).is_some_and(|c| NUMERAL.contains(*c));
        if !NUMERAL.contains(chars[i]) && !separator {
            break;
        }
        i += 1;
    }
    let number = &chars[..i];
    let rest: String = chars[i..].iter().take(16).collect();
    // 空白で区切った数は号の番号や表の行の頭（「四　」「四一八〇　防府北飛行場」）。
    if i == 0 || rest.starts_with(char::is_whitespace) {
        return NumberCue::Heading;
    }
    let positional = number.contains(&'〇') || number.windows(2).any(|w| DIGITS.contains(w[0]) && DIGITS.contains(w[1]));
    if positional {
        return NumberCue::Positional;
    }
    let Some(next) = rest.chars().next() else {
        return NumberCue::Plain;
    };
    let strong = |b: bool| if b { NumberCue::Strong } else { NumberCue::Plain };
    if matches!(next, '号' | '条' | '項' | '円' | '）' | '」' | '』' | 'の') {
        return strong(!["条例", "条約", "項目", "円滑"].iter().any(|w| rest.starts_with(w)));
    }
    // 「十三年法律第…」「二年厚生労働省令第…」: 年の後ろ、「第」の前が法令の種類で終わる。
    if let Some(law) = rest.strip_prefix('年').and_then(|r| r.split('第').next().filter(|_| r.contains('第'))) {
        if law.chars().count() <= 12 && law.ends_with(['律', '令', '則', '示']) {
            return NumberCue::Strong;
        }
    }
    strong(["日）", "日から", "日まで", "日以前", "日以後", "日分", "月）", "月以内", "月から", "月まで", "年以内", "年度）"]
        .iter()
        .any(|w| rest.starts_with(w)))
}

impl Document {
    /// `reconstruct_vertical` 由来のマーカー付きテキストを構造化する。
    ///
    /// 「改正後」「改正前」の独立見出し行を境に新旧対照表ブロックを作り、
    /// それ以外の連続行は段落ブロックにする。形式は [`detect_format`] で判定する。
    pub fn from_text(text: &str) -> Document {
        let format = detect_format(text);
        let lines: Vec<&str> = text.lines().collect();
        let mut blocks: Vec<Block> = Vec::new();
        let mut para: Vec<&str> = Vec::new();
        let flush_para = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
            let runs = cell_runs(para);
            if !runs.is_empty() {
                blocks.push(Block::Paragraph { runs });
            }
            para.clear();
        };
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "改正後" {
                flush_para(&mut para, &mut blocks);
                // 改正後セル: 次の「改正前」まで。
                let after_start = i + 1;
                let mut j = after_start;
                while j < lines.len() && lines[j].trim() != "改正前" {
                    j += 1;
                }
                let after_lines = &lines[after_start..j];
                // 改正前セル: 次の「改正後」（次段の開始）か、表の終わりの目印まで。
                let before_start = (j + 1).min(lines.len());
                let mut k = before_start;
                while k < lines.len() && lines[k].trim() != "改正後" && lines[k].trim() != SHINKYU_END {
                    k += 1;
                }
                let before_lines = &lines[before_start..k];
                // 片側の帯にだけある附則（施行期日・経過措置）は改正全体の付則が帯に紛れ込んだもので、
                // 新旧対照表の条ではない。表の行から外し、表ブロックの後ろに段落として置く。
                // 両側にある附則（「附　則〔１〜24略〕」と「附　則〔１〜24同上〕」）は、改正される
                // 法令の附則の行なので表に残す。
                let mut rows = align_rows(after_lines, before_lines);
                // 附則の後に続く同じ側だけの行（「（施行期日）」で分かれた行）も附則の続き。
                let mut furisoku: Vec<Vec<Run>> = Vec::new();
                let mut furi_side: Option<bool> = None;
                rows.retain(|r| {
                    let is_furi = |runs: &[Run]| {
                        runs.first().map(|x| starts_with_spaced(&x.text, "附則")).unwrap_or(false)
                    };
                    let one_sided = r.after.is_empty() || r.before.is_empty();
                    let side = r.before.is_empty();
                    let continues = one_sided && furi_side == Some(side);
                    if continues || (one_sided && (is_furi(&r.after) || is_furi(&r.before))) {
                        let content = if !r.after.is_empty() { r.after.clone() } else { r.before.clone() };
                        if !content.is_empty() {
                            furisoku.push(content);
                        }
                        furi_side = Some(side);
                        false
                    } else {
                        furi_side = None;
                        true
                    }
                });
                // 改正後・改正前とも空の行（空白だけの帯）は作らない。
                rows.retain(|r| !r.after.is_empty() || !r.before.is_empty());
                if !rows.is_empty() {
                    blocks.push(Block::Shinkyu { rows });
                }
                for runs in furisoku {
                    blocks.push(Block::Paragraph { runs });
                }
                i = if lines.get(k).is_some_and(|l| l.trim() == SHINKYU_END) { k + 1 } else { k };
            } else if lines[i].trim() == SHINKYU_END {
                // 表の終わりの目印は本文に入れない（表の後ろに本文があればそれが段落になり、
                // 続く表とはつながらない。ページの切れ目のように間に本文が無ければつながる）。
                i += 1;
            } else {
                para.push(lines[i]);
                i += 1;
            }
        }
        flush_para(&mut para, &mut blocks);
        Document {
            format,
            blocks: merge_continued_tables(blocks),
        }
    }

    /// [`Document::from_text`] と同じだが、テキスト中の表の目印（[`crate::trace::table_marker`]）を
    /// `tables` の表に置き換え、本文の流れの中の表（`Run::table`）にする。
    pub(crate) fn from_text_with_tables(text: &str, tables: &[Table]) -> Document {
        let mut doc = Document::from_text(text);
        let splice = |runs: &mut Vec<Run>| *runs = join_adjacent_tables(splice_tables(std::mem::take(runs), tables));
        for block in &mut doc.blocks {
            match block {
                Block::Paragraph { runs } => splice(runs),
                Block::Shinkyu { rows } => {
                    for r in rows {
                        splice(&mut r.after);
                        splice(&mut r.before);
                    }
                }
            }
        }
        doc
    }

    /// 構造化表現を `reconstruct_vertical` 互換のフラットテキストへ戻す。
    /// 記事分割・標題突合など既存のテキスト前提処理との後方互換に使う。
    pub fn to_text(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        let runs_text = |runs: &[Run]| -> String { runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("") };
        for block in &self.blocks {
            match block {
                Block::Paragraph { runs } => {
                    let t = runs_text(runs);
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
                Block::Shinkyu { rows } => {
                    let after: String = rows
                        .iter()
                        .map(|r| runs_text(&r.after))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let before: String = rows
                        .iter()
                        .map(|r| runs_text(&r.before))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push("改正後".to_string());
                    out.push(after);
                    out.push("改正前".to_string());
                    out.push(before);
                }
            }
        }
        out.join("\n").trim().to_string()
    }
}

/// 続けて現れる新旧対照表のブロック（ページ・段の切れ目で分かれたもの）を 1 つにつなぐ。
///
/// 後ろのブロックの先頭の行が条の見出し（`（見出し）`・`別表`・`附則`）で始まらなければ、
/// 前のブロックの最後の行（ページの切れ目で途中まで載った条）の続きとしてつなぐ。
fn merge_continued_tables(blocks: Vec<Block>) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    for block in blocks {
        let (Some(Block::Shinkyu { rows: prev }), Block::Shinkyu { rows }) = (out.last_mut(), &block) else {
            out.push(block);
            continue;
        };
        let mut rows = rows.clone();
        let continues = |runs: &[Run]| {
            runs.first()
                .and_then(|r| r.text.lines().next())
                .is_some_and(|l| is_row_heading(l).is_none())
        };
        if let (Some(last), Some(first)) = (prev.last_mut(), rows.first()) {
            let cont = (first.after.is_empty() || continues(&first.after)) && (first.before.is_empty() || continues(&first.before));
            if cont && (!first.after.is_empty() || !first.before.is_empty()) {
                let first = rows.remove(0);
                append_runs(&mut last.after, first.after);
                append_runs(&mut last.before, first.before);
            }
        }
        prev.extend(rows);
    }
    out
}

/// セルの Run 列に続きをつなぐ。前の文が文末（。）で終わっていなければ改行せずに連結する
/// （ページの切れ目は列の折り返しと同じ）。ページの切れ目で分かれた表（列数が同じ表が続く）は
/// 1 つの表につなぐ。
fn append_runs(dst: &mut Vec<Run>, src: Vec<Run>) {
    let mut src = src.into_iter();
    if let (Some(last), Some(first)) = (dst.last_mut(), src.as_slice().first()) {
        if let (Some(a), Some(b)) = (last.table.as_mut(), first.table.as_ref()) {
            if join_tables(a, b) {
                src.next();
            }
        } else if last.table.is_none() && first.table.is_none() {
            let next_line = first.text.lines().next().unwrap_or("");
            let table_edge = last.text.ends_with('\u{E001}') || next_line.starts_with('\u{E000}');
            // 「…第」＋「一号イ…」のように語の途中で切れているときは、次が項目の頭に見えても改行しない。
            let mid_word = last.text.ends_with('第');
            // ページの切れ目では前の列の長さが分からないので、数の形だけで続きか決める。
            let wrapped_number = continues_number(&last.text, next_line) || starts_reference(next_line);
            let sep = if last.text.ends_with('。') || table_edge || (starts_structural_marker(next_line) && !mid_word && !wrapped_number) { "\n" } else { "" };
            last.text = format!("{}{sep}{}", last.text, first.text);
            src.next();
        }
    }
    dst.extend(src);
}

/// 表 `b` が表 `a` の続き（列数が同じ）なら `a` の下につなぐ。`b` の先頭行が `a` の先頭行
/// （見出し行）と同じなら、ページごとに繰り返された見出しとして落とす。つないだら true。
fn join_tables(a: &mut NestedTable, b: &NestedTable) -> bool {
    let cols = |t: &NestedTable| t.rows.first().map(|r| r.len()).unwrap_or(0);
    if cols(a) == 0 || cols(a) != cols(b) {
        return false;
    }
    let text = |row: &Vec<Vec<Run>>| row.iter().map(|c| c.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>();
    let mut skip = usize::from(b.rows.first().map(text) == a.rows.first().map(text));
    // ページの切れ目で行が途中で切れている（前の行が「、」などで終わり、次の行が項目の頭で
    // 始まらない）なら、次の先頭行を前の最後の行の続きとしてセルごとに連結する。
    let lead = |row: &Vec<Vec<Run>>| row.iter().map(|c| c.iter().map(|r| r.text.as_str()).collect::<String>()).find(|t| !t.is_empty()).unwrap_or_default();
    let a_rows = a.rows.len();
    let merged_cells = b.merges.iter().any(|m| m.row == skip) || a.merges.iter().any(|m| m.row + m.rowspan == a_rows);
    if let (Some(last), Some(next)) = (a.rows.last_mut(), b.rows.get(skip)) {
        let tail = lead(last);
        let head = lead(next);
        let cut = tail.ends_with(['、', '，', ',']) || !tail.ends_with(['。', '）', '」', '〕', '］']);
        if !tail.is_empty() && !head.is_empty() && cut && !crate::vertical::starts_item(&head) && !merged_cells {
            for (dst, src) in last.iter_mut().zip(next.iter()) {
                let add: String = src.iter().map(|r| r.text.as_str()).collect();
                match dst.last_mut() {
                    Some(r) if r.table.is_none() => r.text.push_str(&add),
                    _ if !add.is_empty() => dst.push(Run::plain(add)),
                    _ => {}
                }
            }
            skip += 1;
        }
    }
    let offset = a.rows.len();
    a.rows.extend(b.rows.iter().skip(skip).cloned());
    a.merges.extend(b.merges.iter().filter(|m| m.row >= skip).map(|m| CellMerge {
        row: m.row - skip + offset,
        ..*m
    }));
    true
}

/// 続けて並ぶ表の Run（本文を挟まない）で列数が同じものを 1 つの表につなぐ。
fn join_adjacent_tables(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for run in runs {
        if let (Some(Run { table: Some(a), .. }), Some(b)) = (out.last_mut(), run.table.as_ref()) {
            if join_tables(a, b) {
                continue;
            }
        }
        out.push(run);
    }
    out
}

/// Run の中の表の目印の行を表の Run に置き換える（前後の本文は別の Run に分ける）。
fn splice_tables(runs: Vec<Run>, tables: &[Table]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for run in runs {
        if run.table.is_some() || !run.text.contains('\u{E000}') {
            out.push(run);
            continue;
        }
        let mut buf: Vec<&str> = Vec::new();
        let flush = |buf: &mut Vec<&str>, out: &mut Vec<Run>| {
            let t = buf.concat().trim().to_string();
            if !t.is_empty() {
                out.push(Run::plain(t));
            }
            buf.clear();
        };
        // 目印は通常は独立した行だが、行の途中にあっても表に置き換える。
        let mut rest = run.text.as_str();
        while let Some(start) = rest.find('\u{E000}') {
            let Some(len) = rest[start..].find('\u{E001}') else { break };
            let end = start + len + '\u{E001}'.len_utf8();
            buf.push(&rest[..start]);
            if let Some(t) = parse_table_marker(&rest[start..end]).and_then(|i| tables.get(i)) {
                flush(&mut buf, &mut out);
                out.push(Run {
                    text: String::new(),
                    underline: false,
                    table: Some(t.to_nested()),
                });
            }
            rest = &rest[end..];
        }
        buf.push(rest);
        flush(&mut buf, &mut out);
    }
    out
}

/// 新旧対照表 1 段を条（別表）ごとの行に分け、改正後/改正前を対応させる。
///
/// 各帯を行見出し（`（…）`の条見出し・`別表`・`附則`）で区切り、見出しテキストの
/// 最長共通部分列(LCS)で対応付ける。共通見出しが揃った条は同じ行に、片側だけの条
/// （新設・削除）はもう一方が空セルの行になる。見出しが揃わない/少ない場合は単一行に
/// フォールバックする（＝従来挙動、退行なし）。
fn align_rows(after_lines: &[&str], before_lines: &[&str]) -> Vec<ShinkyuRow> {
    align_rows_by(after_lines, before_lines, is_row_heading, 2)
        .or_else(|| align_rows_by(after_lines, before_lines, go_heading_key, 2))
        .or_else(|| align_rows_by(after_lines, before_lines, unit_heading, 1))
        .unwrap_or_else(|| vec![single_row(after_lines, before_lines)])
}

/// 行見出しの判定 `heading`（見出しなら対応付けの鍵を返す）で両帯を区切り、LCS で対応付ける。
/// 見出しが両側に 2 つ以上無い、または共通見出しが無ければ `None`。
fn align_rows_by(
    after_lines: &[&str],
    before_lines: &[&str],
    heading: fn(&str) -> Option<String>,
    min_heads: usize,
) -> Option<Vec<ShinkyuRow>> {
    let a = segment_band(after_lines, heading);
    let b = segment_band(before_lines, heading);
    let a_head = a.iter().filter(|s| s.heading.is_some()).count();
    let b_head = b.iter().filter(|s| s.heading.is_some()).count();
    // 見出しが両側に `min_heads` 個以上無いか、分ける所が無ければ行分割しない（条の見出しは
    // 1 つでよい: ページの続きで前の条の続きの後に見出しが 1 つだけ来るときは、続きと見出しの
    // 条の 2 行に分ける。号の番号での分割は誤りやすいので 2 つ以上）。
    if a_head < min_heads || b_head < min_heads || (a.len() < 2 && b.len() < 2) {
        return None;
    }
    let keys_a: Vec<&str> = a.iter().map(|s| s.heading.as_deref().unwrap_or("")).collect();
    let keys_b: Vec<&str> = b.iter().map(|s| s.heading.as_deref().unwrap_or("")).collect();
    let aln = lcs_align(&keys_a, &keys_b);
    // 共通見出しが 1 つも無ければ対応付け不能としてフォールバック。
    if !aln.iter().any(|(i, j)| i.is_some() && j.is_some()) {
        return None;
    }
    let raw: Vec<(String, String)> = aln
        .into_iter()
        .map(|(ai, bi)| (ai.map(|i| a[i].text.clone()).unwrap_or_default(), bi.map(|j| b[j].text.clone()).unwrap_or_default()))
        .filter(|(after, before)| !(after.trim().is_empty() && before.trim().is_empty()))
        .collect();
    let mut rows: Vec<(String, String)> = Vec::new();
    for (after, before) in pair_renumbered(realign_weak_windows(raw)) {
        // 片側だけの行が、反対側だけの行のすぐ後に続く（見出しの文言が改正で変わった条）なら
        // 1 行にまとめる。新設の条は改正前の欄にも「（新設）」があるので片側が空にはならない。
        if let Some(prev) = rows.last_mut() {
            let empty = |s: &str| s.trim().is_empty();
            let complement = (empty(&prev.0) && !empty(&prev.1) && !empty(&after) && empty(&before))
                || (!empty(&prev.0) && empty(&prev.1) && empty(&after) && !empty(&before));
            if complement {
                if empty(&prev.0) {
                    prev.0 = after;
                } else {
                    prev.1 = before;
                }
                continue;
            }
        }
        rows.push((after, before));
    }
    // 「附則」などの見出しの後の「（新設）」「（削る）」（いくつか並ぶこともある）は、片側が空の
    // 次の行（新設・削った条）のもの。印 1 つを片側が空の行 1 つへ順に移す。
    for i in 0..rows.len() {
        for side in [0, 1] {
            let cell = |r: &(String, String)| if side == 0 { r.0.clone() } else { r.1.clone() };
            let empty = rows[i + 1..].iter().take_while(|r| cell(r).trim().is_empty()).count();
            if empty == 0 {
                continue;
            }
            let text = cell(&rows[i]);
            let (keep, marks) = split_trailing_marks(&text);
            if marks.is_empty() {
                continue;
            }
            // 印が空の行より多ければ、余りの（先の）印は元の行に残す。
            let moved = marks.len().min(empty);
            let mut kept: Vec<String> = keep.lines().map(str::to_string).collect();
            kept.extend(marks[..marks.len() - moved].iter().cloned());
            let set = |r: &mut (String, String), v: String| if side == 0 { r.0 = v } else { r.1 = v };
            set(&mut rows[i], kept.join("\n"));
            for (k, m) in marks[marks.len() - moved..].iter().enumerate() {
                set(&mut rows[i + 1 + k], m.clone());
            }
        }
    }
    shift_omitted_past_new(&mut rows);
    spread_marks_back(&mut rows);
    move_leading_marks_back(&mut rows);
    let rows = absorb_joined_articles(rows);
    let rows = merge_new_item_runs(rows);
    Some(rows.into_iter().map(|(a, b)| ShinkyuRow { after: cell_runs_from_str(&a), before: cell_runs_from_str(&b), ..Default::default() }).collect())
}

/// 印だけのセルに印がいくつも入り（「（新設）」が 4 つ）、その直前の行の同じ側が空なら、印は直前の新設・
/// 削った条のもの（下の帯の印を読む順で、続く条の印がまとめて最後の条に付いていた）。1 つを残して
/// 前の空の行へ後ろから順に移す。
fn spread_marks_back(rows: &mut [(String, String)]) {
    for i in 0..rows.len() {
        for side in [0, 1] {
            let cell = |r: &(String, String)| if side == 0 { r.0.clone() } else { r.1.clone() };
            let marks = only_marks(&cell(&rows[i]));
            if marks.len() < 2 {
                continue;
            }
            let empty = rows[..i].iter().rev().take_while(|r| cell(r).trim().is_empty()).count();
            let moved = (marks.len() - 1).min(empty);
            if moved == 0 {
                continue;
            }
            let set = |r: &mut (String, String), v: String| if side == 0 { r.0 = v } else { r.1 = v };
            for k in 0..moved {
                set(&mut rows[i - moved + k], marks[k].clone());
            }
            set(&mut rows[i], marks[moved..].join("\n"));
        }
    }
}

/// 改正前の欄で「（新設）」の印が k 個続いた後の「三十七　（略）」が、番号の見出しで改正後の新設の
/// 「三十七　ペルフルオロ…」と組になっていれば、改正前の「三十七」は号の繰り下げで k 行後の改正後の
/// 「四十一　（略）」（改正前の欄が空）のもの。改正前の行を k 行後へ移し、印を新設の k 行へ配る。
fn shift_omitted_past_new(rows: &mut [(String, String)]) {
    let omitted = |s: &str| {
        let first = s.trim().lines().next().unwrap_or("");
        first.split_once('　').is_some_and(|(_, rest)| matches!(rest.trim(), "（略）" | "略" | "［略］"))
    };
    for i in 1..rows.len() {
        if !omitted(&rows[i].1) || omitted(&rows[i].0) {
            continue;
        }
        let (keep, marks) = split_trailing_marks(&rows[i - 1].1);
        let k = marks.len();
        if k == 0 || i + k >= rows.len() || !rows[i + 1..=i + k].iter().all(|r| r.1.trim().is_empty()) || !omitted(&rows[i + k].0) {
            continue;
        }
        rows[i + k].1 = std::mem::take(&mut rows[i].1);
        rows[i - 1].1 = keep;
        for (r, m) in rows[i..i + k].iter_mut().zip(marks) {
            r.1 = m;
        }
    }
}

/// セルの頭の印（「（新設）」「（削る。）」）の後に文が続き、前の行の同じ側が空なら、印は前の行（新設・削った
/// 条）のもの（下の帯の印が次の条の文の頭に付いていた）。
fn move_leading_marks_back(rows: &mut [(String, String)]) {
    for i in 1..rows.len() {
        for side in [0, 1] {
            let cell = |r: &(String, String)| if side == 0 { r.0.clone() } else { r.1.clone() };
            if !cell(&rows[i - 1]).trim().is_empty() {
                continue;
            }
            let text = cell(&rows[i]);
            let lines: Vec<&str> = text.lines().collect();
            let k = lines.iter().take_while(|l| !l.trim().is_empty() && !only_marks(l).is_empty()).count();
            if k == 0 || k == lines.len() {
                continue;
            }
            let (marks, rest) = (lines[..k].join("\n"), lines[k..].join("\n"));
            let set = |r: &mut (String, String), v: String| if side == 0 { r.0 = v } else { r.1 = v };
            set(&mut rows[i - 1], marks);
            set(&mut rows[i], rest);
        }
    }
}

/// まとめて削った条（改正後「第十二条及び第十三条　削除」）の行の次に、反対の側だけに 2 つ目の条
/// （改正前「第十三条　削除」）の行があれば、同じ行にまとめる。
fn absorb_joined_articles(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    // まとめて削った条の最後の条と、範囲（「から…まで」）か。
    let last_of = |text: &str| -> Option<(String, bool)> {
        let first = text.trim().lines().next()?;
        article_number(first)?;
        let (rest, range) = match first.split_once("及び") {
            Some((_, r)) => (r, false),
            None => (first.split_once("から")?.1, true),
        };
        let (head, _) = rest.split_once('\u{3000}')?;
        article_number(&format!("{}\u{3000}削除", head.trim_end_matches("まで"))).map(|k| (k, range))
    };
    // 行の条（見出し「（…）」の後の条の書き出し）。
    let first_key = |t: &str| t.trim().lines().take(3).find_map(article_number);
    let mut out: Vec<(String, String)> = Vec::new();
    // まとめている行の側（0: 改正後に「及び」の書き出し）と、最後の条・範囲か。
    let mut open: Option<(usize, String, bool)> = None;
    for (after, before) in rows {
        if let (Some((side, last, range)), Some(prev)) = (open.clone(), out.last_mut()) {
            let (other_empty, text) = if side == 0 { (after.trim().is_empty(), &before) } else { (before.trim().is_empty(), &after) };
            let key = first_key(text);
            if other_empty && (key.as_deref() == Some(last.as_str()) || (range && key.is_some())) {
                let cell = if side == 0 { &mut prev.1 } else { &mut prev.0 };
                *cell = format!("{}\n{}", cell.trim_end(), text);
                open = (key.as_deref() != Some(last.as_str())).then_some((side, last, range));
                continue;
            }
        }
        open = last_of(&after).map(|(k, r)| (0, k, r)).or_else(|| last_of(&before).map(|(k, r)| (1, k, r)));
        out.push((after, before));
    }
    out
}

/// 号の番号（「一」〜「二十」）。
fn item_number(text: &str) -> Option<u32> {
    let head: String = text.trim_start().chars().take_while(|c| "一二三四五六七八九十".contains(*c)).collect();
    let rest = text.trim_start()[head.len()..].chars().next();
    if head.is_empty() || !matches!(rest, Some('　') | Some(' ')) {
        return None;
    }
    let digit = |c: char| "一二三四五六七八九".find(c).map(|i| i as u32 / 3 + 1);
    let cs: Vec<char> = head.chars().collect();
    match cs.as_slice() {
        [a] if *a == '十' => Some(10),
        [a] => digit(*a),
        ['十', b] => digit(*b).map(|d| 10 + d),
        [a, '十'] => digit(*a).map(|d| d * 10),
        [a, '十', b] => Some(digit(*a)? * 10 + digit(*b)?),
        _ => None,
    }
}

/// 改正前の欄が印 1 つ（「（新設）」）の行の後に、改正前の欄が空で改正後の欄が、印の行の最後の号に続く番号
/// （印の行が「一　…」なら「二　…」、新設の条の書き出しで終わるなら「一　…」）の行が続けば、新設の項・条の号
/// なので 1 つの行にまとめる（新設の項の号ごとに行が分かれ、改正前の欄の空いた行になっていた）。
fn merge_new_item_runs(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // 今まとめている行の、最後の号の番号。
    let mut open: Option<u32> = None;
    // 直前の行の改正前の欄が印だけ（新設の条・項）か。
    let mut in_new = false;
    // 号・項の書き出し（「三　」「２　」）。条の見出し・条の書き出しではない。
    let sub_item = |t: &str| {
        let t = t.trim_start();
        item_number(t).is_some() || t.chars().next().is_some_and(|c| ('２'..='９').contains(&c)) && t.chars().nth(1) == Some('　')
    };
    for (after, before) in rows {
        if let (Some(n), true) = (open, before.trim().is_empty()) {
            if item_number(&after) == Some(n + 1) {
                let last = out.last_mut().unwrap();
                last.0 = format!("{}\n{}", last.0.trim_end(), after);
                open = Some(n + 1);
                continue;
            }
        }
        // 新設の条・項の行の後に、改正前の欄が空で号・項から書き出す行が続けば、印が足りずに分かれた新設の
        // 条・項の続き。
        if in_new && before.trim().is_empty() && sub_item(&after) {
            let last = out.last_mut().unwrap();
            last.0 = format!("{}\n{}", last.0.trim_end(), after);
            open = None;
            continue;
        }
        in_new = !only_marks(&before).is_empty();
        // 印の行の改正後の欄の最後の号の番号（その後に新設の条の書き出しがあれば 0。号はその条の号から始まる）。
        let last_item = after.lines().fold(None, |n, line| {
            // 横書きの「第145条の７」も条の書き出し。
            let arabic = line.trim_start().strip_prefix('第').is_some_and(|r| {
                let digits = r.chars().take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c)).count();
                digits > 0 && r.chars().nth(digits) == Some('条')
            });
            if article_number(line).is_some() || arabic {
                Some(0)
            } else {
                item_number(line).or(n)
            }
        });
        open = if only_marks(&before).len() == 1 { last_item } else { None };
        out.push((after, before));
    }
    out
}

/// セルが印（「（新設）」「（削る）」など）だけなら、その印の列（1 行に並ぶものも分ける）。印以外があれば空。
fn only_marks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim();
        while let Some(m) = leading_mark(rest) {
            out.push(m.to_string());
            rest = rest[m.len()..].trim_start();
        }
        if !rest.is_empty() {
            return Vec::new();
        }
    }
    out
}

/// 片側だけの行が続くところで、号・条の番号を除いた書き出しが同じ改正後の行と改正前の行を、順序を
/// 保って 1 行にまとめる。号の繰り下げ（改正後「十八　特定非上場会社…」と改正前「二十二　特定非上場会社…」）
/// では番号の見出しが揃わず、どちらも片側だけの行になっていた。
fn pair_renumbered(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    let empty = |s: &str| s.trim().is_empty();
    let one_side = |r: &(String, String)| empty(&r.0) != empty(&r.1);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if !one_side(&rows[i]) {
            out.push(rows[i].clone());
            i += 1;
            continue;
        }
        let end = (i..rows.len()).find(|&k| !one_side(&rows[k])).unwrap_or(rows.len());
        let seg = &rows[i..end];
        let a: Vec<&str> = seg.iter().filter(|r| !empty(&r.0)).map(|r| r.0.as_str()).collect();
        let b: Vec<&str> = seg.iter().filter(|r| !empty(&r.1)).map(|r| r.1.as_str()).collect();
        // 新設・削った条の印（「（新設）」「〔条を加える。〕」）だけの行がある所では、条番号が同じでも別の条のことが
        // ある（新設の条が前に入り、元の条が繰り下がった）ので、条番号では組まない。
        let marked = seg.iter().any(|r| !only_marks(&r.0).is_empty() || !only_marks(&r.1).is_empty());
        let aln = lcs_align_by(a.len(), b.len(), |x, y| same_opening(a[x], b[y]) || (!marked && same_article_alike(a[x], b[y])));
        if a.is_empty() || b.is_empty() || !aln.iter().any(|(x, y)| x.is_some() && y.is_some()) {
            out.extend(seg.iter().cloned());
        } else {
            for (x, y) in aln {
                out.push((x.map(|x| a[x].to_string()).unwrap_or_default(), y.map(|y| b[y].to_string()).unwrap_or_default()));
            }
        }
        i = end;
    }
    out
}

/// 確かに組になった行（両側に文があり、2 字の組の Jaccard 係数 0.4 以上）の間で、片側だけの行のある所を、
/// 文の似方で組み直す。号の繰り下げ（改正後「十八　特定非上場会社…」、改正前「二十二　特定非上場会社…」）では
/// 番号で組むと別の号どうしが組になり、片側だけの行が離れた所にいくつも残っていた。組み直して片側だけの行が
/// 減るときだけ使う。
fn realign_weak_windows(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    let empty = |s: &str| s.trim().is_empty();
    let sim = |a: &str, b: &str| crate::shinkyu::bigram_jaccard(a, b);
    let confident = |r: &(String, String)| !empty(&r.0) && !empty(&r.1) && sim(&r.0, &r.1) >= 0.4;
    // 番号を除いた書き出しが印だけ（「〔略〕」「〔同上〕」）の号。
    let bare_mark = |s: &str| {
        let first = s.trim().lines().next().unwrap_or("");
        first.split_once('　').is_some_and(|(_, rest)| {
            let r = rest.trim();
            r.starts_with(['〔', '（', '［']) && r.chars().count() <= 6 && s.trim().lines().count() == 1
        })
    };
    let eq = |a: &str, b: &str| same_opening(a, b) || sim(a, b) >= 0.5 || (bare_mark(a) && bare_mark(b));
    let one_sided = |rs: &[(String, String)]| rs.iter().filter(|r| empty(&r.0) != empty(&r.1)).count();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if confident(&rows[i]) {
            out.push(rows[i].clone());
            i += 1;
            continue;
        }
        let end = (i..rows.len()).find(|&k| confident(&rows[k])).unwrap_or(rows.len());
        let window = &rows[i..end];
        let a: Vec<&str> = window.iter().filter(|r| !empty(&r.0)).map(|r| r.0.as_str()).collect();
        let b: Vec<&str> = window.iter().filter(|r| !empty(&r.1)).map(|r| r.1.as_str()).collect();
        let aln = lcs_align_by(a.len(), b.len(), |x, y| eq(a[x], b[y]));
        let realigned: Vec<(String, String)> = aln
            .into_iter()
            .map(|(x, y)| (x.map(|x| a[x].to_string()).unwrap_or_default(), y.map(|y| b[y].to_string()).unwrap_or_default()))
            .collect();
        if one_sided(window) > 0 && one_sided(&realigned) < one_sided(window) {
            out.extend(realigned);
        } else {
            out.extend(window.iter().cloned());
        }
        i = end;
    }
    out
}

/// 条番号が同じで、文が似ている（2 字の組の Jaccard 係数 0.3 以上）か。条の見出しが改正で変わった条
/// （「（審判手続調書の形式的記載事項）第二十五条」→「（審判手続に係る電子調書の形式的記録事項等）第二十五条」）は
/// 見出しの鍵がそろわず、どちらも片側だけの行になっていた。
fn same_article_alike(a: &str, b: &str) -> bool {
    let number = |s: &str| s.trim().lines().take(3).find_map(article_number);
    matches!((number(a), number(b)), (Some(x), Some(y)) if x == y) && crate::shinkyu::bigram_jaccard(a, b) >= 0.3
}

/// 号・条の番号（「十八　」「第二十条　」）を除いた書き出し 10 字。
fn opening_of(s: &str) -> Vec<char> {
    let first = s.trim().lines().next().unwrap_or("");
    let rest = first.trim_start_matches(|c: char| "第一二三四五六七八九十百千条の0123456789０１２３４５６７８９".contains(c));
    rest.trim_start().chars().filter(|c| !c.is_whitespace()).take(10).collect()
}

/// 号・条の番号（「十八　」「第二十条　」）を除いた書き出しが 10 字以上同じか（「〔同上〕」などの印だけの行は除く）。
fn same_opening(a: &str, b: &str) -> bool {
    let (x, y) = (opening_of(a), opening_of(b));
    x.len() >= 10 && x == y && !x.starts_with(&['〔', '（', '［'])
}

/// [`lcs_align`] の、要素の一致を `eq(i, j)` で決める版。
fn lcs_align_by(n: usize, m: usize, eq: impl Fn(usize, usize) -> bool) -> Vec<(Option<usize>, Option<usize>)> {
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if eq(i, j) { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut res = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if eq(i, j) && dp[i][j] == dp[i + 1][j + 1] + 1 {
            res.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            res.push((Some(i), None));
            i += 1;
        } else {
            res.push((None, Some(j)));
            j += 1;
        }
    }
    res.extend((i..n).map(|i| (Some(i), None)));
    res.extend((j..m).map(|j| (None, Some(j))));
    res
}

/// 文字列の頭の、新設・削った条などの印（「（新設）」「〔削る〕」「〔号を加える。〕」「〔項を削る。〕」）。
fn leading_mark(text: &str) -> Option<&str> {
    const MARKS: [&str; 7] = ["（新設）", "〔新設〕", "［新設］", "（削る）", "〔削る〕", "［削る］", "（削る。）"];
    if let Some(m) = MARKS.iter().find(|m| text.starts_with(**m)) {
        return Some(&text[..m.len()]);
    }
    // 「〔号を加える。〕」「〔項を削る。〕」「〔条を加える。〕」など（〔 の後 8 字以内で「を加える。」「を削る。」）。
    let body = text.strip_prefix('〔')?;
    let end = body.find('〕')?;
    let inner = &body[..end];
    let ok = (inner.ends_with("を加える。") || inner.ends_with("を削る。")) && inner.chars().count() <= 12;
    ok.then(|| &text[..'〔'.len_utf8() + end + '〕'.len_utf8()])
}

/// セルの末尾の、印だけの行（「（新設）」「〔新設〕〔新設〕」など）の印を分ける。`(残り, 印)`。
fn split_trailing_marks(text: &str) -> (String, Vec<String>) {
    let mut lines: Vec<&str> = text.trim_end().lines().collect();
    let mut marks: Vec<String> = Vec::new();
    while let Some(last) = lines.last() {
        let mut rest = last.trim();
        let mut found: Vec<String> = Vec::new();
        while let Some(m) = leading_mark(rest) {
            found.push(m.to_string());
            rest = rest[m.len()..].trim_start();
        }
        if found.is_empty() || !rest.is_empty() || lines.len() == 1 {
            break;
        }
        lines.pop();
        found.extend(marks);
        marks = found;
    }
    (lines.join("\n"), marks)
}

/// 文字列（複数行）を列折り返し結合してセルの Run 列にする。
fn cell_runs_from_str(text: &str) -> Vec<Run> {
    let lines: Vec<&str> = text.lines().collect();
    cell_runs(&lines)
}

fn single_row(after_lines: &[&str], before_lines: &[&str]) -> ShinkyuRow {
    ShinkyuRow {
        after: cell_runs(after_lines),
        before: cell_runs(before_lines),
        ..Default::default()
    }
}

/// 帯の 1 セグメント（行見出し＋本文、または先頭の見出し無し前置き）。
struct Seg {
    heading: Option<String>,
    text: String,
}

/// 帯テキストを行見出しで区切る。各セグメントは見出し行から次の見出し行の手前まで。
/// 先頭の見出し無し部分は `heading=None` の前置きセグメントになる。
fn segment_band(lines: &[&str], is_heading: fn(&str) -> Option<String>) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut heading: Option<String> = None;
    let mut cur: Vec<&str> = Vec::new();
    for &line in lines {
        // 「（見出し）」（折り返して複数行のこともある）のすぐ後の「第○条　…」は見出しと同じ行。
        let caption: String = cur.iter().map(|l| l.trim()).collect();
        let under_caption = article_number(line).is_some() && is_one_paren(&caption);
        // 帯の先頭の「（見出し）」の後の条は、見出しと同じ行のまま条の鍵を付ける。
        if under_caption && heading.is_none() && segs.is_empty() {
            heading = is_heading(line);
        }
        if let Some(key) = is_heading(line).filter(|_| !under_caption) {
            // 条の書き出しの直前の「（見出し）」（折り返して複数行のこともある）は新しい行へ移す。
            let mut carried: Vec<&str> = Vec::new();
            if article_number(line).is_some() && cur.last().is_some_and(|l| l.trim().ends_with('）')) {
                let mut k = cur.len();
                while k > 0 {
                    k -= 1;
                    let tail: String = cur[k..].iter().map(|l| l.trim()).collect();
                    if tail.starts_with('（') {
                        if k > 0 && tail.chars().filter(|c| *c == '（').count() == 1 {
                            carried = cur.split_off(k);
                        }
                        break;
                    }
                }
            }
            if !cur.is_empty() {
                let text = cur.join("\n").trim().to_string();
                if !text.is_empty() {
                    segs.push(Seg { heading: heading.take(), text });
                }
                cur.clear();
            }
            heading = Some(key);
            cur.extend(carried);
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        let text = cur.join("\n").trim().to_string();
        if !text.is_empty() {
            segs.push(Seg { heading, text });
        }
    }
    segs
}

/// 全体が 1 つの「（…）」か（中の括弧は入れ子で数える）。「（見出し）…本文…（見出し）」は違う。
fn is_one_paren(s: &str) -> bool {
    let mut depth = 0usize;
    let n = s.chars().count();
    for (i, c) in s.chars().enumerate() {
        match c {
            '（' => depth += 1,
            '）' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i + 1 < n {
                    return false;
                }
            }
            _ if depth == 0 => return false,
            _ => {}
        }
    }
    n > 0 && depth == 0
}

/// 行見出し（新旧対照表の 1 行の先頭）なら、その行（対応付けの鍵）を返す。
/// - `別表` / `附則` 始まり
/// - 単一の `（…）` 見出し行（中身 3 文字以上、途中に `）` を含まない）。`（略）`/`（新設）`は除外。
fn is_row_heading(line: &str) -> Option<String> {
    let t = line.trim();
    let key = Some(t.to_string());
    if starts_with_spaced(t, "別表") || starts_with_spaced(t, "附則") {
        return key;
    }
    // 見出しの無い条（前の条の見出しを共有する「第六条の二　…」）。鍵は条番号。
    if let Some(num) = article_number(t) {
        return Some(num);
    }
    if let Some(rest) = t.strip_prefix('（') {
        if let Some(inner) = rest.strip_suffix('）') {
            // 凡例「（傍線部分は…）」は条見出しではない（注記であり、片側だけの偽行を生む）。
            if inner.starts_with("傍線") {
                return None;
            }
            if inner.chars().count() >= 3 && !inner.contains('）') {
                return key;
            }
        }
    }
    None
}

/// 条・別表・附則の書き出しなら鍵（見出しが 1 つでも行を分けてよい、はっきりした区切り）。
fn unit_heading(line: &str) -> Option<String> {
    let t = line.trim();
    if starts_with_spaced(t, "別表") || starts_with_spaced(t, "附則") {
        // 鍵は「別表第一号」のような番号まで（改正後・改正前で後に続く文が違う）。
        let key: String = t.chars().take_while(|c| !c.is_whitespace() && !"（〔［".contains(*c)).collect();
        return Some(key);
    }
    article_number(t)
}

/// 行が条の書き出し（`第六条の二　…`。条番号の後に全角空白）なら条番号。列の折り返しで
/// 行頭に来た「第三条の規定により」（空白が無い）は条の書き出しではない。
fn article_number(line: &str) -> Option<String> {
    const NUM: &str = "〇一二三四五六七八九十百千";
    let t = line.trim();
    let rest = t.strip_prefix('第')?;
    let n = rest.chars().take_while(|c| NUM.contains(*c)).count();
    if n == 0 {
        return None;
    }
    let mut chars: Vec<char> = rest.chars().collect();
    if chars.get(n) != Some(&'条') {
        return None;
    }
    let mut end = n + 1;
    // 枝番号（「の二」「の二の三」）。
    while chars.get(end) == Some(&'の') {
        let m = chars[end + 1..].iter().take_while(|c| NUM.contains(**c)).count();
        if m == 0 {
            break;
        }
        end += 1 + m;
    }
    // 「第十二条及び第十三条　削除」「第九条から第十一条まで　削除」は、まとめて削った条の書き出し（鍵は頭の条）。
    if chars.get(end) != Some(&'\u{3000}') {
        let tail: String = chars[end..].iter().collect();
        let joined = ["及び", "から"].iter().any(|j| {
            tail.strip_prefix(j).and_then(|r| r.split_once('\u{3000}')).is_some_and(|(second, _)| {
                article_number(&format!("{}\u{3000}削除", second.trim_end_matches("まで"))).is_some()
            })
        });
        if !joined {
            return None;
        }
    }
    chars.truncate(end);
    Some(format!("第{}", chars.into_iter().collect::<String>()))
}

/// 号の見出し行（`一　（略）`・`二　次に掲げる…`）なら号番号を鍵として返す。
///
/// 条見出しの無い表（告示の別表の号ごとの改正など）を号単位で揃えるためのフォールバック。
/// 帯の復元で号番号と本文の間の空き（全角空白）が保たれることを前提にし、列折り返しで
/// たまたま漢数字から始まった行（`五重量パーセント以上…`）と区別する。
fn go_heading_key(line: &str) -> Option<String> {
    let t = line.trim();
    let num: String = t.chars().take_while(|c| "一二三四五六七八九十".contains(*c)).collect();
    if num.is_empty() {
        return None;
    }
    t[num.len()..].starts_with('\u{3000}').then_some(num)
}

/// 字間の空白（`附　則`・`別　表` のような均等割り）を無視して `prefix` で始まるか。
pub(crate) fn starts_with_spaced(text: &str, prefix: &str) -> bool {
    let mut chars = text.chars().filter(|c| !c.is_whitespace());
    prefix.chars().all(|p| chars.next() == Some(p))
}

/// 2 列の系列を最長共通部分列で対応付ける。返り値は `(a の index, b の index)` の列で、
/// 片側のみのギャップは `None`。
fn lcs_align(a: &[&str], b: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut res = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            res.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            res.push((Some(i), None));
            i += 1;
        } else {
            res.push((None, Some(j)));
            j += 1;
        }
    }
    while i < n {
        res.push((Some(i), None));
        i += 1;
    }
    while j < m {
        res.push((None, Some(j)));
        j += 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structures_prose_as_single_paragraph() {
        let doc = Document::from_text("第一条中「甲」を「乙」に改める。");
        assert_eq!(doc.format, "prose");
        assert_eq!(
            doc.blocks,
            vec![Block::Paragraph {
                runs: vec![Run::plain("第一条中「甲」を「乙」に改める。")]
            }]
        );
    }

    #[test]
    fn structures_shinkyu_into_after_before() {
        // 見出しが無い小さなセルは単一行になる。
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.format, "shinkyu");
        assert_eq!(
            doc.blocks,
            vec![
                Block::Paragraph {
                    runs: vec![Run::plain("前文")]
                },
                Block::Shinkyu {
                    rows: vec![ShinkyuRow {
                        after: vec![Run::plain("甲種")],
                        before: vec![Run::plain("乙類")],
                        ..Default::default()
                    }],
                },
            ]
        );
    }

    #[test]
    fn aligns_rows_by_heading_lcs() {
        // 共通見出し（特例A/特例B）は同じ行に、改正後のみの新設（特例C）は片側行に。
        let after = "（特例A）\n第一条新\n（特例B）\n第二条新\n（特例C）\n第三条新";
        let before = "（特例A）\n第一条旧\n（特例B）\n第二条旧";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let rows = match &doc.blocks[0] {
            Block::Shinkyu { rows } => rows,
            _ => panic!("shinkyu block"),
        };
        assert_eq!(rows.len(), 3);
        // 行1: 特例A 改正後/改正前 両方。
        assert!(rows[0].after[0].text.contains("第一条新") && rows[0].before[0].text.contains("第一条旧"));
        // 行2: 特例B 両方。
        assert!(rows[1].after[0].text.contains("第二条新") && rows[1].before[0].text.contains("第二条旧"));
        // 行3: 特例C は改正後のみ（新設）。改正前セルは空。
        assert!(rows[2].after[0].text.contains("第三条新"));
        assert!(rows[2].before.is_empty());
    }

    #[test]
    fn aligns_rows_by_go_number_without_article_headings() {
        // 条見出しの無い表は号番号で揃える。列折り返しで漢数字始まりになった行
        // （五重量…）は号ではない。改正前に無い号（三）は片側行になる。
        let after = "一\u{3000}（略）\n二\u{3000}（１）\u{3000}甲酸溶液（濃度が四十\n五重量パーセント）\n三\u{3000}（略）";
        let before = "一\u{3000}（略）\n二\u{3000}（新設）";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let rows = match &doc.blocks[0] {
            Block::Shinkyu { rows } => rows,
            _ => panic!("shinkyu block"),
        };
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert!(rows[1].after[0].text.contains("五重量パーセント"));
        assert!(rows[1].before[0].text.contains("（新設）"));
        assert!(rows[2].before.is_empty());
    }

    #[test]
    fn pairs_renumbered_go_rows_by_their_opening() {
        // 号の繰り下げ（改正前の十八・十九が改正後では二十・二十一）。番号が揃わず片側だけの行に
        // なっていたが、番号を除いた書き出しが同じ行を組にする。書き出しが違う行は片側のまま。
        let after = "十七\u{3000}（略）\n二十\u{3000}特定非上場会社の議決権に係る議決権代理行使受任\n\
                     二十一\u{3000}有価証券の引受けのうち元引受契約に係るもの\n二十二\u{3000}新しく加える号の文である";
        let before = "十七\u{3000}（略）\n十八\u{3000}特定非上場会社の議決権に係る議決権代理行使受任\n十九\u{3000}有価証券の引受けのうち元引受契約に係るもの";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        let text = |r: &[Run]| r.iter().map(|x| x.text.as_str()).collect::<String>();
        let row = |k: &str| rows.iter().find(|r| text(&r.after).starts_with(k)).unwrap();
        assert!(text(&row("二十\u{3000}").before).starts_with("十八"), "{rows:?}");
        assert!(text(&row("二十一").before).starts_with("十九"), "{rows:?}");
        assert!(row("二十二").before.is_empty(), "{rows:?}");
    }

    #[test]
    fn moves_bunched_new_marks_back_to_the_new_articles() {
        // 新設の条が 3 つ続き、改正前の欄の「（新設）」3 つがまとめて最後の条の行に付いていた。
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let mut rows = vec![
            row("第一条　甲", "第一条　旧甲"),
            row("第一条の二　乙", ""),
            row("第一条の三　丙", ""),
            row("第一条の四　丁", "（新設）\n（新設）\n（新設）"),
        ];
        spread_marks_back(&mut rows);
        assert_eq!(rows[0].1, "第一条　旧甲");
        for r in &rows[1..] {
            assert_eq!(r.1, "（新設）", "{rows:?}");
        }
        // 印以外の字があるセルは動かさない。
        let mut rows = vec![row("第一条の二　乙", ""), row("第一条の三　丙", "第一条の三　旧丙\n（新設）\n（新設）")];
        spread_marks_back(&mut rows);
        assert_eq!(rows[0].1, "");
    }

    #[test]
    fn pairs_same_numbered_articles_with_changed_captions() {
        // 条の見出しが改正で変わり、見出しの鍵がそろわない条も、条番号が同じで文が似ていれば組にする。
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let rows = vec![
            row("（審判手続に係る電子調書の形式的記録事項等）\n第二十五条　審判手続の事務を行う職員は、審判手続の期日ごとに電子調書を作成し、これに次に掲げる事項を記録しなければならない。", ""),
            row("（他の電磁的記録の引用）\n第二十七条　審判手続に係る電子調書には、他の電磁的記録を引用することができる。", ""),
            row("", "（審判手続調書の形式的記載事項）\n第二十五条　審判手続の事務を行う職員は、審判手続の期日ごとに調書を作成しなければならない。調書には、次に掲げる事項を記載しなければならない。"),
            row("", "（調書への引用）\n第二十七条　審判手続の調書には、書面、写真その他審判官が適当と認めるものを引用し、事件記録に添付して調書の一部とすることができる。"),
        ];
        let out = pair_renumbered(rows);
        assert!(out[0].0.contains("第二十五条") && out[0].1.contains("第二十五条"), "{out:?}");
        // 新設の印のある所では条番号で組まない（新設の条が前に入り、元の条が繰り下がったことがある）。
        let rows = vec![
            row("（選定の方法）\n第九条の二　法第二条第十二号の国土交通省令で定める方法は、公募とする。", ""),
            row("", "（新設）"),
            row("", "（選定の方法）\n第九条の二　法第二条第十一号の国土交通省令で定める方法は、公募とする。"),
        ];
        let out = pair_renumbered(rows.clone());
        assert_eq!(out, rows);
    }

    #[test]
    fn realigns_renumbered_items_between_confident_rows() {
        // 号の繰り下げで、番号で組むと別の号が組になる（十七↔十七）。文の似方で組み直す。
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let rows = vec![
            row("十五　特別上場会社等（法第二十六条第一項第三号に掲げるもの）", "十五　特別上場会社等（法第二十六条第一項第三号に掲げるもの）"),
            row("十六　特定非上場会社の議決権に係る議決権代理行使受任", ""),
            row("十七　非上場会社の議決権に係る議決権代理行使受任の後における保有", "十七　有価証券の引受けのうち、次に掲げるもの"),
            row("", "十八　特定非上場会社の議決権に係る議決権代理行使受任"),
            row("", "十九　非上場会社の議決権に係る議決権代理行使受任の後における保有"),
        ];
        let out = realign_weak_windows(rows);
        assert!(out.iter().any(|r| r.0.starts_with("十六") && r.1.starts_with("十八")), "{out:?}");
        assert!(out.iter().any(|r| r.0.starts_with("十七") && r.1.starts_with("十九")), "{out:?}");
    }

    #[test]
    fn pairs_jointly_deleted_articles() {
        let after = ["（調査事項）", "第五条　調査は、次の事項について行う。", "八　法人番号", "第十二条及び第十三条　削除"];
        let before = ["（調査事項）", "第五条　調査は、次の事項について行う。", "第十二条　前条の入力は、", "第十三条　削除"];
        let rows = align_rows(&after, &before);
        let text = |r: &[Run]| r.iter().map(|x| x.text.as_str()).collect::<String>();
        assert_eq!(rows.len(), 2, "{:?}", rows.iter().map(|r| (text(&r.after), text(&r.before))).collect::<Vec<_>>());
        assert!(text(&rows[1].before).contains("第十三条"));
        assert_eq!(article_number("第九条から第十一条まで　削除"), Some("第九条".to_string()));
    }

    #[test]
    fn moves_leading_marks_to_the_previous_row() {
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let mut rows = vec![row("第十一条の二　新しい条", ""), row("第十二条　甲", "（新設）\n第十二条　旧甲")];
        move_leading_marks_back(&mut rows);
        assert_eq!(rows[0].1, "（新設）");
        assert_eq!(rows[1].1, "第十二条　旧甲");
        assert_eq!(leading_mark("（削る。）"), Some("（削る。）"));
    }

    #[test]
    fn merges_items_of_a_new_paragraph() {
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let rows = vec![
            row("一　被保険者等記号・番号", "（新設）"),
            row("二　計算期間の始期及び終期", ""),
            row("三　申請者の氏名", ""),
            row("（見出し）\n第十条　…", "（見出し）\n第十条　…"),
        ];
        let out = merge_new_item_runs(rows);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out[0].0.contains("三　申請者の氏名"));
        assert_eq!(item_number("十二　…"), Some(12));
        assert_eq!(item_number("二十　…"), Some(20));
        assert_eq!(item_number("一般"), None);
    }

    #[test]
    fn shifts_omitted_row_past_new_items() {
        // 号を 4 つ加えて「三十七」以降を繰り下げた。改正前の「三十七　（略）」は改正後の「四十一　（略）」の行。
        let row = |a: &str, b: &str| (a.to_string(), b.to_string());
        let mut rows = vec![
            row("三十二〜三十六　（略）", "三十二〜三十六　（略）\n（新設）\n（新設）\n（新設）\n（新設）"),
            row("三十七　ペルフルオロアルカン酸", "三十七　（略）"),
            row("三十八　長鎖ペルフルオロアルカン酸関連物質", ""),
            row("三十九　チオりん酸", ""),
            row("四十　ポリ塩化直鎖パラフィン", ""),
            row("四十一　（略）\n附　則", ""),
        ];
        shift_omitted_past_new(&mut rows);
        assert_eq!(rows[0].1, "三十二〜三十六　（略）");
        for r in &rows[1..5] {
            assert_eq!(r.1, "（新設）", "{rows:?}");
        }
        assert_eq!(rows[5].1, "三十七　（略）");
    }

    #[test]
    fn recognizes_add_and_delete_marks() {
        // 新しい書き方の印（「〔号を加える。〕」「〔項を削る。〕」）も新設・削った条の印。
        assert_eq!(leading_mark("〔号を加える。〕"), Some("〔号を加える。〕"));
        assert_eq!(leading_mark("〔項を削る。〕〔項を削る。〕"), Some("〔項を削る。〕"));
        assert_eq!(leading_mark("（新設）"), Some("（新設）"));
        assert_eq!(leading_mark("〔同上〕"), None);
        assert_eq!(leading_mark("〔第三条の規定により読み替えて適用する場合を含む。〕"), None);
        assert_eq!(only_marks("〔号を削る。〕\n〔号を削る。〕"), vec!["〔号を削る。〕", "〔号を削る。〕"]);
    }

    #[test]
    fn merges_table_continued_on_next_page() {
        // 1 ページ目の最後の条（第三条）が 2 ページ目へ続き、2 ページ目で次の条が始まる。
        let text = "改正後\n（甲の規定）\n第一条　甲\n（乙の規定）\n第三条　乙の前半\n改正前\n（甲の規定）\n第一条　旧甲\n（乙の規定）\n第三条　旧乙の前半\n\
                    改正後\nの後半。\n（丙の規定）\n第四条　丙\n（丁の規定）\n第五条　丁\n改正前\nの後半。\n（丙の規定）\n第四条　旧丙\n（丁の規定）\n第五条　旧丁";
        let doc = Document::from_text(text);
        assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert!(rows[1].after[0].text.contains("乙の前半の後半。"), "{rows:?}");
        assert!(rows[1].before[0].text.contains("旧乙の前半の後半。"), "{rows:?}");
        assert!(rows[2].after[0].text.contains("第四条"));
    }

    #[test]
    fn pairs_rows_whose_heading_changed() {
        // 見出しの文言が変わった条（改正後「（乙の手続）」・改正前「（乙の申請）」）は 1 行にまとめる。
        let after = "（甲の規定）\n第一条　甲\n（乙の手続）\n第二条　新";
        let before = "（甲の規定）\n第一条　甲\n（乙の申請）\n第二条　旧";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows[1].after[0].text.contains("新") && rows[1].before[0].text.contains("旧"));
    }

    #[test]
    fn splits_rows_at_article_without_caption() {
        // 前の条の見出しを共有する「第六条の二」は別の行。見出しの直後の「第六条」は見出しと同じ行。
        let after = "（免許を要しない無線局）\n第六条　〔略〕\n第六条の二　法第四条第三号の総務省令で定める機能は、次のとおり。";
        let before = "（免許を要しない無線局）\n第六条　〔同上〕\n第六条の二　〔同上〕";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows[0].after[0].text.starts_with("（免許を要しない無線局）\n第六条"));
        assert!(rows[1].after[0].text.starts_with("第六条の二") && rows[1].before[0].text.starts_with("第六条の二"));
    }

    #[test]
    fn moves_new_article_mark_to_its_row() {
        // 「附　則」の後の「（新設）」は、改正後だけにある（新設の）条の行のもの。
        let after = "附　則\n（経過措置）\n第二条　新しい条";
        let before = "附　則\n（新設）";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].before[0].text.trim(), "附　則");
        assert_eq!(rows[1].before[0].text.trim(), "（新設）");
    }

    #[test]
    fn splits_rows_at_article_after_changed_caption() {
        // 見出しの文言が変わった条（改正後と改正前で見出しが違う）が続いても、条ごとの行に分ける。
        let after = "（甲室）\n第一条　甲\n（削る）\n（乙室）\n第二条　乙";
        let before = "（甲室及び丙室）\n第一条　甲\n２　丙\n（乙室及び丁室）\n第二条　乙";
        let doc = Document::from_text(&format!("改正後\n{after}\n改正前\n{before}"));
        let Block::Shinkyu { rows } = &doc.blocks[0] else { panic!() };
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows[1].after[0].text.starts_with("（乙室）") && rows[1].before[0].text.starts_with("（乙室及び丁室）"));
    }

    #[test]
    fn one_paren_caption() {
        assert!(is_one_paren("（令附則第四条（経過措置）の申出）"));
        assert!(!is_one_paren("（甲室）第一条　甲（乙室）"));
    }

    #[test]
    fn article_number_needs_space_after_number() {
        assert_eq!(article_number("第六条の二　法第四条").as_deref(), Some("第六条の二"));
        assert_eq!(article_number("第六十五条の十四　令第八十七条"), Some("第六十五条の十四".to_string()));
        // 列の折り返しで行頭に来た参照は条の書き出しではない。
        assert_eq!(article_number("第三条の規定により"), None);
    }

    #[test]
    fn handles_multiple_shinkyu_segments() {
        // 多ページ表: 改正後/改正前 が複数段。2 段目の先頭が条の見出しで始まらなければ、
        // 1 段目の最後の行の続きとしてつなぐ（ページの切れ目は列の折り返しと同じ）。
        let text = "改正後\nA1\n改正前\nB1\n改正後\nA2\n改正前\nB2";
        let doc = Document::from_text(text);
        assert_eq!(
            doc.blocks,
            vec![Block::Shinkyu {
                rows: vec![ShinkyuRow {
                    after: vec![Run::plain("A1A2")],
                    before: vec![Run::plain("B1B2")],
                    ..Default::default()
                }]
            }]
        );
    }

    #[test]
    fn to_text_roundtrips_markers() {
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.to_text(), text);
    }

    #[test]
    fn joins_column_wrapped_lines_keeping_structure() {
        // 列折り返し（語の途中改行）は結合し、号・条・文末の改行は残す。
        let lines = vec![
            "第六十二条かつお・まぐろ漁業者は、次に掲げる行為をしな",
            "ければならない。",
            "一（略）",
            "二当該さめを所持すること。",
        ];
        let runs = cell_runs(&lines);
        let text = &runs[0].text;
        // 「しな」+「ければ」は連結される。
        assert!(text.contains("行為をしなければならない。"), "{text}");
        // 号「一」「二」は改行で分かれる。
        let after_first = text.split('\n').collect::<Vec<_>>();
        assert!(after_first.iter().any(|l| l.trim() == "一（略）"), "{text}");
        assert!(after_first.iter().any(|l| l.starts_with("二当該さめ")), "{text}");
    }

    #[test]
    fn joins_numbers_wrapped_mid_word() {
        // 実ページ（g00164 0041 改正前・h01754 0002・g00160 0049）の列。数の途中で列が折り返している。
        let text = join_wrapped_lines(&[
            "する金額と租税特別措置法（昭和三十二年法律第二",
            "十六号）第四十一条の十六の二第一項第一号イに掲げ",
            "る場合（令和九年以後の各年分にあつては、同項に掲",
            "げる場合）における同項の規定による基礎控除の額の",
        ]);
        assert!(text.contains("法律第二十六号）第四十一条"), "{text}");
        let text = join_wrapped_lines(&["額に相当する金額と租税特別措置法（昭和", "三十二年法律第二十六号）第四十一条の十", "六の二第一項第一号イに掲げる場合（令和", "九年以後の各年分にあつては、同項に掲げ"]);
        assert!(!text.contains('\n'), "{text}");
        let text = join_wrapped_lines(&["「二二四、七〇〇」に、「三六九、一〇〇」を「四一四、四〇〇」に、「五五二、四〇〇」を「六〇五、", "四〇〇」に、「七一一、五〇〇」を「八六一、六〇〇」に改める。"]);
        assert!(text.contains("「六〇五、四〇〇」"), "{text}");
        // 号・項を指す語（「第七号並びに…」）は条の見出しではない。
        let text = join_wrapped_lines(&["の科目（以下この号、次号、第五号及び", "第七号並びに第四項及び第七項において", "「実習科目」という。）を除く。）を修めて"]);
        assert!(text.contains("第五号及び第七号並びに"), "{text}");
        // 折り返した括弧書き（g00160 0058）。1 行で閉じる見出しや番号の括弧は改行のまま。
        let text = join_wrapped_lines(&[
            "は、当該事項につき同項各号に掲げる場合の区分に応じ、当該各号に定める方法により投資主",
            "（その期間内に当該方法による提供計算書類等の提供の請求をした者を除く。）に対して提供し",
            "たものとみなす。",
            "（経過措置）",
            "第二条　この府令の施行の際現に存する投資法人については、なお従前の例による。ただし",
            "（１）　前号に掲げるもの",
        ]);
        assert!(text.contains("投資主（その期間内に"), "{text}");
        assert!(text.contains("\n（経過措置）\n"), "{text}");
        assert!(text.contains("\n（１）　前号"), "{text}");
    }

    #[test]
    fn keeps_items_and_lists_that_start_with_numbers() {
        // 号の番号（空白で区切る）は改行のまま。
        let text = join_wrapped_lines(&["四　法第四条第三号の総務省令で定める無線局は、次に掲げるものとする", "一　次に掲げる条件に適合するもの"]);
        assert_eq!(text.lines().count(), 2, "{text}");
        // 散文の段落で号の番号の後ろの空白が落ちたもの（「三日本郵政」）は、数の続きとみなさない。
        let text = join_wrapped_lines(&["二　日本銀行に関する制度の企画及び立案に関すること", "三日本郵政株式会社"]);
        assert_eq!(text.lines().count(), 2, "{text}");
        // 1 列に 1 つずつ並べた地番（短い列）は、位取りの数でもつなげない（h01798 0006）。
        let text = join_wrapped_lines(&["岐阜県岐阜市大字岩井", "字矢之洞三八〇番二〇〇", "字嶽四四二番七から四四二番九まで", "四四三番二", "四四五番二"]);
        assert!(text.ends_with("まで\n四四三番二\n四四五番二"), "{text}");
        // 名簿の行（h01789 0008）。
        let text = join_wrapped_lines(&["三六三四林田俊一八、七、三一", "三八五九早川正八、四、三〇", "四〇五四小川裕通八、七、三一"]);
        assert_eq!(text.lines().count(), 3, "{text}");
    }

    #[test]
    fn separates_omission_marks() {
        // 省略の印は 1 列を占める（g00164 0041 改正後）。
        let text = join_wrapped_lines(&["第六条　〔略〕", "〔２・３　略〕", "４　法第四条第三号の総務省令で定める無線局は、次に掲げるものとする。"]);
        assert_eq!(text, "第六条　〔略〕\n〔２・３　略〕\n４　法第四条第三号の総務省令で定める無線局は、次に掲げるものとする。");
        let text = join_wrapped_lines(&["五MHz以下の周波数の電波を使用するもの", "〔三〜十一　同上〕"]);
        assert_eq!(text.lines().count(), 2, "{text}");
        // 印の後ろの列も続けない（備考は別の行）。
        let text = join_wrapped_lines(&["〔六〜十四　略〕", "備考　表中の〔　〕の記載は注記である。"]);
        assert_eq!(text.lines().count(), 2, "{text}");
        // 文の途中の括弧（「〔　〕の記載」）や、後ろに本文の続く印は行を分けない。
        assert!(!is_bracket_note("〔　〕の記載及び対象規定の二重傍線"));
        assert!(!is_bracket_note("〔項を加える。〕年限二年以上のものに限る。"));
        assert!(is_bracket_note("〔略〕〔２・３　略〕"));
        assert!(is_bracket_note("［様式　別紙二十三　挿入］"));
    }

    #[test]
    fn serializes_to_json() {
        let doc = Document::from_text("改正後\n甲\n改正前\n乙");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"format\":\"shinkyu\""));
        assert!(json.contains("\"kind\":\"shinkyu\""));
        // underline は false のとき省略される。
        assert!(!json.contains("underline"));
    }
}

