//! 官報の目次の標題から、その項目が改める法令・根拠とする法令を引く。
//!
//! e-Gov の改正イベントとの突合 (`match_event`) は、e-Gov への反映が数か月遅れるため公布直後の
//! 官報にはつながらない (2026-10 時点で `kanpo/{date}/index.json` の `linked_laws` は全件空だった)。
//! 標題には改正される法令の題名がそのまま書かれているので、法令一覧の題名と完全一致で突き合わせれば
//! 公布の当日からつなげられる。
//!
//! - 改正・廃止: `Xの一部を改正する政令` / `X及びYの一部を改正する省令` / `Xを廃止する件` → X (, Y)
//! - 施行期日: `Xの一部を改正する法律の一部の施行期日を定める政令` → X
//! - 根拠: 告示は e-Gov に無いので、告示の改正は標題の先頭の根拠法令
//!   (`外国為替令第七条の規定に基づき、…の一部を改正する件` → 外国為替令) につなぐ。
//!   根拠の付く通知 (国債の発行条件など) は毎日大量に出るため、改正・廃止の形の標題に限る。
//!
//! 帰化・破産などの公告は上の形にならないので、どの法令にもつながらない。

use std::collections::HashMap;

/// 標題の末尾の種別。`…の一部を改正する` のあとに続く語。
const KINDS: [&str; 9] = [
    "内閣府令",
    "法律",
    "政令",
    "省令",
    "府令",
    "規則",
    "告示",
    "命令",
    "件",
];
const AMEND_MARKERS: [&str; 2] = ["の一部を改正する", "を廃止する"];
const ENFORCEMENT_MARKERS: [&str; 2] = ["の一部の施行期日を定める政令", "の施行期日を定める政令"];
/// 根拠法令の題名のあとに続く語 (`外国為替令第七条` `電波法等の規定により` `…法附則第二条`)。
const BASIS_FOLLOWERS: [&str; 7] = [
    "第",
    "附則",
    "別表",
    "の規定",
    "等の規定",
    "に基づ",
    "による",
];
/// 「A及びB」「A、B」「A並びにB」の区切り。
const SEPARATORS: [&str; 3] = ["及び", "、", "並びに"];

/// 法令の題名 → 法令 ID。同名の法令が複数ある題名は対応付けに使わない。
#[derive(Debug, Default)]
pub struct LawTitles {
    by_title: HashMap<String, String>,
    max_chars: usize,
}

impl LawTitles {
    pub fn new<I, T, D>(laws: I) -> Self
    where
        I: IntoIterator<Item = (T, D)>,
        T: Into<String>,
        D: Into<String>,
    {
        let mut ids: HashMap<String, Vec<String>> = HashMap::new();
        for (title, id) in laws {
            ids.entry(title.into()).or_default().push(id.into());
        }
        let by_title: HashMap<String, String> = ids
            .into_iter()
            .filter_map(|(t, ids)| (ids.len() == 1).then(|| (t, ids[0].clone())))
            .collect();
        let max_chars = by_title
            .keys()
            .map(|t| t.chars().count())
            .max()
            .unwrap_or(0);
        Self {
            by_title,
            max_chars,
        }
    }

    pub fn get(&self, title: &str) -> Option<&str> {
        self.by_title.get(title).map(String::as_str)
    }

    /// `s` の先頭に置かれた最長の題名と、その残り。
    fn longest_prefix<'a>(&self, s: &'a str) -> Option<(&str, &'a str)> {
        let ends: Vec<usize> = s
            .char_indices()
            .map(|(i, c)| i + c.len_utf8())
            .take(self.max_chars)
            .collect();
        ends.iter()
            .rev()
            .find_map(|&end| self.get(&s[..end]).map(|id| (id, &s[end..])))
    }
}

/// 標題から引いた法令 ID。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleLinks {
    /// この項目が改正・廃止する (または施行期日を定める) 法令。
    pub amends: Vec<String>,
    /// この項目 (告示などの改正) の根拠法令。`amends` が引けたときは空。
    pub basis: Vec<String>,
}

pub fn title_links(title: &str, laws: &LawTitles) -> TitleLinks {
    let t = strip_agency(title);
    // 施行期日: 改正法の施行期日なら改正される法令、新法の施行期日ならその法令。
    for marker in ENFORCEMENT_MARKERS {
        if let Some(core) = t.strip_suffix(marker) {
            let base = core.strip_suffix("の一部を改正する法律").unwrap_or(core);
            return TitleLinks {
                amends: split_laws(strip_tou(base), laws).unwrap_or_default(),
                basis: vec![],
            };
        }
    }
    let Some(core) = amend_core(t) else {
        return TitleLinks::default();
    };
    if let Some(ids) = split_laws(strip_tou(core), laws) {
        return TitleLinks {
            amends: ids,
            basis: vec![],
        };
    }
    let basis = laws
        .longest_prefix(core)
        .filter(|(_, rest)| BASIS_FOLLOWERS.iter().any(|f| rest.starts_with(f)))
        .map(|(id, _)| vec![id.to_string()])
        .unwrap_or_default();
    TitleLinks {
        amends: vec![],
        basis,
    }
}

/// 末尾の制定機関略号「（総務一一六）」を落とす。
pub fn strip_agency(title: &str) -> &str {
    let t = title.trim();
    match t.strip_suffix('）').and_then(|s| s.rfind('（')) {
        Some(i) => t[..i].trim_end(),
        None => t,
    }
}

fn strip_tou(s: &str) -> &str {
    s.strip_suffix('等').unwrap_or(s)
}

/// `Xの一部を改正する政令` → X。標題の末尾が種別で終わるものだけ (`…改正する法律の施行に伴う…` は対象外)。
fn amend_core(t: &str) -> Option<&str> {
    KINDS
        .iter()
        .filter_map(|k| t.strip_suffix(k))
        .find_map(|rest| AMEND_MARKERS.iter().find_map(|m| rest.strip_suffix(m)))
}

/// 「A及びB」を題名の並びに分ける。題名自体に「及び」「、」を含むことがあるため、まず全体を、
/// 次に左側が題名として成り立つ位置で分割を試す。
fn split_laws(s: &str, laws: &LawTitles) -> Option<Vec<String>> {
    if let Some(id) = laws.get(s) {
        return Some(vec![id.to_string()]);
    }
    for sep in SEPARATORS {
        for (i, _) in s.match_indices(sep) {
            let (left, right) = (&s[..i], &s[i + sep.len()..]);
            if let (Some(l), Some(mut r)) = (laws.get(left), split_laws(right, laws)) {
                r.insert(0, l.to_string());
                return Some(r);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laws() -> LawTitles {
        LawTitles::new([
            ("刑事訴訟法", "L-keiso"),
            ("輸出貿易管理令", "L-yushutsu"),
            ("農地法施行規則", "L-nouchi"),
            ("農林漁業の健全な発展と調和のとれた再生可能エネルギー電気の発電の促進に関する法律施行規則", "L-saiene"),
            ("救助隊の編成、装備及び配置の基準を定める省令", "L-kyujo"),
            ("外国為替令", "L-gaitame-rei"),
            ("外国為替及び外国貿易法", "L-gaitame"),
            ("消防組織法", "L-shobo"),
            ("電波法", "L-denpa"),
            ("株式会社日本政策投資銀行法", "L-dbj"),
            ("同名の規則", "D1"),
            ("同名の規則", "D2"),
        ])
    }

    fn amends(title: &str) -> Vec<String> {
        title_links(title, &laws()).amends
    }

    fn basis(title: &str) -> Vec<String> {
        title_links(title, &laws()).basis
    }

    #[test]
    fn amendment_titles_link_to_the_amended_laws() {
        assert_eq!(
            amends("輸出貿易管理令の一部を改正する政令（三二五）"),
            ["L-yushutsu"]
        );
        assert_eq!(
            amends("農地法施行規則及び農林漁業の健全な発展と調和のとれた再生可能エネルギー電気の発電の促進に関する法律施行規則の一部を改正する省令（農林水産七二）"),
            ["L-nouchi", "L-saiene"]
        );
        // 題名の中の「、」「及び」で誤って分割しない。
        assert_eq!(
            amends(
                "救助隊の編成、装備及び配置の基準を定める省令の一部を改正する省令（総務一一六）"
            ),
            ["L-kyujo"]
        );
        assert_eq!(amends("刑事訴訟法等の一部を改正する法律"), ["L-keiso"]);
        assert_eq!(amends("輸出貿易管理令を廃止する政令"), ["L-yushutsu"]);
        assert!(
            amends("同名の規則の一部を改正する省令").is_empty(),
            "同名の法令はつながない"
        );
    }

    #[test]
    fn enforcement_date_orders_link_to_the_base_law() {
        assert_eq!(
            amends("刑事訴訟法の一部を改正する法律の一部の施行期日を定める政令（三二二）"),
            ["L-keiso"]
        );
        assert_eq!(
            amends("外国為替及び外国貿易法の施行期日を定める政令"),
            ["L-gaitame"]
        );
    }

    #[test]
    fn notice_amendments_link_to_their_basis_law() {
        assert_eq!(basis("外国為替令第七条の規定に基づき、財務大臣が指定する取引又は行為を指定する件の一部を改正する件（財務二六七）"), ["L-gaitame-rei"]);
        assert_eq!(basis("消防組織法第十六条第二項の規定に基づき、消防救助操法の基準の一部を改正する件（同一七）"), ["L-shobo"]);
        assert_eq!(
            basis(
                "電波法等の規定により伝搬障害防止区域を指定する件等の一部を改正する件（同三八二）"
            ),
            ["L-denpa"]
        );
        assert_eq!(basis("株式会社日本政策投資銀行法附則第二条の十第一項の規定に基づき、指定営業所を指定する件の一部を改正する件（同二七六）"), ["L-dbj"]);
        // 題名に「及び」を含む根拠法令。
        assert_eq!(basis("外国為替及び外国貿易法第二十一条第一項の規定に基づく財務大臣の許可を受けなければならない資本取引を指定する件の一部を改正する件（同二六八）"), ["L-gaitame"]);
        // 題名の直後が条番号などでなければ根拠とみなさない。
        assert!(basis("電波法改革のための基準の一部を改正する告示").is_empty());
    }

    #[test]
    fn notices_and_public_announcements_link_to_nothing() {
        for t in [
            "政府資金調達事務取扱規則第五条第十一項の規定に基づき発行した政府短期証券の発行条件等を告示（財務二七〇～二七二）",
            "日本国に帰化を許可する件（法務省告示配一四九）",
            "相続、公示催告、失踪、除権決定、破産、特別清算、再生、所有者不明関係",
            "刑事訴訟法の一部を改正する法律の施行に伴う関係政令の整備に関する政令",
        ] {
            assert_eq!(title_links(t, &laws()), TitleLinks::default(), "{t}");
        }
    }
}
