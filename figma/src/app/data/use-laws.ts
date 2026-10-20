import { useCallback, useEffect, useState } from 'react'
import { api, type LawDocumentRaw, type LawsIndex, type TimelineJson, type VersionsJson } from './api'
import { type LawCategory, type LawSummary } from '../components/mock-data'

/** Live JSON のフィールドを mock-data の `LawSummary` 形に寄せる。 */
function adapt(raw: {
  law_id: string
  law_num: string | null
  title: string
  category?: string | null
  last_updated?: string | null
  article_count?: number
}): LawSummary {
  return {
    law_id: raw.law_id,
    law_num: raw.law_num ?? '',
    title: raw.title,
    category: (raw.category as LawCategory) ?? ('行政' as LawCategory),
    promulgation_date: '',
    effective_date: '',
    last_updated: raw.last_updated ?? '',
    status: 'current',
    // 条数を持たない古い index では 0 (= 不明) とし、UI 側で表示しない。
    article_count: raw.article_count ?? 0,
  }
}

export function useLaws() {
  const [attempt, setAttempt] = useState(0)
  const retry = useCallback(() => setAttempt(n => n + 1), [])
  const [state, setState] = useState<{ loading: boolean; laws: LawSummary[]; live: boolean; error: boolean }>({
    loading: true,
    laws: [],
    live: false,
    error: false,
  })
  useEffect(() => {
    let cancelled = false
    setState({ loading: true, laws: [], live: false, error: false })
    api
      .lawsIndex()
      .then((idx: LawsIndex) => {
        if (cancelled) return
        setState({ loading: false, laws: idx.laws.map(adapt), live: true, error: false })
      })
      .catch(() => {
        if (cancelled) return
        setState({ loading: false, laws: [], live: false, error: true })
      })
    return () => {
      cancelled = true
    }
  }, [attempt])
  return { ...state, retry }
}

/** Each request settles independently, so optional metadata cannot hold up the body. */
function useLawResource<T>(lawId: string | null | undefined, fetcher: (id: string) => Promise<T>) {
  const [attempt, setAttempt] = useState(0)
  const retry = useCallback(() => setAttempt(n => n + 1), [])
  const [state, setState] = useState<{
    lawId: typeof lawId; loading: boolean; data: T | null; error: string | null
  }>({
    lawId,
    loading: !!lawId,
    data: null,
    error: null,
  })
  useEffect(() => {
    if (!lawId) {
      setState({ lawId, loading: false, data: null, error: null })
      return
    }
    let cancelled = false
    setState({ lawId, loading: true, data: null, error: null })
    fetcher(lawId)
      .then(data => {
        if (!cancelled) setState({ lawId, loading: false, data, error: null })
      })
      .catch(error => {
        if (!cancelled) setState({ lawId, loading: false, data: null, error: String(error) })
      })
    return () => {
      cancelled = true
    }
  }, [lawId, fetcher, attempt])
  // Also hide the previous law on the render before the effect runs.
  return {
    ...(state.lawId === lawId ? state : { loading: !!lawId, data: null, error: null }),
    retry,
  }
}

export function useLawDetail(lawId: string | null | undefined) {
  const doc = useLawResource<LawDocumentRaw>(lawId, api.law)
  const versions = useLawResource<VersionsJson>(lawId, api.versions)
  const timeline = useLawResource<TimelineJson>(lawId, api.timeline)
  return {
    loading: doc.loading, doc: doc.data, error: doc.error, retry: doc.retry,
    versions: versions.data, versionsLoading: versions.loading,
    versionsError: versions.error, retryVersions: versions.retry,
    timeline: timeline.data, timelineLoading: timeline.loading,
    timelineError: timeline.error, retryTimeline: timeline.retry,
  }
}
