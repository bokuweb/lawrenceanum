import { useEffect, useState } from 'react'
import { api, type LawDocumentRaw, type LawsIndex, type TimelineJson, type VersionsJson } from './api'
import { LAWS, type LawCategory, type LawSummary } from '../components/mock-data'

/** Live JSON のフィールドを mock-data の `LawSummary` 形に寄せる。 */
function adapt(raw: { law_id: string; law_num: string | null; title: string }): LawSummary {
  const matched = LAWS.find((m) => m.law_id === raw.law_id)
  if (matched) {
    return {
      ...matched,
      title: raw.title || matched.title,
      law_num: raw.law_num || matched.law_num,
    }
  }
  return {
    law_id: raw.law_id,
    law_num: raw.law_num ?? '',
    title: raw.title,
    category: '行政' as LawCategory,
    promulgation_date: '',
    effective_date: '',
    last_updated: '',
    status: 'current',
    article_count: 0,
  }
}

export function useLaws(): { loading: boolean; laws: LawSummary[]; live: boolean } {
  const [state, setState] = useState<{ loading: boolean; laws: LawSummary[]; live: boolean }>({
    loading: true,
    laws: LAWS,
    live: false,
  })
  useEffect(() => {
    let cancelled = false
    api
      .lawsIndex()
      .then((idx: LawsIndex) => {
        if (cancelled) return
        setState({ loading: false, laws: idx.laws.map(adapt), live: true })
      })
      .catch(() => {
        if (cancelled) return
        setState({ loading: false, laws: LAWS, live: false })
      })
    return () => {
      cancelled = true
    }
  }, [])
  return state
}

export type LawDetail = {
  loading: boolean
  doc: LawDocumentRaw | null
  versions: VersionsJson | null
  timeline: TimelineJson | null
  error: string | null
}

export function useLawDetail(lawId: string | null | undefined): LawDetail {
  const [state, setState] = useState<LawDetail>({
    loading: !!lawId,
    doc: null,
    versions: null,
    timeline: null,
    error: null,
  })
  useEffect(() => {
    if (!lawId) {
      setState({ loading: false, doc: null, versions: null, timeline: null, error: null })
      return
    }
    let cancelled = false
    // lawId が変わったら前の law の doc を即破棄する。
    // そうしないと BrowseView 側で「前回開いた法令の中身が一瞬チラ見えする」現象が出る。
    setState({ loading: true, doc: null, versions: null, timeline: null, error: null })
    Promise.allSettled([api.law(lawId), api.versions(lawId), api.timeline(lawId)]).then(
      ([docR, vR, tR]) => {
        if (cancelled) return
        setState({
          loading: false,
          doc: docR.status === 'fulfilled' ? docR.value : null,
          versions: vR.status === 'fulfilled' ? vR.value : null,
          timeline: tR.status === 'fulfilled' ? tR.value : null,
          error:
            docR.status === 'rejected' ? String(docR.reason) : null,
        })
      },
    )
    return () => {
      cancelled = true
    }
  }, [lawId])
  return state
}
