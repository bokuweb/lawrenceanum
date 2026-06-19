import { useEffect, useMemo, useRef, useState } from "react";
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

// 文字単位 LCS は O(m·n) で、長文段落 (e.g. 4900 字) どうしだと ~24M セルの
// DP を確保し数秒メインスレッドを止めてしまう。積 m·n がこの閾値を超えたら
// 細かい文字 diff を諦め、共通の接頭/接尾だけ残す粗い diff にフォールバックする。
const LCS_CELL_LIMIT = 250_000;

/** 共通接頭・接尾だけ一致扱いにし、中間をまるごと del/add にする O(n) フォールバック。 */
function coarseDiff(a: string, b: string): { left: DiffSeg[]; right: DiffSeg[] } {
  const min = Math.min(a.length, b.length);
  let p = 0;
  while (p < min && a[p] === b[p]) p++;
  let s = 0;
  while (s < min - p && a[a.length - 1 - s] === b[b.length - 1 - s]) s++;
  const left: DiffSeg[] = [], right: DiffSeg[] = [];
  if (p) { left.push({ type: "eq", text: a.slice(0, p) }); right.push({ type: "eq", text: b.slice(0, p) }); }
  const aMid = a.slice(p, a.length - s), bMid = b.slice(p, b.length - s);
  if (aMid) left.push({ type: "del", text: aMid });
  if (bMid) right.push({ type: "add", text: bMid });
  if (s) { left.push({ type: "eq", text: a.slice(a.length - s) }); right.push({ type: "eq", text: b.slice(b.length - s) }); }
  return { left, right };
}

function diffWords(a: string, b: string): { left: DiffSeg[]; right: DiffSeg[] } {
  if (a === b) return { left: [{ type: "eq", text: a }], right: [{ type: "eq", text: b }] };
  const m = a.length, n = b.length;
  // 長文どうしは LCS の DP が爆発するので粗い diff に切り替える。
  if (m * n > LCS_CELL_LIMIT) return coarseDiff(a, b);
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

/** 条の本文を 1 本の文字列に畳んで同一判定に使う (JSON.stringify より軽い)。 */
function articleSig(a: Article): string {
  let s = `${a.article_no}|${a.caption ?? ""}`;
  for (const p of a.paragraphs) s += `${p.paragraph_no ?? ""}${p.text}`;
  return s;
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
  const [versionsLoading, setVersionsLoading] = useState(true);
  const [versionA, setVersionA] = useState<string>("");
  const [versionB, setVersionB] = useState<string>("");

  // revision_id -> 本文。版は選択された 2 つだけを on-demand で取得しキャッシュする。
  // (旧実装は全版を 1 つの history.ndjson.zst に束ね、ブラウザで zstd 展開していたが、
  //  大法令だと展開後 ~200MB・fzstd が大窓 LDM を誤展開し、12 秒固まった上に本文が
  //  壊れて比較が成立しなかった。比較に要るのは常に 2 版なので個別取得に切り替える。)
  const [bodies, setBodies] = useState<Map<string, LawDocumentRaw>>(new Map());
  // 取得に失敗した revision_id (404 等)。再取得ループを避けるため覚えておく。
  const failedRef = useRef<Set<string>>(new Set());
  // 既定選択の「本文が変わる版まで A を遡る」自動処理の状態。ユーザが手で選んだら
  // 止め、暴走 (毎ステップ 1 fetch) を避けるため遡り段数にも上限を設ける。
  const autoWalkRef = useRef(false);
  const walkStepsRef = useRef(0);

  // versions.json をロード — 版一覧 (ラベル + body_available + 既定選択) の唯一の源。
  useEffect(() => {
    if (!lawId) return;
    let cancelled = false;
    setVersions(null);
    setVersionsLoading(true);
    setVersionA("");
    setVersionB("");
    setBodies(new Map());
    failedRef.current = new Set();
    autoWalkRef.current = false;
    walkStepsRef.current = 0;
    api.versions(lawId)
      .then(v => { if (!cancelled) setVersions(v); })
      .catch(() => { if (!cancelled) setVersions(null); })
      .finally(() => { if (!cancelled) setVersionsLoading(false); });
    return () => { cancelled = true; };
  }, [lawId]);

  // 比較できる版 = versions.json で本文ファイルへの path を持つ版。
  //   - 判定は path の有無のみ。body_available は CI の部分 cache から再生成され
  //     当てにならず (path はあるのに false など) 、これで弾くと全滅し得る。本文の
  //     実在は実際の fetch (404 は failedRef で吸収) で確かめる。
  //   - 未施行版は本番では path=null なので自然に除外される。
  // 日付付きがあれば日付付きだけを並べてセレクタを綺麗に保つ (hash 形式の作業
  // スナップショットは重複なので除外)。
  const availVersions = useMemo<{ revision_id: string; date: string }[]>(() => {
    const all = (versions?.versions ?? [])
      .filter(v => !!v.path)
      .map(v => ({ revision_id: v.revision_id, date: revDate(v.revision_id) }));
    const dated = all.filter(v => v.date);
    return (dated.length ? dated : all).sort((a, b) => a.date.localeCompare(b.date));
  }, [versions]);

  // 既定の比較版を確定する。
  //   - After (B): 今日以前で最新の版 (未来施行版まで取ると未来 vs 未来になり無意味)。
  //   - Before (A): その 1 つ前。本文が同一なら下の「distinct 化」effect が更に遡る。
  useEffect(() => {
    if (versionsLoading || !availVersions.length) return;
    const ids = new Set(availVersions.map(v => v.revision_id));
    if (versionA && versionB && ids.has(versionA) && ids.has(versionB)) return;
    const today = new Date().toISOString().slice(0, 10).replace(/-/g, "");
    let bIdx = availVersions.length - 1;
    for (let i = availVersions.length - 1; i >= 0; i--) {
      if (availVersions[i].date && availVersions[i].date <= today) { bIdx = i; break; }
    }
    const aIdx = Math.max(0, bIdx - 1);
    setVersionA(availVersions[aIdx].revision_id);
    setVersionB(availVersions[bIdx].revision_id);
  }, [availVersions, versionA, versionB, versionsLoading]);

  // 選択中の 2 版の本文を on-demand 取得 (キャッシュ済みはスキップ)。
  useEffect(() => {
    let cancelled = false;
    const want = [versionA, versionB].filter(Boolean).filter(
      id => !bodies.has(id) && !failedRef.current.has(id)
    );
    if (!want.length) return;
    Promise.all(want.map(id =>
      api.revision(lawId, id)
        .then(doc => ({ id, doc }))
        .catch(() => { failedRef.current.add(id); return { id, doc: null as LawDocumentRaw | null }; })
    )).then(results => {
      if (cancelled) return;
      const got = results.filter(r => r.doc);
      if (!got.length) return;
      setBodies(prev => {
        const next = new Map(prev);
        for (const r of got) next.set(r.id, r.doc!);
        return next;
      });
    });
    return () => { cancelled = true; };
  }, [lawId, versionA, versionB, bodies]);

  const docA = versionA ? bodies.get(versionA) ?? null : null;
  const docB = versionB ? bodies.get(versionB) ?? null : null;

  // 既定で選んだ A=B (本文が同一) のとき、本文が実際に変わる版まで A を更に遡らせる。
  // e-Gov は隣接版の本文が同一なことが多く、単純な「1 つ前」だと差分 0 で空に見える。
  // 各ステップで 1 fetch するので段数に上限を設け、見つからなければそのまま諦める。
  const MAX_WALK_STEPS = 15;
  useEffect(() => {
    if (versionsLoading || !docA || !docB) return;
    if (walkStepsRef.current >= MAX_WALK_STEPS) return;
    if (autoWalkRef.current) return;                 // ユーザが手で選んだら止める
    const aIdx = availVersions.findIndex(v => v.revision_id === versionA);
    const bIdx = availVersions.findIndex(v => v.revision_id === versionB);
    if (aIdx <= 0 || bIdx < 0) return;
    const sigA = docA.articles.map(articleSig).join("");
    const sigB = docB.articles.map(articleSig).join("");
    if (sigA === sigB) {
      walkStepsRef.current += 1;
      setVersionA(availVersions[aIdx - 1].revision_id);
    }
  }, [docA, docB, availVersions, versionA, versionB, versionsLoading]);

  // ユーザがセレクタを触ったら自動遡りを止める。
  const onPickA = (id: string) => { autoWalkRef.current = true; setVersionA(id); };
  const onPickB = (id: string) => { autoWalkRef.current = true; setVersionB(id); };

  // 3 状態: skeleton (ロード中 / 本文取得待ち) → 実比較 / mock (本当にデータが無い)。
  const willBeLive = availVersions.length >= 2;            // 実データで比較できる見込み
  const bothFetched = !!docA && !!docB;
  const bothFailed = (versionA ? failedRef.current.has(versionA) : false)
    && (versionB ? failedRef.current.has(versionB) : false);
  const liveAvailable = bothFetched && willBeLive;        // 実データが揃った
  // ロード中、または「実データは来るが本文取得がまだ終わっていない一瞬」は skeleton。
  // 取得が両方とも失敗したときは無限 skeleton を避け mock に落とす。
  const showSkeleton = !bothFailed && (versionsLoading || (willBeLive && !liveAvailable));
  const showMock = !showSkeleton && !liveAvailable;       // 本当にデータが無い時だけ mock
  const articlesA: Article[] = liveAvailable ? docA!.articles : (ARTICLES_V1 as unknown as Article[]);
  const articlesB: Article[] = liveAvailable ? docB!.articles : (ARTICLES_V2 as unknown as Article[]);
  const law = laws.find(l => l.law_id === lawId) ?? laws[0] ?? { title: "民法", law_id: "?" } as any;

  // 条の照合は Map で O(1) に (find の O(n²) を避ける)。状態 (追加/削除/変更/同一) も
  // ここで 1 度だけ確定し、stats と本文レンダの双方で使い回す。
  const { orderedIds, mapA, mapB, statusOf, stats } = useMemo(() => {
    const mapA = new Map(articlesA.map(a => [a.article_id, a]));
    const mapB = new Map(articlesB.map(a => [a.article_id, a]));
    const orderedIds: string[] = [];
    const seen = new Set<string>();
    for (const a of articlesA) { orderedIds.push(a.article_id); seen.add(a.article_id); }
    for (const b of articlesB) if (!seen.has(b.article_id)) { orderedIds.push(b.article_id); seen.add(b.article_id); }
    const statusOf = new Map<string, "added" | "removed" | "modified" | "same">();
    let added = 0, removed = 0, modified = 0;
    for (const id of orderedIds) {
      const a = mapA.get(id);
      const b = mapB.get(id);
      let st: "added" | "removed" | "modified" | "same";
      if (!a) { st = "added"; added++; }
      else if (!b) { st = "removed"; removed++; }
      else if (articleSig(a) !== articleSig(b)) { st = "modified"; modified++; }
      else st = "same";
      statusOf.set(id, st);
    }
    return { orderedIds, mapA, mapB, statusOf, stats: { added, removed, modified } };
  }, [articlesA, articlesB]);

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
            {versionsLoading ? <Skeleton className="h-9 w-full" /> : (
            <Select value={versionA} onValueChange={onPickA}>
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
            {versionsLoading ? <Skeleton className="h-9 w-full" /> : (
            <Select value={versionB} onValueChange={onPickB}>
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
        <span className="ml-auto text-xs text-muted-foreground self-center">{law.title} · {orderedIds.length} 条を比較</span>
      </div>

      {stats.added + stats.modified + stats.removed === 0 && (
        <Card><CardContent className="p-6 text-sm text-muted-foreground text-center">
          選択した 2 版の条文に差分はありません。上のセレクタで別の版を選んでください。
        </CardContent></Card>
      )}

      <div className="space-y-3">
        {orderedIds.map(id => {
          const status = statusOf.get(id);
          if (status === "same") return null;
          const a = mapA.get(id);
          const b = mapB.get(id);
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
