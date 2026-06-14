import { Suspense, lazy, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Badge } from "../ui/badge";
import { TrendingUp, Database, FileText, CheckCircle2, ArrowUpRight, ChevronDown, ChevronRight, GitCompare } from "lucide-react";
import { Button } from "../ui/button";
import { useLiveSnapshot } from "../../data/use-live-data";
import { Link } from "react-router";

// recharts を含む可視化要素は別チャンクへ。
const StatTrend = lazy(() => import("./dashboard-charts").then(m => ({ default: m.StatTrend })));
const UpdateTrendCard = lazy(() => import("./dashboard-charts").then(m => ({ default: m.UpdateTrendCard })));

import { type UpdateDay } from "../../data/use-live-data";

function RecentUpdatesCard({ trend14, loading }: { trend14: UpdateDay[]; loading: boolean }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const days = trend14.filter(d => d.count > 0).slice().reverse().slice(0, 7);

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle>直近の更新</CardTitle>
        <Button variant="ghost" size="sm" asChild>
          <Link to="/updates">すべて見る</Link>
        </Button>
      </CardHeader>
      <CardContent className="space-y-1 p-4 pt-0">
        {days.map(u => {
          const open = expanded === u.fullDate;
          return (
            <div key={u.fullDate} className="border border-border rounded-md overflow-hidden">
              <button
                className="w-full flex items-center gap-3 px-3 py-2.5 hover:bg-accent transition-colors text-left"
                onClick={() => setExpanded(open ? null : u.fullDate)}
              >
                {open ? <ChevronDown className="size-3.5 text-muted-foreground shrink-0" /> : <ChevronRight className="size-3.5 text-muted-foreground shrink-0" />}
                <span className="text-xs text-muted-foreground tabular-nums w-12 shrink-0">{u.date}</span>
                <Badge variant="secondary" className="tabular-nums text-xs">{u.count} 件</Badge>
              </button>
              {open && (
                <div className="border-t border-border bg-muted/30">
                  {u.laws.map(l => (
                    <div key={l.law_id} className="flex items-center justify-between px-4 py-2 gap-2 border-b border-border/50 last:border-0 hover:bg-accent/50 transition-colors">
                      <div className="min-w-0 flex-1">
                        <Link to={`/laws/${l.law_id}`} className="text-xs truncate block hover:underline">{l.title}</Link>
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        <Badge variant={l.change_type === 'added' ? 'default' : l.change_type === 'removed' ? 'destructive' : 'secondary'} className="text-xs px-1.5">
                          {l.change_type === 'added' ? '追加' : l.change_type === 'removed' ? '廃止' : '改正'}
                        </Badge>
                        <Button variant="ghost" size="icon" className="size-6" asChild>
                          <Link to={`/laws/${l.law_id}/compare`} title="差分を見る">
                            <GitCompare className="size-3" />
                          </Link>
                        </Button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
        {!loading && days.length === 0 && (
          <div className="text-xs text-muted-foreground py-4 text-center">直近 14 日に更新はありません</div>
        )}
        {loading && (
          <div className="text-xs text-muted-foreground py-4 text-center">読み込み中…</div>
        )}
      </CardContent>
    </Card>
  );
}

function StatCard({ label, value, delta, icon: Icon, trend }: any) {
  return (
    <Card>
      <CardContent className="p-5">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <div className="text-sm text-muted-foreground">{label}</div>
            <div className="text-2xl tabular-nums">{value}</div>
            <div className="flex items-center gap-1 text-xs text-emerald-500">
              <ArrowUpRight className="size-3" />
              {delta}
            </div>
          </div>
          <div className="size-10 rounded-md bg-muted flex items-center justify-center">
            <Icon className="size-5 text-muted-foreground" />
          </div>
        </div>
        {trend && (
          <Suspense fallback={<div className="h-10 mt-3 -mx-1" />}>
            <StatTrend label={label} data={trend} />
          </Suspense>
        )}
      </CardContent>
    </Card>
  );
}

function ChartFallback({ height = "h-64" }: { height?: string }) {
  return (
    <Card>
      <CardContent className="p-5 flex items-center justify-center">
        <div className={`${height} w-full flex items-center justify-center text-xs text-muted-foreground`}>
          チャート読み込み中…
        </div>
      </CardContent>
    </Card>
  );
}

export function DashboardView() {
  const { laws, health, latestUpdates, trend14, loading, error } = useLiveSnapshot();

  const lawCount = laws?.laws.length ?? null;
  const fileCount = health?.file_count ?? null;
  const healthOk = health?.ok ?? null;
  const featuredLaws = laws?.laws.slice(0, 5) ?? [];
  // 直近 14 日のリアル更新だけを使う。読み込み中は空配列。
  const trendForChart = trend14.map(d => ({ month: d.date, count: d.count }))  // d.date は MM-DD;
  const trendSum = trend14.reduce((acc, d) => acc + d.count, 0);
  const fmt = (n: number | null | undefined) =>
    n === null || n === undefined ? "—" : n.toLocaleString();

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl">ダッシュボード</h1>
          <p className="text-sm text-muted-foreground mt-1">
            e-Gov法令データ配信基盤の最新状態
            {health?.generated_at && (
              <span className="ml-2 text-xs">(generated {new Date(health.generated_at).toLocaleString("ja-JP")})</span>
            )}
          </p>
        </div>
        <Badge variant="outline" className="gap-1.5">
          <span className={`size-1.5 rounded-full ${
            healthOk === null ? "bg-muted-foreground" : healthOk ? "bg-emerald-500 animate-pulse" : "bg-red-500"
          }`} />
          {loading ? "読み込み中" : healthOk === null ? "未取得" : healthOk ? "稼働中" : "異常"}
        </Badge>
      </div>

      {error && (
        <div className="text-xs text-amber-600 dark:text-amber-400">
          ライブ JSON 取得に失敗しました ({error})。
        </div>
      )}

      {(() => {
        // generated_at が 24h 以上前なら cron が止まっている疑いあり。
        if (!health?.generated_at) return null;
        const ageMs = Date.now() - new Date(health.generated_at).getTime();
        if (ageMs < 24 * 60 * 60 * 1000) return null;
        const hours = Math.floor(ageMs / (60 * 60 * 1000));
        return (
          <div className="text-xs text-amber-600 dark:text-amber-400 border border-amber-500/40 bg-amber-500/10 rounded-md px-3 py-2">
            ⚠ データが {hours} 時間以上更新されていません — GitHub Actions の cron 状況を確認してください。
          </div>
        );
      })()}

      <div className="grid grid-cols-4 gap-4">
        <StatCard label="登録法令数" value={fmt(lawCount)} delta={lawCount === null ? "" : `${lawCount}件`} icon={Database} trend={trendForChart} />
        <StatCard label="直近 14 日更新" value={fmt(trend14.length ? trendSum : null)} delta={latestUpdates?.latest_update_date ?? ""} icon={TrendingUp} trend={trendForChart} />
        <StatCard label="配信ファイル数" value={fmt(fileCount)} delta={health ? "manifest基準" : ""} icon={FileText} />
        <StatCard label="ヘルス" value={healthOk === null ? "—" : healthOk ? "OK" : "NG"} delta={health?.latest_egov_update_date ?? ""} icon={CheckCircle2} />
      </div>

      <div className="grid grid-cols-1 gap-4">
        <Suspense fallback={<ChartFallback />}>
          <UpdateTrendCard data={trendForChart} title="更新トレンド (直近 14 日)" />
        </Suspense>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <RecentUpdatesCard trend14={trend14} loading={loading} />

        <Card>
          <CardHeader><CardTitle>注目の法令</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            {featuredLaws.map((l: any) => (
              <div key={l.law_id} className="flex items-center justify-between p-3 rounded-md border border-border hover:bg-accent transition-colors cursor-pointer">
                <div className="min-w-0">
                  <div className="text-sm truncate">{l.title}</div>
                  <div className="text-xs text-muted-foreground truncate">{l.law_num ?? l.law_id}</div>
                </div>
                <Badge variant="outline">現行</Badge>
              </div>
            ))}
            {!loading && featuredLaws.length === 0 && (
              <div className="text-xs text-muted-foreground py-4 text-center">
                法令データ未取得
              </div>
            )}
            {loading && featuredLaws.length === 0 && (
              <div className="text-xs text-muted-foreground py-4 text-center">読み込み中…</div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
