import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Building2, Search, ExternalLink, ChevronLeft, ArrowRightLeft } from "lucide-react";
import {
  api,
  type ReikiIndex,
  type ReikiMunicipalityIndex,
  type ReikiDocument,
  type ReikiArticle,
  type ReikiItem,
} from "../../data/api";
import { searchReiki, reikiGenericTitle, type ReikiHit } from "../../data/search-engine";

function useAsync<T>(fn: (() => Promise<T>) | null, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(!!fn);
  useEffect(() => {
    if (!fn) { setData(null); setLoading(false); return; }
    let cancelled = false;
    setLoading(true); setData(null);
    fn()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { data, loading };
}

/** 段落テキスト。表（タブ区切り行）は表として描画する。 */
function TextBlock({ text }: { text: string }) {
  const lines = text.split("\n");
  const out: ReactNode[] = [];
  let table: string[][] = [];
  const flush = (key: number) => {
    if (table.length === 0) return;
    out.push(
      <div key={`t${key}`} className="my-2 overflow-x-auto">
        <table className="text-xs border-collapse">
          <tbody>
            {table.map((row, i) => (
              <tr key={i}>{row.map((c, j) => <td key={j} className="border border-border px-2 py-1 align-top">{c}</td>)}</tr>
            ))}
          </tbody>
        </table>
      </div>,
    );
    table = [];
  };
  lines.forEach((l, i) => {
    if (l.includes("\t")) { table.push(l.split("\t")); return; }
    flush(i);
    if (l) out.push(<span key={i} className="block">{l}</span>);
  });
  flush(lines.length);
  return <>{out}</>;
}

function Items({ items, depth = 0 }: { items: ReikiItem[]; depth?: number }) {
  return (
    <div className={depth === 0 ? "mt-1 space-y-0.5" : "mt-0.5 space-y-0.5"}>
      {items.map((it, i) => (
        <div key={i} className="pl-4">
          <div className="flex gap-2">
            <span className="shrink-0 text-muted-foreground tabular-nums">{it.num}</span>
            <div className="min-w-0"><TextBlock text={it.text} /></div>
          </div>
          {it.subitems && it.subitems.length > 0 && <Items items={it.subitems} depth={depth + 1} />}
        </div>
      ))}
    </div>
  );
}

function Article({ a, focused }: { a: ReikiArticle; focused: boolean }) {
  return (
    <section id={a.article_id} className={["scroll-mt-4 rounded-md px-2 py-1.5 -mx-2", focused ? "bg-amber-50 dark:bg-amber-900/20" : ""].join(" ")}>
      {a.caption && <div className="text-xs text-muted-foreground mb-0.5">（{a.caption}）</div>}
      {a.paragraphs.map((p, i) => (
        <div key={i} className="text-sm leading-relaxed [overflow-wrap:anywhere]">
          {p.caption && <div className="text-xs text-muted-foreground mt-1">（{p.caption}）</div>}
          <div className="flex gap-2">
            {i === 0 && a.article_no ? (
              <span className="shrink-0 font-semibold">{a.article_no}</span>
            ) : p.num ? (
              <span className="shrink-0 text-muted-foreground tabular-nums">{p.num}</span>
            ) : null}
            <div className="min-w-0"><TextBlock text={p.text} /></div>
          </div>
          {p.items && p.items.length > 0 && <Items items={p.items} />}
        </div>
      ))}
    </section>
  );
}

function ReikiDetail({ code, reikiId, articleId }: { code: string; reikiId: string; articleId: string | null }) {
  const navigate = useNavigate();
  const { data: doc, loading } = useAsync<ReikiDocument>(() => api.reikiDoc(code, reikiId), [code, reikiId]);

  useEffect(() => {
    if (!doc || !articleId) return;
    document.getElementById(articleId)?.scrollIntoView({ block: "start" });
  }, [doc, articleId]);

  if (loading) return <div className="p-6 space-y-3">{[...Array(6)].map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (!doc) return <div className="p-6 text-sm text-muted-foreground">読み込めませんでした</div>;

  const generic = reikiGenericTitle(doc.title, doc.municipality_name);
  let lastHeading: string | null | undefined = null;
  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="px-5 py-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2 mb-1 flex-wrap">
          <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{doc.prefecture} {doc.municipality_name}</span>
          {doc.kind && <span className="text-xs px-1.5 py-0.5 rounded border border-border">{doc.kind}</span>}
        </div>
        <h2 className="text-base font-semibold leading-snug" data-testid="reiki-title">{doc.title}</h2>
        <div className="text-xs text-muted-foreground mt-1.5 flex gap-3 flex-wrap tabular-nums">
          {doc.promulgated_date && <span>制定 {doc.promulgated_date}</span>}
          {doc.reiki_number && <span>{doc.reiki_number}</span>}
          {doc.current_as_of && <span>例規集 {doc.current_as_of} 現在</span>}
        </div>
        <div className="flex gap-2 mt-2 flex-wrap">
          {doc.source?.detail_url && (
            <a href={doc.source.detail_url} target="_blank" rel="noreferrer"
              className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
              例規集で原文を見る <ExternalLink className="size-2.5" />
            </a>
          )}
          <button onClick={() => navigate(`/search?q=${encodeURIComponent(generic)}`)}
            className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
            <ArrowRightLeft className="size-3" /> 他自治体の「{generic}」を探す
          </button>
        </div>
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        <div className="px-5 py-4 space-y-3">
          {doc.preamble && doc.preamble.length > 0 && (
            <div className="text-sm text-muted-foreground space-y-1">
              {doc.preamble.map((p, i) => <div key={i}><TextBlock text={p} /></div>)}
            </div>
          )}
          {doc.articles.map(a => {
            const showHeading = a.heading && a.heading !== lastHeading;
            lastHeading = a.heading;
            return (
              <div key={a.article_id}>
                {showHeading && <h3 className="text-sm font-semibold mt-4 mb-1">{a.heading}</h3>}
                <Article a={a} focused={a.article_id === articleId} />
              </div>
            );
          })}
          {doc.supplementary && doc.supplementary.length > 0 && (
            <details className="pt-2 border-t border-border">
              <summary className="text-xs font-semibold text-muted-foreground cursor-pointer select-none py-1">
                附則（{doc.supplementary.length}）
              </summary>
              <div className="space-y-3 mt-2">
                {doc.supplementary.map((s, i) => (
                  <div key={i}>
                    <div className="text-xs font-semibold mb-1">{s.title}</div>
                    {s.articles.map(a => <Article key={a.article_id} a={a} focused={a.article_id === articleId} />)}
                  </div>
                ))}
              </div>
            </details>
          )}
          <p className="text-xs text-muted-foreground pt-4 border-t border-border">
            自治体が公開する例規集から自動取得した非公式の写しです。正確な内容は原文で確認してください。
            {doc.source?.checked_at && <> 取得 {doc.source.checked_at.slice(0, 10)}</>}
          </p>
        </div>
      </ScrollArea>
    </div>
  );
}

function MunicipalityList({ onSelect }: { onSelect: (code: string) => void }) {
  const { data, loading } = useAsync<ReikiIndex>(() => api.reikiIndex(), []);
  const [query, setQuery] = useState("");
  const [pref, setPref] = useState("all");
  const prefectures = useMemo(() => {
    const seen: string[] = [];
    for (const m of data?.municipalities ?? []) if (!seen.includes(m.prefecture)) seen.push(m.prefecture);
    return seen; // 団体コード順 = 北から
  }, [data]);
  const filtered = useMemo(() => {
    const q = query.trim();
    return (data?.municipalities ?? []).filter(m =>
      (pref === "all" || m.prefecture === pref) && (!q || m.name.includes(q) || m.prefecture.includes(q)));
  }, [data, query, pref]);

  return (
    <>
      <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold flex-1">自治体例規</h2>
          {data && <span className="text-xs text-muted-foreground tabular-nums">{data.count.toLocaleString()}自治体 / {data.reiki_count.toLocaleString()}件</span>}
        </div>
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="自治体名…" className="pl-8 h-8 text-sm" />
        </div>
        <Select value={pref} onValueChange={setPref}>
          <SelectTrigger className="h-7 text-xs"><SelectValue placeholder="都道府県" /></SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全都道府県</SelectItem>
            {prefectures.map(p => <SelectItem key={p} value={p}>{p}</SelectItem>)}
          </SelectContent>
        </Select>
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        {loading ? (
          <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-12 w-full" />)}</div>
        ) : filtered.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted-foreground">{data ? "該当する自治体がありません" : "データがありません"}</p>
        ) : (
          filtered.map(m => (
            <button key={m.municipality_code} onClick={() => onSelect(m.municipality_code)}
              className="w-full text-left px-4 py-2.5 border-b border-border hover:bg-accent/50 transition-colors">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium flex-1 truncate">{m.name}</span>
                <span className="text-xs text-muted-foreground tabular-nums">{m.count.toLocaleString()}件</span>
              </div>
              <div className="text-xs text-muted-foreground flex gap-2">
                <span>{m.prefecture}</span>
                {m.current_as_of && <span className="tabular-nums">{m.current_as_of} 現在</span>}
              </div>
            </button>
          ))
        )}
      </ScrollArea>
    </>
  );
}

function ReikiList({ code, reikiId, onBack, onSelect }: {
  code: string;
  reikiId: string | null;
  onBack: () => void;
  onSelect: (id: string, articleId?: string) => void;
}) {
  const { data, loading } = useAsync<ReikiMunicipalityIndex>(() => api.reikiMunicipality(code), [code]);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [hits, setHits] = useState<ReikiHit[] | null>(null);
  const [searching, setSearching] = useState(false);
  useEffect(() => { setQuery(""); setKind("all"); setHits(null); }, [code]);

  const kinds = useMemo(() => {
    const count = new Map<string, number>();
    for (const r of data?.reiki ?? []) if (r.kind) count.set(r.kind, (count.get(r.kind) ?? 0) + 1);
    return [...count.entries()].sort((a, b) => b[1] - a[1]).map(([k]) => k);
  }, [data]);
  const filtered = useMemo(() => {
    const q = query.trim();
    return (data?.reiki ?? []).filter(r => (kind === "all" || r.kind === kind) && (!q || r.title.includes(q)));
  }, [data, query, kind]);

  const runFullText = async () => {
    if (query.trim().length < 2) return;
    setSearching(true);
    setHits(await searchReiki(query, { scope: { kind: "municipality", code }, limit: 50 }));
    setSearching(false);
  };

  return (
    <>
      <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
        <button onClick={onBack} className="inline-flex items-center gap-0.5 text-xs text-muted-foreground hover:text-foreground">
          <ChevronLeft className="size-3.5" /> 自治体一覧
        </button>
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold flex-1 truncate">{data ? `${data.prefecture} ${data.name}` : "…"}</h2>
          {data && (
            <span className="text-xs text-muted-foreground tabular-nums" data-testid="reiki-list-count">
              {hits ? `本文 ${hits.length.toLocaleString()}件` : `${filtered.length.toLocaleString()}件`}
            </span>
          )}
        </div>
        {data?.current_as_of && <div className="text-xs text-muted-foreground">例規集 {data.current_as_of} 現在</div>}
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input value={query}
            onChange={e => { setQuery(e.target.value); setHits(null); }}
            onKeyDown={e => { if (e.key === "Enter") runFullText(); }}
            placeholder="題名で絞り込み（Enter で本文検索）" className="pl-8 h-8 text-sm" />
        </div>
        <div className="flex gap-2">
          <Select value={kind} onValueChange={setKind}>
            <SelectTrigger className="h-7 text-xs flex-1"><SelectValue placeholder="種別" /></SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全種別</SelectItem>
              {kinds.map(k => <SelectItem key={k} value={k}>{k}</SelectItem>)}
            </SelectContent>
          </Select>
          <button onClick={runFullText} disabled={query.trim().length < 2 || searching}
            className="text-xs px-2 rounded border border-border hover:border-primary hover:text-primary disabled:opacity-40">
            本文検索
          </button>
        </div>
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        {loading || searching ? (
          <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-12 w-full" />)}</div>
        ) : hits ? (
          hits.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">本文に該当する例規がありません</p>
          ) : (
            hits.map((h, i) => (
              <button key={`${h.reiki_id}/${h.article_id}/${i}`} onClick={() => onSelect(h.reiki_id, h.article_id || undefined)}
                className="w-full text-left px-4 py-2.5 border-b border-border hover:bg-accent/50 transition-colors">
                <div className="text-sm font-medium line-clamp-1">{h.title}</div>
                {h.article_no && <div className="text-xs text-muted-foreground">{h.article_no}{h.caption ? `（${h.caption}）` : ""}</div>}
                <div className="text-xs text-muted-foreground line-clamp-2 mt-0.5">{h.excerpt}</div>
              </button>
            ))
          )
        ) : filtered.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted-foreground">{data ? "該当する例規がありません" : "データがありません"}</p>
        ) : (
          filtered.map(r => (
            <button key={r.reiki_id} onClick={() => onSelect(r.reiki_id)}
              className={["w-full text-left px-4 py-2.5 border-b border-border transition-colors", r.reiki_id === reikiId ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"].join(" ")}>
              <div className="text-sm font-medium line-clamp-2 [overflow-wrap:anywhere]">{r.title}</div>
              <div className="text-xs text-muted-foreground flex gap-2 tabular-nums">
                {r.reiki_number && <span>{r.reiki_number}</span>}
                {r.promulgated_date && <span>{r.promulgated_date}</span>}
              </div>
            </button>
          ))
        )}
      </ScrollArea>
    </>
  );
}

export function ReikiView({ muniCode, reikiId, articleId, onSelectMunicipality, onSelectReiki, onBack }: {
  muniCode: string | null;
  reikiId: string | null;
  articleId: string | null;
  onSelectMunicipality: (code: string) => void;
  onSelectReiki: (code: string, reikiId: string, articleId?: string) => void;
  onBack: () => void;
}) {
  return (
    <div className="flex h-full">
      <div className="w-96 shrink-0 border-r border-border flex flex-col">
        {muniCode ? (
          <ReikiList code={muniCode} reikiId={reikiId} onBack={onBack} onSelect={(id, a) => onSelectReiki(muniCode, id, a)} />
        ) : (
          <MunicipalityList onSelect={onSelectMunicipality} />
        )}
      </div>
      <div className="flex-1 flex flex-col min-w-0">
        {muniCode && reikiId ? (
          <ReikiDetail code={muniCode} reikiId={reikiId} articleId={articleId} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <Building2 className="size-10 opacity-30" />
            <p className="text-sm">{muniCode ? "例規を選択すると本文が表示されます" : "自治体を選択してください"}</p>
          </div>
        )}
      </div>
    </div>
  );
}
