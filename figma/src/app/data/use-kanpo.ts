import { useCallback, useEffect, useState } from "react";
import { api, type KanpoDates, type KanpoDay, type KanpoIssue, type KanpoItem } from "./api";

/** `kanpo/index.json`（配信している日付の一覧）。 */
export function useKanpoDates() {
  const [data, setData] = useState<KanpoDates | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt(n => n + 1), []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(false);
    api.kanpoDates()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) { setError(true); setLoading(false); } });
    return () => { cancelled = true; };
  }, [attempt]);

  return { data, loading, error, retry };
}

/** `kanpo/{date}/index.json`（その日の号と項目）。 */
export function useKanpoDay(date: string | null) {
  const [state, setState] = useState<{
    date: string | null; data: KanpoDay | null; loading: boolean; error: boolean;
  }>({ date: null, data: null, loading: false, error: false });
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt(n => n + 1), []);

  useEffect(() => {
    if (!date) { setState({ date: null, data: null, loading: false, error: false }); return; }
    let cancelled = false;
    setState({ date, data: null, loading: true, error: false });
    api.kanpoOnDate(date)
      .then(data => { if (!cancelled) setState({ date, data, loading: false, error: false }); })
      .catch(() => { if (!cancelled) setState({ date, data: null, loading: false, error: true }); });
    return () => { cancelled = true; };
  }, [date, attempt]);

  // 日付変更直後の描画にも、前の日付の本文やエラーを混ぜない。
  return {
    data: date && state.date === date ? state.data : null,
    loading: !!date && (state.date !== date || state.loading),
    error: !!date && state.date === date && state.error,
    retry,
  };
}

/** 号の種別の表示名。 */
export function kanpoIssueLabel(issue: Pick<KanpoIssue, "issue_type" | "issue_no">): string {
  const kind = ({ regular: "本紙", extra: "号外", special_extra: "特別号外", procurement: "政府調達" } as Record<string, string>)[issue.issue_type];
  return kind ? `${kind} ${issue.issue_no}` : issue.issue_no;
}

/**
 * 検索結果（kanpo_fts の 号・頁・標題）から、その日の何号目の何項目目かを引く。
 * 索引は号・項目の並び順を持たないので、標題で一致させ、同じ標題が複数あれば頁で絞る。
 */
export function findKanpoItem(
  day: KanpoDay,
  key: { issueNo?: string | null; page?: number | null; title: string },
): { issue: number; item: number } | null {
  let fallback: { issue: number; item: number } | null = null;
  for (const [i, issue] of day.issues.entries()) {
    if (key.issueNo && issue.issue_no !== key.issueNo) continue;
    for (const [j, item] of issue.items.entries()) {
      if (item.title !== key.title) continue;
      if (!key.page || item.page === key.page) return { issue: i, item: j };
      fallback ??= { issue: i, item: j };
    }
  }
  return fallback;
}
