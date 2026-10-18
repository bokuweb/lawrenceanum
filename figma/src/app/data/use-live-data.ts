import { useEffect, useState } from 'react'
import {
  api,
  type EnforcementUpcoming,
  type Health,
  type LawsIndex,
  type RecentFeed,
  type UpdatesByDate,
  type UpdateEntry,
  type WikiIndex,
} from './api'

export type UpdateDay = {
  /** MM-DD (表示用) */
  date: string
  /** YYYY-MM-DD (ルーティング・API 用) */
  fullDate: string
  count: number
  laws: UpdateEntry[]
}

export type LiveSnapshot = {
  loading: boolean
  error: string | null
  laws: LawsIndex | null
  health: Health | null
  latestUpdates: UpdatesByDate | null
  /** 直近 14 日分の更新（古い順）。`updates/{date}.json` が無い日は 0 件として埋める。 */
  trend14: UpdateDay[]
}

/**
 * ダッシュボードに必要な静的 JSON を並列に取得する。失敗時は値を null のまま
 * 残し `error` をセットする — 呼び元はモックにフォールバックできる。
 */
export function useLiveSnapshot(): LiveSnapshot {
  const [state, setState] = useState<LiveSnapshot>({
    loading: true,
    error: null,
    laws: null,
    health: null,
    latestUpdates: null,
    trend14: [],
  })

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const today = new Date()
      const dates: string[] = []
      for (let i = 13; i >= 0; i--) {
        const d = new Date(today)
        d.setUTCDate(d.getUTCDate() - i)
        dates.push(d.toISOString().slice(0, 10))
      }
      const [lawsR, healthR, updatesR, ...perDay] = await Promise.allSettled([
        api.lawsIndex(),
        api.health(),
        api.latestUpdates(),
        ...dates.map(d => api.updatesOnDate(d).catch(() => null)),
      ])
      if (cancelled) return
      const trend14 = perDay.map((r, i) => {
        const v = r.status === 'fulfilled' ? r.value : null
        return {
          date: dates[i].slice(5),  // MM-DD だけ表示用に短縮
          fullDate: dates[i],
          count: v?.updated_laws.length ?? 0,
          laws: v?.updated_laws ?? [],
        }
      })
      setState({
        loading: false,
        error:
          [lawsR, healthR, updatesR]
            .map((r) => (r.status === 'rejected' ? String(r.reason) : ''))
            .filter(Boolean)
            .join('; ') || null,
        laws: lawsR.status === 'fulfilled' ? lawsR.value : null,
        health: healthR.status === 'fulfilled' ? healthR.value : null,
        latestUpdates: updatesR.status === 'fulfilled' ? updatesR.value : null,
        trend14,
      })
    })()
    return () => {
      cancelled = true
    }
  }, [])

  return state
}

export type DashboardExtras = {
  loading: boolean
  /** LLM wiki のページ一覧。未配信なら null。 */
  wiki: WikiIndex | null
  /** 規制変化フィード (法令改正・法案・パブコメ・官報の新着)。 */
  feed: RecentFeed | null
  /** 今後の施行予定。 */
  enforcement: EnforcementUpcoming | null
}

/**
 * ダッシュボードの追加パネル (wiki / 新着 / 施行予定) 用の静的 JSON。
 * どれも欠けていても他のパネルを出せるように、1 本ずつ独立に null へ倒す。
 * 法令の数値 (useLiveSnapshot) は先に描けるので、こちらは別フックに分けている。
 */
export function useDashboardExtras(): DashboardExtras {
  const [state, setState] = useState<DashboardExtras>({
    loading: true,
    wiki: null,
    feed: null,
    enforcement: null,
  })

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const [wiki, feed, enforcement] = await Promise.all([
        api.wikiIndex().catch(() => null),
        api.recentFeed().catch(() => null),
        api.enforcementUpcoming().catch(() => null),
      ])
      if (cancelled) return
      setState({ loading: false, wiki, feed, enforcement })
    })()
    return () => {
      cancelled = true
    }
  }, [])

  return state
}
