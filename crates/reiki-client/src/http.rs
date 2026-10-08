//! 例規集サイト向けの礼儀正しい HTTP クライアント。
//!
//! - ホストごとに最小間隔（既定 1 秒、robots.txt の Crawl-delay が長ければそちら）を空ける。
//!   429 / 503 を受けたらそのホストの間隔を倍にし（`Retry-After` があれば従う）、
//!   1 分ほど成功が続くごとに 3/4 ずつ戻す。共有ホスト（www1.g-reiki.net は 500 自治体超）は
//!   1 req/sec でも 429 を返すことがある（2026-10 確認）。
//! - robots.txt を尊重する（`User-agent: *` または `lawpub` のグループの Disallow/Allow）。
//!   取得が 5xx・タイムアウトなら RFC 9309 に従い、その実行中はホスト全体を禁止扱いにする。
//! - Shift_JIS 等の古いテナントがあるため、Content-Type / `<meta charset>` で復号する。

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const USER_AGENT: &str = "lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)";
const ROBOTS_AGENT: &str = "lawpub";

/// robots.txt を置いておらず、存在しないパス全般に空応答（接続断）を返すホスト。
/// RFC 9309 上は「到達不能 = 全面禁止」だが、これらは 404 の代わりに接続を切るだけなので
/// 「robots.txt なし」として扱う。2026-10-08 に `/robots.txt` と `/nonexistent.html` が
/// 同じ空応答になり、例規集ページは 200 を返すことを確認したホストだけを載せる。
const HOSTS_WITHOUT_ROBOTS: &[&str] = &["en3-jg.d1-law.com"];

#[derive(Debug, Clone, Default)]
pub struct Robots {
    rules: Vec<(bool, String)>, // (allow, path prefix)
    pub crawl_delay: Option<Duration>,
    disallow_all: bool,
}

impl Robots {
    pub fn disallow_all() -> Self {
        Self {
            disallow_all: true,
            ..Default::default()
        }
    }

    /// robots.txt を解析する。`lawpub` 向けグループがあればそれを、無ければ `*` を使う。
    pub fn parse(body: &str) -> Self {
        #[derive(Default)]
        struct Group {
            agents: Vec<String>,
            rules: Vec<(bool, String)>,
            delay: Option<Duration>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut cur = Group::default();
        let mut last_was_agent = false;
        for raw in body.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "user-agent" => {
                    if !last_was_agent && !cur.agents.is_empty() {
                        groups.push(std::mem::take(&mut cur));
                    }
                    cur.agents.push(v.to_ascii_lowercase());
                    last_was_agent = true;
                    continue;
                }
                "disallow" if !v.is_empty() => cur.rules.push((false, v.to_string())),
                "allow" if !v.is_empty() => cur.rules.push((true, v.to_string())),
                "crawl-delay" => {
                    cur.delay = v
                        .parse::<f64>()
                        .ok()
                        .filter(|d| *d >= 0.0)
                        .map(Duration::from_secs_f64)
                }
                _ => {}
            }
            last_was_agent = false;
        }
        if !cur.agents.is_empty() {
            groups.push(cur);
        }
        let pick = groups
            .iter()
            .find(|g| {
                g.agents
                    .iter()
                    .any(|a| !a.is_empty() && a != "*" && ROBOTS_AGENT.starts_with(a.as_str()))
            })
            .or_else(|| groups.iter().find(|g| g.agents.iter().any(|a| a == "*")));
        match pick {
            Some(g) => Self {
                rules: g.rules.clone(),
                crawl_delay: g.delay,
                disallow_all: false,
            },
            None => Self::default(),
        }
    }

    /// 最長一致の規則で判定する（同長なら Allow 優先）。`*` / `$` は簡易対応。
    pub fn allows(&self, path: &str) -> bool {
        if self.disallow_all {
            return false;
        }
        let mut best: Option<(usize, bool)> = None;
        for (allow, pat) in &self.rules {
            if pattern_matches(pat, path) {
                let len = pat.len();
                if best.is_none_or(|(l, a)| len > l || (len == l && *allow && !a)) {
                    best = Some((len, *allow));
                }
            }
        }
        best.is_none_or(|(_, a)| a)
    }
}

fn pattern_matches(pat: &str, path: &str) -> bool {
    let (pat, anchored) = match pat.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pat, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
            continue;
        }
        match path[pos..].find(part) {
            Some(off) => pos += off + part.len(),
            None => return false,
        }
    }
    !anchored || pos == path.len() || parts.last().is_some_and(|p| p.is_empty())
}

/// 429 等を受けたときに広げる間隔の上限。
const MAX_BACKOFF_INTERVAL: Duration = Duration::from_secs(30);
/// この時間ぶん連続で成功したら間隔を縮める（最低 RECOVER_MIN_SUCCESSES 回）。
/// 回数だけで数えると 30 秒間隔からの回復に何時間もかかるため時間で揃える。
const RECOVER_WINDOW: Duration = Duration::from_secs(60);
const RECOVER_MIN_SUCCESSES: u32 = 5;
/// 同じホストで 403 がこの回数続いたら、その実行中はホストへのアクセスをやめる。
/// www1.g-reiki.net は GitHub Actions の IP からの要求をすべて 403 にする（2026-10-08 確認）。
/// アクセス制限なので回避はせず、同居する自治体それぞれに 403 を出し続けないようにする。
const FORBIDDEN_STREAK_TO_BLOCK: u32 = 3;

struct HostState {
    robots: Robots,
    next_at: Instant,
    /// 現在のリクエスト間隔（最小間隔以上、429 で広がる）。
    interval: Duration,
    /// 成功が続いていた間隔で 429 を受けたときの、その間隔の最大値。これ以下は
    /// 速すぎると分かっているので、回復時もその 5/4 倍より短くしない
    /// （持続できる速度に収束させる）。429 が連続する間（制限期間中）は記録しない。
    too_fast: Duration,
    ok_streak: u32,
}

impl HostState {
    fn base_interval(&self, min: Duration) -> Duration {
        self.robots.crawl_delay.map_or(min, |d| d.max(min))
    }

    /// 速度制限を受けた: 間隔を倍にし、Retry-After があればその時刻まで次を出さない。
    fn on_throttled(&mut self, min: Duration, retry_after: Option<Duration>) {
        let base = self.base_interval(min);
        if self.ok_streak > 0 {
            self.too_fast = self.too_fast.max(self.interval.max(base));
        }
        self.interval = (self.interval.max(base) * 2).min(MAX_BACKOFF_INTERVAL);
        self.ok_streak = 0;
        let wait = retry_after
            .unwrap_or(self.interval)
            .min(Duration::from_secs(120));
        self.next_at = self.next_at.max(Instant::now() + wait);
    }

    fn on_success(&mut self, min: Duration) {
        let base = self.base_interval(min);
        self.ok_streak += 1;
        let floor = if self.too_fast.is_zero() {
            base
        } else {
            (self.too_fast * 5 / 4).max(base)
        };
        let needed = ((RECOVER_WINDOW.as_millis() / self.interval.as_millis().max(1)) as u32)
            .max(RECOVER_MIN_SUCCESSES);
        if self.ok_streak >= needed && self.interval > floor {
            self.interval = (self.interval * 3 / 4).max(floor);
            self.ok_streak = 0;
        }
    }
}

pub struct PoliteClient {
    client: reqwest::blocking::Client,
    min_interval: Duration,
    hosts: Mutex<HashMap<String, HostState>>,
    /// http で要求すると https へ転送されたホスト。以降は最初から https で取りに行き、
    /// 1 ページ 2 リクエスト（307 + 本体）にならないようにする。
    https_hosts: Mutex<std::collections::HashSet<String>>,
    /// ホスト名ごとの 403 の連続回数（http/https は同じホストとして数える）。
    forbidden: Mutex<HashMap<String, u32>>,
}

/// ホストが 403 を返し続けるため、この実行中はアクセスしない。
#[derive(Debug)]
pub struct HostBlocked(pub String);
impl std::fmt::Display for HostBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} returned HTTP 403 repeatedly; skipped for this run",
            self.0
        )
    }
}
impl std::error::Error for HostBlocked {}

/// 404 などで本文が取れなかったことを表す（リトライしない）。
#[derive(Debug)]
pub struct HttpStatusError(pub u16);
impl std::fmt::Display for HttpStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}", self.0)
    }
}
impl std::error::Error for HttpStatusError {}

impl PoliteClient {
    pub fn new(min_interval: Duration) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            // 接続を使い回さない。一部の例規集ホストは失敗した要求の後の keep-alive 接続に
            // 404 を返す（2026-10 en3-jg.d1-law.com で確認）。1 req/sec なので再接続の負荷は小さい。
            .pool_max_idle_per_host(0)
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            client,
            min_interval,
            hosts: Mutex::new(HashMap::new()),
            https_hosts: Mutex::new(Default::default()),
            forbidden: Mutex::new(HashMap::new()),
        })
    }

    /// 403 が続いてアクセスを止めたホストか。
    pub fn is_blocked(&self, host: &str) -> bool {
        self.forbidden
            .lock()
            .unwrap()
            .get(host)
            .is_some_and(|n| *n >= FORBIDDEN_STREAK_TO_BLOCK)
    }

    fn record_forbidden(&self, host: &str, forbidden: bool) {
        let mut m = self.forbidden.lock().unwrap();
        if forbidden {
            let n = m.entry(host.to_string()).or_default();
            *n += 1;
            if *n == FORBIDDEN_STREAK_TO_BLOCK {
                tracing::warn!("{host}: HTTP 403 が {n} 回続いたため、この実行ではアクセスしない");
            }
        } else {
            m.remove(host);
        }
    }

    /// 既に https へ転送されたことのあるホストなら URL を https に書き換える。
    fn upgrade(&self, url: &str) -> String {
        match url.strip_prefix("http://") {
            Some(rest) => {
                let host = rest.split('/').next().unwrap_or("");
                if self.https_hosts.lock().unwrap().contains(host) {
                    format!("https://{rest}")
                } else {
                    url.to_string()
                }
            }
            None => url.to_string(),
        }
    }

    fn origin(url: &url::Url) -> String {
        format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""))
            + &url.port().map(|p| format!(":{p}")).unwrap_or_default()
    }

    /// ホストの次回リクエスト可能時刻まで待ち、予約を進める。
    fn wait_turn(&self, host: &str) {
        let wait = {
            let mut hosts = self.hosts.lock().unwrap();
            let st = hosts.get_mut(host).expect("robots loaded");
            let interval = st.interval.max(st.base_interval(self.min_interval));
            let now = Instant::now();
            let at = st.next_at.max(now);
            st.next_at = at + interval;
            at - now
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    fn ensure_robots(&self, origin: &str) {
        if self.hosts.lock().unwrap().contains_key(origin) {
            return;
        }
        let url = format!("{origin}/robots.txt");
        let robots = match self.client.get(&url).send() {
            Ok(r) if r.status().is_success() => match r.text() {
                Ok(body) if !body.trim_start().starts_with('<') => Robots::parse(&body),
                // HTML のエラーページを 200 で返すホストがある（= robots.txt 無し）。
                Ok(_) => Robots::default(),
                Err(_) => Robots::disallow_all(),
            },
            Ok(r) if r.status().is_server_error() => {
                tracing::warn!(
                    "robots.txt {url}: HTTP {} → このホストは今回スキップ",
                    r.status()
                );
                Robots::disallow_all()
            }
            Ok(_) => Robots::default(), // 4xx = 制限なし (RFC 9309)
            Err(e)
                if HOSTS_WITHOUT_ROBOTS
                    .iter()
                    .any(|h| origin.ends_with(&format!("://{h}"))) =>
            {
                tracing::debug!("robots.txt {url}: {e} (known host without robots.txt)");
                Robots::default()
            }
            Err(e) => {
                tracing::warn!("robots.txt {url}: {e} → このホストは今回スキップ");
                Robots::disallow_all()
            }
        };
        self.hosts
            .lock()
            .unwrap()
            .entry(origin.to_string())
            .or_insert(HostState {
                robots,
                next_at: Instant::now() + self.min_interval,
                interval: self.min_interval,
                too_fast: Duration::ZERO,
                ok_streak: 0,
            });
    }

    pub fn allowed(&self, url: &str) -> Result<bool> {
        let url = self.upgrade(url);
        let u = url::Url::parse(&url).with_context(|| format!("parse url {url}"))?;
        let origin = Self::origin(&u);
        self.ensure_robots(&origin);
        let hosts = self.hosts.lock().unwrap();
        Ok(hosts[&origin].robots.allows(u.path()))
    }

    fn with_host(&self, origin: &str, f: impl FnOnce(&mut HostState, Duration)) {
        if let Some(st) = self.hosts.lock().unwrap().get_mut(origin) {
            f(st, self.min_interval);
        }
    }

    /// 現在のホスト間隔（ログ・テスト用）。
    pub fn host_interval(&self, origin: &str) -> Option<Duration> {
        self.hosts.lock().unwrap().get(origin).map(|s| s.interval)
    }

    /// HTML を取得して文字列に復号する。429/5xx・通信エラーは間隔を広げて 4 回まで再試行。
    pub fn get_html(&self, url: &str) -> Result<String> {
        let upgraded = self.upgrade(url);
        let url = upgraded.as_str();
        let u = url::Url::parse(url).with_context(|| format!("parse url {url}"))?;
        let origin = Self::origin(&u);
        let host = u.host_str().unwrap_or("").to_string();
        if self.is_blocked(&host) {
            return Err(HostBlocked(host).into());
        }
        if !self.allowed(url)? {
            bail!("robots.txt disallows {url}");
        }
        let mut last_err = None;
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(2 * attempt));
            }
            self.wait_turn(&origin);
            match self.client.get(url).send() {
                Ok(r) => {
                    let status = r.status();
                    self.record_forbidden(&host, status.as_u16() == 403);
                    if status.is_success() {
                        self.with_host(&origin, |st, min| st.on_success(min));
                        if u.scheme() == "http"
                            && r.url().scheme() == "https"
                            && r.url().host_str() == u.host_str()
                        {
                            if let Some(h) = u.host_str() {
                                self.https_hosts.lock().unwrap().insert(h.to_string());
                            }
                        }
                        let ct = r
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string);
                        let bytes = r.bytes().with_context(|| format!("read {url}"))?;
                        return Ok(decode_html(&bytes, ct.as_deref()));
                    }
                    if status.is_server_error() || status.as_u16() == 429 {
                        if matches!(status.as_u16(), 429 | 503) {
                            let retry_after = r
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.trim().parse::<u64>().ok())
                                .map(Duration::from_secs);
                            self.with_host(&origin, |st, min| st.on_throttled(min, retry_after));
                            tracing::debug!(
                                "{origin}: HTTP {status} → interval {:?}",
                                self.host_interval(&origin).unwrap_or_default()
                            );
                        }
                        last_err = Some(anyhow::anyhow!("GET {url}: HTTP {status}"));
                        continue;
                    }
                    return Err(anyhow::Error::new(HttpStatusError(status.as_u16()))
                        .context(format!("GET {url}")));
                }
                Err(e) => last_err = Some(anyhow::Error::new(e).context(format!("GET {url}"))),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("GET {url} failed")))
    }
}

/// Content-Type の charset → `<meta charset>` → UTF-8 の順で文字コードを決める。
pub fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    let from_header = content_type.and_then(|ct| {
        ct.to_ascii_lowercase()
            .split("charset=")
            .nth(1)
            .map(|s| s.trim_matches(['"', ' ', ';']).to_string())
    });
    let label = from_header.or_else(|| sniff_meta_charset(bytes));
    let enc = label
        .as_deref()
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = enc.decode(bytes);
    text.into_owned()
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(2048)];
    let lower: String = head
        .iter()
        .map(|b| (*b as char).to_ascii_lowercase())
        .collect();
    let idx = lower.find("charset=")? + "charset=".len();
    let label: String = lower[idx..]
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!label.is_empty()).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_groups_and_longest_match() {
        let r = Robots::parse(
            "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /search/\nAllow: /search/public\nCrawl-delay: 3\n",
        );
        assert!(r.allows("/reiki/reiki_menu.html"));
        assert!(!r.allows("/search/x"));
        assert!(r.allows("/search/public/a"));
        assert_eq!(r.crawl_delay, Some(Duration::from_secs(3)));

        let own = Robots::parse("User-agent: *\nDisallow:\n\nUser-agent: lawpub\nDisallow: /\n");
        assert!(!own.allows("/a"));

        let wild = Robots::parse("User-agent: *\nDisallow: /*.pdf$\n");
        assert!(!wild.allows("/x/y.pdf"));
        assert!(wild.allows("/x/y.pdf.html"));
        assert!(Robots::default().allows("/anything"));
        assert!(!Robots::disallow_all().allows("/"));
    }

    #[test]
    fn throttle_backs_off_and_recovers() {
        let min = Duration::from_secs(1);
        let mut st = HostState {
            robots: Robots::default(),
            next_at: Instant::now(),
            interval: min,
            too_fast: Duration::ZERO,
            ok_streak: 0,
        };
        st.on_throttled(min, None);
        assert_eq!(st.interval, Duration::from_secs(2));
        st.on_throttled(min, Some(Duration::from_secs(10)));
        assert_eq!(st.interval, Duration::from_secs(4));
        assert!(st.next_at >= Instant::now() + Duration::from_secs(9));
        for _ in 0..6 {
            st.on_throttled(min, None);
        }
        assert_eq!(st.interval, MAX_BACKOFF_INTERVAL);
        // 30 秒間隔なら 5 回（2.5 分）の成功で縮み始める
        for _ in 0..RECOVER_MIN_SUCCESSES {
            st.on_success(min);
        }
        assert!(st.interval < MAX_BACKOFF_INTERVAL);
        for _ in 0..2000 {
            st.on_success(min);
        }
        // 429 が連続しただけ（制限期間）なら下限は付かず元の間隔に戻る
        assert_eq!(st.interval, min);

        // 1 秒間隔で成功が続いた後に 429 → 1 秒は速すぎるので 1.25 秒に落ち着く
        let mut st2 = HostState {
            robots: Robots::default(),
            next_at: Instant::now(),
            interval: min,
            too_fast: Duration::ZERO,
            ok_streak: 0,
        };
        st2.on_success(min);
        st2.on_throttled(min, None);
        for _ in 0..500 {
            st2.on_success(min);
        }
        assert_eq!(st2.interval, Duration::from_millis(1250));
    }

    #[test]
    fn blocks_host_after_repeated_403_and_counts_http_https_together() {
        let c = PoliteClient::new(Duration::from_secs(1)).unwrap();
        c.record_forbidden("www1.g-reiki.net", true);
        c.record_forbidden("www1.g-reiki.net", true);
        assert!(!c.is_blocked("www1.g-reiki.net"));
        // 途中で成功すれば数え直し
        c.record_forbidden("www1.g-reiki.net", false);
        for _ in 0..FORBIDDEN_STREAK_TO_BLOCK {
            c.record_forbidden("www1.g-reiki.net", true);
        }
        assert!(c.is_blocked("www1.g-reiki.net"));
        assert!(!c.is_blocked("en3-jg.d1-law.com"));
        // ブロック中はネットワークに出ずに HostBlocked を返す
        let err = c
            .get_html("http://www1.g-reiki.net/x/reiki_menu.html")
            .unwrap_err();
        assert!(err.downcast_ref::<HostBlocked>().is_some());
    }

    #[test]
    fn decodes_shift_jis_by_meta() {
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("<meta charset=\"shift_jis\">例規集");
        assert!(decode_html(&sjis, None).contains("例規集"));
        let (sjis2, _, _) = encoding_rs::SHIFT_JIS.encode("例規");
        assert_eq!(
            decode_html(&sjis2, Some("text/html; charset=Shift_JIS")),
            "例規"
        );
        assert_eq!(decode_html("例規".as_bytes(), Some("text/html")), "例規");
    }
}
