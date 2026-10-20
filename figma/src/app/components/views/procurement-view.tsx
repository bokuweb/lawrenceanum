import { useEffect, useMemo, useState } from "react";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Button } from "../ui/button";
import { Briefcase, Search, ExternalLink } from "lucide-react";
import { api, type ProcurementIndex, type ProcurementItem } from "../../data/api";

// 公告は 2.5 万件超あるため、一覧は段階的に描画する。
const PAGE_SIZE = 200;

function useProcurementIndex() {
  const [data, setData] = useState<ProcurementIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.procurementIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [attempt]);
  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

function formatAmount(v: ProcurementItem["contract_amount"]): string | null {
  if (v === null || v === undefined || v === "") return null;
  const n = typeof v === "number" ? v : Number(String(v).replace(/[,円\s]/g, ""));
  return Number.isFinite(n) ? `${n.toLocaleString()} 円` : String(v);
}

function ItemDetail({ itemId }: { itemId: string }) {
  const [item, setItem] = useState<ProcurementItem | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setItem(null);
    api.procurementItem(itemId)
      .then(d => { if (!cancelled) { setItem(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [itemId, attempt]);

  if (loading) return <div className="p-6 space-y-3">{[...Array(6)].map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (!item) return (
    <div role="alert" className="p-6 text-sm text-muted-foreground">
      <p>公告の詳細を読み込めませんでした</p>
      <button type="button" onClick={() => setAttempt(n => n + 1)} className="mt-2 text-primary hover:underline">再試行</button>
    </div>
  );

  const rows: [string, string | null][] = [
    ["公告種別", item.notice_type],
    ["調達機関", item.organization],
    ["公示日", item.publish_date],
    ["締切", item.deadline],
    ["契約日", item.contract_date],
    ["落札者", item.contractor],
    ["契約金額", formatAmount(item.contract_amount)],
  ];
  return (
    <div className="flex flex-col h-full min-h-0 [overflow-wrap:anywhere]">
      <div className="px-5 py-4 border-b border-border shrink-0">
        <h2 className="text-base font-semibold leading-snug [overflow-wrap:anywhere]">{item.title}</h2>
        <div className="text-xs text-muted-foreground mt-1.5 flex flex-wrap gap-x-3 gap-y-1">
          {item.organization && <span>{item.organization}</span>}
          {item.publish_date && <span>公示 {item.publish_date}</span>}
        </div>
        {item.detail_url && (
          <a href={item.detail_url} target="_blank" rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs mt-2 px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
            公告原文 <ExternalLink className="size-2.5" />
          </a>
        )}
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        <table className="w-full table-fixed text-sm">
          <tbody>
            {rows.map(([k, v]) => (
              <tr key={k} className="border-b border-border last:border-0">
                <th className="text-left align-top font-medium text-muted-foreground px-5 py-2 w-1/3">{k}</th>
                <td className="align-top px-3 py-2">{v || <span className="text-muted-foreground">—</span>}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="px-5 py-3 text-xs text-muted-foreground">
          出典: 官公需情報ポータルサイト（{item.source.provider}）・取得 {item.source.fetched_at.slice(0, 10)}
        </p>
      </ScrollArea>
    </div>
  );
}

export function ProcurementView({ itemId, onSelect }: {
  itemId: string | null;
  onSelect: (itemId: string) => void;
}) {
  const { data, loading, retry } = useProcurementIndex();
  const [query, setQuery] = useState("");
  const [typeFilter, setTypeFilter] = useState("all");
  const [limit, setLimit] = useState(PAGE_SIZE);

  // 公告種別は表記揺れが多いので件数上位だけを選択肢にする。
  const types = useMemo(() => {
    const counts = new Map<string, number>();
    for (const it of data?.items ?? []) {
      if (it.notice_type) counts.set(it.notice_type, (counts.get(it.notice_type) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 12).map(([t]) => t);
  }, [data]);

  const filtered = useMemo(() => {
    const q = query.trim();
    return (data?.items ?? []).filter(it => {
      if (typeFilter !== "all" && it.notice_type !== typeFilter) return false;
      if (q) return it.title.includes(q) || (it.organization ?? "").includes(q);
      return true;
    });
  }, [data, query, typeFilter]);

  useEffect(() => { setLimit(PAGE_SIZE); }, [query, typeFilter]);

  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <div className="w-full max-h-64 md:max-h-none md:w-96 min-h-0 shrink-0 border-b md:border-b-0 md:border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1">政府調達</h2>
            {data && <span className="text-xs text-muted-foreground tabular-nums">{filtered.length.toLocaleString()}件</span>}
          </div>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="件名・調達機関…" className="pl-8 h-8 text-sm" />
          </div>
          <Select value={typeFilter} onValueChange={setTypeFilter}>
            <SelectTrigger className="h-7 text-xs"><SelectValue placeholder="公告種別" /></SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全種別</SelectItem>
              {types.map(t => <SelectItem key={t} value={t}>{t}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
          {loading ? (
            <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}</div>
          ) : !data ? (
            <div role="alert" className="p-6 text-center text-sm text-muted-foreground">
              <p>公告一覧を読み込めませんでした</p>
              <button type="button" onClick={retry} className="mt-2 text-primary hover:underline">再試行</button>
            </div>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">該当する公告がありません</p>
          ) : (
            <>
              {filtered.slice(0, limit).map(it => (
                <button key={it.item_id} onClick={() => onSelect(it.item_id)}
                  className={["w-full text-left px-4 py-3 border-b border-border transition-colors", itemId === it.item_id ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"].join(" ")}>
                  <div className="text-sm font-medium line-clamp-2 [overflow-wrap:anywhere]">{it.title}</div>
                  <div className="flex items-center gap-2 mt-0.5 flex-wrap">
                    {it.notice_type && <span className="text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{it.notice_type}</span>}
                    {it.publish_date && <span className="text-xs text-muted-foreground tabular-nums">{it.publish_date}</span>}
                    {it.organization && <span className="text-xs text-muted-foreground truncate">{it.organization}</span>}
                  </div>
                </button>
              ))}
              {filtered.length > limit && (
                <div className="p-3">
                  <Button variant="outline" size="sm" className="w-full" onClick={() => setLimit(l => l + PAGE_SIZE)}>
                    さらに表示（残り {(filtered.length - limit).toLocaleString()} 件）
                  </Button>
                </div>
              )}
            </>
          )}
        </ScrollArea>
      </div>
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        {itemId ? (
          <ItemDetail itemId={itemId} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <Briefcase className="size-10 opacity-30" />
            <p className="text-sm">公告を選択すると詳細が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
