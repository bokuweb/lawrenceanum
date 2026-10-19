import { useEffect, useMemo, useRef, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Label } from "../ui/label";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { type LawSummary } from "../mock-data";
import { Search, SlidersHorizontal, ChevronRight, FileText, Database, Landmark, LoaderCircle, Newspaper, ExternalLink, BookOpen, ScrollText, Building2, Languages } from "lucide-react";
import { useLaws } from "../../data/use-laws";
import { search as ftsSearch, getMeta as getFtsMeta, getCategories, buildFtsMatch, unbigramSnippet, searchSpeeches, searchKanpo, searchTsutatsu, searchReiki, searchEnglish, synonymExpansions, type SearchHit, type SpeechHit, type KanpoHit, type TsutatsuHit, type ReikiHit, type EnglishHit, searchDocuments, type DocumentHit, type DocumentKind } from "../../data/search-engine";
import { useNavigate } from "react-router";
import { createSearchQueue } from "../../data/search-queue";

const SEARCH_TARGETS = [
  ["laws", "法令・条文"], ["speeches", "国会会議録"], ["kanpo", "官報"],
  ["tsutatsu", "通達"], ["reiki", "自治体例規"], ["pubcomment", "パブコメ"],
  ["gian", "議案"], ["shingikai", "審議会資料"], ["english", "法令英訳"],
] as const;
type SearchTarget = typeof SEARCH_TARGETS[number][0];
const DOCUMENT_LABELS: Record<DocumentKind, string> = {
  pubcomment: "パブコメ", gian: "議案", shingikai: "審議会資料",
};

export function SearchView({ initialQuery = "", onOpen, onQueryChange }: { initialQuery?: string; onOpen: (l: LawSummary) => void; onQueryChange?: (q: string) => void }) {
  const navigate = useNavigate();
  const [q, setQ] = useState(initialQuery);
  useEffect(() => { setQ(initialQuery); }, [initialQuery]);

  const [targets, setTargets] = useState<Set<SearchTarget>>(() => new Set(SEARCH_TARGETS.map(([key]) => key)));
  const [activeTarget, setActiveTarget] = useState<SearchTarget>("laws");
  const selectedTargets = SEARCH_TARGETS.filter(([key]) => targets.has(key));
  const visibleTarget = targets.has(activeTarget) ? activeTarget : selectedTargets[0]?.[0];
  useEffect(() => {
    if (visibleTarget) setActiveTarget(visibleTarget);
  }, [visibleTarget]);
  const visibleTargetRef = useRef(visibleTarget);
  visibleTargetRef.current = visibleTarget;
  const queueRef = useRef<ReturnType<typeof createSearchQueue<SearchTarget>> | null>(null);
  useEffect(() => {
    if (visibleTarget) queueRef.current?.prioritize(visibleTarget);
  }, [visibleTarget]);
  const [cats, setCats] = useState<Set<string>>(new Set());
  const { laws, live: lawsLive, loading } = useLaws();

  const searchDetailsRequestedRef = useRef(false);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [speechHits, setSpeechHits] = useState<SpeechHit[]>([]);
  const [kanpoHits, setKanpoHits] = useState<KanpoHit[]>([]);
  const [tsutatsuHits, setTsutatsuHits] = useState<TsutatsuHit[]>([]);
  const [reikiHits, setReikiHits] = useState<ReikiHit[]>([]);
  const [englishHits, setEnglishHits] = useState<EnglishHit[]>([]);
  const [documentHits, setDocumentHits] = useState<Record<DocumentKind, DocumentHit[]>>({ pubcomment: [], gian: [], shingikai: [] });
  const [ftsAvailable, setFtsAvailable] = useState<boolean | null>(null);
  const [ftsMeta, setFtsMeta] = useState<Record<string, string> | null>(null);
  const [pending, setPending] = useState<Set<SearchTarget>>(() =>
    new Set(initialQuery.trim() ? SEARCH_TARGETS.map(([key]) => key) : []));
  const [running, setRunning] = useState<Set<SearchTarget>>(new Set());
  const [failures, setFailures] = useState<Partial<Record<SearchTarget, string>>>({});
  const [retry, setRetry] = useState(0);
  const searching = pending.has("laws");
  const supplementarySearching = [...pending].some(target => target !== "laws");
  // search.db の laws.category から取れる e-Gov 法令分類 (50 区分)。
  const [ftsCategories, setFtsCategories] = useState<string[]>([]);

  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    const query = q.trim();
    setHits([]); setSpeechHits([]); setKanpoHits([]); setTsutatsuHits([]); setReikiHits([]); setEnglishHits([]);
    setDocumentHits({ pubcomment: [], gian: [], shingikai: [] });
    setFailures({});
    setRunning(new Set());
    if (!query || !buildFtsMatch(query) || targets.size === 0) {
      setPending(new Set());
      return;
    }
    setPending(new Set(targets));
    const request = { signal: controller.signal, includeSnippet: false };
    const timer = setTimeout(() => {
      const tasks: [SearchTarget, () => Promise<void>][] = [
        ["laws", async () => {
          const rows = await ftsSearch(query, 50, Array.from(cats), 0, request);
          if (active) { setHits(rows); setFtsAvailable(true); }
        }],
        ["speeches", async () => { const rows = await searchSpeeches(query, 10, 0, request); if (active) setSpeechHits(rows); }],
        ["kanpo", async () => { const rows = await searchKanpo(query, 10, true, 0, request); if (active) setKanpoHits(rows); }],
        ["tsutatsu", async () => { const rows = await searchTsutatsu(query, 10, true, 0, request); if (active) setTsutatsuHits(rows); }],
        ["reiki", async () => { const rows = await searchReiki(query, { limit: 20, strict: true, ...request }); if (active) setReikiHits(rows); }],
        // 英訳 (laws_en_fts) は英単語を含むクエリだけ。旧 search.db (表なし) では空で返す。
        ["english", async () => { const rows = await searchEnglish(query, 10, false, 0, request); if (active) setEnglishHits(rows); }],
        ...(["pubcomment", "gian", "shingikai"] as const).map(kind => [kind, async () => {
          const rows = await searchDocuments(query, kind, 10, true, 0, { ...request, includeSnippet: false });
          if (active) setDocumentHits(prev => ({ ...prev, [kind]: rows }));
        }] as [SearchTarget, () => Promise<void>]),
      ];
      const selectedTasks = new Map(tasks.filter(([target]) => targets.has(target)).map(([target, task]) => [target, async () => {
        if (!active) return;
        setRunning(prev => new Set(prev).add(target));
        try {
          await task();
        } catch (error: any) {
          if (!active) return;
          if (target === "laws") setFtsAvailable(false);
          const message = error?.name === "TimeoutError"
            ? "時間がかかっているため中断しました。語句や検索対象を絞って再検索してください。"
            : "検索できませんでした。再試行してください。";
          setFailures(prev => ({ ...prev, [target]: message }));
        } finally {
          if (active) {
            setPending(prev => { const next = new Set(prev); next.delete(target); return next; });
            setRunning(prev => { const next = new Set(prev); next.delete(target); return next; });
          }
        }
      }] as const));
      queueRef.current = createSearchQueue(selectedTasks, visibleTargetRef.current);
    }, 200);
    // 結果を捨てるだけでなく、同期XHRやSQLの実行もWorkerごと止める。
    return () => { active = false; clearTimeout(timer); queueRef.current?.cancel(); queueRef.current = null; controller.abort(); };
  }, [q, cats, targets, retry]);

  // 件数 meta とカテゴリは初回の法令検索をブロックさせない。
  // 検索結果が画面に出てから、1 度だけ遅延読み込みする。
  useEffect(() => {
    if (ftsAvailable !== true || searching || supplementarySearching || searchDetailsRequestedRef.current) return;
    searchDetailsRequestedRef.current = true;
    getFtsMeta().then(setFtsMeta).catch(() => setFtsMeta(null));
    getCategories().then(setFtsCategories).catch(() => setFtsCategories([]));
  }, [ftsAvailable, searching, supplementarySearching]);

  // FTS 不可のときの法令単位フィルタ (旧来動作)。
  const filteredLaws = useMemo(() => {
    return laws.filter(l => {
      const matchQ = !q || l.title.includes(q) || l.law_num.includes(q) || l.law_id.includes(q);
      const matchC = cats.size === 0 || cats.has(l.category);
      return matchQ && matchC;
    });
  }, [q, cats, laws]);

  const toggle = <T,>(set: Set<T>, v: T, fn: (s: Set<T>) => void) => {
    const next = new Set(set);
    next.has(v) ? next.delete(v) : next.add(v);
    fn(next);
  };


  // FTS が使えるかどうかで表示モードを切り替える。
  const useFts = ftsAvailable !== false;
  const resultCount = useFts ? hits.length : filteredLaws.length;
  const resultCounts: Record<SearchTarget, number> = {
    laws: resultCount, speeches: speechHits.length, kanpo: kanpoHits.length,
    tsutatsu: tsutatsuHits.length, reiki: reikiHits.length,
    pubcomment: documentHits.pubcomment.length, gian: documentHits.gian.length,
    shingikai: documentHits.shingikai.length, english: englishHits.length,
  };
  // bigram index は 2 文字以上でないと検索できない。クエリはあるが
  // 使えるトークン (2 文字以上) が 1 つも無いとき = 短すぎ。
  const tooShort = q.trim() !== "" && buildFtsMatch(q.trim()) === "";
  // クエリに含まれる法律 term の別表記 (シソーラス)。検索は自動でこれらも OR 検索する。
  const synonyms = useMemo(() => synonymExpansions(q), [q]);

  return (
    <div className="p-6">
      <div className="mb-6">
        <h1 className="text-2xl">検索</h1>
        <p className="text-sm text-muted-foreground mt-1">
          法令・会議録・官報・通達・例規・パブコメ・議案・審議会資料を横断検索
          {useFts && ftsMeta && (
            <span className="ml-2 inline-flex items-center gap-1 text-xs text-muted-foreground">
              <Database className="size-3" />
              FTS5 / 法令 {ftsMeta.law_count} · 条文 {ftsMeta.article_count}
            </span>
          )}
        </p>
        {synonyms.length > 0 && (
          <div className="mt-2 flex items-center gap-1.5 flex-wrap text-xs text-muted-foreground">
            <span>同義語も検索:</span>
            {synonyms.map(s => (
              <span key={s} className="px-1.5 py-0.5 rounded bg-muted text-foreground/80">{s}</span>
            ))}
          </div>
        )}
      </div>

      <div className="grid grid-cols-[260px_minmax(0,1fr)] gap-6">
        <aside className="space-y-5">
          <Card>
            <CardContent className="p-4 space-y-4">
              <div className="flex items-center gap-2 text-sm">
                <SlidersHorizontal className="size-4" />
                絞り込み
                {cats.size > 0 && (
                  <button
                    className="ml-auto text-[10px] text-muted-foreground hover:text-foreground underline"
                    onClick={() => setCats(new Set())}
                  >
                    クリア ({cats.size})
                  </button>
                )}
              </div>
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-xs text-muted-foreground">検索対象</Label>
                  <div className="flex gap-2 text-xs">
                    <button onClick={() => setTargets(new Set(SEARCH_TARGETS.map(([key]) => key)))}>すべて</button>
                    <button onClick={() => setTargets(new Set())}>解除</button>
                  </div>
                </div>
                {SEARCH_TARGETS.map(([key, label]) => (
                  <div key={key} className="flex items-center gap-2">
                    <Checkbox id={`target-${key}`} checked={targets.has(key)} onCheckedChange={() => toggle(targets, key, setTargets)} />
                    <Label htmlFor={`target-${key}`} className="text-sm cursor-pointer">{label}</Label>
                  </div>
                ))}
              </div>
              {targets.has("laws") && <div>
                <Label className="text-xs text-muted-foreground mb-2 block">
                  カテゴリ (e-Gov 法令分類)
                </Label>
                {ftsCategories.length > 0 ? (
                  <ScrollArea className="h-72 pr-3">
                    <div className="space-y-2">
                      {ftsCategories.map(c => (
                        <div key={c} className="flex items-center gap-2">
                          <Checkbox
                            id={`c-${c}`}
                            checked={cats.has(c)}
                            onCheckedChange={() => toggle(cats, c, setCats)}
                          />
                          <Label htmlFor={`c-${c}`} className="text-sm cursor-pointer">{c}</Label>
                        </div>
                      ))}
                    </div>
                  </ScrollArea>
                ) : (
                  <div className="text-xs text-muted-foreground">読み込み中…</div>
                )}
              </div>}
            </CardContent>
          </Card>
        </aside>

        <div className="min-w-0 space-y-4">
          {targets.size === 0 ? (
            <div className="text-center py-20 text-sm text-muted-foreground">検索対象を選んでください</div>
          ) : !q.trim() ? (
            // 検索語が空のときは件数 (0 件) ではなく案内を出す。
            <div className="flex flex-col items-center justify-center text-center py-20 gap-3">
              <div className="size-14 rounded-full bg-muted flex items-center justify-center">
                <Search className="size-6 text-muted-foreground" />
              </div>
              <div className="text-sm">資料名や本文のキーワードを入力して検索</div>
              <div className="text-xs text-muted-foreground">
                例: 民法 ／ 第九条 ／ 信義誠実 ／ 労働基準
              </div>
              {useFts && ftsMeta && (
                <div className="text-xs text-muted-foreground inline-flex items-center gap-1">
                  <Database className="size-3" />
                  FTS5 / 法令 {ftsMeta.law_count} · 条文 {ftsMeta.article_count} を全文検索
                </div>
              )}
            </div>
          ) : tooShort ? (
            // bigram index の制約で 1 文字検索は不可。2 文字以上を促す。
            <div className="flex flex-col items-center justify-center text-center py-20 gap-2">
              <div className="size-14 rounded-full bg-muted flex items-center justify-center">
                <Search className="size-6 text-muted-foreground" />
              </div>
              <div className="text-sm">2 文字以上で検索してください</div>
              <div className="text-xs text-muted-foreground">
                全文検索は 2 文字単位 (bigram) で索引しているため、1 文字では検索できません
              </div>
            </div>
          ) : (
            <Tabs value={visibleTarget} onValueChange={value => setActiveTarget(value as SearchTarget)} className="gap-4">
              <TabsList aria-label="検索結果の種類" className="h-auto w-full flex-wrap justify-start gap-1">
                {selectedTargets.map(([key, label]) => (
                  <TabsTrigger key={key} value={key} className="flex-none px-3 py-2">
                    {label}
                    {pending.has(key) ? (
                      running.has(key)
                        ? <><LoaderCircle className="size-3 animate-spin" aria-hidden="true" /><span className="text-xs">検索中</span></>
                        : <span className="text-xs text-muted-foreground">待機中</span>
                    ) : failures[key] ? (
                      <span className="text-xs text-destructive">エラー</span>
                    ) : (
                      <span className="text-xs text-muted-foreground">{resultCounts[key]}件</span>
                    )}
                  </TabsTrigger>
                ))}
              </TabsList>
              {pending.size > 0 && (
                <div role="status" className="text-xs text-muted-foreground">
                  {running.size > 0 && <>検索中: {selectedTargets.filter(([key]) => running.has(key)).map(([, label]) => label).join("・")}</>}
                  {pending.size > running.size && <span className="ml-2">待機中: {pending.size - running.size}種類</span>}
                </div>
              )}
              {selectedTargets.map(([key, label]) => (
                <TabsContent key={key} value={key} className="space-y-4">
                  {failures[key] && (
                    <div role="alert" className="rounded-md border p-3 text-sm space-y-2">
                      <p>{label}：{failures[key]}</p>
                      <Button variant="outline" size="sm" onClick={() => setRetry(value => value + 1)}>再試行</Button>
                    </div>
                  )}
                  {key !== "laws" && pending.has(key) && (
                    <div className="text-center py-12 text-sm text-muted-foreground">{label}を{running.has(key) ? "検索中" : "待機中"}…</div>
                  )}
                  {key !== "laws" && !pending.has(key) && !failures[key] && resultCounts[key] === 0 && (
                    <div className="text-center py-12 text-sm text-muted-foreground">該当する資料がありません</div>
                  )}
                  {(["pubcomment", "gian", "shingikai"] as const).map(kind => key === kind && documentHits[kind].length > 0 && (
                    <div key={kind} className="space-y-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <FileText className="size-3.5" />
                        <span>{DOCUMENT_LABELS[kind]} ({documentHits[kind].length}件)</span>
                      </div>
                      {documentHits[kind].map(hit => (
                        <Card key={hit.route} className="hover:border-primary/50 transition-colors cursor-pointer"
                          onClick={() => navigate(hit.route.split("/").map(encodeURIComponent).join("/"))}>
                          <CardContent className="p-3">
                            <div className="text-sm font-medium">{hit.title}</div>
                            <div className="text-xs text-muted-foreground mt-0.5">{hit.subtitle}</div>
                            {hit.snippet && <div className="text-sm mt-1.5 leading-relaxed [&>mark]:bg-amber-300/40"
                              dangerouslySetInnerHTML={{ __html: unbigramSnippet(hit.snippet) }} />}
                          </CardContent>
                        </Card>
                      ))}
                    </div>
                  ))}
                  {/* 会議録発言 FTS セクション */}
                  {key === "speeches" && speechHits.length > 0 && (
                    <div className="space-y-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Landmark className="size-3.5" />
                        <span>国会会議録 発言 ({speechHits.length}件)</span>
                      </div>
                      {speechHits.map((h, i) => (
                        <Card
                          key={`${h.meeting_id}-${h.speech_id}-${i}`}
                          className="hover:border-primary/50 transition-colors cursor-pointer"
                          onClick={() => navigate(`/proceedings/${h.meeting_id}`)}
                        >
                          <CardContent className="p-3 flex items-start gap-3">
                            <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
                              <Landmark className="size-4 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-sm font-medium">{h.committee ?? "本会議"}</span>
                                <Badge variant="outline" className="text-xs">{h.house}</Badge>
                                <Badge variant="outline" className="text-xs">第{h.session}回</Badge>
                                {h.speaker && <span className="text-xs text-muted-foreground">{h.speaker}</span>}
                              </div>
                              <div className="text-xs text-muted-foreground mt-0.5">{h.date}</div>
                              {h.snippet && (
                                <div
                                  className="text-sm mt-1.5 leading-relaxed [&>mark]:bg-amber-300/40 [&>mark]:rounded-sm [&>mark]:px-0.5"
                                  dangerouslySetInnerHTML={{ __html: unbigramSnippet(h.snippet) }}
                                />
                              )}
                            </div>
                            <ChevronRight className="size-4 text-muted-foreground shrink-0 mt-1" />
                          </CardContent>
                        </Card>
                      ))}
                    </div>
                  )}

                  {/* 官報記事 FTS セクション */}
                  {key === "kanpo" && kanpoHits.length > 0 && (
                    <div className="space-y-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Newspaper className="size-3.5" />
                        <span>官報 ({kanpoHits.length}件)</span>
                      </div>
                      {kanpoHits.map((h, i) => (
                        <Card
                          key={`${h.date}-${h.issue_no}-${h.page}-${i}`}
                          className="hover:border-primary/50 transition-colors"
                        >
                          <CardContent className="p-3 flex items-start gap-3">
                            <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
                              <Newspaper className="size-4 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2 flex-wrap">
                                <button
                                  onClick={() => navigate(`/kanpo/${h.date}?${new URLSearchParams({ no: h.issue_no, p: String(h.page), t: h.title })}`)}
                                  className="text-sm font-medium text-left hover:text-primary hover:underline"
                                  title="官報の項目を開く"
                                >
                                  {h.title}
                                </button>
                                {h.agency && <Badge variant="outline" className="text-xs">{h.agency}</Badge>}
                              </div>
                              <div className="text-xs text-muted-foreground mt-0.5">
                                {h.date} · {h.issue_no}{h.page ? ` · ${h.page}頁` : ""}
                              </div>
                              {h.snippet && (
                                <div
                                  className="text-sm mt-1.5 leading-relaxed [&>mark]:bg-amber-300/40 [&>mark]:rounded-sm [&>mark]:px-0.5"
                                  dangerouslySetInnerHTML={{ __html: unbigramSnippet(h.snippet) }}
                                />
                              )}
                              {h.law_id && h.law_title && (
                                <button
                                  onClick={() => navigate(`/laws/${h.law_id}`)}
                                  className="mt-1.5 inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded border border-border hover:border-primary hover:text-primary transition-colors"
                                  title="改正対象の法令を開く"
                                >
                                  <BookOpen className="size-3 shrink-0" />
                                  <span className="truncate max-w-[18rem]">{h.law_title}</span>
                                </button>
                              )}
                            </div>
                            {h.pdf_url && (
                              <a
                                href={h.pdf_url}
                                target="_blank"
                                rel="noreferrer"
                                className="text-muted-foreground hover:text-primary shrink-0 mt-1"
                                title="官報PDFを開く"
                              >
                                <ExternalLink className="size-4" />
                              </a>
                            )}
                          </CardContent>
                        </Card>
                      ))}
                    </div>
                  )}

                  {/* 通達 (soft law) FTS セクション */}
                  {key === "tsutatsu" && tsutatsuHits.length > 0 && (
                    <div className="space-y-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <ScrollText className="size-3.5" />
                        <span>通達 ({tsutatsuHits.length}件)</span>
                      </div>
                      {tsutatsuHits.map((h, i) => (
                        <Card key={`${h.tax}-${h.number}-${i}`} className="hover:border-primary/50 transition-colors">
                          <CardContent className="p-3 flex items-start gap-3">
                            <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
                              <ScrollText className="size-4 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0">{h.number}</span>
                                {h.caption && <span className="text-sm font-medium">{h.caption}</span>}
                                {h.set_name && <Badge variant="outline" className="text-xs">{h.set_name}</Badge>}
                              </div>
                              {h.snippet && (
                                <div
                                  className="text-sm mt-1.5 leading-relaxed [&>mark]:bg-amber-300/40 [&>mark]:rounded-sm [&>mark]:px-0.5"
                                  dangerouslySetInnerHTML={{ __html: unbigramSnippet(h.snippet) }}
                                />
                              )}
                            </div>
                            {h.source_url && (
                              <a href={h.source_url} target="_blank" rel="noreferrer" className="text-muted-foreground hover:text-primary shrink-0 mt-1" title="通達の原文を開く">
                                <ExternalLink className="size-4" />
                              </a>
                            )}
                          </CardContent>
                        </Card>
                      ))}
                    </div>
                  )}

                  {/* 法令英訳 (日本法令外国語訳データベース) FTS セクション */}
                  {key === "english" && englishHits.length > 0 && (
                    <div className="space-y-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Languages className="size-3.5" />
                        <span>法令英訳 ({englishHits.length}件)</span>
                      </div>
                      {englishHits.map((h, i) => (
                        <Card
                          key={`${h.law_id}-${h.article_id}-${i}`}
                          className="hover:border-primary/50 transition-colors cursor-pointer"
                          onClick={() => navigate(`/laws/${h.law_id}#${h.article_id}`, { state: { showEnglish: true } })}
                        >
                          <CardContent className="p-3 flex items-start gap-3">
                            <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
                              <Languages className="size-4 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1" lang="en">
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-sm font-medium">{h.law_title}</span>
                                <span className="text-xs text-muted-foreground">{h.title}{h.caption && ` ${h.caption}`}</span>
                                {h.article_no && <Badge variant="outline" className="text-xs" lang="ja">{h.article_no}</Badge>}
                              </div>
                              {h.snippet && (
                                <div
                                  className="text-sm mt-1.5 leading-relaxed [&>mark]:bg-amber-300/40 [&>mark]:rounded-sm [&>mark]:px-0.5"
                                  dangerouslySetInnerHTML={{ __html: h.snippet }}
                                />
                              )}
                            </div>
                            {h.url && (
                              <a
                                href={h.url}
                                target="_blank"
                                rel="noopener noreferrer"
                                onClick={ev => ev.stopPropagation()}
                                className="text-muted-foreground hover:text-primary shrink-0 mt-1"
                                title="日本法令外国語訳データベースで開く"
                              >
                                <ExternalLink className="size-4" />
                              </a>
                            )}
                          </CardContent>
                        </Card>
                      ))}
                      <p className="text-[10px] text-muted-foreground">
                        出典: 日本法令外国語訳データベース（法務省, PDL1.0）。翻訳は参考訳で、法的効力は日本語の法令のみ。
                      </p>
                    </div>
                  )}

                  {/* 自治体例規 FTS セクション (reiki-search.db) */}
                  {key === "reiki" && reikiHits.length > 0 && (
                    <div className="space-y-2" data-testid="reiki-hits">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Building2 className="size-3.5" />
                        <span>自治体例規 ({reikiHits.length}件)</span>
                      </div>
                      {reikiHits.map((h, i) => (
                        <Card key={`${h.reiki_id}-${h.article_id}-${i}`} className="hover:border-primary/50 transition-colors cursor-pointer"
                          onClick={() => navigate(`/reiki/${h.municipality_code}/${encodeURIComponent(h.reiki_id)}${h.article_id ? `?a=${encodeURIComponent(h.article_id)}` : ""}`)}>
                          <CardContent className="p-3 flex items-start gap-3">
                            <div className="size-8 rounded-md bg-muted flex items-center justify-center shrink-0 mt-0.5">
                              <Building2 className="size-4 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0">{h.prefecture} {h.municipality_name}</span>
                                <span className="text-sm font-medium">{h.title}</span>
                                {h.article_no && <Badge variant="outline" className="text-xs">{h.article_no}{h.caption ? `（${h.caption}）` : ""}</Badge>}
                              </div>
                              {h.excerpt && <div className="text-sm mt-1.5 leading-relaxed text-muted-foreground line-clamp-2">{h.excerpt}</div>}
                            </div>
                          </CardContent>
                        </Card>
                      ))}
                    </div>
                  )}

                  {key === "laws" && <>
                    <div className="flex items-center justify-between text-sm text-muted-foreground">
                      <span>
                        法令 {resultCount} 件
                        {(loading || searching) && " (読み込み中…)"}
                        {!loading && !lawsLive && !useFts && " (モック)"}
                        {useFts && " · 索引順 (FTS5)"}
                      </span>
                    </div>

                    <div className="space-y-2">
                      {useFts ? (
                        hits.length === 0 && searching ? (
                          // 検索中で結果未到達。skeleton カードを並べて待ちであることを示す。
                          <div className="space-y-2">
                            {Array.from({ length: 5 }).map((_, i) => (
                              <Card key={i}>
                                <CardContent className="p-4 flex items-start gap-4">
                                  <Skeleton className="size-10 shrink-0" />
                                  <div className="min-w-0 flex-1 space-y-2">
                                    <Skeleton className="h-4 w-2/3" />
                                    <Skeleton className="h-3 w-1/3" />
                                    <Skeleton className="h-3 w-full" />
                                  </div>
                                </CardContent>
                              </Card>
                            ))}
                          </div>
                        ) : hits.length === 0 && q ? (
                          <div className="text-center py-12 text-sm text-muted-foreground">該当する条文がありません</div>
                        ) : (
                          hits.map((h, i) => (
                            // FTS5 が同じ article に対し複数ヒットを返す (snippet 位置違い) ケース
                            // があり law_id+article_id だけだと key 衝突する。順序 index を足す。
                            <Card key={`${h.law_id}-${h.article_id}-${i}`} className="hover:border-primary/50 transition-colors cursor-pointer" onClick={() => {
                              // 条文 hit から法令詳細へ。article_id は URL ハッシュに乗せたいが
                              // 現状 Browse 詳細は scroll 制御を持っていないので、まず法令単位で開く。
                              onOpen({
                                law_id: h.law_id,
                                law_num: h.law_num ?? "",
                                title: h.title,
                                category: "行政",
                                promulgation_date: "",
                                effective_date: "",
                                last_updated: "",
                                status: "current",
                                article_count: 0,
                              });
                            }}>
                              <CardContent className="p-4 flex items-start gap-4">
                                <div className="size-10 rounded-md bg-muted flex items-center justify-center shrink-0">
                                  <FileText className="size-5 text-muted-foreground" />
                                </div>
                                <div className="min-w-0 flex-1">
                                  <div className="flex items-center gap-2 flex-wrap">
                                    <span>{h.title}</span>
                                    {h.article_no && <Badge variant="outline" className="text-xs">{h.article_no}</Badge>}
                                    {h.caption && <span className="text-xs text-muted-foreground">{h.caption}</span>}
                                  </div>
                                  <div className="text-xs text-muted-foreground mt-0.5 truncate">{h.law_num ?? ""} · {h.law_id}</div>
                                  {h.snippet && (
                                    <div
                                      className="text-sm mt-2 leading-relaxed [&>mark]:bg-amber-300/40 [&>mark]:rounded-sm [&>mark]:px-0.5"
                                      dangerouslySetInnerHTML={{ __html: unbigramSnippet(h.snippet) }}
                                    />
                                  )}
                                </div>
                                <ChevronRight className="size-4 text-muted-foreground shrink-0 mt-2" />
                              </CardContent>
                            </Card>
                          ))
                        )
                      ) : (
                        <>
                          {filteredLaws.map(l => (
                            <Card key={l.law_id} className="hover:border-primary/50 transition-colors cursor-pointer" onClick={() => onOpen(l)}>
                              <CardContent className="p-4 flex items-center gap-4">
                                <div className="size-10 rounded-md bg-muted flex items-center justify-center shrink-0">
                                  <FileText className="size-5 text-muted-foreground" />
                                </div>
                                <div className="min-w-0 flex-1">
                                  <div className="flex items-center gap-2">
                                    <span>{l.title}</span>
                                    <Badge variant="outline" className="text-xs">{l.category}</Badge>
                                    {l.status === "scheduled" && <Badge className="text-xs">施行待ち</Badge>}
                                  </div>
                                  <div className="text-xs text-muted-foreground mt-0.5 truncate">{l.law_num} · {l.law_id}</div>
                                  <div className="text-xs text-muted-foreground mt-1 flex gap-4">
                                    <span>公布 {l.promulgation_date}</span>
                                    <span>施行 {l.effective_date}</span>
                                    {l.article_count > 0 && <span>条数 {l.article_count}</span>}
                                  </div>
                                </div>
                                <ChevronRight className="size-4 text-muted-foreground shrink-0" />
                              </CardContent>
                            </Card>
                          ))}
                          {filteredLaws.length === 0 && (
                            <div className="text-center py-12 text-sm text-muted-foreground">該当する法令がありません</div>
                          )}
                        </>
                      )}
                    </div>
                  </>}
                </TabsContent>
              ))}
            </Tabs>
          )}
        </div>
      </div>
    </div>
  );
}
