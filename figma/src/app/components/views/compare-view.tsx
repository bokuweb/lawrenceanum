import { useEffect, useMemo, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Badge } from "../ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ARTICLES_V1, ARTICLES_V2 } from "../mock-data";
import { ArrowRight, GitCompare } from "lucide-react";
import { useLaws } from "../../data/use-laws";
import { api, type Article, type LawDocumentRaw, type VersionsJson } from "../../data/api";

type DiffSeg = { type: "eq" | "add" | "del"; text: string };

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
  const [error, setError] = useState<string | null>(null);

  // versions.json をロード (版の日付などメタ情報用)。lawId が変わったら選択をリセット。
  useEffect(() => {
    if (!lawId) return;
    let cancelled = false;
    setError(null);
    setVersionA("");
    setVersionB("");
    api.versions(lawId).then(v => {
      if (cancelled) return;
      setVersions(v);
    }).catch(e => { if (!cancelled) { setVersions(null); setError(String(e)); } });
    return () => { cancelled = true; };
  }, [lawId]);

  // 履歴束 (history.ndjson.zst) を 1 回ロード — 全版を含むので、版選択は束から引く
  // だけで済み、per-revision の追加 fetch が不要 (任意 2 版 diff をクライアント側で)。
  useEffect(() => {
    if (!lawId) return;
    let cancelled = false;
    api.history(lawId)
      .then(docs => { if (!cancelled) setRevDocs(new Map(docs.map(d => [d.revision_id ?? "", d]))); })
      .catch(() => { if (!cancelled) setRevDocs(new Map()); });
    return () => { cancelled = true; };
  }, [lawId]);

  // 比較できる版 = 束 (revDocs) に本文がある版。versions.json の body_available は
  // CI の部分 cache から再生成されるため当てにならない (本文は束にある)。
  // 束が未ロード/失敗のときだけ body_available を暫定フォールバックにする。
  const availVersions = useMemo(() => {
    const list = versions?.versions ?? [];
    if (revDocs.size > 0) {
      const inBundle = list.filter(v => revDocs.has(v.revision_id));
      if (inBundle.length) return inBundle;
    }
    return list.filter(v => v.body_available !== false);
  }, [versions, revDocs]);

  // 既定の比較版は束に本文がある最新 2 版。束ロード後 (availVersions 確定後) に確定する。
  useEffect(() => {
    if (!availVersions.length) return;
    const ids = new Set(availVersions.map(v => v.revision_id));
    if (versionA && versionB && ids.has(versionA) && ids.has(versionB)) return;
    if (availVersions.length >= 2) {
      setVersionA(availVersions[availVersions.length - 2].revision_id);
      setVersionB(availVersions[availVersions.length - 1].revision_id);
    } else {
      setVersionA(availVersions[0].revision_id);
      setVersionB(availVersions[0].revision_id);
    }
  }, [availVersions, versionA, versionB]);

  const docA = versionA ? revDocs.get(versionA) ?? null : null;
  const docB = versionB ? revDocs.get(versionB) ?? null : null;

  // フォールバック: live が無い・足りない場合はモック ARTICLES_V1/V2。
  const liveAvailable = !!docA && !!docB && availVersions.length >= 2;
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
    if (!v) return revId;
    const dates: string[] = [];
    if (v.promulgation_date) dates.push(`公布 ${v.promulgation_date}`);
    if (v.effective_date) dates.push(`施行 ${v.effective_date}`);
    if (v.source_update_date) dates.push(`取込 ${v.source_update_date}`);
    return dates.length ? `${revId.slice(0, 8)} · ${dates.join(" / ")}` : revId;
  };

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-2xl flex items-center gap-2"><GitCompare className="size-6" />バージョン比較</h1>
        <p className="text-sm text-muted-foreground mt-1">
          同一法令の異なる版を並べて差分を確認
          {!liveAvailable && versions && versions.versions.length < 2 && (
            <span className="ml-2 text-amber-600">この法令の蓄積版が 1 つしかないため、モックでデモ表示しています</span>
          )}
          {error && <span className="ml-2 text-amber-600">({error})</span>}
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
          </div>
          <ArrowRight className="size-4 text-muted-foreground mb-3" />
          <div className="flex-1 space-y-1.5">
            <label className="text-xs text-muted-foreground">比較版 (After)</label>
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
          </div>
        </CardContent>
      </Card>

      <div className="flex gap-2">
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-emerald-500" />追加 {stats.added}</Badge>
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-amber-500" />変更 {stats.modified}</Badge>
        <Badge variant="outline" className="gap-1.5"><span className="size-2 rounded-sm bg-rose-500" />削除 {stats.removed}</Badge>
        <span className="ml-auto text-xs text-muted-foreground self-center">{law.title} · {articleIds.length} 条を比較</span>
      </div>

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
    </div>
  );
}
