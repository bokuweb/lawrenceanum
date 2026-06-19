import { useEffect, useMemo, useState } from "react";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { ScrollText, Search, ExternalLink } from "lucide-react";
import { api, type TsutatsuIndex, type TsutatsuSet, type TsutatsuItem } from "../../data/api";

function useTsutatsuIndex() {
  const [data, setData] = useState<TsutatsuIndex | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    api.tsutatsuIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);
  return { data, loading };
}

function useTsutatsuSet(tax: string | null) {
  const [data, setData] = useState<TsutatsuSet | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!tax) { setData(null); return; }
    let cancelled = false;
    setLoading(true); setData(null);
    api.tsutatsuSet(tax)
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [tax]);
  return { data, loading };
}

function ItemCard({ item, query }: { item: TsutatsuItem; query: string }) {
  // テキストはクエリ周辺を抜粋表示。
  const excerpt = useMemo(() => {
    const q = query.trim();
    if (!q) return item.text.length > 220 ? item.text.slice(0, 220) + "…" : item.text;
    const i = item.text.indexOf(q);
    if (i < 0) return item.text.length > 220 ? item.text.slice(0, 220) + "…" : item.text;
    const start = Math.max(0, i - 60);
    return (start > 0 ? "…" : "") + item.text.slice(start, i + q.length + 140) + "…";
  }, [item.text, query]);
  return (
    <div className="px-4 py-3 border-b border-border">
      <div className="flex items-center gap-2 mb-0.5">
        <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0">{item.number}</span>
        {item.caption && <span className="text-sm font-medium">{item.caption}</span>}
        <a href={item.source_url} target="_blank" rel="noreferrer" className="ml-auto text-muted-foreground hover:text-primary shrink-0" title="国税庁 原文">
          <ExternalLink className="size-3.5" />
        </a>
      </div>
      <p className="text-sm text-muted-foreground leading-relaxed">{excerpt}</p>
    </div>
  );
}

// HashRouter のため `#/tsutatsu?tax=shotoku` の query はハッシュ内にある。
function initialTaxFromUrl(): string | null {
  const hash = window.location.hash;
  const qi = hash.indexOf("?");
  if (qi < 0) return null;
  return new URLSearchParams(hash.slice(qi + 1)).get("tax");
}

export function TsutatsuView() {
  const { data: index, loading: idxLoading } = useTsutatsuIndex();
  const [tax, setTax] = useState<string | null>(() => initialTaxFromUrl());
  useEffect(() => {
    // URL 指定が無い場合のみ先頭の通達集を既定選択。
    if (!tax && index && index.sets.length > 0) setTax(index.sets[0].tax);
  }, [index, tax]);
  const { data: set, loading } = useTsutatsuSet(tax);
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const items = set?.items ?? [];
    const q = query.trim();
    if (!q) return items;
    return items.filter(i =>
      i.number.includes(q) || (i.caption ?? "").includes(q) || i.text.includes(q),
    );
  }, [set, query]);

  return (
    <div className="p-6 max-w-4xl">
      <div className="mb-4">
        <h1 className="text-2xl flex items-center gap-2"><ScrollText className="size-6" />通達</h1>
        <p className="text-sm text-muted-foreground mt-1">
          行政の法令解釈通達（法令本文に載らない soft law）。出典: 国税庁
        </p>
      </div>

      <div className="flex flex-wrap gap-2 mb-4 items-center">
        <Select value={tax ?? ""} onValueChange={setTax}>
          <SelectTrigger className="h-8 text-sm w-56"><SelectValue placeholder="通達集" /></SelectTrigger>
          <SelectContent>
            {(index?.sets ?? []).map(s => (
              <SelectItem key={s.tax} value={s.tax}>{s.name}（{s.count}）</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="relative flex-1 min-w-[12rem] max-w-md">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="番号・見出し・本文で絞り込み…" className="pl-8 h-8 text-sm" />
        </div>
        {set && <span className="text-xs text-muted-foreground">{filtered.length}/{set.items.length}件</span>}
      </div>

      <div className="border border-border rounded-lg overflow-hidden">
        <ScrollArea className="max-h-[calc(100vh-240px)]">
          {idxLoading || loading ? (
            <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}</div>
          ) : !set || filtered.length === 0 ? (
            <p className="p-8 text-center text-sm text-muted-foreground">
              {index ? "該当する通達がありません" : "通達を読み込めませんでした"}
            </p>
          ) : (
            filtered.map((item, i) => <ItemCard key={`${item.number}-${i}`} item={item} query={query} />)
          )}
        </ScrollArea>
      </div>
    </div>
  );
}
