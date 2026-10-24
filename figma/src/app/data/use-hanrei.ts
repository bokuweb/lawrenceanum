import { useEffect, useState } from "react";
import { api, type HanreiLawLinks, type HanreiCase } from "./api";

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

export function useHanreiLawLinks(caseId: string) {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ key: string; data: HanreiLawLinks | null; error: boolean }>({ key: caseId, data: null, error: false });
  useEffect(() => {
    let active = true;
    setState({ key: caseId, data: null, error: false });
    api.hanreiLawLinks(caseId).then(data => {
      if (data.schema_version !== 1 || String(data.case_id) !== caseId) throw new Error("case link mismatch");
      if (active) setState({ key: caseId, data, error: false });
    }).catch(() => { if (active) setState({ key: caseId, data: null, error: true }); });
    return () => { active = false; };
  }, [caseId, attempt]);
  return { data: state.key === caseId ? state.data : null, error: state.key === caseId && state.error, retry: () => setAttempt(n => n + 1) };
}

export function useWikiPaths() {
  const [paths, setPaths] = useState<Set<string> | null>(null);
  useEffect(() => {
    let active = true;
    api.wikiIndex().then(index => { if (active) setPaths(new Set(index.pages.filter(p => p.type !== "hanrei" || !p.stub).map(p => p.path))); }).catch(() => {});
    return () => { active = false; };
  }, []);
  return paths;
}
