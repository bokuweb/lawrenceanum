import { useEffect, useState } from "react";
import { setChartPatterns, useChartPatterns } from "../chart-patterns";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Badge } from "../ui/badge";
import { Skeleton } from "../ui/skeleton";
import { FileCheck2, ChevronDown, ChevronRight, GitCompare } from "lucide-react";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { Label } from "../ui/label";
import { useTheme } from "../theme-provider";
import { Link } from "react-router";
import { api, type UpdatesByDate } from "../../data/api";

import { type UpdateEntry } from "../../data/api";
type UpdatesIndexEntry = { date: string; count: number; laws: UpdateEntry[] };

/**
 * `updates/latest.json` と最近 14 日分の `updates/{date}.json` を fetch する。
 * `manifest.json` には全ファイルが載っているのでそれをスキャンしても良いが、
 * ペイロードが大きいので簡易に直近 14 日を試行する。
 */
function useUpdatesIndex(): { entries: UpdatesIndexEntry[]; loading: boolean; live: boolean } {
  const [state, setState] = useState({ entries: [] as UpdatesIndexEntry[], loading: true, live: false });
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const latest = await api.latestUpdates();
        const today = new Date();
        const dates: string[] = [];
        for (let i = 0; i < 14; i++) {
          const d = new Date(today);
          d.setUTCDate(d.getUTCDate() - i);
          dates.push(d.toISOString().slice(0, 10));
        }
        const results = await Promise.all(
          dates.map(d =>
            api.updatesOnDate(d).then(v => ({ d, v })).catch(() => null)
          )
        );
        if (cancelled) return;
        const entries: UpdatesIndexEntry[] = [];
        for (const r of results) {
          if (!r) continue;
          entries.push({
            date: r.d,
            count: r.v.updated_laws.length,
            laws: r.v.updated_laws,
          });
        }
        // latest が dates に含まれない場合は補う。
        const isYmd = (s: string) => /^\d{4}-\d{2}-\d{2}$/.test(s);
        if (
          latest.latest_update_date &&
          isYmd(latest.latest_update_date) &&
          !entries.find(e => e.date === latest.latest_update_date)
        ) {
          entries.unshift({
            date: latest.latest_update_date,
            count: latest.updated_laws.length,
            laws: latest.updated_laws,
          });
        }
        // 過去の bulk-catN ラベルや空文字を除外。
        const cleaned = entries.filter(e => isYmd(e.date));
        cleaned.sort((a, b) => b.date.localeCompare(a.date));
        setState({ entries: cleaned, loading: false, live: cleaned.length > 0 });
      } catch {
        if (!cancelled) setState({ entries: [], loading: false, live: false });
      }
    })();
    return () => { cancelled = true; };
  }, []);
  return state;
}

export function UpdatesView() {
  const { entries, loading } = useUpdatesIndex();
  const [expanded, setExpanded] = useState<string | null>(null);

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-2xl">更新履歴</h1>
          <p className="text-sm text-muted-foreground mt-1">日付別のe-Gov更新サマリ</p>
        </div>
        <div className="text-xs text-muted-foreground">
          {loading ? "読み込み中…" : `${entries.length} 日`}
        </div>
      </div>
      <div className="space-y-2">
        {loading && Array.from({ length: 8 }).map((_, i) => (
          <Card key={i}>
            <CardContent className="p-4 flex items-center gap-4">
              <div className="w-20 shrink-0 space-y-1.5">
                <Skeleton className="h-7 w-10 mx-auto" />
                <Skeleton className="h-3 w-14 mx-auto" />
              </div>
              <div className="flex-1 space-y-2">
                <Skeleton className="h-5 w-28" />
                <Skeleton className="h-3 w-2/3" />
              </div>
            </CardContent>
          </Card>
        ))}
        {!loading && entries.map(u => {
          const open = expanded === u.date;
          const lawEntries: any[] = Array.isArray(u.laws) ? u.laws : [];
          return (
            <Card key={u.date} className="overflow-hidden">
              <button
                className="w-full text-left"
                onClick={() => setExpanded(open ? null : u.date)}
              >
                <CardContent className="p-4 flex items-center gap-4 hover:bg-accent/50 transition-colors">
                  <div className="text-center w-20 shrink-0">
                    <div className="text-2xl tabular-nums">{u.date.slice(8)}</div>
                    <div className="text-xs text-muted-foreground">{u.date.slice(0, 7)}</div>
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <Badge variant="secondary">{u.count} 件の更新</Badge>
                      <FileCheck2 className="size-4 text-emerald-500" />
                    </div>
                    <div className="text-xs text-muted-foreground mt-1">
                      {lawEntries.slice(0, 3).map((l: any) => typeof l === 'string' ? l : l.title).join("、")}
                      {lawEntries.length > 3 && ` 他 ${lawEntries.length - 3} 件`}
                    </div>
                  </div>
                  {open ? <ChevronDown className="size-4 text-muted-foreground shrink-0" /> : <ChevronRight className="size-4 text-muted-foreground shrink-0" />}
                </CardContent>
              </button>
              {open && lawEntries.length > 0 && (
                <div className="border-t border-border">
                  {lawEntries.map((l: any, i: number) => {
                    if (typeof l === 'string') {
                      return (
                        <div key={i} className="px-6 py-2.5 text-sm border-b border-border/50 last:border-0">{l}</div>
                      );
                    }
                    return (
                      <div key={l.law_id ?? i} className="flex items-center justify-between px-6 py-2.5 gap-3 border-b border-border/50 last:border-0 hover:bg-accent/40 transition-colors">
                        <div className="min-w-0 flex-1">
                          <Link to={`/laws/${l.law_id}`} className="text-sm truncate block hover:underline">{l.title}</Link>
                        </div>
                        <div className="flex items-center gap-1.5 shrink-0">
                          <Badge variant={l.change_type === 'added' ? 'default' : l.change_type === 'removed' ? 'destructive' : 'secondary'} className="text-xs px-1.5">
                            {l.change_type === 'added' ? '追加' : l.change_type === 'removed' ? '廃止' : '改正'}
                          </Badge>
                          <Button variant="ghost" size="icon" className="size-7" asChild>
                            <Link to={`/laws/${l.law_id}/compare`} title="差分を見る">
                              <GitCompare className="size-3.5" />
                            </Link>
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </Card>
          );
        })}
        {!loading && entries.length === 0 && (
          <div className="text-sm text-muted-foreground text-center py-12">更新履歴がまだありません</div>
        )}
      </div>
    </div>
  );
}

function ChartPatternsSetting() {
  const [, on] = useChartPatterns();
  return (
    <div className="flex items-center justify-between">
      <div>
        <Label>グラフを模様でも区別する</Label>
        <div className="text-xs text-muted-foreground mt-0.5">色の区別がつきにくい場合に、系列を斜線の向きでも見分けられるようにします</div>
      </div>
      <Switch checked={on} onCheckedChange={setChartPatterns} aria-label="グラフを模様でも区別する" />
    </div>
  );
}

export function SettingsView() {
  const { theme, toggle } = useTheme();
  const [base, setBase] = useState("");
  useEffect(() => { setBase(new URL(".", document.baseURI).toString()); }, []);
  return (
    <div className="p-6 space-y-6 max-w-2xl">
      <div>
        <h1 className="text-2xl">設定</h1>
        <p className="text-sm text-muted-foreground mt-1">表示と通知の設定</p>
      </div>
      <Card>
        <CardHeader><CardTitle>外観</CardTitle></CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <Label>ダークモード</Label>
              <div className="text-xs text-muted-foreground mt-0.5">現在: {theme}</div>
            </div>
            <Switch checked={theme === "dark"} onCheckedChange={toggle} />
          </div>
          <ChartPatternsSetting />
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle>API エンドポイント</CardTitle></CardHeader>
        <CardContent className="space-y-2">
          <div className="text-xs text-muted-foreground">この配信のベースURL</div>
          <pre className="text-xs bg-muted rounded-md p-3 overflow-auto">{base || "—"}</pre>
          <div className="text-xs text-muted-foreground">主要エンドポイント</div>
          <ul className="text-xs space-y-1">
            <li><a className="underline" href="./index.json" target="_blank" rel="noreferrer">./index.json</a> — 全体エントリポイント</li>
            <li><a className="underline" href="./laws/index.json" target="_blank" rel="noreferrer">./laws/index.json</a> — 法令一覧</li>
            <li><a className="underline" href="./updates/latest.json" target="_blank" rel="noreferrer">./updates/latest.json</a> — 直近更新</li>
            <li><a className="underline" href="./health.json" target="_blank" rel="noreferrer">./health.json</a> — ヘルス</li>
            <li><a className="underline" href="./manifest.json" target="_blank" rel="noreferrer">./manifest.json</a> — 全ファイル一覧 + sha256</li>
          </ul>
        </CardContent>
      </Card>
    </div>
  );
}
