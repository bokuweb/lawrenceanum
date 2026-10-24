/**
 * `lawpub` が生成する静的 JSON API を読み出す薄いクライアント。
 *
 * すべて相対パス (`./...`) で取得する — GitHub Pages のサブパス配信
 * (`https://owner.github.io/repo/`) でも、ローカル `vite dev`
 * (`http://localhost:5173/`) でも同じコードで動く。
 *
 * モック (`mock-data.ts`) はオフライン開発用のフォールバックとして残す:
 * fetch が失敗した場合のみ採用される。
 */

export type Endpoints = {
  laws: string
  updates_latest: string
  manifest: string
  health: string
}

export type IndexJson = {
  version: number
  generated_at: string
  endpoints: Endpoints
}

export type LawSummaryRaw = {
  law_id: string
  law_num: string | null
  title: string
  /** v2 meta があれば付く。UI のカテゴリ表示・絞り込み用。 */
  category?: string | null
  revisions_count?: number
  /** 本則の条数。古い index には無い。 */
  article_count?: number
  /** この法令が最後に更新された日 (最新 revision の取込日)。更新順ソート用。 */
  last_updated?: string | null
  current: string
  timeline: string
  versions: string
}

export type LawsIndex = {
  version: number
  generated_at: string
  laws: LawSummaryRaw[]
}

/** 号 (`<Item>`) と号の細分 (`<Subitem1>`…)。num は Num 属性、title は "十五" / "イ" 等。 */
export type Item = {
  num: string | null
  title: string | null
  text: string
  subitems?: Item[]
}
export type Paragraph = {
  /** `<ParagraphNum>` の表示テキスト。第 1 項や旧様式の法令では null。 */
  paragraph_no: string | null
  /** `<Paragraph Num>` 属性 ("1", "2", …)。旧 JSON には無い。 */
  paragraph_num?: string | null
  /** 項の文 + 番号付きの号 ("十五　社外取締役　…") を改行で連結したもの。 */
  text: string
  items?: Item[]
}
export type Article = {
  article_id: string
  article_no: string
  caption: string | null
  paragraphs: Paragraph[]
}

/** 別表 (`<AppdxTable>`)。表形式なら `rows`、号の列挙なら `items` に入る。 */
export type AppendixCell = { text: string; header?: boolean; rowspan?: number; colspan?: number }
export type AppendixTable = {
  /** `appdx_{index}`。search.db の article_id・ページ内アンカーと同じ値。 */
  appdx_id: string
  index: number
  title: string | null
  related_article_num: string | null
  rows?: AppendixCell[][]
  items?: { title: string | null; text: string }[]
  remarks?: string[]
}

export type LawDocumentRaw = {
  schema_version: number
  law_id: string
  law_num: string | null
  title: string
  revision_id: string | null
  promulgation_date: string | null
  effective_date: string | null
  status: 'current' | 'historical' | 'future' | string
  articles: Article[]
  /** `paragraphs` は条に分かれない附則の項 (空なら省略)。 */
  suppl_provisions?: { index: number; amend_law_num: string | null; label: string | null; articles: Article[]; paragraphs?: Paragraph[] }[]
  /** 旧 JSON には無い。空のときも省略される。 */
  appendix_tables?: AppendixTable[]
  source: { provider: string; raw_xml_sha256: string | null; fetched_at: string }
}

export type ArticleDiff = { added: string[]; removed: string[]; modified: string[] }

export type UpdateEntry = {
  law_id: string
  title: string
  change_type: 'added' | 'modified' | 'removed'
  revision_id?: string | null
  current: string
  article_diff?: ArticleDiff
}

export type UpdatesByDate = {
  date?: string
  generated_at?: string
  latest_update_date?: string
  updated_laws: UpdateEntry[]
}

export type Health = {
  ok: boolean
  generated_at: string
  latest_egov_update_date: string
  law_count: number
  file_count: number
  corpora?: Record<string, CorpusHealth>
  errors: string[]
}

export type CorpusHealth = {
  available: boolean
  count: number
  unit: 'meetings' | 'cases' | 'items' | 'bills' | 'municipalities' | 'sets' | 'datasets' | string
  latest_item_date: string | null
  collection_status?: 'success' | 'failure' | 'skipped' | string | null
  last_collection_attempt_at?: string | null
  last_collection_success_at?: string | null
}

export type VersionsJson = {
  law_id: string
  current_revision_id?: string
  versions: {
    revision_id: string
    effective_date: string | null
    promulgation_date: string | null
    /** v2 で未施行 revision のときに入る。`effective_date` は null になる。 */
    scheduled_enforcement_date?: string | null
    /** 改正を起こした法令 (= このリビジョンを生んだ親) のメタ。 */
    amendment_law_id?: string | null
    amendment_law_num?: string | null
    amendment_law_title?: string | null
    /** "1"=制定, "3"=改正, "8"=廃止 (e-Gov v2)。 */
    amendment_type?: string | null
    /** "New" (新規/全部改正) / "Partial" (一部改正)。 */
    mission?: string | null
    repeal_status?: string | null
    /** "CurrentEnforced" / "PreviousEnforced" / "UnEnforced" / "Repealed"。 */
    current_revision_status?: string | null
    /** 本文 JSON が手元にあるかどうか。false のときは path=null。 */
    body_available?: boolean
    path: string | null
    source_update_date?: string | null
  }[]
}

export type TimelineEventRaw = {
  event_id: string
  /** "enactment" (制定) / "amendment" (改正) / "repeal" (廃止) / "snapshot" (旧仕様) */
  event_type: string
  target_law_id: string
  amending_law_id?: string | null
  amending_law_num: string | null
  amending_law_title?: string | null
  promulgation_date: string | null
  effective_date: string | null
  scheduled_enforcement_date?: string | null
  enforcement_comment?: string | null
  revision_id: string | null
  source_update_date?: string | null
  status: string
  repeal_status?: string | null
  mission?: string | null
  kanpo: {
    linked: boolean
    path?: string
    confidence?: number
    match_reasons?: string[]
    /** 官報項目別 PDF の URL（改め文の出所）。 */
    pdf_url?: string
    /** 官報内の開始ページ番号。 */
    page?: number
    /** 官報から抽出・整形した改め文テキスト。 */
    amend_text?: string
    /** "prose"(散文改め文) / "shinkyu"(新旧対照表) / "unknown"。 */
    amend_format?: string
    /** 構造化した改め文。条ごとの行整列・改正後/改正前の対応を持つ（kanpo-parser crate 由来）。 */
    amend_document?: AmendDocument
  }
}

/** 改め文の構造化表現（kanpo-parser crate の Document に対応）。 */
export type AmendRun = {
  text: string
  underline?: boolean
  /** 本文の流れの中に置かれた表（あれば text は空）。段落や改正後/改正前セルの中の別表など。 */
  table?: AmendNestedTable
}
/** 結合セル（row・col の位置から rowspan × colspan マス）。 */
export type AmendCellMerge = { row: number; col: number; rowspan: number; colspan: number }
/**
 * 別表（罫線で区切られた表）。rows[r][c] がセルの Run 列。結合セルは merges に左上のマスと
 * 広がりを持ち、結合で覆われる残りのマスは空配列（描画しない）。
 */
export type AmendNestedTable = { rows: AmendRun[][][]; merges?: AmendCellMerge[] }
export type AmendShinkyuRow = {
  after: AmendRun[]
  before: AmendRun[]
  /** 別表の改正後/改正前 2D 表（あれば）。旧形式のデータ用（新しいデータは Run の table）。 */
  after_table?: AmendNestedTable
  before_table?: AmendNestedTable
}
export type AmendBlock =
  | { kind: "paragraph"; runs: AmendRun[] }
  | { kind: "shinkyu"; rows: AmendShinkyuRow[] }
export type AmendDocument = { format: string; blocks: AmendBlock[] }

/** 官報項目から引いた法令（kanpo-link の linked_laws / basis_laws）。match は timeline / title。 */
export type KanpoLawRef = { law_id: string; title: string; match?: string }
/** 官報の 1 項目（目次の 1 行）。改め文の本文は改正/廃止/制定系の標題だけに入る。 */
export type KanpoItem = {
  title: string
  page: number
  /** 項目の開始ページの PDF。 */
  pdf_url: string
  agency_hint?: string | null
  amend_text?: string
  amend_format?: string
  amend_document?: AmendDocument
  /** 改正対象の法令（先頭が主たる対象）。 */
  linked_laws?: KanpoLawRef[]
  /** 告示などの根拠法令（改正対象が引けないときだけ）。 */
  basis_laws?: KanpoLawRef[]
}
/** 官報の 1 号。issue_type は regular（本紙）/ extra（号外）/ special_extra（特別号外）など。 */
export type KanpoIssue = {
  issue_type: string
  issue_no: string
  pdf_url: string
  page_count?: number | null
  items: KanpoItem[]
}
/** `kanpo/{date}/index.json`。 */
export type KanpoDay = { date: string; generated_at?: string; issues: KanpoIssue[] }
/** `kanpo/index.json`。配信している日付の一覧（新しい順）。 */
export type KanpoDates = {
  generated_at?: string
  dates: { date: string; items: number; linked_items: number }[]
}

export type TimelineJson = {
  law_id: string
  events: TimelineEventRaw[]
}

// gzip 事前圧縮配信のフラグ。配信を圧縮した deploy では `VITE_COMPRESSED=1` を立てる。
// 立っていなければ従来どおり非圧縮 JSON を取得する (本番挙動を変えない)。
const COMPRESSED = import.meta.env.VITE_COMPRESSED === '1' || import.meta.env.VITE_COMPRESSED === 'true'

/** gzip レスポンスをブラウザ標準の DecompressionStream で展開して文字列にする (依存ゼロ)。 */
async function gunzipToText(res: Response): Promise<string> {
  if (!res.body) return await res.text()
  const stream = res.body.pipeThrough(new DecompressionStream('gzip'))
  return await new Response(stream).text()
}

async function getJson<T>(path: string): Promise<T> {
  // 相対 fetch にすることで、Pages のサブパスでも `./laws/index.json` のように
  // 解決される。`new URL(path, document.baseURI)` で base を尊重する。
  // 圧縮配信時は `${path}.gz` を優先し、無ければ非圧縮にフォールバックする。
  // ※ search.db は sql.js-httpvfs が Range アクセスするため、ここでは扱わない (圧縮対象外)。
  if (COMPRESSED) {
    const gzUrl = new URL(`${path}.gz`, document.baseURI).toString()
    const gz = await fetch(gzUrl, { cache: 'no-cache' })
    if (gz.ok) return JSON.parse(await gunzipToText(gz)) as T
  }
  const url = new URL(path, document.baseURI).toString()
  const res = await fetch(url, { cache: 'no-cache' })
  if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`)
  return (await res.json()) as T
}

// Phase 1: 任意 revision 間の構造化 diff (law-diff crate 由来)。
export type DiffTextOp =
  | { op: 'equal'; text: string }
  | { op: 'insert'; text: string }
  | { op: 'delete'; text: string }

export type ParagraphDiff =
  | { change_type: 'unchanged'; paragraph_no: string | null }
  | { change_type: 'added'; paragraph_no: string | null; text: string }
  | { change_type: 'removed'; paragraph_no: string | null; text: string }
  | {
      change_type: 'modified'
      paragraph_no: string | null
      text_diff: DiffTextOp[]
    }

export type ArticleDiff =
  | { change_type: 'unchanged'; article_id: string }
  | {
      change_type: 'added'
      article_id: string
      to: { article_no: string; caption: string | null; paragraphs: { paragraph_no: string | null; text: string }[] }
    }
  | {
      change_type: 'removed'
      article_id: string
      from: { article_no: string; caption: string | null; paragraphs: { paragraph_no: string | null; text: string }[] }
    }
  | {
      change_type: 'modified'
      article_id: string
      from: { article_no: string; caption: string | null }
      to: { article_no: string; caption: string | null }
      paragraphs: ParagraphDiff[]
    }

export type LawDiff = {
  schema_version: number
  law_id: string
  from: { revision_id: string | null; effective_date: string | null; promulgation_date: string | null }
  to: { revision_id: string | null; effective_date: string | null; promulgation_date: string | null }
  summary: {
    articles_added: number
    articles_removed: number
    articles_modified: number
    articles_unchanged: number
  }
  articles: ArticleDiff[]
}

export type DiffsIndex = {
  law_id: string
  diffs: {
    from_revision_id: string
    to_revision_id: string
    from_effective_date: string | null
    to_effective_date: string | null
    path: string
    summary: LawDiff['summary']
  }[]
}

export type SnapshotResolved = {
  law_id: string
  as_of: string
  include_unenforced: boolean
  resolved_revision_id: string | null
  effective_date?: string | null
  promulgation_date?: string | null
  body_available?: boolean
  current?: string | null
  status?: string
}

// NOTE: 旧 `fetchHistory` (history.ndjson.zst を fzstd でブラウザ展開) は廃止した。
// 大法令だと展開後 ~200MB に膨らみ fzstd が大窓 LDM フレームを誤復号して数秒固まった
// 上に本文が壊れ、比較が成立しなかった。compare view は versions.json + 個別 revision
// (api.revision) の on-demand 取得に切り替えている。

// LLM wiki (OKF)。`lawpub wiki-export` が `wiki` ブランチから生成する。
export type WikiPageMeta = {
  /** wiki ルート相対のパス (拡張子なし)。例: `laws/323AC0000000131` */
  path: string
  /** OKF の type: `law` / `meeting` / `person` / `topic` / `index` / `log` */
  type: string
  title: string
  description: string
  date: string | null
  tags: string[]
  /** LLM の文章がまだ無く、機械が作った一覧しかないページ (一覧・グラフ・導線では出さない)。 */
  stub?: boolean
}
export type WikiIndex = { schema_version: number; generated_at: string; pages: WikiPageMeta[]; intake?: { date: string; since: string; scope: string; entries: Record<string, string> } | null }
export type WikiGraph = {
  schema_version: number
  nodes: { id: string; type: string; title: string; description: string; stub?: boolean }[]
  links: { source: string; target: string }[]
}
export type WikiClaim = {
  text: string; kind: string; attribution: string; review_status: string; review_note: string;
  sources: { ref: string; url: string; quote: string; speaker: string | null; group: string | null; position: string | null; date: string; role: string }[];
}
export type WikiCitationSource = {
  path: string; refs: string[]; title: string | null; date: string | null;
  availability: "available" | "stub" | "missing";
  /** 引用元会議の要約の状態。このページの記述がその版に対応するとは限らない。 */
  summary_source_status: "current" | "stale" | "pending" | "untracked" | null;
}
export type HanreiCitation = { label: string; reference: string; field: "holding" | "summary" | "body_text"; quote: string };
export type WikiRelatedCase = {
  path: string; case_id: number; title: string; judged_on: string; court: string; description: string;
  common_articles: { law_id: string; title: string; article: string }[];
}
export type WikiPage = { path: string; frontmatter: Record<string, unknown>; body: string; hanrei_explanation?: string | null; hanrei_citations?: HanreiCitation[] | null; hanrei_related_cases?: WikiRelatedCase[] | null; stub?: boolean; claim_assessments?: WikiClaim[]; citation_sources?: WikiCitationSource[] }

export const api = {
  index: () => getJson<IndexJson>('./index.json'),
  wikiIndex: () => getJson<WikiIndex>('./wiki/index.json'),
  wikiGraph: () => getJson<WikiGraph>('./wiki/graph.json'),
  wikiPage: (path: string) =>
    getJson<WikiPage>(`./wiki/page/${path.split('/').map(encodeURIComponent).join('/')}.json`),
  health: () => getJson<Health>('./health.json'),
  lawsIndex: () => getJson<LawsIndex>('./laws/index.json'),
  law: (lawId: string) => getJson<LawDocumentRaw>(`./laws/${lawId}/current.json`),
  versions: (lawId: string) => getJson<VersionsJson>(`./laws/${lawId}/versions.json`),
  timeline: (lawId: string) => getJson<TimelineJson>(`./laws/${lawId}/timeline.json`),
  revision: (lawId: string, revId: string) =>
    getJson<LawDocumentRaw>(`./laws/${lawId}/revisions/${revId}.json`),
  latestUpdates: () => getJson<UpdatesByDate>('./updates/latest.json'),
  updatesOnDate: (date: string) => getJson<UpdatesByDate>(`./updates/${date}.json`),
  kanpoDates: () => getJson<KanpoDates>('./kanpo/index.json'),
  kanpoOnDate: (date: string) => getJson<KanpoDay>(`./kanpo/${date}/index.json`),
  /** 隣接 revision 間 diff の索引。 */
  diffsIndex: (lawId: string) => getJson<DiffsIndex>(`./laws/${lawId}/diffs.json`),
  /** 特定の from..to の構造化 diff。 */
  diff: (lawId: string, fromRev: string, toRev: string) =>
    getJson<LawDiff>(`./laws/${lawId}/diff/${fromRev}..${toRev}.json`),
  /** 任意日付スナップショット (resolved revision を返すリダイレクト JSON)。 */
  snapshotAt: (lawId: string, date: string) =>
    getJson<SnapshotResolved>(`./laws/${lawId}/at/${date}.json`),

  /** 国会会議録インデックス。 */
  proceedingsIndex: () => getJson<ProceedingsIndex>('./proceedings/index.json'),
  /** 個別会議（発言全文）。 */
  meeting: (meetingId: string) => getJson<Meeting>(`./proceedings/${meetingId}.json`),
  /** 法令 ↔ 国会会議録 クロスリンク。 */
  lawToProceedings: (lawId: string) => getJson<LawToProceedings>(`./links/law-to-proceedings/${lawId}.json`),
  /** 会議 → 言及法令 逆引きリンク。 */
  meetingToLaws: (meetingId: string) => getJson<MeetingToLaws>(`./links/meeting-to-laws/${meetingId}.json`),

  /** パブリックコメント案件インデックス。 */
  pubcommentIndex: () => getJson<PubcommentIndex>('./pubcomment/index.json'),
  /** 個別パブコメ案件（意見と府省の考え方を含む）。 */
  pubcommentCase: (caseId: string) => getJson<PubcommentCase>(`./pubcomment/${encodeURIComponent(caseId)}.json`),
  /** 法令 ↔ パブコメ クロスリンク。 */
  lawToPubcomment: (lawId: string) => getJson<LawToPubcomments>(`./links/law-to-pubcomment/${lawId}.json`),

  /** 規制変化フィード (法令改正・パブコメ・官報の新着, 逆時系列)。 */
  recentFeed: () => getJson<RecentFeed>('./feeds/recent.json'),

  /** 施行予定ごとの出典付き wiki 解説。 */
  enforcementExplanations: (lawId: string) =>
    getJson<EnforcementExplanations>(`./wiki/enforcements/${encodeURIComponent(lawId)}.json`),
  /** 今後の施行予定 (施行期日カレンダー, 施行日昇順)。 */
  enforcementUpcoming: () => getJson<EnforcementUpcoming>('./enforcement/upcoming.json'),

  /** 通達 (soft law) 集インデックス。 */
  tsutatsuIndex: () => getJson<TsutatsuIndex>('./tsutatsu/index.json'),
  /** 個別の通達集 (項目一覧つき)。 */
  tsutatsuSet: (tax: string) => getJson<TsutatsuSet>(`./tsutatsu/${tax}.json`),
  /** 法令 ↔ 通達 クロスリンク (通達集の親法令単位)。 */
  lawToTsutatsu: (lawId: string) => getJson<LawToTsutatsu>(`./links/law-to-tsutatsu/${lawId}.json`),

  /** 法令の制定経緯 (パブコメ → 結果公示 → 公布 → 施行) の連鎖。 */
  lawToRulemaking: (lawId: string) => getJson<LawToRulemaking>(`./links/law-to-rulemaking/${lawId}.json`),
  /** 附則の構造化 (施行期日・経過措置・読替え規定)。附則の無い法令には無い。 */
  lawSupplementary: (lawId: string) => getJson<LawSupplementary>(`./laws/${lawId}/supplementary.json`),
  hanreiIndex: () => getJson<HanreiIndex>('./hanrei/index.json'),
  hanreiLawLinks: (caseId: string | number) => getJson<HanreiLawLinks>(`./links/hanrei-to-laws/${encodeURIComponent(caseId)}.json`),
  hanreiCase: (caseId: string | number) => getJson<HanreiCase>(`./hanrei/${encodeURIComponent(caseId)}.json`),
  /** 法令 ↔ 判例 クロスリンク (判例の参照法条から抽出)。 */
  lawToHanrei: (lawId: string) => getJson<LawToHanrei>(`./links/law-to-hanrei/${lawId}.json`),
  /** 法令 ↔ 質問主意書・答弁書 クロスリンク (件名・本文の法令参照から抽出)。 */
  lawToShitsumon: (lawId: string) => getJson<LawToShitsumon>(`./links/law-to-shitsumon/${lawId}.json`),
  /** 法令 ↔ 政府解釈 (ノーアクションレター・Q&A・ガイドライン等) クロスリンク。 */
  lawToGuidance: (lawId: string) => getJson<LawToGuidance>(`./links/law-to-guidance/${lawId}.json`),
  /** 法令英訳 (日本法令外国語訳データベース)。英訳の無い法令は 404。 */
  lawEnglish: (lawId: string) => getJson<LawEnglish>(`./laws/${lawId}/en.json`),
  /** 法令 → 条約 クロスリンク (法令本文の条約名・条約番号から抽出)。 */
  lawToTreaties: (lawId: string) => getJson<LawToTreaties>(`./links/law-to-treaties/${lawId}.json`),
  /** 条約 1 件 (外務省 条約データ検索・国会提出条約)。 */
  treaty: (id: string) => getJson<Treaty>(`./treaties/${encodeURIComponent(id)}.json`),
  /** 法令 → この法令を改正した法律案の改め文 (law-diff の版と対応づけ済み)。 */
  lawToBillAmendments: (lawId: string) =>
    getJson<LawToBillAmendments>(`./links/law-to-bill-amendments/${lawId}.json`),

  /** 議案 (法案審議トラッキング) 全体インデックス。 */
  gianIndex: () => getJson<GianIndex>('./gian/index.json'),
  /** 個別議案の審議経過。 */
  gianBill: (session: string, billId: string) =>
    getJson<GianBill>(`./gian/${session}/${billId}.json`),

  /** 自治体例規: 収集済み自治体の一覧。 */
  reikiIndex: () => getJson<ReikiIndex>(`${REIKI_BASE}/index.json`),
  /** 自治体例規: 1 自治体の例規一覧。 */
  reikiMunicipality: (code: string) => getJson<ReikiMunicipalityIndex>(`${REIKI_BASE}/${code}/index.json`),
  /** 自治体例規: 本文（条・項・号・附則）。 */
  reikiDoc: (code: string, reikiId: string) =>
    getJson<ReikiDocument>(`${REIKI_BASE}/${code}/${encodeURIComponent(reikiId)}.json`),

  /** 政府調達 (官公需情報ポータル) 公告インデックス。 */
  procurementIndex: () => getJson<ProcurementIndex>('./procurement/index.json'),
  /** 個別の調達公告。item_id は base64url なのでそのままパスに使える。 */
  procurementItem: (itemId: string) => getJson<ProcurementItem>(`./procurement/${itemId}.json`),

  /** 審議会議事録 全府省インデックス (開催日降順)。 */
  shingikaiIndex: () => getJson<ShingikaiIndex>('./shingikai/index.json'),
  /** 個別の審議会会議 (議題・議事録全文・配布資料)。 */
  shingikaiMeeting: (ministry: string, minutesId: string) =>
    getJson<ShingikaiMeeting>(`./shingikai/${ministry}/${minutesId}.json`),

  /** 財政統計 (e-Stat) データセット一覧。 */
  budgetIndex: () => getJson<BudgetIndex>('./budget/index.json'),
  /** 個別の統計表 (値の全件)。 */
  budgetDataset: (statsId: string) => getJson<BudgetDataset>(`./budget/${statsId}.json`),
}

// ── 自治体例規 型定義 ───────────────────────────────────────────

// 全国の例規は Pages に収まらないため R2 から配信する (`VITE_REIKI_BASE_URL`)。
// 未設定時は同 origin の ./reiki（ローカル開発・e2e フィクスチャ）。
const REIKI_BASE: string = (import.meta.env.VITE_REIKI_BASE_URL || './reiki').replace(/\/$/, '')

export type ReikiMunicipalitySummary = {
  municipality_code: string
  name: string
  prefecture: string
  count: number
  current_as_of: string | null
  updated_at: string | null
  vendor: string | null
  source_url: string | null
}

export type ReikiIndex = {
  schema_version: number
  generated_at: string
  /** 収集済み自治体数 */
  count: number
  reiki_count: number
  /** 収集対象として登録済みの自治体数 */
  registered_count: number
  unsupported_count: number
  municipalities: ReikiMunicipalitySummary[]
}

export type ReikiMeta = {
  reiki_id: string
  title: string
  reiki_number: string | null
  kind: string | null
  promulgated_date: string | null
  article_count: number
}

export type ReikiMunicipalityIndex = {
  schema_version: number
  municipality_code: string
  name: string
  prefecture: string
  current_as_of: string | null
  count: number
  reiki: ReikiMeta[]
}

export type ReikiItem = { num: string; text: string; subitems?: ReikiItem[] }
export type ReikiParagraph = { num: string | null; caption?: string | null; text: string; items?: ReikiItem[] }
export type ReikiArticle = {
  article_no: string
  article_id: string
  caption: string | null
  heading?: string | null
  paragraphs: ReikiParagraph[]
}

export type ReikiDocument = {
  schema_version: number
  reiki_id: string
  municipality_code: string
  municipality_name: string
  prefecture: string
  title: string
  reiki_number: string | null
  kind: string | null
  promulgated_date: string | null
  current_as_of: string | null
  preamble?: string[]
  articles: ReikiArticle[]
  supplementary?: { title: string; articles: ReikiArticle[] }[]
  content_sha256: string
  source: {
    provider: string
    fetched_at: string
    checked_at?: string
    detail_url: string
    municipality_official_site: string
  }
}

// ── 政府調達 型定義 ─────────────────────────────────────────────

export type ProcurementItemMeta = {
  item_id: string
  title: string
  organization: string | null
  notice_type: string | null
  publish_date: string | null
}

export type ProcurementIndex = {
  schema_version: number
  count: number
  items: ProcurementItemMeta[]
}

export type ProcurementItem = ProcurementItemMeta & {
  schema_version: number
  deadline: string | null
  contract_amount: number | string | null
  contractor: string | null
  contract_date: string | null
  detail_url: string | null
  source: { provider: string; fetched_at: string }
}

// ── 審議会議事録 型定義 ─────────────────────────────────────────

export type ShingikaiMeetingMeta = {
  minutes_id: string
  ministry: string
  committee_id: string
  committee: string
  date: string | null
  status: 'held' | 'scheduled' | string
  title: string
  attachment_count: number
  has_minutes: boolean
  detail_url: string | null
}

export type ShingikaiIndex = {
  schema_version: number
  count: number
  minutes: ShingikaiMeetingMeta[]
}

export type ShingikaiAttachment = {
  attachment_id: string
  /** `minutes_text` / `minutes_pdf` / `material` */
  kind: string
  label: string
  source_url: string
  media_type?: string
  bytes?: number
  extracted_text?: string
}

export type ShingikaiMeeting = {
  schema_version: number
  minutes_id: string
  ministry: string
  committee_id: string
  committee: string
  date: string | null
  status: 'held' | 'scheduled' | string
  title: string
  agenda: string | null
  summary: string | null
  body_text: string
  minutes_text?: string
  attachments: ShingikaiAttachment[]
  source: { provider: string; fetched_at: string; detail_url: string }
}

// ── 財政統計 型定義 ─────────────────────────────────────────────

export type BudgetIndex = {
  schema_version: number
  count: number
  datasets: { stats_data_id: string; title: string; value_count: number }[]
}

export type BudgetValue = {
  area: string | null
  time: string | null
  category: string | null
  dimensions: Record<string, string>
  value: string
  unit: string | null
}

export type BudgetDataset = {
  schema_version: number
  stats_data_id: string
  title: string
  values: BudgetValue[]
  source: { provider: string; fetched_at: string; stats_data_id: string }
}

// ── 議案 (法案審議トラッキング) 型定義 ─────────────────────────────

export type GianBillMeta = {
  bill_id: string
  session: number | string
  bill_type?: string | null
  number?: string | null
  title: string
  committee?: string | null
  result?: string | null
  status?: string | null
  promulgation_date?: string | null
  latest_date?: string | null
  latest_event?: string | null
  detail_url?: string | null
}

export type GianIndex = {
  schema_version: number
  count: number
  bills: GianBillMeta[]
}

export type GianBill = {
  schema_version: number
  bill_id: string
  session: number
  bill_type?: string | null
  number?: string | null
  title: string
  submitter?: string | null
  parties?: string | null
  committee?: string | null
  result?: string | null
  promulgation_date?: string | null
  law_num?: string | null
  latest_date?: string | null
  latest_event?: string | null
  status?: string | null
  fields: { key: string; value: string }[]
  source: { provider: string; fetched_at: string; detail_url: string }
}

// ── 施行予定 型定義 ───────────────────────────────────────────────

export type EnforcementExplanation = {
  explanation_id: string
  event_id: string
  revision_id: string | null
  date: string
  path: string
  description: string
  body: string
}
export type EnforcementExplanations = {
  schema_version: number
  law_id: string
  items: EnforcementExplanation[]
}

export type EnforcementItem = {
  event_id?: string
  revision_id?: string | null
  explanation_id?: string
  date: string
  law_id: string
  title: string
  amending_law_title?: string
  /** "scheduled"(施行予定日) / "effective"(施行日) / "supplementary"(附則から読んだ一部施行等)。 */
  date_kind: string
  /** date_kind = "supplementary" のときの対象規定。 */
  targets?: string
}

export type EnforcementUpcoming = {
  schema_version: number
  generated_at: string
  as_of: string
  count: number
  items: EnforcementItem[]
}

// ── 通達 (soft law) 型定義 ────────────────────────────────────────

export type TsutatsuItem = {
  tax: string
  /** 通達番号 (例: "2-5")。 */
  number: string
  caption?: string | null
  text: string
  source_url: string
}

export type TsutatsuSet = {
  schema_version: number
  name: string
  tax: string
  parent_law_id?: string | null
  parent_law_title?: string | null
  items: TsutatsuItem[]
  source: { provider: string; fetched_at: string; index_url: string }
}

export type TsutatsuIndexEntry = {
  tax: string
  name: string
  count: number
  parent_law_id?: string | null
  parent_law_title?: string | null
}
export type TsutatsuIndex = {
  schema_version: number
  count: number
  sets: TsutatsuIndexEntry[]
}

/** 法令 → 通達 クロスリンク (通達集の親法令単位)。 */
export type LinkedTsutatsu = {
  tax: string
  name: string
  count: number
  relevance: string
  confidence: number
}
export type LawToTsutatsu = {
  schema_version: number
  law_id: string
  linked_tsutatsu: LinkedTsutatsu[]
}

/** 法令の制定経緯。`steps` は日付昇順。`resolved=false` は公布イベントと突合できなかったパブコメ。 */
export type RulemakingStep = {
  /** pubcomment_start | pubcomment_end | pubcomment_result | promulgation | effective */
  kind: string
  date: string
  label: string
  ref_id: string
}
export type RulemakingChain = {
  chain_id: string
  confidence: number
  match_reasons: string[]
  steps: RulemakingStep[]
  pubcomment_case_ids: string[]
  resolved: boolean
  kanpo_pdf_url?: string | null
}
export type LawToRulemaking = {
  schema_version: number
  law_id: string
  chains: RulemakingChain[]
}

/** 附則の参照 (第X条)。`article_id` は同じ法令内の条に解決できたときだけ。 */
export type SupplementaryRef = {
  text: string
  scope: 'main' | 'suppl' | 'amending_law' | 'other_law'
  article_id?: string | null
}
export type SupplementaryLocation = { article_id: string | null; paragraph: string | null; item?: string | null }
export type EnforcementKind = 'fixed_date' | 'promulgation' | 'after_promulgation' | 'delegated' | 'linked' | 'unknown'
export type SupplementaryEnforcement = {
  location: SupplementaryLocation
  /** whole = 附則を置いた法令全体, partial = ただし書・各号などの一部の規定 */
  scope: 'whole' | 'partial'
  targets?: string | null
  excluding?: string | null
  target_refs?: SupplementaryRef[]
  kind: EnforcementKind
  date: string | null
  date_source?: 'explicit' | 'promulgation' | 'computed' | null
  /** 政令委任の期限 (この日までに施行される) */
  deadline?: string | null
  delegated_to?: string | null
  linked_to?: string | null
  period?: string | null
  applies_from?: string | null
  when: string
  text: string
}
export type TransitionalKind = 'prior_law' | 'penalty' | 'continued_effect' | 'deemed' | 'application' | 'delegated' | 'review' | 'repeal' | 'other'
export type SupplementaryTransitional = {
  location: SupplementaryLocation
  kinds: TransitionalKind[]
  caption?: string | null
  refs?: SupplementaryRef[]
  text: string
}
export type SupplementaryReading = {
  location: SupplementaryLocation
  target: string | null
  target_refs?: SupplementaryRef[]
  from: string
  to: string
  context?: string | null
}
export type SupplementaryProvisionAnalysis = {
  index: number
  amend_law_num: string | null
  label: string | null
  promulgation_date: string | null
  enforcement?: SupplementaryEnforcement[]
  transitional?: SupplementaryTransitional[]
  readings?: SupplementaryReading[]
  enforcement_unresolved: boolean
}
export type LawSupplementary = {
  schema_version: number
  law_id: string
  law_num: string | null
  stats: {
    provisions: number
    with_enforcement: number
    enforcement_unresolved: number
    enforcement_rules: number
    transitional_measures: number
    readings: number
    refs: number
    refs_resolved: number
  }
  provisions: SupplementaryProvisionAnalysis[]
}

export type HanreiSummary = {
  case_id: number
  case_number: string
  case_name: string
  judged_on: string
  court: string
  result: string
  holding: string
}
export type HanreiIndex = {
  source: string
  count: number
  cases: HanreiSummary[]
}
export type HanreiCase = HanreiSummary & {
  judgment_type: string
  reporter: string
  lower_court: string
  lower_case_number: string
  lower_judged_on: string
  summary: string
  referenced_laws: string
  body_citations?: string[]
  /** undefined/null = 未取得、空文字 = PDF に抽出可能な文字情報なし。 */
  body_text?: string | null
  detail_url: string
  pdf_url: string | null
}

/** 法令 → 判例 クロスリンク。出典は裁判所 裁判例検索 (`/hanrei/{case_id}/detail2/`)。 */
export type LinkedCase = {
  case_id: number
  case_number: string
  case_name: string
  judged_on: string
  court: string
  holding: string
  /** 参照された条番号 (例: "709", "5_2" は第5条の2)。 */
  articles: string[]
}
export type LawToHanrei = {
  schema_version: number
  law_id: string
  linked_cases: LinkedCase[]
}
export const hanreiUrl = (caseId: number) =>
  `https://www.courts.go.jp/hanrei/${caseId}/detail2/index.html`

/** 法令 → 質問主意書・答弁書 クロスリンク。出典は衆議院「質問答弁情報」/ 参議院「質問主意書」。 */
export type LinkedQuestion = {
  /** `{house}-{session}-{number}` (例: `shugiin-219-1`)。 */
  id: string
  house: 'shugiin' | 'sangiin' | string
  session: number
  number: number
  title: string
  submitter: string
  submitted_on: string | null
  answered_on: string | null
  /** 件名に法令名が出てくる (その法令を主題にした質問)。 */
  in_title: boolean
  /** 参照された条番号 (例: "74", "5_2" は第5条の2)。条を挙げない言及だけなら空。 */
  articles: string[]
  /** 衆議院は経過ページ、参議院は明細ページ。 */
  detail_url: string
}
export type LawToShitsumon = {
  schema_version: number
  law_id: string
  linked_questions: LinkedQuestion[]
}

/** 政府解釈の種別。 */
export type GuidanceKind =
  | 'no_action_letter'
  | 'written_inquiry'
  | 'consultation_cases'
  | 'qa'
  | 'guideline'
  | 'interpretation_cases'
  | 'other'

export const GUIDANCE_KIND_LABELS: Record<string, string> = {
  no_action_letter: 'ノーアクションレター',
  written_inquiry: '書面照会への回答',
  consultation_cases: '相談事例',
  qa: 'Q&A',
  guideline: 'ガイドライン',
  interpretation_cases: '法令解釈事例集',
  other: 'その他',
}

/** 法令 → 政府解釈 クロスリンク。出典は所管省庁のウェブサイト (金融庁: PDL1.0)。 */
export type LinkedGuidance = {
  /** 例: `fsa-nal-ginkou-10`。 */
  id: string
  ministry: string
  ministry_name: string
  kind: GuidanceKind | string
  title: string
  issued_on: string | null
  /** 発出元 (例: 金融庁監督局 銀行第一課長)。 */
  issuer: string | null
  /** 回答の要旨 (回答書の「１．回答」の冒頭)。 */
  summary: string | null
  /** subject = 照会対象・分野の法令、title = 文書名に出る、mention = 本文で条番号付きで参照。 */
  relation: 'subject' | 'title' | 'mention'
  /** 参照された条番号 (例: "10", "52_36" は第52条の36)。 */
  articles: string[]
  /** 一覧ページ (出典)。 */
  source_url: string
  /** 回答書・本文 PDF の URL。 */
  document_url: string | null
}
export type LawToGuidance = {
  schema_version: number
  law_id: string
  linked_guidance: LinkedGuidance[]
}

/**
 * 法令英訳 (`laws/{law_id}/en.json`)。出典は法務省「日本法令外国語訳データベース」(PDL1.0)。
 * 翻訳は参考訳で、最終改正を反映していないことがある (`translations[].last_version_*`)。
 */
export type LawEnglishParagraph = {
  /** 英訳の項番号 (例: "(2)")。 */
  num: string | null
  /** 号・号の細分は "(i) …" の行として改行で続く。 */
  text: string
}
export type LawEnglishArticle = {
  /** e-Gov の条 ID (`art_3_2` 等)。current.json の articles[].article_id と対応する。 */
  article_id: string
  article_no: string | null
  /** 例: "Article 3-2"。 */
  title: string | null
  caption: string | null
  paragraphs: LawEnglishParagraph[]
  jlt_id: number
  /** JLT の該当条へのリンク。 */
  url: string
}
export type LawEnglishTranslation = {
  jlt_id: number
  title_ja: string
  title_en: string | null
  law_num_en: string | null
  last_version_ja: string | null
  last_version_en: string | null
  translated_on: string | null
  dictionary_version: string | null
  /** 暫定版 (ネイティブ・専門家チェック前)。 */
  tentative: boolean
  source_url: string
  article_count: number
  matched_article_count: number
}
export type LawEnglish = {
  schema_version: number
  law_id: string
  law_num: string | null
  title: string
  title_en: string | null
  source: {
    name_ja: string
    name_en: string
    url: string
    terms_url: string
    license: string
    license_url: string
    attribution: string
    disclaimer_ja: string
    disclaimer_en: string
  }
  translations: LawEnglishTranslation[]
  articles: LawEnglishArticle[]
}

/** 法令 → 条約 クロスリンク。出典は外務省「条約データ検索」「国会提出条約」。 */
export type LinkedTreaty = {
  /** 条約データ検索のファイル名 (例: `A-R4-101`)。未掲載で国会提出条約ページだけのものは `diet-{ページ名}`。 */
  treaty_id: string
  title: string
  short_title: string | null
  kind: 'bilateral' | 'multilateral' | null
  effective_on: string | null
  /** 「平成26年条約第2号」。 */
  treaty_number_label: string | null
  /** 参照した条 (`art_2`、附則は `suppl1_art_1` / `suppl1`)。 */
  articles: string[]
  /** 法令の題名に条約名が入っている (実施法など)。 */
  in_title: boolean
  matched_by: string[]
  /** 同名の条約が複数あり、条約番号で決められなかった (最新のものを採った)。 */
  ambiguous: boolean
  source_url: string | null
}
export type LawToTreaties = {
  schema_version: number
  law_id: string
  linked_treaties: LinkedTreaty[]
}
export type TreatyEvent = { date: string | null; text: string }
export type Treaty = {
  schema_version: number
  id: string
  title: string
  short_title: string | null
  kind: 'bilateral' | 'multilateral' | null
  parties: string[]
  signed_on: string | null
  diet_approved_on: string | null
  concluded_on: string | null
  promulgated_on: string | null
  effective_on: string | null
  treaty_number: number | null
  treaty_number_label: string | null
  notice_number: string | null
  diet_session: number | null
  events: TreatyEvent[]
  in_database: boolean
  text: string | null
  text_lang: string | null
  text_source: string | null
  text_url: string | null
  attachments: { label: string; url: string; sha256?: string; bytes?: number }[]
  source_url: string
  diet_url: string | null
}

// ── 法律案の改め文 ↔ 改正差分 型定義 ──────────────────────────────

export type LinkedAmendmentSentence = {
  /** 改め文 1 文の原文 (「第二条中「A」を「B」に改める。」)。 */
  text: string
  /** 改め文が指す条 (`art_*`)。linked なら差分で実際に変化した条。 */
  article_ids: string[]
  /** verified / article / pending / unmatched / skipped、または法令単位の status。 */
  status: string
}

export type LinkedBillAmendment = {
  bill_id: string
  session: number
  bill_title: string
  law_num: string | null
  amendment_law_id: string | null
  /** 法案内の位置 (「第一条」「附則第六条」)。 */
  provision: string | null
  /** linked / pending / not_promulgated / revision_not_found。 */
  status: string
  from_revision_id: string | null
  to_revision_ids: string[]
  source_url: string
  path: string
  ops: number
  ops_matched: number
  sentences: LinkedAmendmentSentence[]
}

export type LawToBillAmendments = {
  schema_version: number
  law_id: string
  bills: LinkedBillAmendment[]
}

// ── 規制変化フィード 型定義 ───────────────────────────────────────

export type FeedItem = {
  kind: 'law' | 'bill' | 'pubcomment' | 'kanpo' | string
  date: string
  title: string
  /** アプリ内ルート ("/laws/..") か外部URL (官報PDF)。 */
  href: string
  internal: boolean
  law_id?: string
  /** 逆引き: 官報項目が改正する対象法令名 (あれば)。 */
  law_title?: string
  ministry?: string
  summary?: string
}

export type RecentFeed = {
  schema_version: number
  generated_at: string
  count: number
  items: FeedItem[]
}

// ── 国会会議録 型定義 ──────────────────────────────────────────────

export type MeetingMeta = {
  meeting_id: string
  session: number
  house: string
  committee: string | null
  date: string
  issue: string | null
  speech_count: number
}

export type ProceedingsIndex = {
  schema_version: number
  count: number
  meetings: MeetingMeta[]
}

export type Speech = {
  speech_id: string
  order: number
  speaker: string | null
  speaker_id: string | null
  speaker_group: string | null
  speaker_position: string | null
  speech: string
}

export type Meeting = {
  schema_version: number
  meeting_id: string
  session: number
  house: string
  committee: string | null
  date: string
  issue: string | null
  speeches: Speech[]
  source: { provider: string; fetched_at: string }
}

export type LinkedLaw = {
  law_id: string
  title: string
  relevance: string
  confidence: number
  match_reasons: string[]
}

export type MeetingToLaws = {
  schema_version: number
  meeting_id: string
  linked_laws: LinkedLaw[]
}

export type LinkedProceeding = {
  meeting_id: string
  date: string
  house: string
  committee: string | null
  relevance: string
  speech_count_mentioning: number
  confidence: number
  match_reasons: string[]
}

export type LawToProceedings = {
  schema_version: number
  law_id: string
  linked_proceedings: LinkedProceeding[]
}

// ── パブリックコメント 型定義 ──────────────────────────────────────

export type PubcommentCaseMeta = {
  case_id: string
  title: string
  ministry: string | null
  result_published: string | null
  /** 受付締切日時（意見募集中のとき）。 */
  reception_end?: string | null
  /** "open"(意見募集中) / "closed"(結果公示済み)。 */
  status?: string | null
  related_law_name: string | null
  category?: string | null
  responsible_office?: string | null
  opinion_count?: number | null
}

export type PubcommentIndex = {
  schema_version: number
  count: number
  cases: PubcommentCaseMeta[]
}

export type OpinionSummary = {
  item: string
  opinion: string
  ministry_response: string
}

export type PubcommentAttachment = {
  name: string
  url: string
  media_type?: string | null
  filename?: string | null
  sha256?: string | null
  bytes?: number | null
  /** 添付 PDF 等から機械抽出した全文。意見ペアに未構造化でも閲覧・検索できる。 */
  extracted_text?: string | null
  extraction_method?: string | null
  extraction_error?: string | null
}

export type PubcommentCase = {
  schema_version: number
  case_id: string
  title: string
  ministry: string | null
  reception_start: string | null
  reception_end: string | null
  result_published: string | null
  related_law_name: string | null
  category?: string | null
  command_title?: string | null
  legal_basis?: string | null
  responsible_office?: string | null
  opinion_count?: number | null
  /** "open"(意見募集中) / "closed"(結果公示済み)。 */
  status?: string | null
  opinions: OpinionSummary[]
  attachments?: PubcommentAttachment[]
  source: { provider: string; fetched_at: string; detail_url: string }
}

export type LinkedPubcomment = {
  case_id: string
  title: string
  ministry: string
  start_date: string
  end_date: string
  relevance: string
  confidence: number
  match_reasons: string[]
}

export type LawToPubcomments = {
  schema_version: number
  law_id: string
  linked_pubcomments: LinkedPubcomment[]
}

/** Shared version candidates used by the case view and the Wiki planner. Offsets are UTF-16. */
export type CaseLawCitation = { field: string; law_id: string; article: string; paragraph: number | null; start: number; end: number; text: string };
export type CaseLawVersion = { revision_id: string; effective_date: string; body_available: boolean };
export type CaseLawReference = {
  law_id: string; title: string; articles: string[]; wiki_path: string;
  basis: string; evidence: { from: string; to: string; text: string }[];
  versions: CaseLawVersion[];
  reference_versions?: CaseLawVersion[];
  incomplete: boolean;
};
export type HanreiLawLinks = {
  schema_version: number; case_id: number; notice: string; wiki_path: string;
  citations: CaseLawCitation[]; laws: CaseLawReference[];
};
