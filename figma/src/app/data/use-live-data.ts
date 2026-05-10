import { useEffect, useState } from 'react'
import { api, type Health, type LawsIndex, type UpdatesByDate } from './api'

export type LiveSnapshot = {
  loading: boolean
  error: string | null
  laws: LawsIndex | null
  health: Health | null
  latestUpdates: UpdatesByDate | null
  /** 直近 14 日分の更新件数 (古い順)。`updates/{date}.json` が無い日は 0 件として埋める。 */
  trend14: { date: string; count: number }[]
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
          count: v?.updated_laws.length ?? 0,
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
