import { Suspense, lazy, useEffect, useMemo, useState, type ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ArrowLeft, ExternalLink, Network, Search } from "lucide-react";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Skeleton } from "../ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { api, type WikiGraph, type WikiIndex, type WikiPage, type WikiPageMeta } from "../../data/api";
import { WIKI_TYPES, WIKI_TYPE_ORDER, resolveWikiLink } from "./wiki-types";

const WikiGraphCanvas = lazy(() => import("./wiki-graph").then(m => ({ default: m.WikiGraphCanvas })));

function useLoad<T>(load: (() => Promise<T>) | null, deps: unknown[]) {
  const [state, setState] = useState<{ data: T | null; loading: boolean; error: boolean }>({
    data: null,
    loading: !!load,
    error: false,
  });
  useEffect(() => {
    if (!load) {
      setState({ data: null, loading: false, error: false });
      return;
    }
    let cancelled = false;
    setState({ data: null, loading: true, error: false });
    load()
      .then(d => { if (!cancelled) setState({ data: d, loading: false, error: false }); })
      .catch(() => { if (!cancelled) setState({ data: null, loading: false, error: true }); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}

function TypeBadge({ type }: { type: string }) {
  const t = WIKI_TYPES[type];
  if (!t) return null;
  return (
    <span
      className="text-[11px] font-medium px-1.5 py-0.5 rounded shrink-0"
      style={{ color: t.color, backgroundColor: `${t.color}1f` }}
    >
      {t.label}
    </span>
  );
}

export function WikiView({ path, onOpen }: { path: string | null; onOpen: (path: string | null) => void }) {
  const index = useLoad<WikiIndex>(() => api.wikiIndex(), []);
  const graph = useLoad<WikiGraph>(() => api.wikiGraph(), []);
  if (path) return <WikiPageView path={path} graph={graph.data} index={index.data} onOpen={onOpen} />;
  return <WikiHome index={index} graph={graph.data} onOpen={onOpen} />;
}

function WikiHome({
  index,
  graph,
  onOpen,
}: {
  index: { data: WikiIndex | null; loading: boolean; error: boolean };
  graph: WikiGraph | null;
  onOpen: (path: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const [visible, setVisible] = useState<Set<string>>(() => new Set(WIKI_TYPE_ORDER));
  const pages = useMemo(
    () => (index.data?.pages ?? []).filter(p => WIKI_TYPES[p.type]),
    [index.data],
  );
  const counts = useMemo(() => {
    const c = new Map<string, number>();
    for (const p of pages) c.set(p.type, (c.get(p.type) ?? 0) + 1);
    return c;
  }, [pages]);
  const filtered = useMemo(() => {
    const q = query.trim();
    return pages
      .filter(p => visible.has(p.type))
      .filter(p => !q || p.title.includes(q) || p.description.includes(q) || p.tags.some(t => t.includes(q)))
      .sort((a, b) =>
        WIKI_TYPE_ORDER.indexOf(a.type) - WIKI_TYPE_ORDER.indexOf(b.type) ||
        (b.date ?? "").localeCompare(a.date ?? "") ||
        a.title.localeCompare(b.title, "ja"),
      );
  }, [pages, visible, query]);

  const toggle = (type: string) =>
    setVisible(prev => {
      const next = new Set(prev);
      if (next.has(type)) next.delete(type);
      else next.add(type);
      return next;
    });

  return (
    <div className="p-6 max-w-6xl">
      <div className="mb-4">
        <h1 className="text-2xl flex items-center gap-2"><Network className="size-6" />wiki</h1>
        <p className="text-sm text-muted-foreground mt-1">
          審議会・パブコメ・国会・議案・公布/施行をまたいで、「きっかけ → 議論 → 意思決定 → 結果 → その後」をたどれる wiki（OKF 形式）。
          文章は LLM が書き、すべての記述を会議録・議案文書の原文引用で検証しています。
        </p>
      </div>

      {index.loading && <Skeleton className="h-64 w-full" />}
      {index.error && (
        <p className="text-sm text-muted-foreground">wiki はまだ配信されていません。</p>
      )}

      {index.data && (
        <>
          <div className="flex flex-wrap items-center gap-2 mb-3">
            {WIKI_TYPE_ORDER.map(type => {
              const t = WIKI_TYPES[type];
              const on = visible.has(type);
              return (
                <button
                  key={type}
                  type="button"
                  aria-pressed={on}
                  onClick={() => toggle(type)}
                  className="inline-flex items-center gap-1.5 text-xs px-2 py-1 rounded-full border transition-opacity"
                  style={{ borderColor: t.color, opacity: on ? 1 : 0.4 }}
                >
                  <span className="size-2 rounded-full" style={{ backgroundColor: t.color }} />
                  {t.label} {counts.get(type) ?? 0}
                </button>
              );
            })}
          </div>

          <Tabs defaultValue="graph">
            <TabsList>
              <TabsTrigger value="graph">グラフ</TabsTrigger>
              <TabsTrigger value="list">一覧</TabsTrigger>
            </TabsList>
            <TabsContent value="graph">
              {graph ? (
                <Suspense fallback={<Skeleton className="h-[560px] w-full" />}>
                  <WikiGraphCanvas graph={graph} visibleTypes={visible} onOpen={p => onOpen(p)} />
                </Suspense>
              ) : (
                <Skeleton className="h-[560px] w-full" />
              )}
              <p className="text-xs text-muted-foreground mt-2">ノードをクリックするとページを開きます。大きいノードほど多くのページとつながっています。</p>
            </TabsContent>
            <TabsContent value="list">
              <div className="relative mb-3 max-w-md">
                <Search className="size-4 absolute left-2.5 top-2.5 text-muted-foreground" />
                <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="タイトル・概要・タグで絞り込み" className="pl-8" />
              </div>
              <ul className="divide-y divide-border border border-border rounded-lg">
                {filtered.map(p => (
                  <li key={p.path}>
                    <button
                      type="button"
                      onClick={() => onOpen(p.path)}
                      className="w-full text-left px-4 py-2.5 hover:bg-muted/50 flex items-start gap-2"
                    >
                      <TypeBadge type={p.type} />
                      <span className="min-w-0">
                        <span className="text-sm font-medium block truncate">{p.title}</span>
                        {p.description && <span className="text-xs text-muted-foreground line-clamp-2">{p.description}</span>}
                      </span>
                      {p.date && <span className="ml-auto text-xs text-muted-foreground shrink-0">{p.date}</span>}
                    </button>
                  </li>
                ))}
                {filtered.length === 0 && <li className="px-4 py-6 text-sm text-muted-foreground">該当するページはありません。</li>}
              </ul>
            </TabsContent>
          </Tabs>
        </>
      )}
    </div>
  );
}

function WikiPageView({
  path,
  graph,
  index,
  onOpen,
}: {
  path: string;
  graph: WikiGraph | null;
  index: WikiIndex | null;
  onOpen: (path: string | null) => void;
}) {
  const page = useLoad<WikiPage>(() => api.wikiPage(path), [path]);
  const fm = page.data?.frontmatter ?? {};
  const str = (k: string) => (typeof fm[k] === "string" ? (fm[k] as string) : "");
  const tags = Array.isArray(fm.tags) ? (fm.tags as string[]) : [];

  // グラフ上の隣接ページ (このページへのリンクと、このページからのリンク)。
  const neighbors = useMemo(() => {
    if (!graph || !index) return [];
    const byPath = new Map(index.pages.map(p => [p.path, p]));
    const ids = new Set<string>();
    for (const l of graph.links) {
      if (l.source === path) ids.add(l.target);
      if (l.target === path) ids.add(l.source);
    }
    return [...ids]
      .map(id => byPath.get(id))
      .filter((p): p is WikiPageMeta => !!p)
      .sort((a, b) =>
        WIKI_TYPE_ORDER.indexOf(a.type) - WIKI_TYPE_ORDER.indexOf(b.type) ||
        (b.date ?? "").localeCompare(a.date ?? ""),
      );
  }, [graph, index, path]);

  useEffect(() => {
    document.querySelector("main")?.scrollTo({ top: 0 });
  }, [path]);

  return (
    <div className="p-6 max-w-6xl">
      <Button variant="ghost" size="sm" onClick={() => onOpen(null)} className="gap-1 -ml-2 mb-3">
        <ArrowLeft className="size-4" /> wiki トップへ
      </Button>

      {page.loading && <Skeleton className="h-64 w-full" />}
      {page.error && <p className="text-sm text-muted-foreground">ページが見つかりません: {path}</p>}

      {page.data && (
        <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_260px]">
          <article className="min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <TypeBadge type={str("type")} />
              {str("date") && <span className="text-xs text-muted-foreground">{str("date")}</span>}
            </div>
            <h1 className="text-2xl mb-1">{str("title") || path}</h1>
            {str("description") && <p className="text-sm text-muted-foreground mb-2">{str("description")}</p>}
            <div className="flex flex-wrap items-center gap-1.5 mb-5">
              {str("resource") && (
                <a
                  href={str("resource")}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-xs text-primary hover:underline mr-2"
                >
                  <ExternalLink className="size-3" />原文・本文
                </a>
              )}
              {tags.map(t => <Badge key={t} variant="outline" className="text-[11px]">{t}</Badge>)}
            </div>
            <WikiMarkdown body={page.data.body} from={path} onOpen={onOpen} />
          </article>

          {neighbors.length > 0 && (
            <aside className="text-sm">
              <div className="text-xs font-semibold text-muted-foreground mb-2">つながっているページ ({neighbors.length})</div>
              <ul className="space-y-1.5">
                {neighbors.map(n => (
                  <li key={n.path}>
                    <button type="button" onClick={() => onOpen(n.path)} className="text-left flex items-start gap-1.5 hover:text-primary">
                      <TypeBadge type={n.type} />
                      <span className="leading-snug">{n.title}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </aside>
          )}
        </div>
      )}
    </div>
  );
}

/** wiki の Markdown を描画する。相対リンクは SPA 内遷移、脚注はページ内スクロールにする (HashRouter のため)。 */
function WikiMarkdown({ body, from, onOpen }: { body: string; from: string; onOpen: (path: string) => void }) {
  return (
    <div className="text-sm leading-relaxed space-y-3 [&_.footnotes]:mt-8 [&_.footnotes]:pt-3 [&_.footnotes]:border-t [&_.footnotes]:border-border [&_.footnotes]:text-xs [&_.footnotes]:text-muted-foreground">
      <Markdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        remarkRehypeOptions={{ footnoteLabel: "出典", footnoteBackLabel: "本文へ戻る", footnoteLabelProperties: {} }}
        components={{
          // タイトルはヘッダで表示済み。
          h1: () => null,
          h2: ({ children, id }) => <h2 id={id} className="text-base font-semibold mt-6 mb-2">{children}</h2>,
          h3: ({ children }) => <h3 className="text-sm font-semibold mt-4 mb-1">{children}</h3>,
          h4: ({ children }) => <h4 className="text-sm font-medium text-muted-foreground mt-3 mb-1">{children}</h4>,
          ul: ({ children }) => <ul className="list-disc pl-5 space-y-1.5">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal pl-5 space-y-1">{children}</ol>,
          table: ({ children }) => (
            // 日付・種別などの短い列が 1 文字ずつ折り返されないようにする。
            <div className="overflow-x-auto border border-border rounded-md [&_td:first-child]:whitespace-nowrap [&_td]:min-w-[3.5em]">
              <table className="w-full text-xs">{children}</table>
            </div>
          ),
          th: ({ children }) => <th className="text-left font-medium bg-muted/50 px-2 py-1.5 border-b border-border">{children}</th>,
          td: ({ children }) => <td className="px-2 py-1.5 border-b border-border align-top">{children}</td>,
          code: ({ children }) => <code className="px-1 py-0.5 rounded bg-muted text-[0.9em]">{children}</code>,
          a: ({ href = "", children }) => <WikiLink href={href} from={from} onOpen={onOpen}>{children}</WikiLink>,
        }}
      >
        {body}
      </Markdown>
    </div>
  );
}

function WikiLink({ href, from, onOpen, children }: { href: string; from: string; onOpen: (path: string) => void; children: ReactNode }) {
  if (/^[a-z]+:/i.test(href)) {
    return <a href={href} target="_blank" rel="noreferrer" className="text-primary hover:underline">{children}</a>;
  }
  if (href.startsWith("#")) {
    return (
      <a
        href={href}
        className="text-primary hover:underline"
        onClick={e => {
          e.preventDefault();
          document.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView({ behavior: "smooth", block: "center" });
        }}
      >
        {children}
      </a>
    );
  }
  const target = resolveWikiLink(from, href);
  return (
    <a
      href={target ? `#/wiki/${target}` : href}
      className="text-primary hover:underline"
      onClick={e => {
        if (!target) return;
        e.preventDefault();
        onOpen(target);
      }}
    >
      {children}
    </a>
  );
}
