import { useEffect, useState } from "react";
import { LayoutDashboard, Search, BookOpen, History, Settings, Scale, Landmark, MessageSquare, Bell, CalendarClock, FileText, ScrollText, Network, Building2, Users, Briefcase, BarChart3 } from "lucide-react";
import { NavLink, useLocation } from "react-router";
import { cn } from "./ui/utils";
import { api } from "../data/api";
import { EgovAttribution } from "./egov-attribution";

/** ISO (UTC) を "YYYY-MM-DD HH:mm JST" に整形。 */
function formatJst(iso: string | undefined | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (isNaN(d.getTime())) return null;
  const jst = new Date(d.getTime() + 9 * 60 * 60 * 1000); // UTC+9
  const p = (n: number) => String(n).padStart(2, "0");
  return `${jst.getUTCFullYear()}-${p(jst.getUTCMonth() + 1)}-${p(jst.getUTCDate())} ${p(jst.getUTCHours())}:${p(jst.getUTCMinutes())} JST`;
}

/**
 * 最新同期時刻と e-Gov 更新日。同期時刻は health.json の generated_at を採用し、
 * 無ければ index.json にフォールバック。e-Gov 更新日は出典表記の「時点」に使う。
 */
function useLastSync(): { sync: string | null; egovDate: string | null } {
  const [state, setState] = useState<{ sync: string | null; egovDate: string | null }>({ sync: null, egovDate: null });
  useEffect(() => {
    let cancelled = false;
    api
      .health()
      .then((h) => ({ sync: formatJst(h.generated_at), egovDate: h.latest_egov_update_date || null }))
      .catch(() =>
        api.index().then((i) => ({ sync: formatJst(i.generated_at), egovDate: null })).catch(() => null),
      )
      .then((v) => { if (!cancelled && v) setState(v); });
    return () => { cancelled = true; };
  }, []);
  return state;
}

const items: { path: string; label: string; icon: any; matchPrefix?: string }[] = [
  { path: "/", label: "ダッシュボード", icon: LayoutDashboard },
  { path: "/feed", label: "新着", icon: Bell, matchPrefix: "/feed" },
  { path: "/search", label: "検索", icon: Search },
  { path: "/laws", label: "法令閲覧", icon: BookOpen, matchPrefix: "/laws" },
  { path: "/proceedings", label: "国会会議録", icon: Landmark, matchPrefix: "/proceedings" },
  { path: "/gian", label: "議案", icon: FileText, matchPrefix: "/gian" },
  { path: "/pubcomment", label: "パブコメ", icon: MessageSquare, matchPrefix: "/pubcomment" },
  { path: "/tsutatsu", label: "通達", icon: ScrollText, matchPrefix: "/tsutatsu" },
  { path: "/shingikai", label: "審議会", icon: Users, matchPrefix: "/shingikai" },
  { path: "/procurement", label: "政府調達", icon: Briefcase, matchPrefix: "/procurement" },
  { path: "/budget", label: "財政統計", icon: BarChart3, matchPrefix: "/budget" },
  { path: "/reiki", label: "自治体例規", icon: Building2, matchPrefix: "/reiki" },
  { path: "/wiki", label: "wiki", icon: Network, matchPrefix: "/wiki" },
  { path: "/enforcement", label: "施行予定", icon: CalendarClock, matchPrefix: "/enforcement" },
  { path: "/updates", label: "更新履歴", icon: History },
  { path: "/settings", label: "設定", icon: Settings },
];

export function SidebarNav() {
  const loc = useLocation();
  const { sync: lastSync, egovDate } = useLastSync();
  return (
    <aside className="w-60 border-r border-border bg-sidebar flex flex-col">
      <div className="h-16 flex items-center gap-2 px-5 border-b border-border">
        <div className="size-8 rounded-md bg-primary flex items-center justify-center">
          <Scale className="size-4 text-primary-foreground" />
        </div>
        <div className="flex flex-col leading-tight">
          <span className="text-sidebar-foreground">Lawrenceanum</span>
          <span className="text-xs text-muted-foreground">e-Gov 法令データ</span>
        </div>
      </div>
      <nav className="flex-1 p-3 space-y-1">
        {items.map(it => {
          const Icon = it.icon;
          // `/` is exact-match only; everything else matches by prefix so that
          // /laws/:lawId still highlights "法令閲覧".
          const exact = it.path === "/";
          const isActive = exact
            ? loc.pathname === "/"
            : loc.pathname === it.path || loc.pathname.startsWith((it.matchPrefix ?? it.path) + "/");
          return (
            <NavLink
              key={it.path}
              to={it.path}
              end={exact}
              className={cn(
                "flex items-center gap-3 h-9 px-3 rounded-md text-sm transition-colors",
                isActive
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "hover:bg-accent text-foreground/80",
              )}
            >
              <Icon className="size-4" />
              {it.label}
            </NavLink>
          );
        })}
      </nav>
      <div className="p-3 border-t border-border">
        <div className="rounded-md bg-muted/50 p-3 text-xs text-muted-foreground">
          <div className="text-foreground mb-1">最新同期</div>
          {lastSync ?? "読み込み中…"}
        </div>
        <EgovAttribution asOf={egovDate} className="mt-2 px-1 text-[11px]" />
      </div>
    </aside>
  );
}
