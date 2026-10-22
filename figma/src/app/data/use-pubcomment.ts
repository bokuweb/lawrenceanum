import { useEffect, useState } from "react";
import { api, type PubcommentCaseMeta, type PubcommentCase, type PubcommentIndex } from "./api";

export function usePubcommentIndex() {
  const [data, setData] = useState<PubcommentIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    api.pubcommentIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(e => { if (!cancelled) { setError(String(e)); setLoading(false); } });
    return () => { cancelled = true; };
  }, [attempt]);

  return { data, loading, error, retry: () => setAttempt(n => n + 1) };
}

export function usePubcommentCase(caseId: string | null) {
  const [data, setData] = useState<PubcommentCase | null>(null);
  const [loading, setLoading] = useState(Boolean(caseId));
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!caseId) { setData(null); setLoading(false); return; }
    let cancelled = false;
    setLoading(true);
    setData(null);
    api.pubcommentCase(caseId)
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [caseId, attempt]);

  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

export type { PubcommentCaseMeta };
