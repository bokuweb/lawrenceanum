import { useEffect, useMemo, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Input } from "../ui/input";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Label } from "../ui/label";
import { ScrollArea } from "../ui/scroll-area";
import { type LawSummary } from "../mock-data";
import { Search, SlidersHorizontal, ChevronRight, FileText, Database } from "lucide-react";
import { useLaws } from "../../data/use-laws";
import { search as ftsSearch, getMeta as getFtsMeta, getCategories, buildFtsMatch, unbigramSnippet, type SearchHit } from "../../data/search-engine";

export function SearchView({ initialQuery = "", onOpen, onQueryChange }: { initialQuery?: string; onOpen: (l: LawSummary) => void; onQueryChange?: (q: string) => void }) {
  const [q, setQ] = useState(initialQuery);
  useEffect(() => { setQ(initialQuery); }, [initialQuery]);

  const [cats, setCats] = useState<Set<string>>(new Set());
  const { laws, live: lawsLive, loading } = useLaws();

  // FTS 検索結果と meta。
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [ftsAvailable, setFtsAvailable] = useState<boolean | null>(null);
  const [ftsMeta, setFtsMeta] = useState<Record<string, string> | null>(null);
  const [searching, setSearching] = useState(false);
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
    if (!q.trim()) { setHits([]); return; }
    if (ftsAvailable === false) return; // FTS 不可ならフィルタ側に倒す。
    let cancelled = false;
    setSearching(true);
    ftsSearch(q, 50, Array.from(cats))
      .then(r => { if (!cancelled) { setHits(r); setSearching(false); } })
      .catch(() => { if (!cancelled) setSearching(false); });
    return () => { cancelled = true; };
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
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
            <Input value={q} onChange={e => { setQ(e.target.value); onQueryChange?.(e.target.value); }} className="pl-9 h-11" placeholder="例: 民法、第一条、信義誠実、労働..." />
          </div>
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
          <div className="flex items-center justify-between text-sm text-muted-foreground">
            <span>
              {resultCount} 件の結果
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
              hits.length === 0 && q ? (
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
