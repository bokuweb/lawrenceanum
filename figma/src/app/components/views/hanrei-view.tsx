import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { ArrowLeft, ExternalLink, FileText, Gavel, Search } from "lucide-react";
import { api, hanreiUrl, type HanreiIndex } from "../../data/api";
import { useHanreiCase, useHanreiLawLinks, useWikiPaths } from "../../data/use-hanrei";
import { Input } from "../ui/input";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

import { LinkedCaseText, CaseLawEvidence, caseArticleUrl, articleLabel } from "./hanrei-law-view";

const PAGE_SIZE = 50;

export function HanreiView({ caseId }: { caseId: string | null }) {
  return caseId ? <HanreiDetail key={caseId} caseId={caseId} /> : <HanreiList />;
}

function HanreiList() {
  const [params, setParams] = useSearchParams();
  const query = params.get("filter") ?? "";
  const court = params.get("court") ?? "all";
  const [data, setData] = useState<HanreiIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [limit, setLimit] = useState(PAGE_SIZE);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(false);
    api.hanreiIndex().then(d => {
      if (active) { setData(d); setLoading(false); }
    }).catch(() => {
      if (active) { setError(true); setLoading(false); }
    });
    return () => { active = false; };
  }, [attempt]);
  useEffect(() => { setLimit(PAGE_SIZE); }, [query, court]);
  const courts = useMemo(() => [...new Set((data?.cases ?? []).map(c => c.court).filter(Boolean))].sort(), [data]);
  const filtered = useMemo(() => {
    const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    return (data?.cases ?? []).filter(c => {
      const text = [c.case_name, c.case_number, c.court, c.judged_on, c.holding, c.result].join(" ").toLocaleLowerCase();
      return (court === "all" || c.court === court) && words.every(word => text.includes(word));
    });
  }, [data, query, court]);
  const update = (key: string, value: string) => setParams(prev => {
    const next = new URLSearchParams(prev);
    if (value && value !== "all") next.set(key, value); else next.delete(key);
    return next;
  }, { replace: true });
  const suffix = params.toString() ? `?${params}` : "";
  return (
    <div className="p-4 md:p-6 max-w-5xl mx-auto [overflow-wrap:anywhere]">
      <h1 className="text-2xl flex items-center gap-2"><Gavel className="size-6" />判例</h1>
      <p className="text-sm text-muted-foreground mt-1">収集済みの裁判例を新しい登録順で表示します。出典: 裁判所 裁判例検索</p>
      <div className="flex flex-wrap gap-2 items-center my-4">
        <div className="relative flex-1 min-w-0 basis-64">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
          <Input aria-label="判例一覧の絞り込み" placeholder="事件名・事件番号・判示事項で絞り込み…" value={query} onChange={e => update("filter", e.target.value)} className="pl-8" />
        </div>
        <Select value={court} onValueChange={v => update("court", v)}>
          <SelectTrigger aria-label="裁判所" className="w-full sm:w-64"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="all">すべての裁判所</SelectItem>{courts.map(c => <SelectItem key={c} value={c}>{c}</SelectItem>)}</SelectContent>
        </Select>
      </div>
      <div className="flex flex-wrap justify-between gap-2 mb-3 text-xs text-muted-foreground">
        <span>{data ? `${filtered.length.toLocaleString()} / ${data.count.toLocaleString()}件` : ""}</span>
        <Link to={query.trim() ? `/search?q=${encodeURIComponent(query.trim())}` : "/search"} className="text-primary hover:underline">横断検索で本文を検索</Link>
      </div>
      {loading ? <p className="p-8 text-center text-sm text-muted-foreground">判例一覧を読み込み中…</p>
        : error ? <div role="alert" className="p-8 text-center text-sm"><p>判例一覧を読み込めませんでした</p><Button variant="outline" size="sm" className="mt-2" onClick={() => setAttempt(n => n + 1)}>再試行</Button></div>
        : filtered.length === 0 ? <p className="p-8 text-center text-sm text-muted-foreground">該当する判例がありません</p>
        : <div className="border border-border rounded-lg divide-y divide-border overflow-hidden">
          {filtered.slice(0, limit).map(c => (
            <Link key={c.case_id} to={`/hanrei/${c.case_id}${suffix}`} className="block p-4 hover:bg-accent/50 transition-colors">
              <div className="flex flex-wrap items-center gap-2"><span className="font-medium">{c.case_name || c.case_number}</span>{c.result && <Badge variant="outline">{c.result}</Badge>}</div>
              <p className="text-xs text-muted-foreground mt-1">{[c.judged_on, c.court, c.case_number].filter(Boolean).join(" · ")}</p>
              {c.holding && <p className="text-sm text-muted-foreground mt-2 line-clamp-2 whitespace-pre-wrap">{c.holding}</p>}
            </Link>
          ))}
        </div>}
      {!loading && !error && filtered.length > limit && <Button variant="outline" className="w-full mt-3" onClick={() => setLimit(n => n + PAGE_SIZE)}>さらに表示（残り {(filtered.length - limit).toLocaleString()}件）</Button>}
    </div>
  );
}

function HanreiDetail({ caseId }: { caseId: string }) {
  const [params] = useSearchParams();
  const { data, loading, error, retry } = useHanreiCase(caseId);
  const lawLinks = useHanreiLawLinks(caseId);
  const wikiPaths = useWikiPaths();
  const backParams = new URLSearchParams();
  for (const key of ["filter", "court"]) if (params.has(key)) backParams.set(key, params.get(key)!);
  const back = `/hanrei${backParams.size ? `?${backParams}` : ""}`;
  const metadata = data ? [
    ["事件番号", data.case_number], ["裁判年月日", data.judged_on], ["裁判所", data.court],
    ["裁判種別", data.judgment_type], ["結果", data.result], ["判例集等", data.reporter],
    ["原審裁判所", data.lower_court], ["原審事件番号", data.lower_case_number], ["原審裁判年月日", data.lower_judged_on],
  ].filter(([, value]) => value) : [];
  return (
    <div className="p-4 md:p-6 max-w-5xl mx-auto [overflow-wrap:anywhere]">
      <Link to={back} className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-primary mb-4"><ArrowLeft className="size-4" />判例一覧に戻る</Link>
      {loading ? <p className="text-sm text-muted-foreground">判例を読み込み中…</p>
        : error || !data ? <div role="alert" className="text-sm"><p>判例を読み込めませんでした</p><Button variant="outline" size="sm" className="mt-2" onClick={retry}>再試行</Button></div>
        : <>
          <h1 className="text-xl md:text-2xl font-semibold">{data.case_name || data.case_number}</h1>
          <div className="flex flex-wrap gap-3 my-4 text-sm">
            <a href={data.detail_url || hanreiUrl(data.case_id)} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 text-primary hover:underline">裁判所の原文ページ<ExternalLink className="size-3" /></a>
            {data.pdf_url && <a href={data.pdf_url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 text-primary hover:underline"><FileText className="size-3.5" />全文PDF</a>}
          </div>
          <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm border rounded-lg p-4">
            {metadata.map(([label, value]) => <div key={label} className="contents"><dt className="text-muted-foreground">{label}</dt><dd className="min-w-0">{value}</dd></div>)}
          </dl>
          {[["判示事項", data.holding, "holding"], ["裁判要旨", data.summary, "summary"], ["参照法条", data.referenced_laws, "referenced_laws"]].map(([label, value, field]) => value && (
            <section key={label} className="mt-6"><h2 className="text-base font-semibold mb-2">{label}</h2><p className="whitespace-pre-wrap text-sm leading-7"><LinkedCaseText text={value} field={field} links={lawLinks.data} /></p></section>
          ))}
          <section className="mt-6" data-testid="hanrei-body">
            <h2 className="text-base font-semibold mb-2">本文</h2>
            {data.body_text?.trim() ? <>
              <p className="text-xs text-muted-foreground mb-3">判決全文PDFから抽出したテキスト</p>
              <div className="whitespace-pre-wrap text-sm leading-8 border rounded-lg p-4 md:p-6"><LinkedCaseText text={data.body_text} field="body_text" links={lawLinks.data} /></div>
            </> : <p className="text-sm text-muted-foreground">{data.body_text === "" ? "PDFから抽出できる本文テキストがありません。" : "本文テキストは未取得です。"}{data.pdf_url && "全文PDFをご覧ください。"}</p>}
          </section>
          <section className="mt-6 border rounded-lg p-4 space-y-3" aria-label="関連条文とwiki">
            <h2 className="font-semibold">関連条文・LLM wiki</h2>
            <p className="text-sm text-muted-foreground">{lawLinks.data?.notice ?? "条文リンクは機械抽出した候補です。実際の適用法条・適用時点は原文と経過措置を確認してください。"}</p>
            {wikiPaths?.has(`hanrei/${caseId}`) ? <Link to={`/wiki/hanrei/${caseId}`} className="block text-primary underline text-sm">判例のLLM wikiを開く</Link> : <p className="text-xs text-muted-foreground">判例wikiの公開をまだ確認できません。</p>}
            {lawLinks.error ? <div className="text-sm">条文リンクを読み込めませんでした。<Button variant="outline" size="sm" onClick={lawLinks.retry}>条文リンクを再試行</Button></div>
              : !lawLinks.data ? <p className="text-sm">条文リンクを読み込み中…</p>
              : lawLinks.data.laws.length === 0 ? <p className="text-sm">条番号を伴う法令参照を抽出できませんでした。</p>
              : lawLinks.data.laws.map(law => <div key={law.law_id} className="border-t pt-3 space-y-2">
                <p className="text-sm font-medium">{law.title} {law.articles.map(a => <Link key={a} to={caseArticleUrl(caseId, law.law_id, a)} className="text-primary underline mr-3">{articleLabel(a)}</Link>)}</p>
                <CaseLawEvidence law={law} />
                {wikiPaths?.has(law.wiki_path) && <Link to={`/wiki/${law.wiki_path}`} className="inline-block text-primary underline text-sm">関連法令のwikiを開く</Link>}
              </div>)}
          </section>
          <p className="text-xs text-muted-foreground mt-6">出典: 裁判所 裁判例検索</p>
        </>}
    </div>
  );
}
