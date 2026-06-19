import { useEffect, useMemo, useRef, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Label } from "../ui/label";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Separator } from "../ui/separator";
import { type LawSummary } from "../mock-data";
import { Search, SlidersHorizontal, ChevronRight, FileText, Database, Landmark, MessageSquare, Newspaper, ExternalLink, BookOpen, ScrollText } from "lucide-react";
import { useLaws } from "../../data/use-laws";
import { search as ftsSearch, getMeta as getFtsMeta, getCategories, buildFtsMatch, unbigramSnippet, searchSpeeches, searchKanpo, searchTsutatsu, synonymExpansions, type SearchHit, type SpeechHit, type KanpoHit, type TsutatsuHit } from "../../data/search-engine";
import { useNavigate } from "react-router";

export function SearchView({ initialQuery = "", onOpen, onQueryChange }: { initialQuery?: string; onOpen: (l: LawSummary) => void; onQueryChange?: (q: string) => void }) {
  const navigate = useNavigate();
  const [q, setQ] = useState(initialQuery);
  useEffect(() => { setQ(initialQuery); }, [initialQuery]);

  const [cats, setCats] = useState<Set<string>>(new Set());
  const { laws, live: lawsLive, loading } = useLaws();

  // FTS 検索結果と meta。
  const queryGenRef = useRef(0);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [speechHits, setSpeechHits] = useState<SpeechHit[]>([]);
  const [kanpoHits, setKanpoHits] = useState<KanpoHit[]>([]);
  const [tsutatsuHits, setTsutatsuHits] = useState<TsutatsuHit[]>([]);
  const [ftsAvailable, setFtsAvailable] = useState<boolean | null>(null);
  const [ftsMeta, setFtsMeta] = useState<Record<string, string> | null>(null);
  // 初期クエリがあれば検索中扱いで開始する。さもないと初回 render で
  // hits=[] のまま「該当する条文がありません」が一瞬表示されてしまう。
  const [searching, setSearching] = useState(() => initialQuery.trim() !== "");
  // search.db の laws.category から取れる e-Gov 法令分類 (50 区分)。
  const [ftsCategories, setFtsCategories] = useState<string[]>([]);

  useEffect(() => {
    getFtsMeta().then(m => {
      setFtsAvailable(m !== null);
      setFtsMeta(m);
    });
    getCategories().then(setFtsCategories).catch(() => setFtsCategories([]));
  }, []);

  useEffect(() => {
    if (!q.trim()) { setHits([]); setSpeechHits([]); setKanpoHits([]); setTsutatsuHits([]); return; }
    if (ftsAvailable === false) return; // FTS 不可ならフィルタ側に倒す。
    // 世代カウンタをインクリメント。このエフェクトより前に発行されたクエリが
    // 後から返ってきても、gen が古ければ結果を捨てる。
    const gen = ++queryGenRef.current;
    setSearching(true);
    const timer = setTimeout(() => {
      Promise.all([
        ftsSearch(q, 50, Array.from(cats)),
        searchSpeeches(q, 10),
        searchKanpo(q, 10),
        searchTsutatsu(q, 10),
      ])
        .then(([lawHits, spHits, kpHits, tsHits]) => {
          if (queryGenRef.current === gen) {
            setHits(lawHits);
            setSpeechHits(spHits);
            setKanpoHits(kpHits);
            setTsutatsuHits(tsHits);
            setSearching(false);
          }
        })
        .catch(() => { if (queryGenRef.current === gen) setSearching(false); });
    }, 300);
    return () => { clearTimeout(timer); };
  }, [q, ftsAvailable, cats]);

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
  const useFts = ftsAvailable === true;
  const resultCount = useFts ? hits.length : filteredLaws.length;
  // bigram index は 2 文字以上でないと検索できない。クエリはあるが
  // 使えるトークン (2 文字以上) が 1 つも無いとき = 短すぎ。
  const tooShort = useFts && q.trim() !== "" && buildFtsMatch(q.trim()) === "";
  // クエリに含まれる法律 term の別表記 (シソーラス)。検索は自動でこれらも OR 検索する。
  const synonyms = useMemo(() => synonymExpansions(q), [q]);

  return (
    <div className="p-6">
      <div className="mb-6">
        <h1 className="text-2xl">検索</h1>
        <p className="text-sm text-muted-foreground mt-1">
          法令・条文・改正履歴を横断検索
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

      <div className="grid grid-cols-[260px_1fr] gap-6">
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
              <div>
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
              </div>
            </CardContent>
          </Card>
        </aside>

        <div className="space-y-4">
          {!q.trim() ? (
            // 検索語が空のときは件数 (0 件) ではなく案内を出す。
            <div className="flex flex-col items-center justify-center text-center py-20 gap-3">
              <div className="size-14 rounded-full bg-muted flex items-center justify-center">
                <Search className="size-6 text-muted-foreground" />
              </div>
              <div className="text-sm">法令名・法令番号・条文キーワードを入力して検索</div>
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
            <>
          {/* 会議録発言 FTS セクション */}
          {useFts && speechHits.length > 0 && (
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
              <Separator />
            </div>
          )}

          {/* 官報記事 FTS セクション */}
          {useFts && kanpoHits.length > 0 && (
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
                        <span className="text-sm font-medium">{h.title}</span>
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
              <Separator />
            </div>
          )}

          {/* 通達 (soft law) FTS セクション */}
          {useFts && tsutatsuHits.length > 0 && (
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
              <Separator />
            </div>
          )}

          <div className="flex items-center justify-between text-sm text-muted-foreground">
            <span>
              法令 {resultCount} 件
              {(loading || searching) && " (読み込み中…)"}
              {!loading && !lawsLive && !useFts && " (モック)"}
              {useFts && " · 関連度順 (FTS5)"}
            </span>
            <div className="flex gap-2">
              <Button variant="outline" size="sm">関連度順</Button>
              <Button variant="ghost" size="sm">更新日順</Button>
            </div>
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
                          <span>条数 {l.article_count}</span>
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
            </>
          )}
        </div>
      </div>
    </div>
  );
}
