import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Badge } from "../ui/badge";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { BookOpen, MessageSquare, Newspaper, Rss, ArrowUpRight, Bell, Landmark, MessagesSquare, Users, BriefcaseBusiness } from "lucide-react";
import { api, type RecentFeed, type FeedItem } from "../../data/api";

const KINDS = [
  { key: "law", label: "法令改正", icon: BookOpen },
  { key: "bill", label: "法案", icon: Landmark },
  { key: "pubcomment", label: "パブコメ", icon: MessageSquare },
  { key: "kanpo", label: "官報", icon: Newspaper },
  { key: "proceedings", label: "国会会議録", icon: MessagesSquare },
  { key: "shingikai", label: "審議会", icon: Users },
  { key: "procurement", label: "政府調達", icon: BriefcaseBusiness },
] as const;

function kindMeta(kind: string) {
  return KINDS.find(k => k.key === kind) ?? { key: kind, label: kind, icon: Bell };
}

function useRecentFeed() {
  const [data, setData] = useState<RecentFeed | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.recentFeed()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [attempt]);
  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

function FeedRow({ item, onOpen, onLaw }: {
  item: FeedItem;
  onOpen: (i: FeedItem) => void;
  onLaw: (lawId: string) => void;
}) {
  const meta = kindMeta(item.kind);
  const Icon = meta.icon;
  // 官報項目で対象法令が逆引きできるとき、外部PDFとは別に法令へ飛べるようにする。
  const showLawLink = item.kind === "kanpo" && item.law_id && item.law_title;
  return (
    <div className="w-full border-b border-border hover:bg-accent/50 transition-colors flex items-start gap-3 px-4 py-3">
      <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
        <Icon className="size-4 text-muted-foreground" />
      </div>
      <div className="min-w-0 flex-1">
        <button onClick={() => onOpen(item)} className="w-full text-left">
          <div className="flex items-center gap-2 flex-wrap">
            <Badge variant="outline" className="text-xs shrink-0">{meta.label}</Badge>
            <span className="text-xs text-muted-foreground">{item.date}</span>
            {item.ministry && <span className="text-xs text-muted-foreground">{item.ministry}</span>}
          </div>
          <div className="text-sm font-medium mt-0.5 line-clamp-2">{item.title}</div>
          {item.summary && (
            <div className="text-xs text-muted-foreground mt-0.5">{item.summary}</div>
          )}
        </button>
        {showLawLink && (
          <a
            href={`#/laws/${item.law_id}`}
            onClick={(e) => { e.preventDefault(); onLaw(item.law_id!); }}
            className="mt-1.5 inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded border border-border hover:border-primary hover:text-primary transition-colors cursor-pointer"
            title="改正対象の法令を開く"
          >
            <BookOpen className="size-3 shrink-0" />
            <span className="truncate max-w-[18rem]">{item.law_title}</span>
          </a>
        )}
      </div>
      <ArrowUpRight className="size-4 text-muted-foreground shrink-0 mt-1" />
    </div>
  );
}

export function FeedView() {
  const { data, loading, retry } = useRecentFeed();
  const navigate = useNavigate();
  const [active, setActive] = useState<Set<string>>(new Set());
  const items = useMemo(() => [...(data?.items ?? [])].sort((a, b) => b.date.localeCompare(a.date)), [data]);
  const latestByKind = useMemo(() => KINDS.flatMap(kind => {
    const item = items.find(item => item.kind === kind.key);
    return item ? [{ ...kind, item }] : [];
  }), [items]);

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const i of data?.items ?? []) c[i.kind] = (c[i.kind] ?? 0) + 1;
    return c;
  }, [data]);

  const filtered = useMemo(() => {
    if (active.size === 0) return items;
    return items.filter(i => active.has(i.kind));
  }, [items, active]);

  const toggle = (k: string) => {
    setActive(prev => {
      const next = new Set(prev);
      next.has(k) ? next.delete(k) : next.add(k);
      return next;
    });
  };

  const open = (i: FeedItem) => {
    if (i.internal) navigate(i.href);
    else window.open(i.href, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="p-4 sm:p-6 max-w-5xl">
      <div className="mb-5 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl flex items-center gap-2"><Bell className="size-6" />新着 — 規制変化フィード</h1>
          <p className="text-sm text-muted-foreground mt-1">
            法令・法案から、国会会議録・審議会・政府調達まで横断表示
          </p>
        </div>
        <a
          href="./feeds/recent.xml"
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1.5 text-sm px-3 py-1.5 rounded-md border border-border hover:border-primary hover:text-primary transition-colors shrink-0"
          title="RSS で購読"
        >
          <Rss className="size-4" />RSS購読
        </a>
      </div>

      {!loading && active.size === 0 && latestByKind.length > 0 && (
        <section aria-label="種類別の最新" className="mb-6">
          <h2 className="text-sm font-medium mb-2">種類別の最新</h2>
          <p className="text-xs text-muted-foreground mb-3">それぞれの分野の最新情報をひと目で確認できます。</p>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {latestByKind.map(({ key, label, icon: Icon, item }) => (
              <button key={key} onClick={() => open(item)} className="rounded-lg border border-border p-3 text-left hover:border-primary/50 hover:bg-accent/50 transition-colors min-w-0">
                <span className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                  <span className="inline-flex items-center gap-1.5 font-medium text-foreground"><Icon className="size-3.5" />{label}</span>
                  <span>{item.date}</span>
                </span>
                <span className="block mt-2 text-sm font-medium line-clamp-2">{item.title}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      {/* 種別フィルタ */}
      <div className="flex flex-wrap gap-2 mb-4">
        {KINDS.map(k => {
          const Icon = k.icon;
          const on = active.has(k.key);
          return (
            <button
              key={k.key}
              onClick={() => toggle(k.key)}
              aria-label={`フィルタ-${k.label}`}
              aria-pressed={on}
              className={[
                "inline-flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-md border transition-colors",
                on ? "border-primary text-primary bg-primary/5" : "border-border hover:border-primary/50",
              ].join(" ")}
            >
              <Icon className="size-3.5" />
              {k.label}
              <span className="text-muted-foreground">{counts[k.key] ?? 0}</span>
            </button>
          );
        })}
        {active.size > 0 && (
          <button onClick={() => setActive(new Set())} className="text-xs text-muted-foreground hover:text-foreground px-2 py-1.5">
            クリア
          </button>
        )}
      </div>

      <div className="flex items-center justify-between gap-2 mb-2">
        <h2 className="text-sm font-medium">新着一覧 <span className="text-muted-foreground">{filtered.length}件</span></h2>
        <span className="text-xs text-muted-foreground">日付の新しい順</span>
      </div>
      <p className="text-xs text-muted-foreground mb-3">各種類の最新50件まで。パブコメは募集開始日・結果公示日、会議は開催日、調達は公示日で表示します。</p>
      <section aria-label="新着一覧" className="border border-border rounded-lg overflow-hidden">
        <ScrollArea className="max-h-[calc(100vh-220px)]">
          {loading ? (
            <div className="p-4 space-y-2">
              {[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}
            </div>
          ) : !data ? (
            <div role="alert" className="p-8 text-center text-sm text-muted-foreground">
              <p>フィードを読み込めませんでした</p>
              <button type="button" onClick={retry} className="mt-3 text-primary hover:underline">再試行</button>
            </div>
          ) : filtered.length === 0 ? (
            <p className="p-8 text-center text-sm text-muted-foreground">
              該当する新着がありません
            </p>
          ) : (
            filtered.map((item, i) => (
              <FeedRow key={`${item.kind}-${item.href}-${i}`} item={item} onOpen={open} onLaw={(id) => navigate(`/laws/${id}`)} />
            ))
          )}
        </ScrollArea>
      </section>
    </div>
  );
}
