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
    path: string
    source_update_date: string | null
  }[]
}

export type TimelineEventRaw = {
  event_id: string
  event_type: string
  target_law_id: string
  amending_law_num: string | null
  promulgation_date: string | null
  effective_date: string | null
  revision_id: string
  source_update_date?: string | null
  status: string
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

async function getJson<T>(path: string): Promise<T> {
  // 相対 fetch にすることで、Pages のサブパスでも `./laws/index.json` のように
  // 解決される。`new URL(path, document.baseURI)` で base を尊重する。
  const url = new URL(path, document.baseURI).toString()
  const res = await fetch(url, { cache: 'no-cache' })
  if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`)
  return (await res.json()) as T
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
}
