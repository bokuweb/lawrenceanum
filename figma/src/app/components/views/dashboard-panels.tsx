import { useMemo, type ReactNode } from "react";
import { Link } from "react-router";
import { BookOpen, Bell, CalendarClock, ExternalLink, Landmark, MessageSquare, Network, Newspaper } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { WIKI_TYPES, WIKI_TYPE_ORDER } from "./wiki-types";
import { useShowStubs } from "./wiki-prefs";
import type { EnforcementItem, EnforcementUpcoming, FeedItem, RecentFeed, WikiIndex } from "../../data/api";

/**
 * ダッシュボードの横断パネル。法令の数値 (dashboard-view.tsx) とは別に、
 * wiki / 新着フィード / 施行予定という「法令の外側」の収録状況を出す。
 * どのパネルもデータが無ければその旨を 1 行で出し、他を巻き込まない。
 */

function Empty({ children }: { children: ReactNode }) {
  return <div className="text-xs text-muted-foreground py-4 text-center">{children}</div>;
}

/** OKF type の色付きチップ (wiki ビューのバッジと同じ配色)。 */
function TypeChip({ type, count }: { type: string; count?: number }) {
  const t = WIKI_TYPES[type];
  if (!t) return null;
  return (
    <span
      data-wiki-type={type}
      className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs"
      style={{ color: t.color, backgroundColor: `${t.color}1f` }}
    >
      {t.label}
      {count !== undefined && <span className="tabular-nums font-medium">{count.toLocaleString()}</span>}
    </span>
  );
}

/** wiki の収録状況: OKF type ごとのページ数と、直近に日付が付いたページ。 */
export function WikiSummaryCard({ index, loading }: { index: WikiIndex | null; loading: boolean }) {
  // 要約前 (機械が作った一覧だけ) のページは wiki ビューと同じ既定で数から外す。
  const showStubs = useShowStubs();
  const pages = useMemo(
    () => (index?.pages ?? []).filter(p => WIKI_TYPES[p.type]).filter(p => showStubs || !p.stub),
    [index, showStubs],
  );
  const counts = useMemo(() => {
    const c = new Map<string, number>();
    for (const p of pages) c.set(p.type, (c.get(p.type) ?? 0) + 1);
    return c;
  }, [pages]);
  const recent = useMemo(
    () =>
      pages
        .filter(p => p.date)
        .sort((a, b) => (b.date ?? "").localeCompare(a.date ?? "") || a.title.localeCompare(b.title, "ja"))
        .slice(0, 5),
    [pages],
  );
  const stubCount = (index?.pages ?? []).filter(p => WIKI_TYPES[p.type] && p.stub).length;

  return (
    <Card data-testid="dashboard-wiki">
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2"><Network className="size-4" />wiki ページ</CardTitle>
        <Button variant="ghost" size="sm" asChild>
          <Link to="/wiki">すべて見る</Link>
        </Button>
      </CardHeader>
      <CardContent className="p-4 pt-0">
        {loading && !index ? (
          <Empty>読み込み中…</Empty>
        ) : !index ? (
          <Empty>wiki はまだ配信されていません</Empty>
        ) : (
          <>
            <div className="flex items-baseline gap-2">
              <span className="text-2xl tabular-nums">{pages.length.toLocaleString()}</span>
              <span className="text-sm text-muted-foreground">ページ</span>
              {!showStubs && stubCount > 0 && (
                <span className="text-xs text-muted-foreground ml-auto tabular-nums">要約前 {stubCount.toLocaleString()}</span>
              )}
            </div>
            <div className="mt-3 flex flex-wrap gap-1.5">
              {WIKI_TYPE_ORDER.filter(t => (counts.get(t) ?? 0) > 0).map(t => (
                <TypeChip key={t} type={t} count={counts.get(t) ?? 0} />
              ))}
            </div>
            <div className="mt-4 space-y-1">
              <div className="text-xs text-muted-foreground">直近に書かれたページ</div>
              {recent.map(p => (
                <Link
                  key={p.path}
                  to={`/wiki/${p.path}`}
                  className="flex items-center gap-2 px-3 py-2 rounded-md border border-border hover:bg-accent transition-colors"
                >
                  <span className="text-xs text-muted-foreground tabular-nums w-20 shrink-0">{p.date}</span>
                  <TypeChip type={p.type} />
                  <span className="text-sm truncate">{p.title}</span>
                </Link>
              ))}
              {recent.length === 0 && <Empty>日付の付いたページはまだありません</Empty>}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

const FEED_KINDS: Record<string, { label: string; icon: any }> = {
  law: { label: "法令改正", icon: BookOpen },
  hanrei: { label: "判例", icon: BookOpen },
  bill: { label: "法案", icon: Landmark },
  pubcomment: { label: "パブコメ", icon: MessageSquare },
  kanpo: { label: "官報", icon: Newspaper },
};

/** 規制変化フィードの先頭だけを出す。内部リンクは SPA 内、官報 PDF は外部リンク。 */
export function RecentFeedCard({ feed, loading }: { feed: RecentFeed | null; loading: boolean }) {
  const items = (feed?.items ?? []).slice(0, 6);
  const row = (item: FeedItem) => {
    const meta = FEED_KINDS[item.kind] ?? { label: item.kind, icon: Bell };
    const Icon = meta.icon;
    return (
      <>
        <Icon className="size-4 text-muted-foreground shrink-0" />
        <span className="text-xs text-muted-foreground tabular-nums w-20 shrink-0">{item.date}</span>
        <Badge variant="outline" className="text-xs shrink-0">{meta.label}</Badge>
        <span className="text-sm truncate">{item.title}</span>
        {!item.internal && <ExternalLink className="size-3 text-muted-foreground shrink-0 ml-auto" />}
      </>
    );
  };

  return (
    <Card data-testid="dashboard-feed">
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2"><Bell className="size-4" />新着</CardTitle>
        <Button variant="ghost" size="sm" asChild>
          <Link to="/feed">すべて見る</Link>
        </Button>
      </CardHeader>
      <CardContent className="space-y-1 p-4 pt-0">
        {items.map((item, i) =>
          item.internal ? (
            <Link
              key={`${item.kind}-${item.date}-${i}`}
              to={item.href}
              className="flex items-center gap-2 px-3 py-2 rounded-md border border-border hover:bg-accent transition-colors"
            >
              {row(item)}
            </Link>
          ) : (
            <a
              key={`${item.kind}-${item.date}-${i}`}
              href={item.href}
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-2 px-3 py-2 rounded-md border border-border hover:bg-accent transition-colors"
            >
              {row(item)}
            </a>
          ),
        )}
        {items.length === 0 && <Empty>{loading ? "読み込み中…" : "新着フィードはまだ配信されていません"}</Empty>}
      </CardContent>
    </Card>
  );
}

/** 施行日が近い順に、日付ごとの件数と代表の法令名を出す。 */
export function UpcomingEnforcementCard({ data, loading }: { data: EnforcementUpcoming | null; loading: boolean }) {
  const groups = useMemo(() => {
    const m = new Map<string, EnforcementItem[]>();
    for (const i of data?.items ?? []) {
      if (!m.has(i.date)) m.set(i.date, []);
      m.get(i.date)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0])).slice(0, 5);
  }, [data]);
  // 「今月の負荷」を一目で見るための 30 日以内の件数。
  const within30 = useMemo(() => {
    const limit = new Date();
    limit.setUTCDate(limit.getUTCDate() + 30);
    const iso = limit.toISOString().slice(0, 10);
    return (data?.items ?? []).filter(i => i.date <= iso).length;
  }, [data]);

  return (
    <Card data-testid="dashboard-enforcement">
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2"><CalendarClock className="size-4" />施行予定</CardTitle>
        <Button variant="ghost" size="sm" asChild>
          <Link to="/enforcement">すべて見る</Link>
        </Button>
      </CardHeader>
      <CardContent className="p-4 pt-0">
        {groups.length === 0 ? (
          <Empty>{loading ? "読み込み中…" : "施行予定はまだ配信されていません"}</Empty>
        ) : (
          <>
            <div className="flex items-baseline gap-2">
              <span className="text-2xl tabular-nums">{within30.toLocaleString()}</span>
              <span className="text-sm text-muted-foreground">件 / 30 日以内</span>
              <span className="text-xs text-muted-foreground ml-auto tabular-nums">全 {(data?.count ?? 0).toLocaleString()} 件</span>
            </div>
            <div className="mt-3 space-y-1">
              {groups.map(([date, items]) => (
                <Link
                  key={date}
                  to="/enforcement"
                  className="flex items-center gap-2 px-3 py-2 rounded-md border border-border hover:bg-accent transition-colors"
                >
                  <span className="text-xs text-muted-foreground tabular-nums w-20 shrink-0">{date}</span>
                  <Badge variant="secondary" className="text-xs tabular-nums shrink-0">{items.length} 件</Badge>
                  <span className="text-sm truncate">{items[0].title}</span>
                </Link>
              ))}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
