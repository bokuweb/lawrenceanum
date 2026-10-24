import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api, hanreiUrl, type HanreiCase, type WikiPageMeta } from "../../data/api";
import { caseComparisonReport, type ResearchProject } from "../../data/topic-watch";
import { useHanreiLawLinks } from "../../data/use-hanrei";
import { HanreiExplanation } from "./hanrei-explanation";
import { LinkedCaseText } from "./hanrei-law-view";

type CaseState = { data: HanreiCase | null; error: boolean };

export function TopicCaseComparison({ ids, notes, project, onRemove, onNote }: {
  ids: string[]; notes: Record<string, string>; project?: ResearchProject; onRemove: (id: string) => void; onNote: (id: string, note: string) => void;
}) {
  const [cases, setCases] = useState<Record<string, CaseState>>({});
  const [attempt, setAttempt] = useState(0);
  const [wiki, setWiki] = useState<WikiPageMeta[] | null>(null);
  const [wikiError, setWikiError] = useState(false);
  const [wikiAttempt, setWikiAttempt] = useState(0);
  const key = ids.join(",");
  useEffect(() => {
    let active = true;
    setCases({});
    for (const id of key.split(",").filter(Boolean)) {
      api.hanreiCase(id).then(data => {
        if (String(data.case_id) !== id) throw new Error("case comparison mismatch");
        if (active) setCases(prev => ({ ...prev, [id]: { data, error: false } }));
      }).catch(() => { if (active) setCases(prev => ({ ...prev, [id]: { data: null, error: true } })); });
    }
    return () => { active = false; };
  }, [key, attempt]);
  useEffect(() => {
    let active = true;
    setWiki(null); setWikiError(false);
    api.wikiIndex().then(index => {
      if (!Array.isArray(index.pages)) throw new Error("invalid wiki index");
      if (active) setWiki(index.pages.filter(page => page.type !== "hanrei" || !page.stub));
    }).catch(() => { if (active) setWikiError(true); });
    return () => { active = false; };
  }, [wikiAttempt]);
  const paths = wiki ? new Set(wiki.map(page => page.path)) : null;
  const ready = ids.length === 2 && ids.every(id => cases[id]?.data);
  const download = () => {
    if (!ready) return;
    const explained = wiki ? new Set(wiki.filter(page => page.type === "hanrei" && !page.stub).map(page => page.path)) : null;
    const blob = new Blob([caseComparisonReport(ids.map(id => cases[id].data!), notes, explained, window.location.href, project)], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = `hanrei-comparison-${ids.join("-")}.md`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return <section aria-label="判例の比較" className="rounded-lg border border-border p-4 space-y-4 [overflow-wrap:anywhere]">
    <div className="flex flex-wrap gap-3 items-center justify-between">
      <h2 className="text-base font-semibold">判例の比較（{ids.length}/2件）</h2>
      <button type="button" disabled={!ready} onClick={download} className="text-sm text-primary underline disabled:opacity-40">比較メモを出力</button>
    </div>
    {project && <p className="text-sm font-medium">調査案件: {project.title}</p>}
    <p className="text-xs text-muted-foreground">比較する判例を一覧から2件選んでください。比較対象と各判例のメモはこのブラウザに保存します。未読の絞り込みやフィードの更新後も比較を再開できます。</p>
    <p className="text-xs text-muted-foreground">結論の違いだけでは、事案の違いや判例変更は判断できません。裁判要旨・理由・当時の条文候補を原文と照合してください。</p>
    {wikiError ? <p role="status" className="text-xs">AI解説の収録状況を確認できませんでした。<button type="button" onClick={() => setWikiAttempt(n => n + 1)} className="ml-2 text-primary underline">Wikiの確認を再試行</button></p>
      : wiki === null && <p role="status" className="text-xs text-muted-foreground">AI解説の収録状況を確認中…</p>}
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      {ids.map(id => <div key={id} className="min-w-0 border border-border rounded-lg p-3 space-y-3">
        <div className="flex gap-2 justify-between items-start">
          <h3 className="text-sm font-semibold">{cases[id]?.data?.case_name || `判例 ${id}`}</h3>
          <button type="button" onClick={() => onRemove(id)} className="text-xs text-primary underline shrink-0" aria-label={`判例${id}を比較から外す`}>比較から外す</button>
        </div>
        {cases[id]?.error ? <div role="alert" className="text-sm"><p>判例を読み込めないか、比較対象との対応を確認できませんでした。</p><button type="button" onClick={() => setAttempt(n => n + 1)} className="text-primary underline mt-2">判例を再試行</button></div>
          : !cases[id]?.data ? <p role="status" className="text-sm text-muted-foreground">判例を読み込み中…</p>
            : <CaseContent source={cases[id].data!} paths={paths} wiki={wiki?.find(page => page.type === "hanrei" && !page.stub && page.path === `hanrei/${id}`)} />}
        <label className="block text-sm">判例{id}の比較メモ<textarea maxLength={2000} rows={3} value={notes[id] ?? ""} onChange={event => onNote(id, event.target.value)} placeholder="事案の違い、確認した理由、未確認の点など" className="block w-full mt-1 p-2 rounded border border-border bg-background text-sm" /></label>
      </div>)}
    </div>
    <p className="text-xs text-muted-foreground">出力には裁判所公表データ、原文・条文候補・公開済みAI解説へのリンク、自分の比較メモを含みます。判例の比較対象とメモは共有・同期されません。</p>
  </section>;
}

function CaseContent({ source, paths, wiki }: { source: HanreiCase; paths: ReadonlySet<string> | null; wiki?: WikiPageMeta }) {
  const id = String(source.case_id);
  const links = useHanreiLawLinks(id);
  const [open, setOpen] = useState(false);
  return <>
    <dl className="text-sm space-y-2">
      {[["事件番号", source.case_number], ["裁判年月日", source.judged_on], ["裁判所", source.court], ["結果", source.result]].map(([label, text]) => <div key={label}><dt className="text-xs text-muted-foreground">{label}</dt><dd>{text || "未収録"}</dd></div>)}
    </dl>
    {[["裁判所公表の判示事項", source.holding, "holding"], ["裁判所公表の裁判要旨", source.summary, "summary"], ["参照法条（裁判所公表）", source.referenced_laws, "referenced_laws"]].map(([label, text, field]) => <div key={field} className="space-y-1">
      <h4 className="text-xs font-medium">{label}</h4>
      <p className="whitespace-pre-wrap text-sm leading-6">{text?.trim() ? <LinkedCaseText text={text} field={field} links={links.data} /> : "未収録"}</p>
    </div>)}
    {links.error && <p role="status" className="text-xs text-muted-foreground">条文候補のリンクを確認できませんでした。<button type="button" onClick={links.retry} className="ml-1 text-primary underline">条文リンクを再試行</button></p>}
    <div className="flex flex-wrap gap-3 text-xs">
      <a href={hanreiUrl(source.case_id)} target="_blank" rel="noreferrer" className="text-primary underline">裁判所の原文</a>
      <Link to={`/hanrei/${id}?source=summary`} className="text-primary underline">裁判要旨・本文・条文候補を確認</Link>
    </div>
    {wiki ? <div className="border-t border-border pt-3 space-y-2">
      <h4 className="text-xs font-medium">AI概要（LLM生成）</h4>
      <p className="text-sm whitespace-pre-wrap">{wiki.description || "概要は解説本文で確認できます。"}</p>
      <p className="text-xs text-muted-foreground">LLMによる要約・解説です。引用先の原文と照合してください。</p>
      <button type="button" aria-expanded={open} onClick={() => setOpen(value => !value)} className="text-xs text-primary underline">{open ? "AI解説を閉じる" : "AI解説・引用根拠をこの場で読む"}</button>
      {open && <HanreiExplanation caseId={id} wikiPaths={paths} source={source} />}
      <Link to={`/wiki/hanrei/${id}`} className="block text-xs text-primary underline">Wikiで解説・関連判例を読む</Link>
    </div> : paths && <p className="text-xs text-muted-foreground">公開済みのAI解説は未収録です。裁判所の原文を確認してください。</p>}
  </>;
}
