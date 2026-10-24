import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api, type HanreiCase, type HanreiCitation } from "../../data/api";
import { Button } from "../ui/button";

const sourceLabels = { holding: "判示事項", summary: "裁判要旨", body_text: "本文（PDF抽出）" };

/** 保存時の引用と現在の原文の完全一致だけを示す。解説の意味的な正しさは判定しない。 */
export function HanreiCitationContext({ caseId, citations, source }: { caseId: string; citations: HanreiCitation[]; source?: HanreiCase }) {
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<HanreiCase | null>(null);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const own = citations.filter(c => Object.hasOwn(sourceLabels, c.field) && c.quote.trim()
    && c.reference === `hanrei:${caseId}#${c.field === "body_text" ? "body" : c.field}`);
  const current = source ?? (String(loaded?.case_id) === caseId ? loaded : null);
  useEffect(() => {
    if (!open || current || !own.length) return;
    let active = true;
    setError(false);
    api.hanreiCase(caseId).then(data => {
      if (String(data.case_id) !== caseId) throw new Error("case source mismatch");
      if (active) setLoaded(data);
    }).catch(() => { if (active) setError(true); });
    return () => { active = false; };
  }, [open, current, caseId, own.length, attempt]);
  if (!own.length) return null;
  return <details className="border-t pt-3" data-testid="hanrei-citation-context" onToggle={e => setOpen(e.currentTarget.open)}>
    <summary className="cursor-pointer text-sm font-medium">引用箇所を原文で確認（{own.length}件）</summary>
    <p className="text-xs text-muted-foreground mt-2">引用と配信中の原文が完全一致する箇所の前後を表示します。最初の一致箇所を示しており、解説の内容が正しいことを保証するものではありません。</p>
    {!current ? error ? <div role="alert" className="text-sm mt-3">
      <p>引用元の原文を読み込めないか、事件との対応を確認できませんでした。</p>
      <Button variant="outline" size="sm" className="mt-2" onClick={() => setAttempt(n => n + 1)}>引用元を再試行</Button>
    </div> : <p role="status" className="text-sm mt-3">引用元の原文を読み込み中…</p> : <ul className="mt-3 space-y-4">
      {own.map((c, i) => {
        const text = current[c.field] ?? "";
        const start = text.indexOf(c.quote);
        const end = start + c.quote.length;
        return <li key={`${c.label}-${i}`} className="space-y-2 text-sm">
          <p className="font-medium">引用 {c.label} · {sourceLabels[c.field]}</p>
          {start >= 0 ? <>
            <blockquote className="border-l-2 pl-3 whitespace-pre-wrap leading-7">{start > 100 && "…"}{text.slice(Math.max(0, start - 100), start)}<mark>{c.quote}</mark>{text.slice(end, end + 100)}{text.length > end + 100 && "…"}</blockquote>
            {source ? <Button variant="outline" size="sm" onClick={() => {
              const target = document.getElementById(`hanrei-source-${c.field}`);
              target?.focus({ preventScroll: true });
              target?.scrollIntoView({ block: "start" });
            }}>{sourceLabels[c.field]}の原文へ移動</Button>
              : <Link to={`/hanrei/${caseId}?source=${c.field}`} className="inline-block text-primary underline">{sourceLabels[c.field]}の原文へ移動</Link>}
          </> : <>
            <blockquote className="border-l-2 pl-3 whitespace-pre-wrap leading-7">{c.quote}</blockquote>
            <p className="text-xs text-muted-foreground">配信中の原文には完全一致する箇所がありません。資料の更新や空白・改行の違い、本文の未収録が考えられます。解説の出典リンクで確認してください。</p>
          </>}
        </li>;
      })}
    </ul>}
  </details>;
}
