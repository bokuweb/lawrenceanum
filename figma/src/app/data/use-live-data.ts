import { useEffect, useState } from 'react'
import { api, type Health, type LawsIndex, type UpdatesByDate } from './api'

export type LiveSnapshot = {
  loading: boolean
  error: string | null
  laws: LawsIndex | null
  health: Health | null
  latestUpdates: UpdatesByDate | null
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
  })

  useEffect(() => {
    let cancelled = false
    Promise.allSettled([api.lawsIndex(), api.health(), api.latestUpdates()]).then(
      ([lawsR, healthR, updatesR]) => {
        if (cancelled) return
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
        })
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

  return state
}
