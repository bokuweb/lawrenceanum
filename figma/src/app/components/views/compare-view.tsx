import { useEffect, useMemo, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Badge } from "../ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { Skeleton } from "../ui/skeleton";
import { ARTICLES_V1, ARTICLES_V2 } from "../mock-data";
import { ArrowRight, GitCompare } from "lucide-react";
import { useLaws } from "../../data/use-laws";
import { api, type Article, type LawDocumentRaw, type VersionsJson } from "../../data/api";

type DiffSeg = { type: "eq" | "add" | "del"; text: string };

/** revision_id (`{lawId}_{YYYYMMDD}_{改正法}`) から施行日 YYYYMMDD を抜く。hash 形式は ""。 */
function revDate(revId: string): string {
  const m = /_(\d{8})(?:_|$)/.exec(revId);
  return m ? m[1] : "";
}

function fmtDate(d: string): string {
  return d.length === 8 ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}` : d;
}

function diffWords(a: string, b: string): { left: DiffSeg[]; right: DiffSeg[] } {
  if (a === b) return { left: [{ type: "eq", text: a }], right: [{ type: "eq", text: b }] };
  const m = a.length, n = b.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) for (let j = n - 1; j >= 0; j--) {
    dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  }
  const left: DiffSeg[] = [], right: DiffSeg[] = [];
  let i = 0, j = 0;
  const push = (arr: DiffSeg[], type: DiffSeg["type"], ch: string) => {
    const last = arr[arr.length - 1];
    if (last && last.type === type) last.text += ch; else arr.push({ type, text: ch });
  };
  while (i < m && j < n) {
    if (a[i] === b[j]) { push(left, "eq", a[i]); push(right, "eq", b[j]); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { push(left, "del", a[i]); i++; }
    else { push(right, "add", b[j]); j++; }
  }
  while (i < m) push(left, "del", a[i++]);
  while (j < n) push(right, "add", b[j++]);
  return { left, right };
}

function renderSegs(segs: DiffSeg[]) {
  return segs.map((s, i) => (
    <span key={i} className={
      s.type === "add" ? "bg-emerald-500/20 text-emerald-700 dark:text-emerald-300 rounded px-0.5" :
      s.type === "del" ? "bg-rose-500/20 text-rose-700 dark:text-rose-300 line-through rounded px-0.5" :
      ""
    }>{s.text}</span>
  ));
}

export function CompareView({ initialLawId }: { initialLawId: string | null }) {
  const { laws } = useLaws();
  const fallbackLaw = laws[0];
  const [lawId, setLawId] = useState(initialLawId ?? fallbackLaw?.law_id ?? "");
  useEffect(() => {
    // 法令リストが後から live で埋まった場合の補正。
    if (!lawId && fallbackLaw) setLawId(fallbackLaw.law_id);
  }, [fallbackLaw?.law_id]);

  const [versions, setVersions] = useState<VersionsJson | null>(null);
  const [versionA, setVersionA] = useState<string>("");
  const [versionB, setVersionB] = useState<string>("");
  // revision_id -> 本文。履歴束 (history.ndjson.zst) を 1 回展開して全版を保持する。
  // (注: 変数名 `history` は window.history と衝突するので使わない)
  const [revDocs, setRevDocs] = useState<Map<string, LawDocumentRaw>>(new Map());
  // 履歴束のロード中フラグ。ロード完了まではモックではなく skeleton を出す。
  const [loadingHistory, setLoadingHistory] = useState(true);

  // versions.json をロード (版の日付などラベル用)。lawId が変わったら選択をリセット。
  // ※ deploy によっては versions.json が無い (404) ことがある。本文は履歴束にあるので
  //    ここが失敗しても比較自体は成立する — 失敗はラベルが素っ気なくなるだけで致命的でない。
  useEffect(() => {
    if (!lawId) return;
    let cancelled = false;
    setVersions(null);
    setVersionA("");
    setVersionB("");
    api.versions(lawId).then(v => {
      if (cancelled) return;
      setVersions(v);
    }).catch(() => { if (!cancelled) setVersions(null); });
    return () => { cancelled = true; };
  }, [lawId]);

  // 履歴束 (history.ndjson.zst) を 1 回ロード — 全版を含むので、版選択は束から引く
  // だけで済み、per-revision の追加 fetch が不要 (任意 2 版 diff をクライアント側で)。
  useEffect(() => {
    if (!lawId) return;
    let cancelled = false;
    setLoadingHistory(true);
    api.history(lawId)
      .then(docs => { if (!cancelled) setRevDocs(new Map(docs.map(d => [d.revision_id ?? "", d]))); })
      .catch(() => { if (!cancelled) setRevDocs(new Map()); })
      .finally(() => { if (!cancelled) setLoadingHistory(false); });
    return () => { cancelled = true; };
  }, [lawId]);

  // 比較できる版 = 履歴束 (revDocs) に本文がある版。束こそが本文の真の在処で、
  // versions.json は (a) deploy によっては 404 で欠ける (b) body_available が CI の
  // 部分 cache から再生成され当てにならない — ので、束があるならそれを起点にする。
  // 束が無い (取得失敗) ときだけ versions.json の body_available を暫定フォールバックに。
  const availVersions = useMemo<{ revision_id: string; date: string }[]>(() => {
    if (revDocs.size > 0) {
      const all = Array.from(revDocs.keys())
        .filter(Boolean)
        .map(id => ({ revision_id: id, date: revDate(id) }));
      // 日付付き revision があれば、それだけを版として並べる (hash 形式の作業
      // スナップショットは日付付きの重複なので除外し、セレクタを綺麗に保つ)。
      const dated = all.filter(v => v.date);
      return (dated.length ? dated : all).sort((a, b) => a.date.localeCompare(b.date));
    }
    return (versions?.versions ?? [])
      .filter(v => v.body_available !== false)
      .map(v => ({ revision_id: v.revision_id, date: revDate(v.revision_id) }));
  }, [versions, revDocs]);

  // 既定の比較版 = 「現在施行中の最新版」と、それと本文が実際に異なる直近の版。
  //   - After: 今日以前で最新の版 (未来施行版まで取ると未来 vs 未来になり無意味)。
  //   - Before: After から遡って最初に本文が変わる版。e-Gov の隣接版は本文が同一の
  //     ことが多く、単純な「1 つ前」だと差分 0 で空に見える (= 今回の不具合の体感)。
  useEffect(() => {
    // 履歴束のロード完了を待ってから確定する。ロード前に versions.json フォールバック
    // (本文ありが 1 版だけ) で A=B を仮確定してしまうと、束が来た後も下のガードが
    // 「A/B は今のリストにも在る」と見て据え置き、同一版どうしの 0 差分で固まる。
    if (loadingHistory || !availVersions.length) return;
    const ids = new Set(availVersions.map(v => v.revision_id));
    if (versionA && versionB && ids.has(versionA) && ids.has(versionB)) return;
    const today = new Date().toISOString().slice(0, 10).replace(/-/g, "");
    let bIdx = availVersions.length - 1;
    for (let i = availVersions.length - 1; i >= 0; i--) {
      if (availVersions[i].date && availVersions[i].date <= today) { bIdx = i; break; }
    }
    const sig = (rid: string) => {
      const d = revDocs.get(rid);
      return d ? JSON.stringify(d.articles) : rid;
    };
    const bSig = sig(availVersions[bIdx].revision_id);
    let aIdx = Math.max(0, bIdx - 1);
    for (let i = bIdx - 1; i >= 0; i--) {
      if (sig(availVersions[i].revision_id) !== bSig) { aIdx = i; break; }
    }
    setVersionA(availVersions[aIdx].revision_id);
    setVersionB(availVersions[bIdx].revision_id);
  }, [availVersions, versionA, versionB, revDocs, loadingHistory]);

  const docA = versionA ? revDocs.get(versionA) ?? null : null;
  const docB = versionB ? revDocs.get(versionB) ?? null : null;

  // 3 状態: skeleton (ロード中 / 実データ確定待ち) → 実比較 / mock (本当にデータが無い)。
  const willBeLive = availVersions.length >= 2;            // 実データで比較できる見込み
  const liveAvailable = !!docA && !!docB && willBeLive;    // 実データが揃った
  // ロード中、または「実データは来るが版選択がまだ確定していない一瞬」は skeleton。
  // こうしないと、その一瞬だけモックがチラ見えしてしまう。
  const showSkeleton = loadingHistory || (willBeLive && !liveAvailable);
  const showMock = !showSkeleton && !liveAvailable;        // 本当にデータが無い時だけ mock
  const articlesA: Article[] = liveAvailable ? docA!.articles : (ARTICLES_V1 as unknown as Article[]);
  const articlesB: Article[] = liveAvailable ? docB!.articles : (ARTICLES_V2 as unknown as Article[]);
  const law = laws.find(l => l.law_id === lawId) ?? laws[0] ?? { title: "民法", law_id: "?" } as any;

  const articleIds = useMemo(() => {
    const ids = new Set([...articlesA.map(a => a.article_id), ...articlesB.map(a => a.article_id)]);
    return Array.from(ids);
  }, [articlesA, articlesB]);

  const stats = useMemo(() => {
    let added = 0, removed = 0, modified = 0;
    for (const id of articleIds) {
      const a = articlesA.find(x => x.article_id === id);
      const b = articlesB.find(x => x.article_id === id);
      if (!a) added++;
      else if (!b) removed++;
      else if (JSON.stringify(a) !== JSON.stringify(b)) modified++;
    }
    return { added, removed, modified };
  }, [articleIds, articlesA, articlesB]);

  const versionLabel = (revId: string) => {
    const v = versions?.versions.find(x => x.revision_id === revId);
    if (v) {
      const dates: string[] = [];
      if (v.promulgation_date) dates.push(`公布 ${v.promulgation_date}`);
      if (v.effective_date) dates.push(`施行 ${v.effective_date}`);
      if (v.source_update_date) dates.push(`取込 ${v.source_update_date}`);
      if (dates.length) return `${revId.slice(0, 8)} · ${dates.join(" / ")}`;
    }
    // versions.json が無い / メタ欠落のときは revision_id から日付を起こす。
    const d = revDate(revId);
    const amend = d ? revId.split("_").pop() : "";
    return d ? `施行 ${fmtDate(d)}${amend ? ` · ${amend}` : ""}` : revId.slice(0, 12);
  };

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-2xl flex items-center gap-2"><GitCompare className="size-6" />バージョン比較</h1>
        <p className="text-sm text-muted-foreground mt-1">
          同一法令の異なる版を並べて差分を確認
          {showMock && (
            <span className="ml-2 text-amber-600">この法令の蓄積版が 1 つしかないため、モックでデモ表示しています</span>
          )}
        </p>
      </div>

      <Card>
        <CardContent className="p-4 flex items-end gap-3">
          <div className="flex-1 space-y-1.5">
            <label className="text-xs text-muted-foreground">法令</label>
            <Select value={lawId} onValueChange={setLawId}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {laws.map(l => <SelectItem key={l.law_id} value={l.law_id}>{l.title}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="flex-1 space-y-1.5">
            <label className="text-xs text-muted-foreground">基準版 (Before)</label>
            {loadingHistory ? <Skeleton className="h-9 w-full" /> : (
            <Select value={versionA} onValueChange={setVersionA}>
              <SelectTrigger><SelectValue placeholder="選択" /></SelectTrigger>
              <SelectContent>
                {availVersions.length ? availVersions.map(v => (
                  <SelectItem key={v.revision_id} value={v.revision_id}>{versionLabel(v.revision_id)}</SelectItem>
                )) : (
                  <>
                    <SelectItem value="v1-mock">v1 · モック</SelectItem>
                    <SelectItem value="v2-mock">v2 · モック</SelectItem>
                  </>
                )}
              </SelectContent>
            </Select>
            )}
          </div>
          <ArrowRight className="size-4 text-muted-foreground mb-3" />
          <div className="flex-1 space-y-1.5">
            <label className="text-xs text-muted-foreground">比較版 (After)</label>
            {loadingHistory ? <Skeleton className="h-9 w-full" /> : (
            <Select value={versionB} onValueChange={setVersionB}>
              <SelectTrigger><SelectValue placeholder="選択" /></SelectTrigger>
              <SelectContent>
                {availVersions.length ? availVersions.map(v => (
                  <SelectItem key={v.revision_id} value={v.revision_id}>{versionLabel(v.revision_id)}</SelectItem>
                )) : (
                  <>
                    <SelectItem value="v1-mock">v1 · モック</SelectItem>
                    <SelectItem value="v2-mock">v2 · モック</SelectItem>
                  </>
                )}
              </SelectContent>
            </Select>
            )}
          </div>
        </CardContent>
      </Card>

      {showSkeleton ? (
        <div className="space-y-3">
          <div className="flex gap-2">
            <Skeleton className="h-6 w-20" />
            <Skeleton className="h-6 w-20" />
            <Skeleton className="h-6 w-20" />
            <Skeleton className="ml-auto h-6 w-40" />
          </div>
          {Array.from({ length: 4 }).map((_, i) => (
            <Card key={i} className="overflow-hidden">
              <div className="px-4 py-2 border-b border-border bg-muted/40">
                <Skeleton className="h-4 w-48" />
              </div>
              <div className="grid grid-cols-2 divide-x divide-border">
                {[0, 1].map(side => (
                  <div key={side} className="p-4 space-y-2">
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-4 w-11/12" />
                    <Skeleton className="h-4 w-4/5" />
                  </div>
                ))}
              </div>
            </Card>
          ))}
        </div>
      ) : (
      <>
      <div className="flex gap-2">
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-emerald-500" />追加 {stats.added}</Badge>
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-amber-500" />変更 {stats.modified}</Badge>
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-rose-500" />削除 {stats.removed}</Badge>
        <span className="ml-auto text-xs text-muted-foreground self-center">{law.title} · {articleIds.length} 条を比較</span>
      </div>

      {stats.added + stats.modified + stats.removed === 0 && (
        <Card><CardContent className="p-6 text-sm text-muted-foreground text-center">
          選択した 2 版の条文に差分はありません。上のセレクタで別の版を選んでください。
        </CardContent></Card>
      )}

      <div className="space-y-3">
        {articleIds.map(id => {
          const a = articlesA.find(x => x.article_id === id);
          const b = articlesB.find(x => x.article_id === id);
          const status = !a ? "added" : !b ? "removed" : JSON.stringify(a) === JSON.stringify(b) ? "same" : "modified";
          if (status === "same") return null;
          const head = (a ?? b)!;
          return (
            <Card key={id} className="overflow-hidden">
              <div className="px-4 py-2 border-b border-border flex items-center justify-between bg-muted/40">
                <div className="text-sm">{head.article_no} {head.caption && `（${head.caption}）`}</div>
                <Badge variant={status === "added" ? "default" : status === "removed" ? "destructive" : "secondary"} className="text-xs">
                  {status === "added" ? "追加" : status === "removed" ? "削除" : "変更"}
                </Badge>
              </div>
              <div className="grid grid-cols-2 divide-x divide-border">
                <div className="p-4 text-sm leading-relaxed space-y-2">
                  {a ? a.paragraphs.map((p, i) => {
                    const bp = b?.paragraphs[i];
                    const segs = bp ? diffWords(p.text, bp.text).left : [{ type: "del" as const, text: p.text }];
                    return (
                      <p key={i} className="flex gap-3">
                        <span className="text-muted-foreground tabular-nums shrink-0 w-6">{p.paragraph_no ?? ""}</span>
                        <span>{renderSegs(segs)}</span>
                      </p>
                    );
                  }) : <div className="text-xs text-muted-foreground italic">（この版には存在しない）</div>}
                </div>
                <div className="p-4 text-sm leading-relaxed space-y-2">
                  {b ? b.paragraphs.map((p, i) => {
                    const ap = a?.paragraphs[i];
                    const segs = ap ? diffWords(ap.text, p.text).right : [{ type: "add" as const, text: p.text }];
                    return (
                      <p key={i} className="flex gap-3">
                        <span className="text-muted-foreground tabular-nums shrink-0 w-6">{p.paragraph_no ?? ""}</span>
                        <span>{renderSegs(segs)}</span>
                      </p>
                    );
                  }) : <div className="text-xs text-muted-foreground italic">（この版には存在しない）</div>}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      </>
      )}
    </div>
  );
}
