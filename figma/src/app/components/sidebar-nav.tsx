import { useEffect, useState } from "react";
import { LayoutDashboard, Search, BookOpen, History, Newspaper, Settings, Scale } from "lucide-react";
import { NavLink, useLocation } from "react-router";
import { cn } from "./ui/utils";
import { api } from "../data/api";

/** ISO (UTC) を "YYYY-MM-DD HH:mm JST" に整形。 */
function formatJst(iso: string | undefined | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (isNaN(d.getTime())) return null;
  const jst = new Date(d.getTime() + 9 * 60 * 60 * 1000); // UTC+9
  const p = (n: number) => String(n).padStart(2, "0");
  return `${jst.getUTCFullYear()}-${p(jst.getUTCMonth() + 1)}-${p(jst.getUTCDate())} ${p(jst.getUTCHours())}:${p(jst.getUTCMinutes())} JST`;
}

/** 最新同期時刻。health.json の generated_at を採用し、無ければ index.json にフォールバック。 */
function useLastSync(): string | null {
  const [sync, setSync] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    api
      .health()
      .then((h) => formatJst(h.generated_at))
      .catch(() => api.index().then((i) => formatJst(i.generated_at)).catch(() => null))
      .then((v) => { if (!cancelled && v) setSync(v); });
    return () => { cancelled = true; };
  }, []);
  return sync;
}

const items: { path: string; label: string; icon: any; matchPrefix?: string }[] = [
  { path: "/", label: "ダッシュボード", icon: LayoutDashboard },
  { path: "/search", label: "検索", icon: Search },
  { path: "/laws", label: "法令閲覧", icon: BookOpen, matchPrefix: "/laws" },
  { path: "/updates", label: "更新履歴", icon: History },
  { path: "/kanpo", label: "官報リンク", icon: Newspaper },
  { path: "/settings", label: "設定", icon: Settings },
];

export function SidebarNav() {
  const loc = useLocation();
  const lastSync = useLastSync();
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
      </div>
    </aside>
  );
}
