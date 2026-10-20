import { useEffect, useMemo, useState } from "react";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Button } from "../ui/button";
import { BarChart3, ExternalLink } from "lucide-react";
import { api, type BudgetIndex, type BudgetDataset, type BudgetValue } from "../../data/api";

function useBudgetIndex() {
  const [data, setData] = useState<BudgetIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.budgetIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [attempt]);
  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

function formatValue(v: string): string {
  // Number への変換は大きな整数の桁を失い、toLocaleString は小数を丸める。
  // 欠測記号や指数表記は元のまま保ち、通常の数値は文字列のまま桁区切りする。
  const match = /^([+-]?)(\d+)(\.\d+)?$/.exec(v.trim());
  return match ? match[1] + match[2].replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (match[3] ?? "") : v;
}

/** 全レコードで値が `time` と一致する次元を時間軸とみなす。 */
function findTimeDimension(values: BudgetValue[], keys: string[]): string | null {
  return keys.find(k => values.every(v => v.dimensions[k] === v.time)) ?? null;
}

function DatasetDetail({ statsId }: { statsId: string }) {
  const [dataset, setDataset] = useState<BudgetDataset | null>(null);
  const [loading, setLoading] = useState(true);
  const [selection, setSelection] = useState<Record<string, string>>({});
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setDataset(null); setSelection({});
    api.budgetDataset(statsId)
      .then(d => { if (!cancelled) { setDataset(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [statsId, attempt]);

  // 時間軸以外で値が複数ある次元を絞り込み条件にし、時系列表として見せる。
  const { timeKey, filterDims } = useMemo(() => {
    const values = dataset?.values ?? [];
    const keys = [...new Set(values.flatMap(v => Object.keys(v.dimensions)))];
    const timeKey = findTimeDimension(values, keys);
    const filterDims = keys
      .filter(k => k !== timeKey)
      .map(k => {
        const options: string[] = [];
        for (const v of values) {
          const x = v.dimensions[k];
          if (x !== undefined && !options.includes(x)) options.push(x);
        }
        return { key: k, options };
      })
      .filter(d => d.options.length > 1);
    return { timeKey, filterDims };
  }, [dataset]);

  // 先に選んだ次元に存在する選択肢だけを示す。上流を切り替えた場合も
  // 有効な系列へ補正し、実在しない組み合わせで空の表になるのを防ぐ。
  const filters = useMemo(() => {
    let candidates = dataset?.values ?? [];
    return filterDims.map(d => {
      const options = [...new Set(candidates.map(v => v.dimensions[d.key]).filter(v => v !== undefined))];
      const value = options.includes(selection[d.key]) ? selection[d.key] : options[0];
      candidates = candidates.filter(v => v.dimensions[d.key] === value);
      return { key: d.key, options, value };
    });
  }, [dataset, filterDims, selection]);

  const rows = useMemo(() => {
    const values = (dataset?.values ?? []).filter(v =>
      filters.every(d => v.dimensions[d.key] === d.value),
    );
    return values.slice().sort((a, b) =>
      (b.time ?? "").localeCompare(a.time ?? "", "ja", { numeric: true }),
    );
  }, [dataset, filters]);

  if (loading) return <div className="p-6 space-y-3">{[...Array(6)].map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (!dataset) return (
    <div role="alert" className="p-6 text-sm text-muted-foreground">
      <p>統計を読み込めませんでした</p>
      <Button variant="outline" size="sm" className="mt-2" onClick={() => setAttempt(n => n + 1)}>再試行</Button>
    </div>
  );

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="px-5 py-4 border-b border-border shrink-0 space-y-3">
        <div>
          <h2 className="text-base font-semibold leading-snug">{dataset.title}</h2>
          <div className="text-xs text-muted-foreground mt-1.5 flex flex-wrap gap-x-3 gap-y-1">
            <span>統計表ID {dataset.stats_data_id}</span>
            <span>{dataset.values.length.toLocaleString()} 値</span>
            <span>取得 {dataset.source.fetched_at.slice(0, 10)}</span>
          </div>
          <a href={`https://www.e-stat.go.jp/dbview?sid=${dataset.stats_data_id}`} target="_blank" rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs mt-2 px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
            e-Stat で見る <ExternalLink className="size-2.5" />
          </a>
        </div>
        {filters.length > 0 && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
            {filters.map(d => (
              <label key={d.key} className="space-y-1 min-w-0">
                <span className="text-xs text-muted-foreground block truncate">{d.key}</span>
                <Select value={d.value ?? ""} onValueChange={v => setSelection(s => ({ ...s, [d.key]: v }))}>
                  <SelectTrigger aria-label={d.key} className="h-7 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {d.options.map(o => <SelectItem key={o} value={o}>{o}</SelectItem>)}
                  </SelectContent>
                </Select>
              </label>
            ))}
          </div>
        )}
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border text-xs text-muted-foreground">
              <th className="text-left font-medium px-5 py-2">{timeKey ?? "時点"}</th>
              <th className="text-right font-medium px-5 py-2">値</th>
              <th className="text-left font-medium px-3 py-2 w-24">単位</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((v, i) => (
              <tr key={i} className="border-b border-border last:border-0">
                <td className="px-5 py-1.5 tabular-nums">{v.time ?? "—"}</td>
                <td className="px-5 py-1.5 text-right tabular-nums">{formatValue(v.value)}</td>
                <td className="px-3 py-1.5 text-muted-foreground">{v.unit ?? ""}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr><td colSpan={3} className="px-5 py-6 text-center text-sm text-muted-foreground">該当する値がありません</td></tr>
            )}
          </tbody>
        </table>
      </ScrollArea>
    </div>
  );
}

export function BudgetView({ statsId, onSelect }: {
  statsId: string | null;
  onSelect: (statsId: string) => void;
}) {
  const { data, loading, retry } = useBudgetIndex();
  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <div className="w-full max-h-52 md:max-h-none md:w-80 min-h-0 shrink-0 border-b md:border-b-0 md:border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 flex items-center gap-2">
          <h2 className="text-sm font-semibold flex-1">財政統計</h2>
          {data && <span className="text-xs text-muted-foreground">{data.datasets.length}統計</span>}
        </div>
        <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
          {loading ? (
            <div className="p-4 space-y-2">{[...Array(3)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}</div>
          ) : !data ? (
            <div role="alert" className="p-6 text-center text-sm text-muted-foreground">
              <p>統計の一覧を読み込めませんでした</p>
              <Button variant="outline" size="sm" className="mt-2" onClick={retry}>再試行</Button>
            </div>
          ) : data.datasets.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">データがありません</p>
          ) : (
            data.datasets.map(d => (
              <button key={d.stats_data_id} onClick={() => onSelect(d.stats_data_id)}
                className={["w-full text-left px-4 py-3 border-b border-border transition-colors", statsId === d.stats_data_id ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"].join(" ")}>
                <div className="text-sm font-medium">{d.title}</div>
                <div className="text-xs text-muted-foreground mt-0.5 tabular-nums">{d.stats_data_id} · {d.value_count.toLocaleString()} 値</div>
              </button>
            ))
          )}
        </ScrollArea>
      </div>
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        {statsId ? (
          <DatasetDetail statsId={statsId} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <BarChart3 className="size-10 opacity-30" />
            <p className="text-sm">統計を選択すると時系列の値が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
