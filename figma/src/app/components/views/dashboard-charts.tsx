import { Area, AreaChart, Bar, BarChart, CartesianGrid, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Activity } from "lucide-react";

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

export function UpdateTrendCard({ data }: { data: { month: string; count: number }[] }) {
  return (
    <Card className="col-span-2">
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle>更新トレンド</CardTitle>
        <Activity className="size-4 text-muted-foreground" />
      </CardHeader>
      <CardContent>
        <div className="h-64">
          <ResponsiveContainer>
            <BarChart data={data}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
              <XAxis dataKey="month" stroke="var(--muted-foreground)" fontSize={11} />
              <YAxis stroke="var(--muted-foreground)" fontSize={11} />
              <Tooltip contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }} />
              <Bar dataKey="count" fill="var(--primary)" radius={[6, 6, 0, 0]} />
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
