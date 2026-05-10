//! e-Gov 法令データ取得クライアント。
//!
//! Phase 1.5 では `MockProvider` (組み込みサンプル) と `HttpProvider` (e-Gov 法令API
//! v2 を想定) の二系統を提供する。HttpProvider のエンドポイントは
//! `LAWPUB_EGOV_BASE_URL` で上書き可能で、既定値は v1 (`https://laws.e-gov.go.jp/api/1`)。
//!
//! ## エンドポイント (v2 想定)
//!
//! - 更新一覧:  `GET {base}/updatelawlists/{YYYYMMDD}` (XML)
//! - 法令本文:  `GET {base}/lawdata/{lawId}`        (XML, ZIP応答含む)
//!
//! 実エンドポイント仕様は変更されることがあるため、parse は寛容に行い、失敗時は
//! 個別法令単位で skip する。

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedLaw {
    pub law_id: String,
    pub xml: Vec<u8>,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBatch {
    pub date: String,
    pub laws: Vec<FetchedLaw>,
}

pub trait EgovProvider: Send + Sync {
    fn fetch_update(&self, date: &str) -> Result<UpdateBatch>;

    /// 全件バルク取得。`category` は e-Gov v2 の分類番号 (1=憲法・法律 など)。
    /// 規定実装は未対応エラー — provider ごとに実装する。
    fn fetch_bulk(&self, _category: u32, _limit: Option<usize>) -> Result<UpdateBatch> {
        anyhow::bail!("fetch_bulk is not implemented for this provider")
    }
}

pub struct MockProvider;

impl EgovProvider for MockProvider {
    fn fetch_update(&self, date: &str) -> Result<UpdateBatch> {
        // 複数の代表法令でモックを構成する — ローカル開発時にダッシュボード/検索が
        // 1 件しか並ばないと UI 動作確認しづらいため。
        let laws = SAMPLE_LAWS
            .iter()
            .map(|(id, xml)| FetchedLaw {
                law_id: (*id).to_string(),
                source_url: format!("mock://egov/{}/{}.xml", date, id),
                xml: xml.as_bytes().to_vec(),
            })
            .collect();
        Ok(UpdateBatch { date: date.to_string(), laws })
    }

    fn fetch_bulk(&self, category: u32, limit: Option<usize>) -> Result<UpdateBatch> {
        // モックでは category を無視して同じ 5 件を返す。
        let mut laws: Vec<FetchedLaw> = SAMPLE_LAWS
            .iter()
            .map(|(id, xml)| FetchedLaw {
                law_id: (*id).to_string(),
                source_url: format!("mock://egov/bulk/cat{}/{}.xml", category, id),
                xml: xml.as_bytes().to_vec(),
            })
            .collect();
        if let Some(n) = limit {
            laws.truncate(n);
        }
        Ok(UpdateBatch { date: format!("bulk-cat{category}"), laws })
    }
}

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into().trim_end_matches('/').to_string() }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(60))
            .build()
            .context("build reqwest client")
    }

    fn get_with_retry(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..3 {
            match client.get(url).send().and_then(|r| r.error_for_status()) {
                Ok(resp) => match resp.bytes() {
                    Ok(b) => return Ok(b.to_vec()),
                    Err(e) => last_err = Some(anyhow!(e)),
                },
                Err(e) => last_err = Some(anyhow!(e)),
            }
            // exponential backoff: 1s, 3s
            std::thread::sleep(Duration::from_millis(1000 * (1u64 << attempt)));
        }
        Err(last_err.unwrap_or_else(|| anyhow!("unknown fetch error")))
            .with_context(|| format!("GET {url}"))
    }

    /// 応答が ZIP (PK\x03\x04) なら最初の .xml エントリを取り出す。
    /// path は相対化・`..` 拒否で Zip Slip 対策する (展開はせずメモリ内のみ)。
    fn maybe_unzip_xml(bytes: Vec<u8>) -> Result<Vec<u8>> {
        if bytes.len() < 4 || &bytes[..4] != b"PK\x03\x04" {
            return Ok(bytes);
        }
        use std::io::{Cursor, Read};
        let cur = Cursor::new(bytes);
        let mut zip = zip::ZipArchive::new(cur).context("open zip")?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).context("zip entry")?;
            // Zip Slip 対策: 安全なパスでなければ拒否。
            let safe_name = entry
                .enclosed_name()
                .ok_or_else(|| anyhow!("unsafe zip entry path"))?
                .to_path_buf();
            if !safe_name
                .extension()
                .map(|e| e.eq_ignore_ascii_case("xml"))
                .unwrap_or(false)
            {
                continue;
            }
            // サイズ上限 (50 MiB) — plan §15 に基づく。
            const MAX: u64 = 50 * 1024 * 1024;
            if entry.size() > MAX {
                anyhow::bail!("zip entry too large: {} bytes", entry.size());
            }
            let mut out = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut out).context("read zip entry")?;
            return Ok(out);
        }
        anyhow::bail!("zip contained no .xml entry")
    }

    /// `<DataRoot><Result><Code>...</Code></Result><ApplData>...<LawId>...</LawId>...`
    /// の形式から ID を抽出する。`Result/Code` が 0 以外なら e-Gov 側エラーとして空を返す。
    fn extract_law_ids(xml: &[u8]) -> Vec<String> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;
        let mut reader = Reader::from_reader(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let mut ids = Vec::new();
        let mut path: Vec<String> = Vec::new();
        let mut text = String::new();
        let mut result_code: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    path.push(String::from_utf8_lossy(e.name().as_ref()).to_string());
                    text.clear();
                }
                Ok(Event::Text(t)) => {
                    text.push_str(&t.unescape().unwrap_or_default());
                }
                Ok(Event::End(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let collected = std::mem::take(&mut text);
                    let trimmed = collected.trim();
                    // Result/Code は通常 ApplData の手前に出現。
                    if name == "Code" && path.iter().any(|p| p == "Result") {
                        result_code = Some(trimmed.to_string());
                    }
                    if matches!(name.as_str(), "LawId" | "LawID") && !trimmed.is_empty() {
                        ids.push(trimmed.to_string());
                    }
                    path.pop();
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
        if let Some(code) = result_code.as_deref() {
            if !matches!(code, "0" | "") {
                tracing::warn!("e-Gov returned Result/Code = {} — treating as empty list", code);
                return Vec::new();
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_law_ids_from_dataroot() {
        let xml = r#"<?xml version="1.0"?>
<DataRoot>
  <Result><Code>0</Code></Result>
  <ApplData>
    <LawNameListInfo>
      <LawId>129AC0000000089</LawId>
      <LawName>民法</LawName>
    </LawNameListInfo>
    <LawNameListInfo>
      <LawId>140AC0000000045</LawId>
      <LawName>刑法</LawName>
    </LawNameListInfo>
  </ApplData>
</DataRoot>"#;
        let ids = HttpProvider::extract_law_ids(xml.as_bytes());
        assert_eq!(ids, vec!["129AC0000000089", "140AC0000000045"]);
    }

    #[test]
    fn returns_empty_when_egov_signals_error() {
        let xml = br#"<?xml version="1.0"?>
<DataRoot>
  <Result><Code>1</Code><Message>NG</Message></Result>
  <ApplData>
    <LawId>SHOULD_BE_IGNORED</LawId>
  </ApplData>
</DataRoot>"#;
        assert!(HttpProvider::extract_law_ids(xml.as_slice()).is_empty());
    }
}

impl EgovProvider for HttpProvider {
    fn fetch_update(&self, date: &str) -> Result<UpdateBatch> {
        let client = Self::client()?;
        let yyyymmdd = date.replace('-', "");
        let list_url = format!("{}/updatelawlists/{}", self.base_url, yyyymmdd);
        let list_xml = match Self::get_with_retry(&client, &list_url).and_then(Self::maybe_unzip_xml) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("update list fetch failed: {e:#}");
                return Ok(UpdateBatch { date: date.to_string(), laws: Vec::new() });
            }
        };
        let ids = Self::extract_law_ids(&list_xml);
        tracing::info!("date={date} update list contains {} law ids", ids.len());

        let mut laws = Vec::new();
        for id in ids {
            let url = format!("{}/lawdata/{}", self.base_url, id);
            match Self::get_with_retry(&client, &url).and_then(Self::maybe_unzip_xml) {
                Ok(xml) => laws.push(FetchedLaw {
                    law_id: id,
                    xml,
                    source_url: url,
                }),
                Err(e) => tracing::warn!("skip {url}: {e:#}"),
            }
        }
        Ok(UpdateBatch { date: date.to_string(), laws })
    }

    fn fetch_bulk(&self, category: u32, limit: Option<usize>) -> Result<UpdateBatch> {
        let client = Self::client()?;
        let list_url = format!("{}/lawlists/{}", self.base_url, category);
        let list_xml = Self::get_with_retry(&client, &list_url)
            .and_then(Self::maybe_unzip_xml)
            .with_context(|| format!("fetch lawlists/{category}"))?;
        let mut ids = Self::extract_law_ids(&list_xml);
        tracing::info!("bulk: category={category} → {} law ids", ids.len());
        if let Some(n) = limit {
            ids.truncate(n);
        }
        let total = ids.len();

        // 並列取得: e-Gov のレスポンスが ~1.8s/req と遅いので逐次だと 5h かかる。
        // 4 並列なら ~75 min に抑えられる。LAWPUB_BULK_CONCURRENCY で上書き可。
        let concurrency: usize = std::env::var("LAWPUB_BULK_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4)
            .clamp(1, 16);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(concurrency)
            .build()
            .context("rayon pool")?;
        let counter = std::sync::atomic::AtomicUsize::new(0);
        let base_url = self.base_url.clone();
        let laws_arc = std::sync::Mutex::new(Vec::<FetchedLaw>::with_capacity(total));

        pool.install(|| {
            use rayon::prelude::*;
            ids.par_iter().for_each(|id| {
                let url = format!("{}/lawdata/{}", base_url, id);
                let result = Self::get_with_retry(&client, &url).and_then(Self::maybe_unzip_xml);
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 200 == 0 {
                    tracing::info!("bulk: {}/{}", n, total);
                }
                match result {
                    Ok(xml) => {
                        if let Ok(mut g) = laws_arc.lock() {
                            g.push(FetchedLaw {
                                law_id: id.clone(),
                                xml,
                                source_url: url,
                            });
                        }
                    }
                    Err(e) => tracing::warn!("skip {url}: {e:#}"),
                }
                // 並列実行下では sleep は意味が薄いので外す。サーバ側で過負荷が
                // 観測される場合は LAWPUB_BULK_CONCURRENCY=2 などで下げる。
            });
        });

        let laws = laws_arc.into_inner().unwrap_or_default();
        Ok(UpdateBatch {
            date: format!("bulk-cat{category}"),
            laws,
        })
    }
}

const SAMPLE_LAWS: &[(&str, &str)] = &[
    ("129AC0000000089", SAMPLE_MINPO),
    ("140AC0000000045", SAMPLE_KEIHO),
    ("322AC0000000049", SAMPLE_ROKIHO),
    ("417AC0000000086", SAMPLE_KAISHA),
    ("321CONSTITUTION", SAMPLE_CONSTITUTION),
];

const SAMPLE_MINPO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law>
  <LawNum>明治二十九年法律第八十九号</LawNum>
  <PromulgationDate>1896-04-27</PromulgationDate>
  <LawBody>
    <LawTitle>民法</LawTitle>
    <MainProvision>
      <Article Num="1"><ArticleTitle>第一条</ArticleTitle><ArticleCaption>基本原則</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>私権は、公共の福祉に適合しなければならない。</ParagraphSentence></Paragraph>
        <Paragraph><ParagraphNum>2</ParagraphNum><ParagraphSentence>権利の行使及び義務の履行は、信義に従い誠実に行わなければならない。</ParagraphSentence></Paragraph>
        <Paragraph><ParagraphNum>3</ParagraphNum><ParagraphSentence>権利の濫用は、これを許さない。</ParagraphSentence></Paragraph>
      </Article>
      <Article Num="2"><ArticleTitle>第二条</ArticleTitle><ArticleCaption>解釈の基準</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>この法律は、個人の尊厳と両性の本質的平等を旨として、解釈しなければならない。</ParagraphSentence></Paragraph>
      </Article>
      <Article Num="3"><ArticleTitle>第三条</ArticleTitle><ArticleCaption>権利能力</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>第一条の規定に従い、私権の享有は、出生に始まる。</ParagraphSentence></Paragraph>
        <Paragraph><ParagraphNum>2</ParagraphNum><ParagraphSentence>前条の解釈に基づき、外国人は、法令又は条約の規定により禁止される場合を除き、私権を享有する。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;

const SAMPLE_KEIHO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law>
  <LawNum>明治四十年法律第四十五号</LawNum>
  <PromulgationDate>1907-04-24</PromulgationDate>
  <LawBody>
    <LawTitle>刑法</LawTitle>
    <MainProvision>
      <Article Num="1"><ArticleTitle>第一条</ArticleTitle><ArticleCaption>国内犯</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>この法律は、日本国内において罪を犯したすべての者に適用する。</ParagraphSentence></Paragraph>
      </Article>
      <Article Num="2"><ArticleTitle>第二条</ArticleTitle><ArticleCaption>準用</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>不法行為については、民法第三条の規定を参照すること。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;

const SAMPLE_ROKIHO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law>
  <LawNum>昭和二十二年法律第四十九号</LawNum>
  <PromulgationDate>1947-04-07</PromulgationDate>
  <LawBody>
    <LawTitle>労働基準法</LawTitle>
    <MainProvision>
      <Article Num="1"><ArticleTitle>第一条</ArticleTitle><ArticleCaption>労働条件の原則</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>労働条件は、労働者が人たるに値する生活を営むための必要を充たすべきものでなければならない。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;

const SAMPLE_KAISHA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law>
  <LawNum>平成十七年法律第八十六号</LawNum>
  <PromulgationDate>2005-07-26</PromulgationDate>
  <LawBody>
    <LawTitle>会社法</LawTitle>
    <MainProvision>
      <Article Num="1"><ArticleTitle>第一条</ArticleTitle><ArticleCaption>趣旨</ArticleCaption>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>会社の設立、組織、運営及び管理については、他の法律に特別の定めがある場合を除くほか、この法律の定めるところによる。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;

const SAMPLE_CONSTITUTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law>
  <LawNum>昭和二十一年憲法</LawNum>
  <PromulgationDate>1946-11-03</PromulgationDate>
  <LawBody>
    <LawTitle>日本国憲法</LawTitle>
    <MainProvision>
      <Article Num="1"><ArticleTitle>第一条</ArticleTitle>
        <Paragraph><ParagraphNum>1</ParagraphNum><ParagraphSentence>天皇は、日本国の象徴であり日本国民統合の象徴であつて、この地位は、主権の存する日本国民の総意に基く。</ParagraphSentence></Paragraph>
      </Article>
    </MainProvision>
  </LawBody>
</Law>"#;
