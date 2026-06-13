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
  }
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
}
