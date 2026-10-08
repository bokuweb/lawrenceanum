import { Area, AreaChart, Bar, BarChart, CartesianGrid, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Activity } from "lucide-react";
import { LAW_KINDS, type LawKind, type UpdateBreakdown } from "../../data/law-kind";

/**
 * recharts (≈557KB) を含むビジュアライズ部分をまとめたモジュール。
 * `dashboard-view.tsx` から `lazy(() => import('./dashboard-charts'))` で
 * 遅延読み込みするので、ダッシュボード初期レンダーは
 * recharts なしで stat カード本体だけ即時に出る。
 */

const COLORS = [
  "var(--foreground)",
  "color-mix(in oklab, var(--foreground) 78%, var(--background))",
  "color-mix(in oklab, var(--foreground) 60%, var(--background))",
  "color-mix(in oklab, var(--foreground) 45%, var(--background))",
  "color-mix(in oklab, var(--foreground) 32%, var(--background))",
  "color-mix(in oklab, var(--foreground) 20%, var(--background))",
  "color-mix(in oklab, var(--foreground) 12%, var(--background))",
];

export function StatTrend({ label, data }: { label: string; data: { month: string; count: number }[] }) {
  const id = `g-${label.replace(/[^a-z0-9]/gi, "-")}`;
  return (
    <div className="h-10 mt-3 -mx-1">
      <ResponsiveContainer>
        <AreaChart data={data}>
          <defs>
            <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--primary)" stopOpacity={0.4} />
              <stop offset="100%" stopColor="var(--primary)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <Area dataKey="count" stroke="var(--primary)" fill={`url(#${id})`} strokeWidth={1.5} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

const LAW_KIND_COLORS: Record<LawKind, string> = {
  法律: "var(--chart-law-act)",
  政令: "var(--chart-law-cabinet)",
  府省令: "var(--chart-law-ministerial)",
  その他: "var(--chart-law-other)",
};

const TOOLTIP_TITLE_LIMIT = 5;

/** 積み上げの最上段だけ上端を角丸にする (途中の段は角丸にすると隙間が空く)。 */
function StackSegment(props: any) {
  const { x, y, width, height, fill, payload, kind } = props;
  if (!height || height <= 0) return null;
  const top = [...LAW_KINDS].reverse().find(k => (payload as UpdateBreakdown).kinds[k] > 0);
  const r = top === kind ? Math.min(4, width / 2, height) : 0;
  const d = r
    ? `M${x},${y + height}V${y + r}Q${x},${y} ${x + r},${y}H${x + width - r}Q${x + width},${y} ${x + width},${y + r}V${y + height}Z`
    : `M${x},${y + height}V${y}H${x + width}V${y + height}Z`;
  // 上下の段の stroke が重なって境目に約 2px のカード地色の隙間ができる。
  return <path d={d} fill={fill} stroke="var(--card)" strokeWidth={1} />;
}

function BreakdownTooltip({ active, payload }: any) {
  if (!active || !payload?.length) return null;
  const d = payload[0].payload as UpdateBreakdown;
  const rest = d.titles.length - TOOLTIP_TITLE_LIMIT;
  return (
    <div className="rounded-lg border border-border bg-popover text-popover-foreground px-3 py-2 text-xs shadow-md min-w-44 max-w-72">
      <div className="flex items-baseline justify-between gap-3 mb-1.5">
        <span className="text-muted-foreground tabular-nums">{d.date}</span>
        <span className="tabular-nums font-medium">{d.total.toLocaleString()} 件</span>
      </div>
      {d.total === 0 ? (
        <div className="text-muted-foreground">更新なし</div>
      ) : (
        <>
          <div className="space-y-0.5">
            {[...LAW_KINDS].reverse().filter(k => d.kinds[k] > 0).map(k => (
              <div key={k} className="flex items-center gap-2">
                <span className="size-2 rounded-sm shrink-0" style={{ background: LAW_KIND_COLORS[k] }} />
                <span>{k}</span>
                <span className="ml-auto tabular-nums">{d.kinds[k]}</span>
              </div>
            ))}
          </div>
          <div className="mt-1.5 pt-1.5 border-t border-border flex gap-3 text-muted-foreground tabular-nums">
            <span>改正 {d.changes.modified}</span>
            <span>追加 {d.changes.added}</span>
            {d.changes.removed > 0 && <span>廃止 {d.changes.removed}</span>}
          </div>
          <ul className="mt-1.5 pt-1.5 border-t border-border space-y-0.5">
            {d.titles.slice(0, TOOLTIP_TITLE_LIMIT).map((t, i) => (
              <li key={i} className="truncate">{t}</li>
            ))}
            {rest > 0 && <li className="text-muted-foreground">他 {rest.toLocaleString()} 件</li>}
          </ul>
        </>
      )}
    </div>
  );
}

/** 直近の法令更新件数を、法令種別ごとに積み上げて表示する。 */
export function UpdateBreakdownCard({ data, title }: { data: UpdateBreakdown[]; title?: string }) {
  const totals = Object.fromEntries(
    LAW_KINDS.map(k => [k, data.reduce((acc, d) => acc + d.kinds[k], 0)]),
  ) as Record<LawKind, number>;
  return (
    <Card data-testid="update-breakdown">
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle>{title ?? "更新トレンド"}</CardTitle>
        <Activity className="size-4 text-muted-foreground" />
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap gap-x-4 gap-y-1 mb-3 text-xs" data-testid="update-breakdown-legend">
          {LAW_KINDS.map(k => (
            <div key={k} className="flex items-center gap-1.5">
              <span className="size-2.5 rounded-sm" style={{ background: LAW_KIND_COLORS[k] }} />
              <span className="text-muted-foreground">{k}</span>
              <span className="tabular-nums">{totals[k].toLocaleString()}</span>
            </div>
          ))}
        </div>
        <div className="h-64">
          <ResponsiveContainer>
            <BarChart data={data} barCategoryGap="20%">
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
              <XAxis dataKey="date" stroke="var(--muted-foreground)" fontSize={11} />
              <YAxis stroke="var(--muted-foreground)" fontSize={11} allowDecimals={false} />
              <Tooltip content={<BreakdownTooltip />} cursor={{ fill: "var(--accent)", opacity: 0.6 }} />
              {LAW_KINDS.map(k => (
                <Bar
                  key={k}
                  name={k}
                  dataKey={(d: UpdateBreakdown) => d.kinds[k]}
                  stackId="kind"
                  fill={LAW_KIND_COLORS[k]}
                  isAnimationActive={false}
                  shape={(p: any) => <StackSegment {...p} kind={k} />}
                />
              ))}
            </BarChart>
          </ResponsiveContainer>
        </div>
      </CardContent>
    </Card>
  );
}

export function CategoryCard({ data }: { data: { name: string; value: number }[] }) {
  return (
    <Card>
      <CardHeader><CardTitle>カテゴリ分布</CardTitle></CardHeader>
      <CardContent>
        <div className="h-64">
          <ResponsiveContainer>
            <PieChart>
              <Pie data={data} dataKey="value" nameKey="name" innerRadius={50} outerRadius={85} paddingAngle={2} stroke="var(--background)" strokeWidth={2}>
                {data.map((_, i) => <Cell key={i} fill={COLORS[i % COLORS.length]} />)}
              </Pie>
              <Tooltip contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }} />
            </PieChart>
          </ResponsiveContainer>
        </div>
        <div className="grid grid-cols-2 gap-1.5 mt-2">
          {data.map((c, i) => (
            <div key={c.name} className="flex items-center gap-2 text-xs">
              <span className="size-2 rounded-sm" style={{ background: COLORS[i % COLORS.length] }} />
              <span className="text-muted-foreground">{c.name}</span>
              <span className="ml-auto tabular-nums">{c.value}</span>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
