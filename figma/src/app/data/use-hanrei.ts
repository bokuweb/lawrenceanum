import { useEffect, useState } from "react";
import { api, type HanreiCase } from "./api";

export function useHanreiCase(caseId: string | number) {
  const key = String(caseId);
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ key: string; data: HanreiCase | null; loading: boolean; error: boolean }>({
    key, data: null, loading: true, error: false,
  });
  useEffect(() => {
    let active = true;
    setState({ key, data: null, loading: true, error: false });
    api.hanreiCase(key).then(data => {
      if (active) setState({ key, data, loading: false, error: false });
    }).catch(() => {
      if (active) setState({ key, data: null, loading: false, error: true });
    });
    return () => { active = false; };
  }, [key, attempt]);
  return {
    data: state.key === key ? state.data : null,
    loading: state.key !== key || state.loading,
    error: state.key === key && state.error,
    retry: () => setAttempt(n => n + 1),
  };
}
