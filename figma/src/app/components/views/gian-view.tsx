import { useEffect, useMemo, useState } from "react";
import { Badge } from "../ui/badge";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Landmark, Search, ExternalLink } from "lucide-react";
import { api, type GianIndex, type GianBillMeta, type GianBill } from "../../data/api";

function billTypeColor(t?: string | null) {
  if (t === "閣法") return "bg-blue-100 text-blue-800 dark:bg-blue-900/40 dark:text-blue-300";
  if (t === "衆法") return "bg-emerald-100 text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-300";
  if (t === "参法") return "bg-purple-100 text-purple-800 dark:bg-purple-900/40 dark:text-purple-300";
  return "bg-muted text-muted-foreground";
}

function useGianIndex() {
  const [data, setData] = useState<GianIndex | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    api.gianIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);
  return { data, loading };
}

function BillDetail({ session, billId }: { session: string; billId: string }) {
  const [bill, setBill] = useState<GianBill | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setBill(null);
    api.gianBill(session, billId)
      .then(d => { if (!cancelled) { setBill(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [session, billId]);

  if (loading) return <div className="p-6 space-y-3">{[...Array(6)].map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (!bill) return <div className="p-6 text-sm text-muted-foreground">読み込めませんでした</div>;

  // 空でない審議経過のみ表示。
  const rows = bill.fields.filter(f => f.value && f.value !== "／");
  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2 mb-1 flex-wrap">
          {bill.bill_type && <span className={["text-xs font-bold px-1.5 py-0.5 rounded", billTypeColor(bill.bill_type)].join(" ")}>{bill.bill_type}</span>}
          <span className="text-xs text-muted-foreground">第{bill.session}回 第{bill.number}号</span>
        </div>
        <h2 className="text-base font-semibold leading-snug">{bill.title}</h2>
        <div className="text-xs text-muted-foreground mt-1.5 flex flex-wrap gap-x-3 gap-y-1">
          {bill.submitter && <span>{bill.submitter}</span>}
          {bill.latest_event && bill.latest_date && <span>最新: {bill.latest_event}（{bill.latest_date}）</span>}
        </div>
        {bill.source?.detail_url && (
          <a href={bill.source.detail_url} target="_blank" rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs mt-2 px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
            衆議院 議案情報 <ExternalLink className="size-2.5" />
          </a>
        )}
      </div>
      <ScrollArea className="flex-1">
        <table className="w-full text-sm">
          <tbody>
            {rows.map((f, i) => (
              <tr key={i} className="border-b border-border last:border-0">
                <th className="text-left align-top font-medium text-muted-foreground px-5 py-2 w-2/5">{f.key}</th>
                <td className="align-top px-3 py-2">{f.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </ScrollArea>
    </div>
  );
}

export function GianView({ billRef, onSelect }: {
  billRef: { session: string; billId: string } | null;
  onSelect: (session: string, billId: string) => void;
}) {
  const { data, loading } = useGianIndex();
  const [query, setQuery] = useState("");
  const [typeFilter, setTypeFilter] = useState("all");

  const types = useMemo(() => {
    const s = new Set((data?.bills ?? []).map(b => b.bill_type).filter((t): t is string => !!t));
    return [...s].sort();
  }, [data]);

  const filtered = useMemo(() => {
    const q = query.trim();
    return (data?.bills ?? []).filter(b => {
      if (typeFilter !== "all" && b.bill_type !== typeFilter) return false;
      if (q) return b.title.includes(q) || (b.committee ?? "").includes(q);
      return true;
    });
  }, [data, query, typeFilter]);

  return (
    <div className="flex h-full">
      <div className="w-96 shrink-0 border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1">議案（法案審議）</h2>
            {data && <span className="text-xs text-muted-foreground">{filtered.length}件</span>}
          </div>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="件名・委員会…" className="pl-8 h-8 text-sm" />
          </div>
          <Select value={typeFilter} onValueChange={setTypeFilter}>
            <SelectTrigger className="h-7 text-xs"><SelectValue placeholder="種別" /></SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全種別</SelectItem>
              {types.map(t => <SelectItem key={t} value={t}>{t}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <ScrollArea className="flex-1">
          {loading ? (
            <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}</div>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">{data ? "該当する議案がありません" : "データがありません"}</p>
          ) : (
            filtered.map(b => {
              const sess = String(b.session);
              const selected = billRef?.billId === b.bill_id && billRef?.session === sess;
              return (
                <button key={`${sess}-${b.bill_id}`} onClick={() => onSelect(sess, b.bill_id)}
                  className={["w-full text-left px-4 py-3 border-b border-border transition-colors", selected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"].join(" ")}>
                  <div className="flex items-center gap-2 mb-0.5">
                    {b.bill_type && <span className={["text-xs font-bold px-1.5 py-0.5 rounded shrink-0", billTypeColor(b.bill_type)].join(" ")}>{b.bill_type}</span>}
                    <span className="text-xs text-muted-foreground">第{b.session}回 {b.number}号</span>
                  </div>
                  <div className="text-sm font-medium line-clamp-2">{b.title}</div>
                  <div className="flex items-center gap-2 mt-0.5 flex-wrap">
                    {b.latest_event && <span className="text-xs text-muted-foreground">{b.latest_event}{b.latest_date ? ` ${b.latest_date}` : ""}</span>}
                    {b.committee && <span className="text-xs text-muted-foreground truncate">{b.committee}</span>}
                  </div>
                </button>
              );
            })
          )}
        </ScrollArea>
      </div>
      <div className="flex-1 flex flex-col min-w-0">
        {billRef ? (
          <BillDetail session={billRef.session} billId={billRef.billId} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <Landmark className="size-10 opacity-30" />
            <p className="text-sm">議案を選択すると審議経過が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}

export type { GianBillMeta };
