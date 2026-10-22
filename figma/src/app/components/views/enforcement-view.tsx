import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router";
import { Badge } from "../ui/badge";
import { Skeleton } from "../ui/skeleton";
import { CalendarClock, ArrowUpRight } from "lucide-react";
import { api, type EnforcementUpcoming, type EnforcementItem } from "../../data/api";

import { EnforcementExplanation } from "./enforcement-explanation";

function useUpcoming() {
  const [data, setData] = useState<EnforcementUpcoming | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.enforcementUpcoming()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [attempt]);
  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

export function EnforcementView() {
  const { data, loading, retry } = useUpcoming();
  const [query, setQuery] = useState("");

  // 施行日でグルーピング（昇順）。
  const groups = useMemo(() => {
    const items = (data?.items ?? []).filter(i => {
      const q = query.trim();
      if (!q) return true;
      return i.title.includes(q) || (i.amending_law_title ?? "").includes(q);
    });
    const m = new Map<string, EnforcementItem[]>();
    for (const i of items) {
      if (!m.has(i.date)) m.set(i.date, []);
      m.get(i.date)!.push(i);
    }
    return [...m.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [data, query]);

  return (
    <div className="p-6 max-w-4xl">
      <div className="mb-5">
        <h1 className="text-2xl flex items-center gap-2"><CalendarClock className="size-6" />施行予定</h1>
        <p className="text-sm text-muted-foreground mt-1">
          今後施行される法令改正の一覧（施行日が近い順）
          {data && <span className="ml-2 text-xs">· {data.as_of} 時点 / {data.count}件</span>}
        </p>
      </div>

      <input
        value={query}
        onChange={e => setQuery(e.target.value)}
        aria-label="施行予定の絞り込み"
        placeholder="法令名・改正名で絞り込み…"
        className="w-full max-w-md mb-4 h-9 px-3 rounded-md border border-border bg-background text-sm"
      />

      {loading ? (
        <div className="space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-12 w-full" />)}</div>
      ) : !data ? (
        <div role="alert" className="py-8 text-center text-sm text-muted-foreground">
          <p>施行予定を読み込めませんでした</p>
          <button type="button" onClick={retry} className="mt-3 text-primary hover:underline">再試行</button>
        </div>
      ) : groups.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted-foreground">該当する施行予定がありません</p>
      ) : (
        <div className="space-y-5">
          {groups.map(([date, items]) => (
            <div key={date}>
              <div className="sticky top-0 bg-background py-1 flex items-center gap-2">
                <CalendarClock className="size-4 text-primary" />
                <span className="text-sm font-semibold">{date}</span>
                <span className="text-xs text-muted-foreground">{items.length}件</span>
              </div>
              <div className="border border-border rounded-lg overflow-hidden mt-1">
                {items.map((i, idx) => (
                  <div className="border-b border-border last:border-0"
                    key={`${i.law_id}-${idx}`}
                  >
                    <Link
                      to={`/laws/${i.law_id}`}
                      className="w-full text-left px-4 py-2.5 hover:bg-accent/50 transition-colors flex items-start gap-3"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="text-sm font-medium break-words">{i.title || i.law_id}</div>
                        {i.targets && <div className="text-xs text-muted-foreground break-words">一部施行: {i.targets}</div>}
                        {i.amending_law_title && (
                          <div className="text-xs text-muted-foreground break-words">{i.amending_law_title}</div>
                        )}
                      </div>
                      {i.date_kind === "scheduled" && (
                        <Badge variant="outline" className="text-xs shrink-0">施行予定</Badge>
                      )}
                      {i.date_kind === "supplementary" && <Badge variant="outline" className="text-xs shrink-0" title="附則の施行期日の規定から読み取った日付">附則</Badge>}
                      <ArrowUpRight className="size-4 text-muted-foreground shrink-0 mt-0.5" />
                    </Link>
                    <div className="px-4">
                      <EnforcementExplanation lawId={i.law_id} eventId={i.event_id} revisionId={i.revision_id} explanationId={i.explanation_id} date={i.date} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
