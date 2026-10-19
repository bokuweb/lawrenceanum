import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { api, type EnforcementExplanations } from "../../data/api";
import { WikiMarkdown } from "./wiki-markdown";

// 同じ法令の段階施行は1回の取得で表示する。失敗はキャッシュせず再試行可能にする。
const requests = new Map<string, Promise<EnforcementExplanations>>();
function load(lawId: string) {
  let request = requests.get(lawId);
  if (!request) {
    request = api.enforcementExplanations(lawId);
    requests.set(lawId, request);
    request.catch(() => { requests.delete(lawId); });
  }
  return request;
}

type Props = {
  lawId: string;
  eventId?: string;
  revisionId?: string | null;
  explanationId?: string;
  date: string;
  initialOpen?: boolean;
};

export function EnforcementExplanation({ lawId, eventId, revisionId, explanationId, date, initialOpen = false }: Props) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(initialOpen);
  const [result, setResult] = useState<{ lawId: string; data: EnforcementExplanations | null; error: boolean } | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setResult(null);
    load(lawId).then(data => {
      if (!cancelled) setResult({ lawId, data, error: false });
    }).catch(error => {
      if (!cancelled) setResult({ lawId, data: null, error: !String(error).includes("404 ") });
    });
    return () => { cancelled = true; };
  }, [lawId, open, attempt]);
  const current = result?.lawId === lawId ? result : null;
  const explanation = current?.data?.law_id === lawId ? current.data.items.find(item =>
    item.date === date && (explanationId
      ? item.explanation_id === explanationId
      : !!eventId && item.event_id === eventId && (item.revision_id ?? null) === (revisionId ?? null))
  ) : undefined;

  return (
    <details open={open} onToggle={e => setOpen(e.currentTarget.open)} className="text-sm">
      <summary className="cursor-pointer text-primary py-2">変更内容・経緯・根拠</summary>
      {open && (
        <div className="pb-3" aria-live="polite">
          {!current ? <p className="text-muted-foreground">解説を読み込み中…</p> : explanation ? (
            <>
              <p className="text-xs text-muted-foreground mb-2">AIによる要約 · 対象施行日 {date} · 各節の出典から原文を確認できます</p>
              <WikiMarkdown body={explanation.body} from={explanation.path} onOpen={path => navigate(`/wiki/${path}`)} />
            </>
          ) : current.error ? (
            <p className="text-muted-foreground">解説を読み込めませんでした。
              <button type="button" className="ml-2 text-primary hover:underline" onClick={() => setAttempt(n => n + 1)}>再試行</button>
            </p>
          ) : <p className="text-muted-foreground">この施行予定の出典付き解説はまだありません。</p>}
        </div>
      )}
    </details>
  );
}
