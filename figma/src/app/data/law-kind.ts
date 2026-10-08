import { type UpdateEntry } from './api'

/** ダッシュボードの更新内訳で使う法令種別。表示順もこの順。 */
export const LAW_KINDS = ['法律', '政令', '府省令', 'その他'] as const
export type LawKind = (typeof LAW_KINDS)[number]

/**
 * e-Gov の law_id (`{元号}{年2桁}{種別コード}{番号}`) から法令種別を返す。
 * AC=法律 / CO=政令 / M=府省令。勅令・規則・憲法・太政官布告などはその他に寄せる。
 */
export function lawKindOf(lawId: string): LawKind {
  const code = /^\d{3}([A-Z]+)/.exec(lawId)?.[1]
  if (code === 'AC') return '法律'
  if (code === 'CO') return '政令'
  if (code === 'M') return '府省令'
  return 'その他'
}

export type UpdateBreakdown = {
  /** MM-DD (軸ラベル) */
  date: string
  total: number
  /** 法令種別ごとの件数。積み上げ棒のキー。 */
  kinds: Record<LawKind, number>
  changes: { added: number; modified: number; removed: number }
  /** tooltip に出す法令名 (先頭数件)。 */
  titles: string[]
}

export function breakdownUpdates(date: string, laws: UpdateEntry[]): UpdateBreakdown {
  const kinds: Record<LawKind, number> = { 法律: 0, 政令: 0, 府省令: 0, その他: 0 }
  const changes = { added: 0, modified: 0, removed: 0 }
  for (const l of laws) {
    kinds[lawKindOf(l.law_id)] += 1
    changes[l.change_type] += 1
  }
  return { date, total: laws.length, kinds, changes, titles: laws.map(l => l.title) }
}
