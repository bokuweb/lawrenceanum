import { api } from './api'

type Args = Record<string, unknown>
type Schema = Record<string, unknown>
type Result = { content: { type: 'text'; text: string }[]; isError?: boolean }
type Tool = {
  name: string
  description: string
  inputSchema: Schema
  annotations: { readOnlyHint: true; untrustedContentHint: true }
  execute: (args: Args, options?: { signal?: AbortSignal }) => Promise<Result>
}
type ModelContext = {
  registerTool: (tool: Tool, options?: { signal: AbortSignal }) => void | Promise<void>
  unregisterTool?: (name: string) => void
}

const CORPORA = ['laws', 'proceedings', 'pubcomment', 'gian', 'tsutatsu', 'municipalities', 'reiki', 'procurement', 'shingikai', 'budget', 'wiki', 'kanpo'] as const
type Corpus = typeof CORPORA[number]
const text = (description: string): Schema => ({ type: 'string', minLength: 1, description })
const corpusSchema: Schema = { type: 'string', enum: CORPORA, default: 'laws' }
const paging = {
  limit: { type: 'integer', minimum: 1, maximum: 100, default: 20 },
  offset: { type: 'integer', minimum: 0, default: 0 },
}
const listing = {
  ...paging,
  query: text('メタデータを部分一致で検索。空白区切りの語は AND 条件。本文の全文検索には search_legal_data を使う。'),
  filters: { type: 'object', additionalProperties: { type: 'string' }, description: '一覧のフィールドに完全一致する条件。例: {"category":"民事"}, {"ministry":"moj"}, {"session":"217"}。' },
  sort_by: text('並べ替えるフィールド。ネストした値は dimensions.area のように指定。'),
  sort_order: { type: 'string', enum: ['asc', 'desc'], default: 'asc' },
}
const documentProperties = {
  corpus: corpusSchema, id: text('一覧・検索結果の ID（Wiki は path）。'),
  session: text('議案の国会回次。文字列で指定。'), ministry: text('審議会の府省 ID。例: moj, cao, mlit, mhlw。'),
  municipality_code: text('自治体コード。'), revision_id: text('法令の版 ID。省略時は配信中の current.json。'),
  date: text('官報発行日 YYYY-MM-DD。'),
}
const SECTIONS: Partial<Record<Corpus, string[]>> = {
  laws: ['articles', 'supplementary', 'appendices'], proceedings: ['speeches'],
  pubcomment: ['opinions', 'attachments'], gian: ['fields'], tsutatsu: ['items'],
  reiki: ['articles', 'supplementary'], shingikai: ['attachments'], budget: ['values'], kanpo: ['items'],
}
const ID_FIELDS: Record<Corpus, string> = { laws: 'law_id', proceedings: 'meeting_id', pubcomment: 'case_id', gian: 'bill_id', tsutatsu: 'tax', municipalities: 'municipality_code', reiki: 'reiki_id', procurement: 'item_id', shingikai: 'minutes_id', budget: 'stats_data_id', wiki: 'path', kanpo: 'date' }
const entryProperties = { ...documentProperties, section: text('文書内の区分。list_data_sources の sections で確認。省略時は先頭の区分。') }

function booleanArg(args: Args, key: string, fallback = false): boolean {
  if (args[key] === undefined) return fallback
  if (typeof args[key] !== 'boolean') throw new Error(`${key} は真偽値が必要です。`)
  return args[key]
}

function stringArg(args: Args, key: string, required = false): string | undefined {
  const value = args[key]
  if (value === undefined && !required) return undefined
  if (typeof value !== 'string' || !value.trim()) throw new Error(`${key} は空でない文字列が必要です。`)
  return value.trim()
}

function idArg(args: Args, key: string): string {
  const value = stringArg(args, key, true)!
  if (!/^[a-zA-Z0-9_-]+$/.test(value)) throw new Error(`${key} には一覧で取得した ID を指定してください。`)
  return value
}

function integerArg(args: Args, key: string, fallback: number, min: number, max = Number.MAX_SAFE_INTEGER): number {
  const value = args[key] === undefined ? fallback : args[key]
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < min || value > max) {
    throw new Error(`${key} は ${min}〜${max} の整数が必要です。`)
  }
  return value
}

function corpusArg(args: Args): Corpus {
  const corpus = args.corpus === undefined ? 'laws' : args.corpus
  if (!CORPORA.includes(corpus as Corpus)) throw new Error('corpus が不正です。')
  return corpus as Corpus
}

function dateArg(args: Args): string {
  const value = stringArg(args, 'date', true)!
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || new Date(value).toISOString().slice(0, 10) !== value) {
    throw new Error('date は有効な YYYY-MM-DD 形式の日付が必要です。')
  }
  return value
}

function fieldValue(row: unknown, path: string): unknown {
  return path.split('.').reduce<unknown>((value, key) => value !== null && typeof value === 'object' && Object.hasOwn(value, key) ? (value as Args)[key] : undefined, row)
}

function searchableText(value: unknown): string {
  if (value === null || value === undefined) return ''
  if (Array.isArray(value)) return value.map(searchableText).join(' ')
  if (typeof value === 'object') return Object.values(value).map(searchableText).join(' ')
  return String(value)
}

function pageItems<T>(items: T[], args: Args) {
  const limit = integerArg(args, 'limit', 20, 1, 100)
  const offset = integerArg(args, 'offset', 0, 0)
  const terms = stringArg(args, 'query')?.toLocaleLowerCase().split(/\s+/) ?? []
  const filters = args.filters === undefined ? {} : args.filters
  if (typeof filters !== 'object' || filters === null || Array.isArray(filters) || Object.values(filters).some(v => typeof v !== 'string')) {
    throw new Error('filters はフィールド名と文字列値のオブジェクトが必要です。')
  }
  const filtered = items.filter(item => {
    const searchable = searchableText(item).toLocaleLowerCase()
    return terms.every(term => searchable.includes(term)) &&
      Object.entries(filters).every(([key, value]) => {
        const actual = fieldValue(item, key)
        return actual != null && typeof actual !== 'object' && String(actual) === value
      })
  })
  const sortBy = stringArg(args, 'sort_by')
  if (sortBy) {
    const direction = args.sort_order === 'desc' ? -1 : 1
    filtered.sort((a, b) => {
      const left = fieldValue(a, sortBy), right = fieldValue(b, sortBy)
      if (left == null) return right == null ? 0 : 1
      if (right == null) return -1
      return direction * (typeof left === 'number' && typeof right === 'number' ? left - right : String(left).localeCompare(String(right), 'ja'))
    })
  }
  const results = filtered.slice(offset, offset + limit)
  return { items: results, total: filtered.length, offset, limit, next_offset: offset + results.length < filtered.length ? offset + results.length : null }
}

/** Arguments are returned verbatim so clients can open a hit without decoding routes. */
function documentArgs(corpus: Corpus, row: Args): Args | undefined {
  if (corpus === 'kanpo') return row.date ? { corpus, date: row.date } : undefined
  const id = row[ID_FIELDS[corpus]] ?? row.document_id
  if (typeof id !== 'string' || !id) return undefined
  const args: Args = { corpus, id }
  const route = typeof row.route === 'string' ? row.route.split('/') : []
  if (corpus === 'gian') args.session = row.session === undefined ? route[2] : String(row.session)
  if (corpus === 'shingikai') args.ministry = row.ministry ?? route[2]
  if (corpus === 'reiki') args.municipality_code = row.municipality_code
  if (Object.values(args).some(value => value === undefined)) return undefined
  return args
}

async function listDataRaw(args: Args) {
  const corpus = corpusArg(args)
  switch (corpus) {
    case 'laws': { const { laws, ...meta } = await api.lawsIndex(); return { corpus, ...meta, ...pageItems(laws, args) } }
    case 'proceedings': { const { meetings, ...meta } = await api.proceedingsIndex(); return { corpus, ...meta, ...pageItems(meetings, args) } }
    case 'pubcomment': { const { cases, ...meta } = await api.pubcommentIndex(); return { corpus, ...meta, ...pageItems(cases, args) } }
    case 'gian': { const { bills, ...meta } = await api.gianIndex(); return { corpus, ...meta, ...pageItems(bills, args) } }
    case 'tsutatsu': { const { sets, ...meta } = await api.tsutatsuIndex(); return { corpus, ...meta, ...pageItems(sets, args) } }
    case 'municipalities': { const { municipalities, ...meta } = await api.reikiIndex(); return { corpus, ...meta, ...pageItems(municipalities, args) } }
    case 'reiki': { const { reiki, ...meta } = await api.reikiMunicipality(idArg(args, 'municipality_code')); return { corpus, ...meta, ...pageItems(reiki, args) } }
    case 'procurement': { const { items, ...meta } = await api.procurementIndex(); return { corpus, ...meta, ...pageItems(items, args) } }
    case 'shingikai': { const { minutes, ...meta } = await api.shingikaiIndex(); return { corpus, ...meta, ...pageItems(minutes, args) } }
    case 'budget': { const { datasets, ...meta } = await api.budgetIndex(); return { corpus, ...meta, ...pageItems(datasets, args) } }
    case 'wiki': { const { pages, ...meta } = await api.wikiIndex(); return { corpus, ...meta, ...pageItems(pages, args) } }
    case 'kanpo': { const { issues, ...meta } = await api.kanpoOnDate(dateArg(args)); return { corpus, ...meta, ...pageItems(issues, args) } }
  }
}

async function listData(args: Args) {
  const result = await listDataRaw(args)
  return { ...result, items: result.items.map(item => {
    const row = item as unknown as Args
    const detail = documentArgs(result.corpus, { date: args.date, municipality_code: args.municipality_code, ...row })
    return { ...row, ...(detail ? { document_args: detail } : {}) }
  }) }
}

function lawDocument(args: Args) {
  const lawId = idArg(args, 'id')
  return args.revision_id === undefined ? api.law(lawId) : api.revision(lawId, idArg(args, 'revision_id'))
}

async function getDocument(args: Args) {
  const corpus = corpusArg(args)
  // 官報は日付で取得。Wiki はスラッシュ区切りのページパスで取得する。
  if (corpus === 'kanpo') return api.kanpoOnDate(dateArg(args))
  if (corpus === 'wiki') {
    const path = stringArg(args, 'id', true)!
    if (path.split('/').some(part => !part || part === '.' || part === '..') || /[\\\x00-\x1f]/.test(path)) throw new Error('Wiki のページパスが不正です。')
    return api.wikiPage(path)
  }
  const id = idArg(args, 'id')
  switch (corpus) {
    case 'laws': return lawDocument(args)
    case 'proceedings': return api.meeting(id)
    case 'pubcomment': return api.pubcommentCase(id)
    case 'gian': return api.gianBill(idArg(args, 'session'), id)
    case 'tsutatsu': return api.tsutatsuSet(id)
    case 'municipalities': return api.reikiMunicipality(id)
    case 'reiki': return api.reikiDoc(idArg(args, 'municipality_code'), id)
    case 'procurement': return api.procurementItem(id)
    case 'shingikai': return api.shingikaiMeeting(idArg(args, 'ministry'), id)
    case 'budget': return api.budgetDataset(id)
  }
}

async function documentEntries(args: Args) {
  const corpus = corpusArg(args)
  const sections = SECTIONS[corpus]
  if (!sections) throw new Error(`${corpus} は項目取得に対応していません。get_legal_document を使ってください。`)
  const section = stringArg(args, 'section') ?? sections[0]
  if (!sections.includes(section)) throw new Error(`section は ${sections.join(', ')} から指定してください。`)
  const document = await getDocument(args) as unknown as Args
  const records = (value: unknown): Args[] => Array.isArray(value) ? value as Args[] : []
  let entries: Args[]
  if (section === 'supplementary') {
    entries = records(document[corpus === 'laws' ? 'suppl_provisions' : 'supplementary']).flatMap((part, sectionIndex) =>
      records(part.articles).map(article => ({ ...article, supplementary_section: { ...part, articles: undefined, section_index: sectionIndex } })))
  } else if (corpus === 'kanpo') {
    entries = records(document.issues).flatMap((issue, issueIndex) => records(issue.items).map(item => ({ ...item, issue: { ...issue, items: undefined, issue_index: issueIndex } })))
  } else {
    entries = records(document[section === 'appendices' ? 'appendix_tables' : section])
  }
  const indexed = entries.map((entry, entry_index) => ({ ...entry, entry_index,
    entry_id: entry.article_id ?? entry.appdx_id ?? entry.speech_id ?? entry.attachment_id ?? entry.number ?? entry.key ?? null,
  }))
  const { articles: _articles, suppl_provisions: _suppl, supplementary: _supplementary, appendix_tables: _appendices,
    speeches: _speeches, opinions: _opinions, attachments: _attachments, fields: _fields, items: _items,
    values: _values, issues: _issues, body_text: _body, minutes_text: _minutes, ...meta } = document
  return { corpus, section, document: meta, entries: indexed }
}

async function cancellable<T>(work: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return work
  let abort!: () => void
  const cancelled = new Promise<never>((_, reject) => {
    abort = () => reject(signal.reason ?? new DOMException('Aborted', 'AbortError'))
    signal.addEventListener('abort', abort, { once: true })
    if (signal.aborted) abort()
  })
  try { return await Promise.race([work, cancelled]) }
  finally { signal.removeEventListener('abort', abort) }
}

function tool(name: string, description: string, properties: Record<string, Schema>, required: string[], execute: (args: Args) => unknown | Promise<unknown>): Tool {
  return {
    name, description,
    inputSchema: { type: 'object', properties, required, additionalProperties: false },
    annotations: { readOnlyHint: true, untrustedContentHint: true },
    execute: async (args, options) => {
      try {
        options?.signal?.throwIfAborted()
        if (typeof args !== 'object' || args === null || Array.isArray(args)) throw new Error('引数はオブジェクトが必要です。')
        const unknown = Object.keys(args).find(key => !Object.hasOwn(properties, key))
        if (unknown) throw new Error(`不明な引数: ${unknown}`)
        for (const key of required) if (args[key] === undefined) throw new Error(`${key} が必要です。`)
        for (const [key, value] of Object.entries(args)) {
          const schema = properties[key]
          const validType = schema.type === 'array' ? Array.isArray(value) : schema.type === 'integer' ? Number.isSafeInteger(value) : schema.type === 'object' ? value !== null && typeof value === 'object' && !Array.isArray(value) : typeof value === schema.type
          if (!validType || (Array.isArray(schema.enum) && !schema.enum.includes(value))) throw new Error(`${key} が不正です。`)
        }
        const result = await cancellable(Promise.resolve().then(() => {
          options?.signal?.throwIfAborted()
          return execute(args)
        }), options?.signal)
        options?.signal?.throwIfAborted()
        return { content: [{ type: 'text', text: JSON.stringify(result) }] }
      } catch (error) {
        return { isError: true, content: [{ type: 'text', text: JSON.stringify({ error: error instanceof Error ? error.message : String(error) }) }] }
      }
    },
  }
}

export function createWebMcpTools(): Tool[] {
  return [
    tool('list_data_sources', '対応する12種のデータと検索可否、一覧・詳細の必須引数、項目取得の section を案内する。公開データの存在・更新状況は get_data_status で確認する。', {}, [], () => ({
      items: CORPORA.map(corpus => ({ corpus, id_field: ID_FIELDS[corpus],
        fulltext_search: ['laws', 'proceedings', 'pubcomment', 'gian', 'tsutatsu', 'reiki', 'shingikai', 'kanpo'].includes(corpus),
        list_required: corpus === 'reiki' ? ['municipality_code'] : corpus === 'kanpo' ? ['date'] : [],
        document_required: corpus === 'kanpo' ? ['date'] : ['id', ...(corpus === 'gian' ? ['session'] : corpus === 'reiki' ? ['municipality_code'] : corpus === 'shingikai' ? ['ministry'] : [])],
        sections: SECTIONS[corpus] ?? [],
      })),
    })),
    tool('search_legal_data', '法令の条文・国会発言・官報・通達・自治体例規・パブコメ・議案・審議会を全文検索する。同義語展開、順位順ページング、詳細取得用 document_args に対応。例規は先頭2000候補内の検索のため自治体・都道府県で絞り込む。DBの取得失敗はエラーを返す。', {
      query: text('2文字以上の全文検索キーワード（空白区切りは AND 条件）'),
      corpus: { type: 'string', enum: ['laws', 'proceedings', 'kanpo', 'tsutatsu', 'reiki', 'pubcomment', 'gian', 'shingikai'], default: 'laws' },
      ...paging,
      categories: { type: 'array', items: { type: 'string' }, description: '法令検索の分類フィルター。例: ["民事"]。laws だけで指定可能。' },
      municipality_code: text('自治体例規検索の自治体コード。reiki だけで指定可能。'),
      prefecture: text('例規検索の都道府県名。municipality_code と同時指定不可。'),
      title_only: { type: 'boolean', default: false, description: '例規の題名だけを検索。reiki だけで指定可能。' },
    }, ['query'], async args => {
      const query = stringArg(args, 'query', true)!
      const corpus = corpusArg(args)
      if (!['laws', 'proceedings', 'kanpo', 'tsutatsu', 'reiki', 'pubcomment', 'gian', 'shingikai'].includes(corpus)) throw new Error('この corpus は list_legal_data の query で検索してください。')
      const limit = integerArg(args, 'limit', 20, 1, 100)
      const offset = integerArg(args, 'offset', 0, 0, corpus === 'reiki' ? 2000 : Number.MAX_SAFE_INTEGER)
      if (args.categories !== undefined && (corpus !== 'laws' || !Array.isArray(args.categories) || args.categories.some(v => typeof v !== 'string' || !v.trim()))) throw new Error('categories は laws 用の文字列配列が必要です。')
      if (['municipality_code', 'prefecture', 'title_only'].some(key => args[key] !== undefined) && corpus !== 'reiki') throw new Error('municipality_code / prefecture / title_only は reiki だけで指定できます。')
      if (args.municipality_code !== undefined && args.prefecture !== undefined) throw new Error('municipality_code と prefecture は同時指定できません。')
      const engine = await import('./search-engine')
      if (!engine.buildFtsMatch(query)) throw new Error('query に検索可能な2文字以上の語を含めてください。')
      if (corpus !== 'reiki' && !await engine.isAvailable()) throw new Error('検索 DB を利用できません。メタデータ検索には list_legal_data を使ってください。')
      let hits: unknown[]
      switch (corpus) {
        case 'laws': hits = await engine.search(query, limit + 1, args.categories as string[] | undefined, offset); break
        case 'proceedings': hits = await engine.searchSpeeches(query, limit + 1, offset); break
        case 'kanpo': hits = await engine.searchKanpo(query, limit + 1, true, offset); break
        case 'tsutatsu': hits = await engine.searchTsutatsu(query, limit + 1, true, offset); break
        case 'pubcomment': case 'gian': case 'shingikai': hits = await engine.searchDocuments(query, corpus, limit + 1, true, offset); break
        default: hits = await engine.searchReiki(query, { limit: limit + 1, offset, strict: true, titleOnly: booleanArg(args, 'title_only'),
          scope: args.municipality_code !== undefined ? { kind: 'municipality', code: idArg(args, 'municipality_code') } : args.prefecture !== undefined ? { kind: 'prefecture', prefecture: stringArg(args, 'prefecture', true)! } : { kind: 'all' } })
      }
      const items = hits.slice(0, limit).map(hit => {
        const row = hit as Record<string, unknown>
        const detail = documentArgs(corpus, row)
        const entryId = corpus === 'proceedings' ? row.speech_id : corpus === 'tsutatsu' ? row.number : corpus === 'reiki' ? row.article_id : undefined
        return { ...row, ...(typeof row.snippet === 'string' ? { snippet: engine.unbigramSnippet(row.snippet).replace(/<\/?mark>/g, '') } : {}),
          ...(detail ? { document_args: detail } : {}),
          ...(detail && entryId ? { entry_args: { ...detail, entry_id: entryId } } : {}),
          ...(corpus === 'laws' ? { article_args: { law_id: row.law_id, article_id: row.article_id } } : {}),
        }
      })
      const hasMore = hits.length > limit
      return { corpus, query, items, limit, offset, has_more: hasMore, next_offset: hasMore ? offset + items.length : null,
        ...(corpus === 'reiki' ? { candidate_limit: 2000, search_scope: '先頭2000候補内。全件の検索には自治体・都道府県で絞り込んでください。' } : {}),
      }
    }),
    tool('list_legal_data', '法令・会議録・パブコメ・議案・通達集・自治体・例規・政府調達・審議会・統計・Wiki・官報の一覧。メタデータ部分一致検索と完全一致フィルター、ページングが可能。reiki は municipality_code、kanpo は date が必須。', {
      corpus: corpusSchema, ...listing,
      municipality_code: text('reiki の一覧を取得する自治体コード。municipalities の一覧で取得。'),
      date: text('kanpo の発行日 YYYY-MM-DD。'),
    }, [], listData),
    tool('get_legal_document', '一覧・検索の document_args をそのまま渡して詳細・本文・出典を取得する。IDと追加必須引数は list_data_sources で確認可能。laws は revision_id で過去版も取得可能。', documentProperties, [], getDocument),
    tool('list_document_entries', '文書内の条文・附則・別表、会議録の発言、パブコメ意見・添付、議案項目、通達、例規、審議会添付、統計値、官報記事を検索・絞り込み・並べ替え・ページングして取得する。entry_index は元の区分内で固定。section は list_data_sources で確認。', {
      ...entryProperties, ...listing,
    }, [], async args => {
      const { entries, ...meta } = await documentEntries(args)
      return { ...meta, ...pageItems(entries, args) }
    }),
    tool('get_document_entry', '文書内の1項目を取得する。検索結果の entry_args または一覧の entry_index / entry_id を使う。entry_index と entry_id はいずれか一方が必須。ID重複時は entry_index を使う。', {
      ...entryProperties, entry_index: { type: 'integer', minimum: 0 }, entry_id: text('条文・発言・添付 ID、通達番号、議案項目の key。'),
    }, [], async args => {
      if ((args.entry_index === undefined) === (args.entry_id === undefined)) throw new Error('entry_index または entry_id のどちらか一方を指定してください。')
      const { entries, ...meta } = await documentEntries(args)
      let entry
      if (args.entry_index !== undefined) entry = entries[integerArg(args, 'entry_index', 0, 0)]
      else {
        const id = stringArg(args, 'entry_id', true)!
        const matches = entries.filter(item => item.entry_id != null && String(item.entry_id) === id)
        if (matches.length > 1) throw new Error('entry_id が重複しています。list_document_entries の entry_index を指定してください。')
        entry = matches[0]
      }
      if (!entry) throw new Error('指定した項目が見つかりません。')
      return { ...meta, entry }
    }),
    tool('get_law_article', '法令の特定の条文（附則を含む）または別表を取得する。article_id は詳細または全文検索で取得。revision_id 省略時は current.json の版。', {
      law_id: text('法令 ID。'), article_id: text('条文 ID。'), revision_id: text('過去版の revision_id。'),
    }, ['law_id', 'article_id'], async args => {
      const law = await lawDocument({ id: idArg(args, 'law_id'), revision_id: args.revision_id })
      const articleId = idArg(args, 'article_id')
      const article = [...law.articles, ...(law.suppl_provisions ?? []).flatMap(section => section.articles)].find(item => item.article_id === articleId)
      const appendix = law.appendix_tables?.find(item => item.appdx_id === articleId)
      if (!article && !appendix) throw new Error(`条文・別表が見つかりません: ${articleId}`)
      return { law_id: law.law_id, title: law.title, revision_id: law.revision_id, status: law.status, source: law.source, ...(article ? { article } : { appendix_table: appendix }) }
    }),
    tool('list_law_versions', '法令の版一覧と本文の取得可否を返す。body_available / path を確認してから過去版を取得する。', { law_id: text('法令 ID。'), ...paging }, ['law_id'], async args => {
      const { versions, ...meta } = await api.versions(idArg(args, 'law_id'))
      return { ...meta, ...pageItems(versions, args) }
    }),
    tool('get_law_timeline', '法令の制定・改正・廃止の履歴と官報リンクを取得する。', { law_id: text('法令 ID。') }, ['law_id'], args => api.timeline(idArg(args, 'law_id'))),
    tool('list_law_diffs', '配信済みの法令の版間差分と変更件数を一覧取得する。from_revision_id / to_revision_id を get_law_diff に渡す。', { law_id: text('法令 ID。'), ...listing }, ['law_id'], async args => {
      const { diffs, ...meta } = await api.diffsIndex(idArg(args, 'law_id'))
      return { ...meta, ...pageItems(diffs, args) }
    }),
    tool('get_law_diff', '配信済みの版間差分を条文単位で取得する。query / filters で変更箇所を絞り込める。任意の版ペアの差分生成は行わない。', {
      law_id: text('法令 ID。'), from_revision_id: text('変更前の版 ID。'), to_revision_id: text('変更後の版 ID。'), ...listing,
    }, ['law_id', 'from_revision_id', 'to_revision_id'], async args => {
      const { articles, ...meta } = await api.diff(idArg(args, 'law_id'), idArg(args, 'from_revision_id'), idArg(args, 'to_revision_id'))
      return { ...meta, ...pageItems(articles, args) }
    }),
    tool('get_law_snapshot', '公開済みの指定日時点の法令の版を解決する。include_body=true なら解決された過去版本文も取得。指定日JSONが未配信の場合はエラー。include_unenforced など配信データの条件を返す。', {
      law_id: text('法令 ID。'), date: text('基準日 YYYY-MM-DD。'), include_body: { type: 'boolean', default: false },
    }, ['law_id', 'date'], async args => {
      const lawId = idArg(args, 'law_id')
      const snapshot = await api.snapshotAt(lawId, dateArg(args))
      if (!booleanArg(args, 'include_body')) return snapshot
      if (!snapshot.resolved_revision_id) return { ...snapshot, document: null }
      if (snapshot.body_available === false) throw new Error('指定日時点の版の本文は配信されていません。')
      const document = await api.revision(lawId, idArg({ revision_id: snapshot.resolved_revision_id }, 'revision_id'))
      return { ...snapshot, document }
    }),
    tool('get_related_documents', '法令に関連する国会会議録・パブコメ・通達、または会議録で言及された法令を取得する。関連度・照合理由と詳細取得用 document_args を返す。', {
      corpus: { type: 'string', enum: ['laws', 'proceedings'], default: 'laws' }, id: text('起点の法令 ID または会議 ID。'),
      target: { type: 'string', enum: ['laws', 'proceedings', 'pubcomment', 'tsutatsu'] }, ...listing,
    }, ['id', 'target'], async args => {
      const corpus = corpusArg(args), id = idArg(args, 'id'), target = args.target as Corpus
      let links: Args[], meta: Args
      if (corpus === 'laws' && target === 'proceedings') { const { linked_proceedings, ...rest } = await api.lawToProceedings(id); links = linked_proceedings; meta = rest }
      else if (corpus === 'laws' && target === 'pubcomment') { const { linked_pubcomments, ...rest } = await api.lawToPubcomment(id); links = linked_pubcomments; meta = rest }
      else if (corpus === 'laws' && target === 'tsutatsu') { const { linked_tsutatsu, ...rest } = await api.lawToTsutatsu(id); links = linked_tsutatsu; meta = rest }
      else if (corpus === 'proceedings' && target === 'laws') { const { linked_laws, ...rest } = await api.meetingToLaws(id); links = linked_laws; meta = rest }
      else throw new Error('laws → proceedings / pubcomment / tsutatsu、proceedings → laws に対応しています。')
      const page = pageItems(links, args)
      return { ...meta, corpus, id, target, ...page, items: page.items.map(row => ({ ...row, document_args: documentArgs(target, row) })) }
    }),
    tool('get_wiki_links', 'Wiki ページの関連ページを無向グラフから取得する。リンク方向の区別は配信データに存在しない。各リンクに接続先の題名・種別と詳細取得用 document_args を付ける。', {
      id: text('Wiki ページの path。'), ...listing,
    }, ['id'], async args => {
      const id = stringArg(args, 'id', true)!
      const graph = await api.wikiGraph()
      const nodes = new Map(graph.nodes.map(node => [node.id, node]))
      if (!nodes.has(id)) throw new Error('Wiki グラフにページが見つかりません。')
      const links = graph.links.filter(link => link.source === id || link.target === id)
        .map(link => {
          const relatedId = link.source === id ? link.target : link.source
          return { ...link, page: nodes.get(relatedId) ?? { id: relatedId }, document_args: { corpus: 'wiki', id: relatedId } }
        })
      return { schema_version: graph.schema_version, id, graph_type: 'undirected', ...pageItems(links, args) }
    }),
    tool('get_law_updates', '法令の追加・変更・削除を日付別に取得する。date 省略時は公開データの最新更新日。', {
      date: text('更新日 YYYY-MM-DD。省略すると latest。'), ...listing,
    }, [], async args => {
      const { updated_laws, ...meta } = await (args.date === undefined ? api.latestUpdates() : api.updatesOnDate(dateArg(args)))
      return { ...meta, ...pageItems(updated_laws, args) }
    }),
    tool('get_law_references', '法令・条文間の引用リンクを取得する。direction は outgoing（参照先）または incoming（被参照）。article_id 省略時は法令全体。', {
      law_id: text('法令 ID。'), article_id: text('条文 ID。'), direction: { type: 'string', enum: ['outgoing', 'incoming'], default: 'outgoing' }, ...paging,
    }, ['law_id'], async args => {
      const lawId = idArg(args, 'law_id')
      const direction = args.direction ?? 'outgoing'
      if (direction !== 'outgoing' && direction !== 'incoming') throw new Error('direction が不正です。')
      const articleId = args.article_id === undefined ? undefined : idArg(args, 'article_id')
      const engine = await import('./search-engine')
      if (!await engine.isAvailable()) throw new Error('検索 DB を利用できません。')
      const refs = direction === 'incoming' ? await engine.getIncomingRefs(lawId, articleId) : await engine.getOutgoingRefs(lawId, articleId)
      return { law_id: lawId, direction, ...pageItems(refs, args) }
    }),
    tool('get_recent_changes', '法令改正・議案・パブコメ・官報の新着フィードを取得する。', listing, [], async args => {
      const { items, ...meta } = await api.recentFeed()
      return { ...meta, ...pageItems(items, args) }
    }),
    tool('get_upcoming_enforcements', '公開データの今後の施行予定を取得する。', listing, [], async args => {
      const { items, ...meta } = await api.enforcementUpcoming()
      return { ...meta, ...pageItems(items, args) }
    }),
    tool('get_data_status', '公開データの更新日時・収集状況・各コーパスの件数とエラーを取得する。', {}, [], () => api.health()),
  ]
}

/** No polyfill: unsupported browsers keep their normal application behavior. */
export function registerWebMcp(): () => void {
  const current = (document as Document & { modelContext?: ModelContext }).modelContext
  const legacy = (navigator as Navigator & { modelContext?: ModelContext }).modelContext
  const context = current ?? legacy
  if (!context?.registerTool) return () => {}
  const controller = new AbortController()
  const registered = new Set<string>()
  const removeLegacy = (name: string) => {
    try { context.unregisterTool?.(name) } catch (error) { console.warn('[webmcp] cleanup failed', name, error) }
  }
  // Start immediately; registration may be asynchronous in modern browsers.
  void (async () => {
    for (const definition of createWebMcpTools()) {
      if (controller.signal.aborted) break
      try {
        await context.registerTool(definition, { signal: controller.signal })
        if (controller.signal.aborted) removeLegacy(definition.name)
        else registered.add(definition.name)
      } catch (error) {
        if (!controller.signal.aborted) console.warn('[webmcp] registration failed', definition.name, error)
      }
    }
  })()
  return () => {
    controller.abort()
    for (const name of registered) removeLegacy(name)
    registered.clear()
  }
}
