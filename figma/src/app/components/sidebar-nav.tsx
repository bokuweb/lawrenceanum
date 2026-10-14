import { useEffect, useState } from "react";
import { LayoutDashboard, Search, BookOpen, History, Settings, Scale, Landmark, MessageSquare, Bell, CalendarClock, FileText, ScrollText, Network, Building2, Users, Briefcase, BarChart3, ScanText, CircleHelp } from "lucide-react";
import { NavLink, useLocation } from "react-router";
import { cn } from "./ui/utils";
import { api } from "../data/api";
import { EgovAttribution } from "./egov-attribution";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { useKanpoParserEnabled } from "../kanpo-parser-flag";

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

const items: { path: string; label: string; icon: any; matchPrefix?: string; kanpoParser?: boolean }[] = [
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
  // 本番では ?kanpo-parser=1 で有効にしたときだけ（kanpo-parser-flag.ts）。
  { path: "/kanpo-playground", label: "官報パーサ", icon: ScanText, kanpoParser: true },
  { path: "/settings", label: "設定", icon: Settings },
];

export function SidebarNav() {
  const loc = useLocation();
  const { sync: lastSync, egovDate } = useLastSync();
  const kanpoParser = useKanpoParserEnabled();
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
        {items.filter(it => !it.kanpoParser || kanpoParser).map(it => {
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
          <div className="flex items-center gap-1 text-foreground mb-1">
            最新同期
            {/* 出典表記はサイドバーに常時出すと場所を取るのでヘルプの tooltip に寄せる。
                法令詳細には常時出しているので、出典の明示はそちらでも満たす。 */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label="データの出典"
                  className="rounded-sm text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <CircleHelp className="size-3.5" />
                </button>
              </TooltipTrigger>
              {/* aria-label を渡すと Radix が読み上げ用に children を複製しなくなる
                  (複製されると data-testid が二重になる)。 */}
              <TooltipContent
                side="right"
                sideOffset={6}
                className="max-w-64"
                aria-label={`出典：e-Gov法令検索（デジタル庁）のデータを lawrenceanum が加工して作成${egovDate ? `（${egovDate} 時点）` : ""}。公共データ利用規約（第1.0版）`}
              >
                <EgovAttribution
                  asOf={egovDate}
                  className="text-[11px] text-primary-foreground [&_a:hover]:text-primary-foreground/80"
                />
              </TooltipContent>
            </Tooltip>
          </div>
          {lastSync ?? "読み込み中…"}
        </div>
      </div>
    </aside>
  );
}
