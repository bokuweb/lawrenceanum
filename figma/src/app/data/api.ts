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

export type Paragraph = { paragraph_no: string | null; text: string }
export type Article = {
  article_id: string
  article_no: string
  caption: string | null
  paragraphs: Paragraph[]
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
  errors: string[]
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
    /** 構造化した改め文。条ごとの行整列・改正後/改正前の対応を持つ（kanpo-amend crate 由来）。 */
    amend_document?: AmendDocument
  }
}

/** 改め文の構造化表現（kanpo-amend crate の Document に対応）。 */
export type AmendRun = { text: string; underline?: boolean }
/** 別表（罫線で区切られた表）。rows[r][c] がセルの Run 列。 */
export type AmendNestedTable = { rows: AmendRun[][][] }
export type AmendShinkyuRow = {
  after: AmendRun[]
  before: AmendRun[]
  /** 別表の改正後/改正前 2D 表（あれば）。 */
  after_table?: AmendNestedTable
  before_table?: AmendNestedTable
}
export type AmendBlock =
  | { kind: "paragraph"; runs: AmendRun[] }
  | { kind: "shinkyu"; rows: AmendShinkyuRow[] }
export type AmendDocument = { format: string; blocks: AmendBlock[] }

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

export const api = {
  index: () => getJson<IndexJson>('./index.json'),
  health: () => getJson<Health>('./health.json'),
  lawsIndex: () => getJson<LawsIndex>('./laws/index.json'),
  law: (lawId: string) => getJson<LawDocumentRaw>(`./laws/${lawId}/current.json`),
  versions: (lawId: string) => getJson<VersionsJson>(`./laws/${lawId}/versions.json`),
  timeline: (lawId: string) => getJson<TimelineJson>(`./laws/${lawId}/timeline.json`),
  revision: (lawId: string, revId: string) =>
    getJson<LawDocumentRaw>(`./laws/${lawId}/revisions/${revId}.json`),
  latestUpdates: () => getJson<UpdatesByDate>('./updates/latest.json'),
  updatesOnDate: (date: string) => getJson<UpdatesByDate>(`./updates/${date}.json`),
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

  /** 今後の施行予定 (施行期日カレンダー, 施行日昇順)。 */
  enforcementUpcoming: () => getJson<EnforcementUpcoming>('./enforcement/upcoming.json'),

  /** 通達 (soft law) 集インデックス。 */
  tsutatsuIndex: () => getJson<TsutatsuIndex>('./tsutatsu/index.json'),
  /** 個別の通達集 (項目一覧つき)。 */
  tsutatsuSet: (tax: string) => getJson<TsutatsuSet>(`./tsutatsu/${tax}.json`),

  /** 議案 (法案審議トラッキング) 全体インデックス。 */
  gianIndex: () => getJson<GianIndex>('./gian/index.json'),
  /** 個別議案の審議経過。 */
  gianBill: (session: string, billId: string) =>
    getJson<GianBill>(`./gian/${session}/${billId}.json`),
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

export type EnforcementItem = {
  date: string
  law_id: string
  title: string
  amending_law_title?: string
  /** "scheduled"(施行予定日) / "effective"(施行日)。 */
  date_kind: string
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
  items: TsutatsuItem[]
  source: { provider: string; fetched_at: string; index_url: string }
}

export type TsutatsuIndexEntry = { tax: string; name: string; count: number }
export type TsutatsuIndex = {
  schema_version: number
  count: number
  sets: TsutatsuIndexEntry[]
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
