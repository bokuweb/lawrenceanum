import { test, expect, type Page } from '@playwright/test'

const BASE = process.env.HISTORY_BASE ?? 'http://127.0.0.1:8799/'
const LAW_ID = '129AC0000000089'
const REVISION = `${LAW_ID}_20251001_505AC0000000053`

// Browser API harness only: production code never installs a WebMCP polyfill.
async function install(page: Page, mode: 'document' | 'navigator' | 'both' | 'reject' = 'document') {
  await page.addInitScript(mode => {
    const state = window as any
    state.webMcpTools = {}
    state.legacyRegistrations = []
    const context = {
      async registerTool(tool: any, options?: { signal?: AbortSignal }) {
        if (mode === 'reject' && tool.name === 'search_legal_data') throw new Error('registration denied')
        state.webMcpTools[tool.name] = tool
        options?.signal?.addEventListener('abort', () => { delete state.webMcpTools[tool.name] }, { once: true })
      },
      unregisterTool(name: string) { delete state.webMcpTools[name] },
    }
    Object.defineProperty(document, 'modelContext', { configurable: true, value: mode === 'navigator' ? undefined : context })
    Object.defineProperty(navigator, 'modelContext', { configurable: true, value: mode === 'navigator' ? context : mode === 'both' ? { registerTool(tool: any) { state.legacyRegistrations.push(tool.name) } } : undefined })
  }, mode)
}

async function open(page: Page) {
  await page.goto(new URL('#/settings', BASE).toString())
  await page.waitForFunction(() => Object.keys((window as any).webMcpTools ?? {}).length === 19)
}

async function call(page: Page, name: string, args: Record<string, unknown> = {}) {
  return page.evaluate(async ({ name, args }) => {
    const result = await (window as any).webMcpTools[name].execute(args)
    return { isError: result.isError === true, data: JSON.parse(result.content[0].text) }
  }, { name, args })
}

test('WebMCP: registers global read-only tools and prefers document over navigator', async ({ page }) => {
  await install(page, 'both')
  await open(page)
  const info = await page.evaluate(() => ({
    tools: Object.values((window as any).webMcpTools).map((tool: any) => ({ name: tool.name, schema: tool.inputSchema, annotations: tool.annotations })),
    legacy: (window as any).legacyRegistrations,
  }))
  expect(info.legacy).toEqual([])
  expect(new Set(info.tools.map(tool => tool.name)).size).toBe(19)
  for (const tool of info.tools) {
    expect(tool.schema).toMatchObject({ type: 'object', additionalProperties: false })
    expect(tool.annotations).toMatchObject({ readOnlyHint: true, untrustedContentHint: true })
  }
  await page.getByRole('link', { name: '法令閲覧', exact: true }).click()
  expect((await call(page, 'get_data_status')).data.ok).toBe(true)
})

test('WebMCP: legacy navigator API can execute tools', async ({ page }) => {
  await install(page, 'navigator')
  await open(page)
  expect((await call(page, 'list_legal_data', { limit: 1 })).data.items[0].title).toBe('民法')
})

test('WebMCP: unsupported browsers retain the application without page errors', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(document, 'modelContext', { configurable: true, value: undefined })
    Object.defineProperty(navigator, 'modelContext', { configurable: true, value: undefined })
  })
  const errors: string[] = []
  page.on('pageerror', error => errors.push(error.message))
  await page.goto(new URL('#/laws', BASE).toString())
  await expect(page.getByRole('heading', { name: '法令閲覧', exact: true })).toBeVisible()
  expect(errors).toEqual([])
})

test('WebMCP: a rejected registration does not block other tools or the app', async ({ page }) => {
  await install(page, 'reject')
  await page.goto(new URL('#/settings', BASE).toString())
  await page.waitForFunction(() => Object.keys((window as any).webMcpTools ?? {}).length === 18)
  expect((await call(page, 'get_data_status')).isError).toBe(false)
})

test('WebMCP: law metadata filtering and pagination preserve total and next offset', async ({ page }) => {
  await install(page)
  await open(page)
  const first = await call(page, 'list_legal_data', { corpus: 'laws', filters: { category: '行政' }, limit: 2 })
  expect(first.isError).toBe(false)
  expect(first.data).toMatchObject({ total: 3, offset: 0, limit: 2, next_offset: 2 })
  const last = await call(page, 'list_legal_data', { corpus: 'laws', filters: { category: '行政' }, limit: 2, offset: first.data.next_offset })
  expect(last.data.items).toHaveLength(1)
  expect(last.data.next_offset).toBeNull()
  expect((await call(page, 'list_legal_data', { query: '民法 明治' })).data.items.map((row: any) => row.law_id)).toEqual([LAW_ID])
})

test('WebMCP: past law body, article, supplementary article, and versions are retrievable', async ({ page }) => {
  await install(page)
  await open(page)
  const args = { law_id: LAW_ID, revision_id: REVISION }
  expect((await call(page, 'get_legal_document', { id: LAW_ID, revision_id: REVISION })).data).toMatchObject({ law_id: LAW_ID, revision_id: REVISION, title: '民法' })
  expect((await call(page, 'get_law_article', { ...args, article_id: 'art_1' })).data.article.paragraphs[0].text).toContain('公共の福祉')
  expect((await call(page, 'get_law_article', { ...args, article_id: 'suppl3_art_1' })).data.article.paragraphs[0].text).toContain('日本国憲法施行の日')
  expect((await call(page, 'get_law_article', { ...args, article_id: 'art_missing' })).isError).toBe(true)
  const versions = await call(page, 'list_law_versions', { law_id: LAW_ID, limit: 2 })
  expect(versions.data.items).toHaveLength(2)
  expect(versions.data.next_offset).toBe(2)
})

test('WebMCP: corpus lists and details use composite identifiers', async ({ page }) => {
  await install(page)
  const fixtures = [
    { corpus: 'proceedings', array: 'meetings', row: { meeting_id: 'meeting1', committee: '法務委員会' }, path: 'proceedings/meeting1.json', args: { id: 'meeting1' } },
    { corpus: 'pubcomment', array: 'cases', row: { case_id: 'case1', title: '意見募集' }, path: 'pubcomment/case1.json', args: { id: 'case1' } },
    { corpus: 'gian', array: 'bills', row: { bill_id: 'bill1', session: 217, title: '法律案' }, path: 'gian/217/bill1.json', args: { id: 'bill1', session: '217' } },
    { corpus: 'procurement', array: 'items', row: { item_id: 'item1', title: '入札公告' }, path: 'procurement/item1.json', args: { id: 'item1' } },
    { corpus: 'shingikai', array: 'minutes', row: { minutes_id: 'minutes1', ministry: 'moj', title: '法制審議会' }, path: 'shingikai/moj/minutes1.json', args: { id: 'minutes1', ministry: 'moj' } },
    { corpus: 'budget', array: 'datasets', row: { stats_data_id: '001', title: '財政統計' }, path: 'budget/001.json', args: { id: '001' } },
    { corpus: 'wiki', array: 'pages', row: { path: 'laws/sample', title: '解説' }, path: 'wiki/page/laws/sample.json', args: { id: 'laws/sample' } },
  ]
  for (const fixture of fixtures) {
    await page.route(`**/${fixture.corpus}/index.json`, route => route.fulfill({ json: { count: 1, [fixture.array]: [fixture.row] } }))
    await page.route(`**/${fixture.path}`, route => route.fulfill({ json: { ...fixture.row, source: { provider: 'fixture' } } }))
  }
  await open(page)
  for (const fixture of fixtures) {
    expect((await call(page, 'list_legal_data', { corpus: fixture.corpus })).data.items).toMatchObject([fixture.row])
    expect((await call(page, 'get_legal_document', { corpus: fixture.corpus, ...fixture.args })).data).toMatchObject(fixture.row)
  }
  const municipalities = await call(page, 'list_legal_data', { corpus: 'municipalities' })
  expect(municipalities.data.items.some((row: any) => row.municipality_code === '012122')).toBe(true)
  const reiki = await call(page, 'list_legal_data', { corpus: 'reiki', municipality_code: '012122' })
  expect((await call(page, 'get_legal_document', { corpus: 'reiki', municipality_code: '012122', id: reiki.data.items[0].reiki_id })).data.title).toContain('留萌市')
  expect((await call(page, 'get_legal_document', { corpus: 'tsutatsu', id: 'shotoku' })).data.items).not.toHaveLength(0)
  expect((await call(page, 'list_legal_data', { corpus: 'kanpo', date: '2026-04-02' })).data.items).toHaveLength(1)
  expect((await call(page, 'get_legal_document', { corpus: 'kanpo', date: '2026-04-02' })).data.issues[0].items[0].title).toContain('郵便法')
})

test('WebMCP: real WASM SQLite full-text search returns usable results across corpora', async ({ page }) => {
  test.setTimeout(90_000)
  await install(page)
  await open(page)
  for (const [corpus, query] of [['kanpo', '郵便法'], ['tsutatsu', '住所'], ['reiki', '個人情報'], ['pubcomment', '行政回答'], ['gian', '制度改善'], ['shingikai', '配布原文']] as const) {
    const result = await call(page, 'search_legal_data', { corpus, query, limit: 1 })
    expect(result.isError, JSON.stringify(result.data)).toBe(false)
    expect(result.data.items).toHaveLength(1)
    expect(result.data.items[0].snippet ?? result.data.items[0].excerpt).toContain(query)
  }
  const scoped = await call(page, 'search_legal_data', { corpus: 'reiki', query: '個人情報', municipality_code: '012122' })
  expect(scoped.data.items.every((row: any) => row.municipality_code === '012122')).toBe(true)
  expect(scoped.data.items.length).toBeGreaterThan(0)
})

test('WebMCP: failed database requests return errors instead of empty search results', async ({ page }) => {
  await install(page)
  await page.route('**/search.db', route => route.fulfill({ status: 404, body: 'missing' }))
  await page.route('**/reiki-search.db', route => route.fulfill({ status: 404, body: 'missing' }))
  await open(page)
  for (const corpus of ['laws', 'reiki']) {
    const result = await call(page, 'search_legal_data', { corpus, query: '個人情報' })
    expect(result.isError).toBe(true)
    expect(result.data.error).toContain('検索 DB')
  }
})

test('WebMCP: timeline, references, recent changes, and enforcement schedules are accessible', async ({ page }) => {
  await install(page)
  await page.route(`**/laws/${LAW_ID}/timeline.json`, route => route.fulfill({ json: { law_id: LAW_ID, entries: [{ revision_id: REVISION }] } }))
  await page.route('**/feeds/recent.json', route => route.fulfill({ json: { generated_at: '2026-10-10', items: [{ kind: 'law', title: '民法改正' }, { kind: 'gian', title: '法律案' }] } }))
  await page.route('**/enforcement/upcoming.json', route => route.fulfill({ json: { generated_at: '2026-10-10', items: [{ law_id: LAW_ID, title: '民法', effective_date: '2027-04-01' }] } }))
  await open(page)
  expect((await call(page, 'get_law_timeline', { law_id: LAW_ID })).data.entries[0].revision_id).toBe(REVISION)
  for (const direction of ['outgoing', 'incoming']) {
    expect((await call(page, 'get_law_references', { law_id: LAW_ID, article_id: 'art_1', direction })).data).toMatchObject({ direction, items: [], total: 0 })
  }
  const recent = await call(page, 'get_recent_changes', { filters: { kind: 'law' }, limit: 1 })
  expect(recent.data).toMatchObject({ generated_at: '2026-10-10', total: 1, items: [{ title: '民法改正' }] })
  expect((await call(page, 'get_upcoming_enforcements', { query: '民法' })).data.items[0].effective_date).toBe('2027-04-01')
})

test('WebMCP: rejects invalid inputs and reports unavailable detail data', async ({ page }) => {
  await install(page)
  await page.route('**/laws/missing/current.json', route => route.fulfill({ status: 404, body: 'missing' }))
  await open(page)
  for (const [name, args] of [
    ['get_legal_document', { id: '../health' }],
    ['get_legal_document', { corpus: 'wiki', id: '../health' }],
    ['get_legal_document', { corpus: 'gian', id: 'bill1' }],
    ['get_legal_document', { id: 'missing' }],
    ['list_legal_data', { corpus: 'unknown' }],
    ['list_legal_data', { corpus: null }],
    ['list_legal_data', { limit: 101 }],
    ['list_legal_data', { offset: -1 }],
    ['list_legal_data', { filters: { category: 3 } }],
    ['list_legal_data', { filters: null }],
    ['list_legal_data', { corpus: 'kanpo', date: '2026-02-30' }],
    ['search_legal_data', { query: ' ' }],
    ['search_legal_data', { query: '!!!' }],
    ['search_legal_data', { query: '法', categories: '民事' }],
    ['get_law_references', { law_id: LAW_ID, direction: null }],
    ['get_data_status', { extra: true }],
  ] as const) {
    const result = await call(page, name, args)
    expect(result.isError, `${name}: ${JSON.stringify(args)}`).toBe(true)
    expect(result.data.error).toBeTruthy()
  }
})

test('WebMCP: honors cancellation before executing data requests', async ({ page }) => {
  await install(page)
  await open(page)
  const requests: string[] = []
  page.on('request', request => { if (request.url().includes('health.json')) requests.push(request.url()) })
  const result = await page.evaluate(async () => {
    const controller = new AbortController()
    controller.abort()
    return (window as any).webMcpTools.get_data_status.execute({}, { signal: controller.signal })
  })
  expect(result.isError).toBe(true)
  expect(requests).toEqual([])
})

test('WebMCP: source discovery describes conditional arguments and document sections', async ({ page }) => {
  await install(page)
  await open(page)
  const sources = (await call(page, 'list_data_sources')).data.items
  expect(sources).toHaveLength(12)
  expect(sources.find((item: any) => item.corpus === 'gian')).toMatchObject({ id_field: 'bill_id', document_required: ['id', 'session'] })
  expect(sources.find((item: any) => item.corpus === 'budget')).toMatchObject({ fulltext_search: false, sections: ['values'] })
  expect(sources.find((item: any) => item.corpus === 'kanpo')).toMatchObject({ list_required: ['date'], document_required: ['date'] })
  const listed = await call(page, 'list_legal_data', { corpus: 'laws', limit: 1 })
  expect(listed.data.items[0].document_args).toEqual({ corpus: 'laws', id: listed.data.items[0].law_id })
})

test('WebMCP: full-text pages are distinct and search hits open their documents and entries', async ({ page }) => {
  test.setTimeout(90_000)
  await install(page)
  await open(page)
  const first = await call(page, 'search_legal_data', { corpus: 'reiki', query: '個人情報', limit: 1 })
  expect(first.data).toMatchObject({ offset: 0, next_offset: 1, has_more: true, candidate_limit: 2000 })
  const second = await call(page, 'search_legal_data', { corpus: 'reiki', query: '個人情報', limit: 1, offset: first.data.next_offset })
  expect(second.isError).toBe(false)
  const identity = (row: any) => [row.municipality_code, row.reiki_id, row.article_id]
  expect(identity(second.data.items[0])).not.toEqual(identity(first.data.items[0]))
  const prefecture = await call(page, 'search_legal_data', { corpus: 'reiki', query: '個人情報', prefecture: '北海道', title_only: true })
  expect(prefecture.data.items.length).toBeGreaterThan(0)
  expect(prefecture.data.items.every((item: any) => item.prefecture === '北海道' && item.article_id === '')).toBe(true)
  for (const [corpus, query] of [['tsutatsu', '住所'], ['pubcomment', '行政回答'], ['gian', '制度改善'], ['shingikai', '配布原文']] as const) {
    const hit = (await call(page, 'search_legal_data', { corpus, query, limit: 1 })).data.items[0]
    expect(hit.document_args.corpus).toBe(corpus)
    const detail = await call(page, 'get_legal_document', hit.document_args)
    expect(detail.isError, JSON.stringify(detail.data)).toBe(false)
    if (hit.entry_args) expect((await call(page, 'get_document_entry', hit.entry_args)).data.entry.text).toContain(query)
    const exhausted = await call(page, 'search_legal_data', { corpus, query, offset: 100 })
    expect(exhausted.isError, JSON.stringify(exhausted.data)).toBe(false)
    expect(exhausted.data).toMatchObject({ items: [], offset: 100, has_more: false, next_offset: null })
  }
  // Exercise the remaining FTS SQL paths with a nonzero offset, even on empty fixtures.
  for (const corpus of ['laws', 'proceedings', 'kanpo']) {
    const result = await call(page, 'search_legal_data', { corpus, query: '法令', offset: 100 })
    expect(result.isError, `${corpus}: ${JSON.stringify(result.data)}`).toBe(false)
  }
})

test('WebMCP: entries keep original indices through nested filtering and sorting', async ({ page }) => {
  await install(page)
  await page.route('**/budget/stats1.json', route => route.fulfill({ json: {
    stats_data_id: 'stats1', title: '統計', source: { provider: 'e-Stat' }, values: [
      { area: '東京', dimensions: { category: '教育' }, value: '10', unit: '円' },
      { area: '大阪', dimensions: { category: '教育' }, value: '20', unit: '円' },
      { area: '東京', dimensions: { category: '福祉' }, value: '30', unit: '円' },
    ],
  } }))
  await page.route('**/proceedings/meeting1.json', route => route.fulfill({ json: {
    meeting_id: 'meeting1', source: { provider: '国会会議録' }, speeches: [
      { speech_id: 'speech1', speaker: '甲', speech: '教育について質問する' },
      { speech_id: 'speech2', speaker: '乙', speech: '福祉について回答する' },
    ],
  } }))
  await page.route('**/pubcomment/case1.json', route => route.fulfill({ json: {
    case_id: 'case1', opinions: [{ item: '教育', opinion: '改善を希望', ministry_response: '検討する' }],
    attachments: [{ name: '結果', url: 'https://example.com/result.pdf', extracted_text: '回答本文' }],
  } }))
  await page.route('**/shingikai/moj/minutes1.json', route => route.fulfill({ json: {
    minutes_id: 'minutes1', attachments: [{ attachment_id: 'att1', label: '資料', extracted_text: '論点' }],
  } }))
  await open(page)
  const budget = await call(page, 'list_document_entries', { corpus: 'budget', id: 'stats1', filters: { 'dimensions.category': '教育' }, sort_by: 'area', sort_order: 'desc', limit: 1 })
  expect(budget.data).toMatchObject({ document: { source: { provider: 'e-Stat' } }, total: 2, next_offset: 1 })
  expect(budget.data.document.values).toBeUndefined()
  const entry = await call(page, 'get_document_entry', { corpus: 'budget', id: 'stats1', entry_index: budget.data.items[0].entry_index })
  expect(entry.data.entry).toEqual(budget.data.items[0])
  expect((await call(page, 'list_document_entries', { corpus: 'budget', id: 'stats1', query: '教育 大阪' })).data.items[0].entry_index).toBe(1)
  expect((await call(page, 'list_document_entries', { corpus: 'proceedings', id: 'meeting1', filters: { speaker: '乙' } })).data.items[0].entry_index).toBe(1)
  expect((await call(page, 'get_document_entry', { corpus: 'proceedings', id: 'meeting1', entry_id: 'speech2' })).data.entry.speech).toContain('福祉')
  expect((await call(page, 'list_document_entries', { corpus: 'pubcomment', id: 'case1', query: '改善' })).data.items[0].ministry_response).toBe('検討する')
  expect((await call(page, 'get_document_entry', { corpus: 'pubcomment', id: 'case1', section: 'attachments', entry_index: 0 })).data.entry.extracted_text).toBe('回答本文')
  expect((await call(page, 'get_document_entry', { corpus: 'shingikai', id: 'minutes1', ministry: 'moj', entry_id: 'att1' })).data.entry.label).toBe('資料')
  const supplementary = await call(page, 'list_document_entries', { id: LAW_ID, revision_id: REVISION, section: 'supplementary', filters: { article_id: 'suppl3_art_1' } })
  expect(supplementary.data.items[0].supplementary_section.section_index).toBeGreaterThanOrEqual(0)
  const kanpo = await call(page, 'get_document_entry', { corpus: 'kanpo', date: '2026-04-02', entry_index: 0 })
  expect(kanpo.data.entry).toMatchObject({ title: expect.stringContaining('郵便法'), issue: { issue_no: '第1678号' } })
})

test('WebMCP: published diffs, dated snapshots, and daily updates preserve provenance', async ({ page }) => {
  await install(page)
  const oldRevision = 'old_revision'
  const summary = { articles_added: 1, articles_removed: 0, articles_modified: 1, articles_unchanged: 2 }
  await page.route(`**/laws/${LAW_ID}/diffs.json`, route => route.fulfill({ json: { law_id: LAW_ID, diffs: [{ from_revision_id: oldRevision, to_revision_id: REVISION, summary }] } }))
  await page.route(`**/laws/${LAW_ID}/diff/${oldRevision}..${REVISION}.json`, route => route.fulfill({ json: {
    law_id: LAW_ID, from: { revision_id: oldRevision }, to: { revision_id: REVISION }, summary, articles: [
      { article_id: 'art_1', change_type: 'modified', paragraphs: [{ change_type: 'modified', text_diff: [{ op: 'insert', text: '教育' }] }] },
      { article_id: 'art_2', change_type: 'added', to: { caption: '追加条文' } },
    ],
  } }))
  await page.route(`**/laws/${LAW_ID}/at/2026-01-01.json`, route => route.fulfill({ json: { law_id: LAW_ID, as_of: '2026-01-01', include_unenforced: false, resolved_revision_id: REVISION, body_available: true } }))
  await page.route(`**/laws/${LAW_ID}/at/1800-01-01.json`, route => route.fulfill({ json: { law_id: LAW_ID, as_of: '1800-01-01', resolved_revision_id: null } }))
  await page.route(`**/laws/${LAW_ID}/at/2025-01-01.json`, route => route.fulfill({ json: { law_id: LAW_ID, resolved_revision_id: oldRevision, body_available: false } }))
  const updates = { date: '2026-01-01', updated_laws: [{ law_id: LAW_ID, title: '民法', change_type: 'modified' }] }
  await page.route('**/updates/latest.json', route => route.fulfill({ json: updates }))
  await page.route('**/updates/2026-01-01.json', route => route.fulfill({ json: updates }))
  await open(page)
  const pair = (await call(page, 'list_law_diffs', { law_id: LAW_ID })).data.items[0]
  const diff = await call(page, 'get_law_diff', { law_id: LAW_ID, from_revision_id: pair.from_revision_id, to_revision_id: pair.to_revision_id, query: '教育', filters: { change_type: 'modified' } })
  expect(diff.data).toMatchObject({ summary, total: 1, items: [{ article_id: 'art_1' }] })
  const snapshot = await call(page, 'get_law_snapshot', { law_id: LAW_ID, date: '2026-01-01', include_body: true })
  expect(snapshot.data).toMatchObject({ as_of: '2026-01-01', include_unenforced: false, document: { law_id: LAW_ID, revision_id: REVISION } })
  expect((await call(page, 'get_law_snapshot', { law_id: LAW_ID, date: '1800-01-01', include_body: true })).data.document).toBeNull()
  expect((await call(page, 'get_law_snapshot', { law_id: LAW_ID, date: '2025-01-01', include_body: true })).isError).toBe(true)
  for (const args of [{}, { date: '2026-01-01', filters: { change_type: 'modified' } }]) {
    expect((await call(page, 'get_law_updates', args)).data).toMatchObject({ date: '2026-01-01', total: 1, items: [{ law_id: LAW_ID }] })
  }
})

test('WebMCP: related documents and Wiki links are traversable in both directions', async ({ page }) => {
  await install(page)
  const targets = [
    { corpus: 'laws', id: LAW_ID, target: 'proceedings', path: `law-to-proceedings/${LAW_ID}`, key: 'linked_proceedings', row: { meeting_id: 'meeting1', confidence: 0.9 } },
    { corpus: 'laws', id: LAW_ID, target: 'pubcomment', path: `law-to-pubcomment/${LAW_ID}`, key: 'linked_pubcomments', row: { case_id: 'case1', confidence: 0.8 } },
    { corpus: 'laws', id: LAW_ID, target: 'tsutatsu', path: `law-to-tsutatsu/${LAW_ID}`, key: 'linked_tsutatsu', row: { tax: 'shotoku', confidence: 0.7 } },
    { corpus: 'proceedings', id: 'meeting1', target: 'laws', path: 'meeting-to-laws/meeting1', key: 'linked_laws', row: { law_id: LAW_ID, confidence: 1 } },
  ]
  for (const target of targets) await page.route(`**/links/${target.path}.json`, route => route.fulfill({ json: { schema_version: 1, [target.key]: [target.row] } }))
  await page.route('**/wiki/graph.json', route => route.fulfill({ json: {
    schema_version: 1, nodes: [{ id: 'laws/civil', title: '民法', type: 'law' }, { id: 'topics/contract', title: '契約', type: 'topic' }, { id: 'topics/property', title: '財産', type: 'topic' }],
    links: [{ source: 'laws/civil', target: 'topics/contract' }, { source: 'topics/property', target: 'laws/civil' }],
  } }))
  await open(page)
  for (const { corpus, id, target, row } of targets) {
    const related = await call(page, 'get_related_documents', { corpus, id, target })
    expect(related.data.items[0]).toMatchObject({ ...row, document_args: { corpus: target } })
  }
  const outgoing = await call(page, 'get_wiki_links', { id: 'laws/civil', query: '契約' })
  expect(outgoing.data).toMatchObject({ total: 1, items: [{ page: { title: '契約' }, document_args: { corpus: 'wiki', id: 'topics/contract' } }] })
  expect((await call(page, 'get_wiki_links', { id: 'topics/property' })).data.items[0].page.title).toBe('民法')
  expect((await call(page, 'get_wiki_links', { id: 'laws/civil' })).data).toMatchObject({ total: 2, graph_type: 'undirected' })
  expect((await call(page, 'get_law_references', { law_id: LAW_ID, direction: 'incoming' })).data).toMatchObject({ total: 0 })
})

test('WebMCP: expanded tools reject ambiguous selectors and unsupported combinations', async ({ page }) => {
  await install(page)
  await page.route('**/gian/217/bill1.json', route => route.fulfill({ json: { fields: [{ key: '審議状況', value: '衆議院' }, { key: '審議状況', value: '参議院' }] } }))
  await open(page)
  for (const [name, args] of [
    ['search_legal_data', { corpus: 'reiki', query: '個人情報', municipality_code: '012122', prefecture: '北海道' }],
    ['search_legal_data', { query: '個人情報', title_only: false }],
    ['search_legal_data', { corpus: 'reiki', query: '個人情報', offset: 2001 }],
    ['list_document_entries', { corpus: 'wiki', id: 'sample' }],
    ['list_document_entries', { corpus: 'proceedings', id: 'meeting1', section: 'opinions' }],
    ['get_document_entry', { corpus: 'gian', id: 'bill1', session: '217' }],
    ['get_document_entry', { corpus: 'gian', id: 'bill1', session: '217', entry_id: '審議状況', entry_index: 0 }],
    ['get_document_entry', { corpus: 'gian', id: 'bill1', session: '217', entry_id: '審議状況' }],
    ['get_document_entry', { corpus: 'gian', id: 'bill1', session: '217', entry_index: 99 }],
    ['get_related_documents', { id: LAW_ID, target: 'laws' }],
    ['get_wiki_links', { id: 'sample', direction: null }],
    ['get_law_snapshot', { law_id: LAW_ID, date: '2026-02-30' }],
    ['get_law_diff', { law_id: LAW_ID, from_revision_id: '../old', to_revision_id: REVISION }],
    ['list_legal_data', { sort_order: 'invalid' }],
  ] as const) {
    const result = await call(page, name, args)
    expect(result.isError, `${name}: ${JSON.stringify(args)}`).toBe(true)
  }
  expect((await call(page, 'get_document_entry', { corpus: 'gian', id: 'bill1', session: '217', entry_index: 1 })).data.entry.value).toBe('参議院')
})

test('WebMCP: cancelling an in-flight request returns before the response arrives', async ({ page }) => {
  await install(page)
  await open(page)
  let release!: () => void, requested!: () => void
  const gate = new Promise<void>(resolve => { release = resolve })
  const started = new Promise<void>(resolve => { requested = resolve })
  await page.route('**/health.json', async route => {
    requested()
    await gate
    await route.fulfill({ json: { ok: true } })
  })
  await page.evaluate(() => {
    const state = window as any
    state.testController = new AbortController()
    state.pendingTool = state.webMcpTools.get_data_status.execute({}, { signal: state.testController.signal })
  })
  try {
    await started
    const result = await page.evaluate(async () => {
      const state = window as any
      state.testController.abort(new Error('cancelled by client'))
      return await state.pendingTool
    })
    expect(result.isError).toBe(true)
    expect(JSON.parse(result.content[0].text).error).toBe('cancelled by client')
  } finally { release() }
  // Other tool calls remain usable after cancellation.
  expect((await call(page, 'list_data_sources')).isError).toBe(false)
})
