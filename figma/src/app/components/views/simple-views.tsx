import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Badge } from "../ui/badge";
import { RECENT_UPDATES } from "../mock-data";
import { Newspaper, ExternalLink, FileCheck2 } from "lucide-react";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { Label } from "../ui/label";
import { useTheme } from "../theme-provider";
import { api, type UpdatesByDate } from "../../data/api";

type UpdatesIndexEntry = { date: string; count: number; laws: string[] };

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
            laws: r.v.updated_laws.map(l => l.title),
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
            laws: latest.updated_laws.map(l => l.title),
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
  const { entries, loading, live } = useUpdatesIndex();
  const display = live
    ? entries
    : RECENT_UPDATES.map(u => ({ date: u.date, count: u.count, laws: u.laws }));

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-2xl">更新履歴</h1>
          <p className="text-sm text-muted-foreground mt-1">日付別のe-Gov更新サマリ</p>
        </div>
        <div className="text-xs text-muted-foreground">
          {loading ? "読み込み中…" : `${display.length} 日${live ? "" : " (モック)"}`}
        </div>
      </div>
      <div className="space-y-2">
        {display.map(u => (
          <Card key={u.date}>
            <CardContent className="p-4 flex items-center gap-4">
              <div className="text-center w-20 shrink-0">
                <div className="text-2xl tabular-nums">{u.date.slice(8)}</div>
                <div className="text-xs text-muted-foreground">{u.date.slice(0, 7)}</div>
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{u.count} 件の更新</Badge>
                  <FileCheck2 className="size-4 text-emerald-500" />
                </div>
                <div className="text-sm text-muted-foreground mt-1 truncate">{u.laws.join("、")}</div>
              </div>
              <Button variant="ghost" size="sm" asChild>
                <a className="gap-1 inline-flex items-center" href={`./updates/${u.date}.json`} target="_blank" rel="noreferrer">
                  JSON <ExternalLink className="size-3" />
                </a>
              </Button>
            </CardContent>
          </Card>
        ))}
        {!loading && display.length === 0 && (
          <div className="text-sm text-muted-foreground text-center py-12">更新履歴がまだありません</div>
        )}
      </div>
    </div>
  );
}

type KanpoMatchedLaw = {
  law_id: string;
  revision_id?: string;
  amending_law_num?: string;
  confidence: number;
  match_reasons: string[];
};
type KanpoIssueRow = {
  date: string;
  issue_type: string;
  issue_no: string;
  pdf_url: string;
  matched_law_events: KanpoMatchedLaw[];
};

/**
 * 直近 14 日の `kanpo/{date}/index.json` を試行的に取得して並べる。
 */
function useKanpoIndex(): { rows: KanpoIssueRow[]; loading: boolean; live: boolean } {
  const [state, setState] = useState({ rows: [] as KanpoIssueRow[], loading: true, live: false });
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const today = new Date();
      const dates: string[] = [];
      for (let i = 0; i < 14; i++) {
        const d = new Date(today);
        d.setUTCDate(d.getUTCDate() - i);
        dates.push(d.toISOString().slice(0, 10));
      }
      const results = await Promise.all(
        dates.map(d =>
          fetch(new URL(`./kanpo/${d}/index.json`, document.baseURI).toString())
            .then(r => (r.ok ? r.json() : Promise.reject(r.status)))
            .then((j: any) => ({ d, j }))
            .catch(() => null)
        )
      );
      if (cancelled) return;
      const rows: KanpoIssueRow[] = [];
      for (const r of results) {
        if (!r) continue;
        for (const issue of r.j.issues ?? []) {
          rows.push({
            date: r.d,
            issue_type: issue.issue_type ?? "",
            issue_no: issue.issue_no ?? "",
            pdf_url: issue.pdf_url ?? "",
            matched_law_events: issue.matched_law_events ?? [],
          });
        }
      }
      rows.sort((a, b) => b.date.localeCompare(a.date));
      setState({ rows, loading: false, live: rows.length > 0 });
    })();
    return () => { cancelled = true; };
  }, []);
  return state;
}

export function KanpoView() {
  const { rows, loading, live } = useKanpoIndex();
  return (
    <div className="p-6 space-y-6">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-2xl flex items-center gap-2"><Newspaper className="size-6" />官報リンク</h1>
          <p className="text-sm text-muted-foreground mt-1">e-Gov改正イベントと官報PDFの突合結果</p>
        </div>
        <div className="text-xs text-muted-foreground">
          {loading ? "読み込み中…" : `${rows.length} 件`}
        </div>
      </div>

      {!loading && !live && (
        <Card>
          <CardContent className="p-4 text-sm space-y-2">
            <div>📰 官報リンクはまだ未収集です。</div>
            <div className="text-xs text-muted-foreground">
              `lawpub kanpo-fetch --date YYYY-MM-DD` の実装は現状モックのみで、公式官報 PDF の取得 / 突合は
              Phase 3 タスクとして残っています。e-Gov の改正イベントには <code>kanpo: {"{ linked: false }"}</code> が
              入った状態です — タイムラインから該当公布日の官報を自分で開くには、各イベントの公布日を
              <a className="underline ml-1" href="https://kanpou.npb.go.jp/" target="_blank" rel="noreferrer">官報公式サイト</a>
              で検索してください。
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-3">
        {rows.map((k, i) => {
          const top = k.matched_law_events[0];
          const conf = top?.confidence ?? 0;
          return (
            <Card key={`${k.date}-${i}`}>
              <CardContent className="p-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-sm tabular-nums">{k.date}</span>
                    <Badge variant="outline">{k.issue_type} {k.issue_no}</Badge>
                  </div>
                  <Badge variant={conf >= 0.8 ? "default" : "secondary"} className="tabular-nums">
                    confidence {conf.toFixed(2)}
                  </Badge>
                </div>
                <div className="text-sm">{top?.amending_law_num ?? top?.law_id ?? "—"}</div>
                <div className="text-xs text-muted-foreground mt-1">
                  match: {top?.match_reasons?.join(", ") ?? "(なし)"}
                  {k.pdf_url && (
                    <>
                      {" · "}
                      <a className="underline" href={k.pdf_url} target="_blank" rel="noreferrer">PDF</a>
                    </>
                  )}
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>
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
