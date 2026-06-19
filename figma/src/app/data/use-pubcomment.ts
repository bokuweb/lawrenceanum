import { useEffect, useState } from "react";
import { api, type PubcommentCaseMeta, type PubcommentCase, type PubcommentIndex } from "./api";

export function usePubcommentIndex() {
  const [data, setData] = useState<PubcommentIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.pubcommentIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(e => { if (!cancelled) { setError(String(e)); setLoading(false); } });
    return () => { cancelled = true; };
  }, []);

  return { data, loading, error };
}

export function usePubcommentCase(caseId: string | null) {
  const [data, setData] = useState<PubcommentCase | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!caseId) { setData(null); return; }
    let cancelled = false;
    setLoading(true);
    api.pubcommentCase(caseId)
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [caseId]);

  return { data, loading };
}

export type { PubcommentCaseMeta };
