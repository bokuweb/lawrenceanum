import { useEffect, useMemo, useState } from "react";
import { Card, CardContent } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Skeleton } from "../ui/skeleton";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { ARTICLES_V2, TIMELINE_EVENTS, type LawSummary } from "../mock-data";
import { ArrowLeft, Download, GitCompare, ExternalLink, Calendar, Hash, Tag, Link2, Check, ArrowUpRight, Search, Landmark, MessageSquare, Newspaper, ScrollText } from "lucide-react";
import { useLocation, useNavigate } from "react-router";
import { useLaws, useLawDetail } from "../../data/use-laws";
import { api, type LawToProceedings, type LawToPubcomments, type LawToTsutatsu, type AmendDocument, type AmendRun, type AmendNestedTable } from "../../data/api";
import { getRefsForLaw, type ArticleRef } from "../../data/search-engine";

/**
 * 新旧対照表の改め文テキストを「改正後」「改正前」の見出し行で区切り、表組み用に
 * { preamble, rows: [{after, before}] } へ分解する。見出しが無ければ null（呼び出し側で
 * プレーン表示にフォールバック）。officialの新旧表は 改正後→改正前 が交互に並ぶ。
 */
function parseShinkyu(text: string): { preamble: string; rows: { after: string; before: string }[] } | null {
  const lines = text.split("\n");
  const isAfter = (l: string) => l.trim() === "改正後";
  const isBefore = (l: string) => l.trim() === "改正前";
  if (!lines.some(isAfter) || !lines.some(isBefore)) return null;
  const noise = (l: string) => l.trim() === "（傍線部分は改正部分）" || l.trim() === "";

  const firstAfter = lines.findIndex(isAfter);
  const preamble = lines.slice(0, firstAfter).filter(l => !noise(l)).join("\n").trim();

  const rows: { after: string; before: string }[] = [];
  let i = firstAfter;
  while (i < lines.length) {
    if (!isAfter(lines[i])) { i++; continue; }
    i++; // skip 改正後
    const after: string[] = [];
    while (i < lines.length && !isBefore(lines[i]) && !isAfter(lines[i])) { if (!noise(lines[i])) after.push(lines[i]); i++; }
    const before: string[] = [];
    if (i < lines.length && isBefore(lines[i])) {
      i++; // skip 改正前
      while (i < lines.length && !isAfter(lines[i])) { if (!noise(lines[i])) before.push(lines[i]); i++; }
    }
    if (after.length || before.length) rows.push({ after: after.join("\n").trim(), before: before.join("\n").trim() });
  }
  return rows.length ? { preamble, rows } : null;
}

/** Run 列を描画。傍線(下線)付きは <u> で表す（現状は常に false だが将来の傍線抽出に対応）。 */
function renderRuns(runs: AmendRun[]) {
  return runs.map((r, i) => (r.underline ? <u key={i}>{r.text}</u> : <span key={i}>{r.text}</span>));
}

/** 別表（罫線で区切られた表）を入れ子テーブルとして描画する。 */
function NestedTableView({ table }: { table: AmendNestedTable }) {
  return (
    <table className="w-full border-collapse my-1">
      <tbody>
        {table.rows.map((row, ri) => (
          <tr key={ri} className="align-top">
            {row.map((cell, ci) => (
              <td key={ci} className="border border-border/70 px-1.5 py-0.5">
                <pre className="whitespace-pre-wrap font-sans leading-relaxed">{renderRuns(cell)}</pre>
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** 新旧対照表のセル。別表(2D表)があればそれを、無ければテキストを描画する。 */
function renderCell(runs: AmendRun[], table?: AmendNestedTable) {
  if (table && table.rows.length > 0) {
    return <NestedTableView table={table} />;
  }
  return <pre className="whitespace-pre-wrap font-sans leading-relaxed">{renderRuns(runs)}</pre>;
}

/**
 * 構造化改め文(amend_document)を描画する。段落はそのまま、新旧対照表は条（別表）ごとの
 * 行に分けて改正後/改正前を左右対比のテーブルで表示する。
 */
function AmendDocumentView({ doc }: { doc: AmendDocument }) {
  return (
    <div className="mt-1 max-h-96 overflow-auto rounded bg-muted/40 p-2 space-y-2">
      {doc.blocks.map((b, bi) => {
        if (b.kind === "paragraph") {
          return (
            <pre key={bi} className="text-xs whitespace-pre-wrap font-sans leading-relaxed">
              {renderRuns(b.runs)}
            </pre>
          );
        }
        return (
          <table key={bi} className="w-full text-xs border-collapse table-fixed">
            <thead>
              <tr>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正後</th>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正前</th>
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, ri) => (
                <tr key={ri} className="align-top">
                  <td className="border border-border px-2 py-1">{renderCell(r.after, r.after_table)}</td>
                  <td className="border border-border px-2 py-1">{renderCell(r.before, r.before_table)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        );
      })}
    </div>
  );
}

export function BrowseView({ lawId, onSelect, onCompare }: { lawId: string | null; onSelect: (id: string | null) => void; onCompare: (id: string) => void }) {
  const { laws, live, loading } = useLaws();
  if (lawId) {
    const matched = laws.find(l => l.law_id === lawId);
    // 一覧に居なければ最小限の LawSummary を仮組み — 詳細は useLawDetail が JSON を埋める。
    const law: LawSummary = matched ?? {
      law_id: lawId,
      law_num: "",
      title: lawId,
      category: "行政",
      promulgation_date: "",
      effective_date: "",
      last_updated: "",
      status: "current",
      article_count: 0,
    };
    // key={lawId}: lawId が変わるたびに LawDetail を作り直し、前の law の doc/state を持ち越さない。
    return <LawDetail key={lawId} law={law} onBack={() => onSelect(null)} onCompare={() => onCompare(lawId)} />;
  }

  return <LawList laws={laws} live={live} loading={loading} onSelect={onSelect} />;
}

type SortKey = "updated" | "title";

function LawList({ laws, live, loading, onSelect }: { laws: LawSummary[]; live: boolean; loading: boolean; onSelect: (id: string) => void }) {
  const [query, setQuery] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("updated");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matched = q
      ? laws.filter(l => l.title.toLowerCase().includes(q) || (l.law_num ?? "").toLowerCase().includes(q))
      : laws;
    const sorted = [...matched];
    if (sortKey === "updated") {
      // 更新日 (last_updated) 降順。空は末尾。同日は title で安定化。
      sorted.sort((a, b) => {
        const ad = a.last_updated || "", bd = b.last_updated || "";
        if (ad !== bd) return ad < bd ? 1 : -1;
        return a.title.localeCompare(b.title, "ja");
      });
    } else {
      sorted.sort((a, b) => a.title.localeCompare(b.title, "ja"));
    }
    return sorted;
  }, [laws, query, sortKey]);

  return (
    <div className="p-6">
      <div className="mb-4 flex items-end justify-between">
        <div>
          <h1 className="text-2xl">法令閲覧</h1>
          <p className="text-sm text-muted-foreground mt-1">登録されている全法令を一覧で参照</p>
        </div>
        <div className="text-xs text-muted-foreground">
          {loading ? "読み込み中…" : `${filtered.length} / ${laws.length} 件${live ? "" : " (モック)"}`}
        </div>
      </div>

      <div className="mb-6 flex items-center gap-3">
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
          <Input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="タイトル・法令番号で絞り込み"
            className="pl-9"
            disabled={loading}
          />
        </div>
        <Select value={sortKey} onValueChange={v => setSortKey(v as SortKey)}>
          <SelectTrigger className="w-36"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="updated">更新順</SelectItem>
            <SelectItem value="title">名称順</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {loading ? (
        <div className="grid grid-cols-3 gap-4">
          {Array.from({ length: 12 }).map((_, i) => (
            <Card key={i}>
              <CardContent className="p-5 space-y-3">
                <div className="flex items-center justify-between">
                  <Skeleton className="h-5 w-16" />
                  <Skeleton className="h-5 w-12" />
                </div>
                <Skeleton className="h-5 w-3/4" />
                <Skeleton className="h-3 w-1/2" />
                <div className="flex items-center justify-between pt-3 border-t border-border">
                  <Skeleton className="h-3 w-10" />
                  <Skeleton className="h-3 w-20" />
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : filtered.length === 0 ? (
        <div className="text-sm text-muted-foreground py-12 text-center">「{query}」に一致する法令はありません</div>
      ) : (
        <div className="grid grid-cols-3 gap-4">
          {filtered.map(l => (
            <Card key={l.law_id} className="hover:border-primary/50 hover:shadow-md transition-all cursor-pointer" onClick={() => onSelect(l.law_id)}>
              <CardContent className="p-5">
                <div className="flex items-start justify-between mb-3">
                  <Badge variant="outline">{l.category}</Badge>
                  <Badge variant={l.status === "scheduled" ? "default" : l.status === "amended" ? "secondary" : "outline"} className="text-xs">
                    {l.status === "scheduled" ? "施行待ち" : l.status === "amended" ? "改正" : "現行"}
                  </Badge>
                </div>
                <div className="text-base mb-1">{l.title}</div>
                <div className="text-xs text-muted-foreground mb-4 truncate">{l.law_num}</div>
                <div className="flex items-center justify-between text-xs text-muted-foreground pt-3 border-t border-border">
                  <span>{l.article_count} 条</span>
                  <span>更新 {l.last_updated || "—"}</span>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

function ShareButton() {
  const [copied, setCopied] = useState(false);
  const onClick = async () => {
    const url = window.location.href;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard 不可環境のフォールバック: prompt を出すだけ。
      window.prompt("URL をコピーしてください", url);
    }
  };
  return (
    <Button variant="outline" size="sm" className="gap-1.5" onClick={onClick}>
      {copied ? <Check className="size-4 text-emerald-500" /> : <Link2 className="size-4" />}
      {copied ? "コピー済" : "共有"}
    </Button>
  );
}

function scrollToArticle(articleId: string) {
  const el = document.getElementById(articleId);
  if (el) {
    el.scrollIntoView({ behavior: "smooth", block: "start" });
    // 視覚的なフォーカスを 1 秒だけ付ける。
    el.classList.add("ring-2", "ring-primary", "ring-offset-2");
    window.setTimeout(() => el.classList.remove("ring-2", "ring-primary", "ring-offset-2"), 1200);
  }
}

/**
 * 条文本文の文字列に含まれる「第○条」を <a> リンクに置き換える。
 * `refs` は同 article から出ている outgoing 参照のみで、from->to 順に並ぶ。
 * 出現順に書き換え、未参照部分はそのままテキストとして残す。
 */
type NavigateFn = (to: { pathname: string; hash?: string } | string) => void;

function linkifyText(
  text: string,
  refs: ArticleRef[],
  navigate: NavigateFn,
  selfLawId: string,
): React.ReactNode[] {
  if (refs.length === 0) return [text];
  type Span = { start: number; end: number; ref: ArticleRef };
  const spans: Span[] = [];
  const sortedRefs = [...refs].sort((a, b) => b.ref_text.length - a.ref_text.length);
  for (const r of sortedRefs) {
    let from = 0;
    while (true) {
      const idx = text.indexOf(r.ref_text, from);
      if (idx < 0) break;
      const end = idx + r.ref_text.length;
      const overlaps = spans.some(s => !(end <= s.start || idx >= s.end));
      if (!overlaps) spans.push({ start: idx, end, ref: r });
      from = end;
    }
  }
  spans.sort((a, b) => a.start - b.start);
  const out: React.ReactNode[] = [];
  let cursor = 0;
  spans.forEach((s, i) => {
    if (s.start > cursor) out.push(text.slice(cursor, s.start));
    const r = s.ref;
    const isCross = r.ref_type === "cross_law" || r.to_law_id !== selfLawId;
    const targetId = r.to_article_id ?? "";
    const onClick = (ev: React.MouseEvent) => {
      ev.preventDefault();
      if (isCross) {
        // 他法令への遷移。HashRouter 配下では `hash` を別フィールドに渡さないと
        // `#` がパス文字列に紛れ込む。
        navigate({
          pathname: `/laws/${r.to_law_id}`,
          hash: targetId ? `#${targetId}` : "",
        });
      } else if (targetId) {
        scrollToArticle(targetId);
      }
    };
    const cls = isCross
      ? "text-emerald-600 dark:text-emerald-400 underline decoration-dotted underline-offset-2 hover:bg-emerald-500/10 rounded px-0.5"
      : r.ref_type === "previous_article" || r.ref_type === "next_article"
      ? "text-amber-600 dark:text-amber-400 underline decoration-dotted underline-offset-2 hover:bg-amber-500/10 rounded px-0.5"
      : "text-primary underline decoration-dotted underline-offset-2 hover:bg-primary/10 rounded px-0.5";
    const title = isCross
      ? `${r.to_law_id}${targetId ? ` / ${targetId}` : ""}`
      : `${r.ref_type}: ${targetId}`;
    out.push(
      <a
        key={`${s.start}-${i}`}
        href={isCross ? `#/laws/${r.to_law_id}${targetId ? `/${targetId}` : ""}` : `#${targetId}`}
        title={title}
        className={cls}
        onClick={onClick}
      >
        {text.slice(s.start, s.end)}
      </a>
    );
    cursor = s.end;
  });
  if (cursor < text.length) out.push(text.slice(cursor));
  return out;
}

function LawDetail({ law, onBack, onCompare }: { law: LawSummary; onBack: () => void; onCompare: () => void }) {
  const navigate = useNavigate();
  const location = useLocation();
  const detail = useLawDetail(law.law_id);
  // ライブ取得結果を尊重: doc が来たらたとえ articles=[] でもライブを採用し、
  // mock にフォールバックして「中身がチラ見えして消える」現象を回避する。
  // 完全 offline (doc も error も無い) のときだけ mock を使う。
  const liveAvailable = !!detail.doc;
  const offlineFallback = !detail.loading && !detail.doc;
  const articles = detail.doc?.articles ?? (offlineFallback ? ARTICLES_V2 : []);
  const articlesEmpty = liveAvailable && articles.length === 0;
  const [activeArt, setActiveArt] = useState(articles[0]?.article_id ?? "");
  // articles が後から確定するので追従する。
  useEffect(() => {
    if (articles[0] && !articles.find(a => a.article_id === activeArt)) {
      setActiveArt(articles[0].article_id);
    }
  }, [articles.length]);
  // 改正履歴は新しいものから順に並べる。公布日を主キーに、無ければ施行日・施行予定日・
  // 取込日にフォールバックして降順ソートする。
  const liveEvents = useMemo(() => {
    const events = detail.timeline?.events ?? [];
    const dateKey = (e: (typeof events)[number]) =>
      e.promulgation_date || e.effective_date || e.scheduled_enforcement_date || e.source_update_date || "";
    return [...events].sort((a, b) => {
      const ad = dateKey(a), bd = dateKey(b);
      if (ad !== bd) return ad < bd ? 1 : -1;
      return 0;
    });
  }, [detail.timeline]);
  const mockEvents = TIMELINE_EVENTS[law.law_id] ?? [];
  const useLiveTimeline = liveEvents.length > 0;

  // URL の hash (#art_X) で初期スクロール対象を受け取る。
  // detail.doc が来た後でないと DOM が無いので、両者揃ったタイミングで scroll する。
  useEffect(() => {
    const target = location.hash.replace(/^#/, "");
    if (!target || !detail.doc) return;
    // 条文が多い法令 (例: 所得税法 1283 条) は描画に時間がかかるうえ、被参照チップが
    // 後から非同期で読み込まれてレイアウトがずれる。対象要素が現れて「上部付近に収まる」
    // まで数回ポーリングしながら scroll し直す (固定遅延 1 回では取りこぼす/ずれる)。
    let tries = 0;
    let highlighted = false;
    let timer = window.setTimeout(function tick() {
      const el = document.getElementById(target);
      if (el) {
        el.scrollIntoView({ behavior: "auto", block: "start" });
        if (!highlighted) {
          highlighted = true;
          el.classList.add("ring-2", "ring-primary", "ring-offset-2");
          window.setTimeout(() => el.classList.remove("ring-2", "ring-primary", "ring-offset-2"), 1500);
        }
        const top = el.getBoundingClientRect().top;
        if (top >= -10 && top < 300) return; // 上部付近に収まったら終了
      }
      if (tries++ < 40) timer = window.setTimeout(tick, 100); // 最大 ~4 秒
    }, 50);
    return () => window.clearTimeout(timer);
  }, [location.hash, detail.doc?.law_id]);

  // 参照グラフ: 法令単位で 1 度だけ load してから、article_id 別にバケット化する。
  const [refs, setRefs] = useState<ArticleRef[]>([]);
  useEffect(() => {
    let cancelled = false;
    getRefsForLaw(law.law_id).then(r => { if (!cancelled) setRefs(r); }).catch(() => {});
    return () => { cancelled = true; };
  }, [law.law_id]);
  const outgoingByArt = useMemo(() => {
    const m = new Map<string, ArticleRef[]>();
    for (const r of refs) {
      if (r.from_law_id !== law.law_id) continue;
      const list = m.get(r.from_article_id) ?? [];
      list.push(r);
      m.set(r.from_article_id, list);
    }
    return m;
  }, [refs, law.law_id]);
  const incomingByArt = useMemo(() => {
    const m = new Map<string, ArticleRef[]>();
    for (const r of refs) {
      if (r.to_law_id !== law.law_id || !r.to_article_id) continue;
      const list = m.get(r.to_article_id) ?? [];
      list.push(r);
      m.set(r.to_article_id, list);
    }
    return m;
  }, [refs, law.law_id]);
  const articleNoById = useMemo(() => {
    const m = new Map<string, string>();
    for (const a of articles) m.set(a.article_id, a.article_no);
    return m;
  }, [articles]);

  // この法令に言及している国会会議録（law→proceedings クロスリンク）。リンクが無ければ非表示。
  const [proceedings, setProceedings] = useState<LawToProceedings | null>(null);
  useEffect(() => {
    let cancelled = false;
    setProceedings(null);
    api.lawToProceedings(law.law_id)
      .then(d => { if (!cancelled) setProceedings(d); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [law.law_id]);
  const linkedProceedings = proceedings?.linked_proceedings ?? [];

  // この法令に関するパブコメ（law→pubcomment クロスリンク）。リンクが無ければ非表示。
  const [pubcomments, setPubcomments] = useState<LawToPubcomments | null>(null);
  useEffect(() => {
    let cancelled = false;
    setPubcomments(null);
    api.lawToPubcomment(law.law_id)
      .then(d => { if (!cancelled) setPubcomments(d); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [law.law_id]);
  const linkedPubcomments = pubcomments?.linked_pubcomments ?? [];

  // この法令の通達（law→tsutatsu クロスリンク, 通達集の親法令単位）。リンクが無ければ非表示。
  const [tsutatsu, setTsutatsu] = useState<LawToTsutatsu | null>(null);
  useEffect(() => {
    let cancelled = false;
    setTsutatsu(null);
    api.lawToTsutatsu(law.law_id)
      .then(d => { if (!cancelled) setTsutatsu(d); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [law.law_id]);
  const linkedTsutatsu = tsutatsu?.linked_tsutatsu ?? [];

  // この法令の官報掲載。timeline イベントのうち官報リンク済み (pdf_url あり) を拾う。
  // liveEvents は公布日降順ソート済み。
  const kanpoEvents = useMemo(
    () => liveEvents.filter(e => e.kanpo?.linked && e.kanpo?.pdf_url),
    [liveEvents],
  );

  return (
    <div className="flex flex-col h-full">
      <div className="border-b border-border bg-background px-6 py-4">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1 -ml-2 mb-3">
          <ArrowLeft className="size-4" /> 一覧に戻る
        </Button>
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 mb-1">
              <h1 className="text-2xl">{law.title}</h1>
              <Badge variant="outline">{law.category}</Badge>
              <Badge variant={law.status === "scheduled" ? "default" : "secondary"}>
                {law.status === "scheduled" ? "施行待ち" : law.status === "amended" ? "改正" : "現行"}
              </Badge>
            </div>
            <div className="text-sm text-muted-foreground">{law.law_num}</div>
            <div className="flex gap-4 mt-3 text-xs text-muted-foreground">
              <span className="flex items-center gap-1"><Hash className="size-3" />{law.law_id}</span>
              <span className="flex items-center gap-1"><Calendar className="size-3" />公布 {law.promulgation_date}</span>
              <span className="flex items-center gap-1"><Calendar className="size-3" />施行 {law.effective_date}</span>
              <span className="flex items-center gap-1"><Tag className="size-3" />{law.article_count} 条</span>
            </div>
          </div>
          <div className="flex gap-2 shrink-0">
            <ShareButton />
            <Button variant="outline" size="sm" className="gap-1.5" asChild>
              <a href={`./laws/${law.law_id}/current.json`} target="_blank" rel="noreferrer">
                <Download className="size-4" />JSON
              </a>
            </Button>
            <Button variant="outline" size="sm" className="gap-1.5" onClick={onCompare}><GitCompare className="size-4" />比較</Button>
            <Button variant="outline" size="sm" className="gap-1.5" asChild>
              <a href={`https://laws.e-gov.go.jp/law/${law.law_id}`} target="_blank" rel="noreferrer">
                <ExternalLink className="size-4" />e-Gov
              </a>
            </Button>
          </div>
        </div>
      </div>

      {linkedProceedings.length > 0 && (
        <div className="border-b border-border bg-background px-6 py-3">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
            <Landmark className="size-3" />
            この法令への国会発言 ({linkedProceedings.length})
          </div>
          <div className="flex flex-wrap gap-1.5">
            {linkedProceedings.map(p => (
              <button
                key={p.meeting_id}
                onClick={() => navigate(`/proceedings/${p.meeting_id}`)}
                title={p.match_reasons?.join(" / ") || undefined}
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors"
              >
                {p.relevance === "amendment_debate" && (
                  <span className="size-1.5 rounded-full bg-orange-400 shrink-0" />
                )}
                <span>{p.date}</span>
                <span className="text-muted-foreground">{p.house}{p.committee ?? "本会議"}</span>
                <ArrowUpRight className="size-2.5 opacity-50" />
              </button>
            ))}
          </div>
          <p className="text-[10px] text-muted-foreground mt-1.5">
            <span className="inline-flex items-center gap-1"><span className="size-1.5 rounded-full bg-orange-400 inline-block" />改正審議</span>
            <span className="ml-2 opacity-60">その他は言及のみ</span>
          </p>
        </div>
      )}

      {linkedPubcomments.length > 0 && (
        <div className="border-b border-border bg-background px-6 py-3">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
            <MessageSquare className="size-3" />
            この法令に関するパブコメ ({linkedPubcomments.length})
          </div>
          <div className="flex flex-wrap gap-1.5">
            {linkedPubcomments.map(p => (
              <button
                key={p.case_id}
                onClick={() => navigate(`/pubcomment/${p.case_id}`)}
                title={p.match_reasons?.join(" / ") || undefined}
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors max-w-xs"
              >
                {p.relevance === "amendment_comment" && (
                  <span className="size-1.5 rounded-full bg-orange-400 shrink-0" />
                )}
                <span className="truncate">{p.title}</span>
                <span className="text-muted-foreground shrink-0">{p.ministry}</span>
                <ArrowUpRight className="size-2.5 opacity-50 shrink-0" />
              </button>
            ))}
          </div>
          <p className="text-[10px] text-muted-foreground mt-1.5">
            <span className="inline-flex items-center gap-1"><span className="size-1.5 rounded-full bg-orange-400 inline-block" />改正パブコメ</span>
            <span className="ml-2 opacity-60">その他は言及のみ</span>
          </p>
        </div>
      )}

      {linkedTsutatsu.length > 0 && (
        <div className="border-b border-border bg-background px-6 py-3">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
            <ScrollText className="size-3" />
            この法令の通達 ({linkedTsutatsu.length})
          </div>
          <div className="flex flex-wrap gap-1.5">
            {linkedTsutatsu.map(t => (
              <button
                key={t.tax}
                onClick={() => navigate(`/tsutatsu?tax=${t.tax}`)}
                title="国税庁 法令解釈通達"
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors max-w-xs"
              >
                <span className="truncate">{t.name}</span>
                <span className="text-muted-foreground shrink-0">{t.count}件</span>
                <ArrowUpRight className="size-2.5 opacity-50 shrink-0" />
              </button>
            ))}
          </div>
          <p className="text-[10px] text-muted-foreground mt-1.5">国税庁の法令解釈通達（本文の「法」はこの法令を指します）</p>
        </div>
      )}

      {kanpoEvents.length > 0 && (
        <div className="border-b border-border bg-background px-6 py-3">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
            <Newspaper className="size-3" />
            この法令の官報掲載 ({kanpoEvents.length})
          </div>
          <div className="flex flex-wrap gap-1.5">
            {kanpoEvents.map(e => (
              <a
                key={e.event_id}
                href={e.kanpo.pdf_url}
                target="_blank"
                rel="noreferrer"
                title={e.amending_law_title ?? undefined}
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors max-w-xs"
              >
                <span className="shrink-0">{e.promulgation_date}</span>
                {e.kanpo.amend_format === "shinkyu" && (
                  <span className="text-[10px] px-1 rounded bg-muted text-muted-foreground shrink-0">新旧対照表</span>
                )}
                <span className="truncate text-muted-foreground">{e.amending_law_title ?? "改正"}</span>
                <ExternalLink className="size-2.5 opacity-50 shrink-0" />
              </a>
            ))}
          </div>
          <p className="text-[10px] text-muted-foreground mt-1.5">官報PDF（改め文の出所）を開きます</p>
        </div>
      )}

      <Tabs defaultValue="content" className="flex-1 flex flex-col min-h-0">
        <div className="border-b border-border px-6">
          <TabsList className="h-11 bg-transparent p-0 gap-2">
            <TabsTrigger value="content" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none h-11">本文</TabsTrigger>
            <TabsTrigger value="timeline" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none h-11">改正履歴</TabsTrigger>
            <TabsTrigger value="versions" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none h-11">バージョン</TabsTrigger>
            <TabsTrigger value="meta" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none h-11">メタデータ</TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="content" className="flex-1 min-h-0 m-0">
          <div className="grid grid-cols-[240px_1fr] h-full">
            <ScrollArea className="border-r border-border">
              <div className="p-3 space-y-1">
                <div className="text-xs text-muted-foreground px-2 py-1.5">条文目次</div>
                {articles.map(a => (
                  <button
                    key={a.article_id}
                    onClick={() => setActiveArt(a.article_id)}
                    className={`w-full text-left px-2 py-1.5 rounded text-sm hover:bg-accent transition-colors ${activeArt === a.article_id ? "bg-accent" : ""}`}
                  >
                    <div>{a.article_no}</div>
                    {a.caption && <div className="text-xs text-muted-foreground truncate">{a.caption}</div>}
                  </button>
                ))}
              </div>
            </ScrollArea>
            <ScrollArea>
              <div className="max-w-3xl mx-auto px-8 py-8 space-y-8">
                {detail.loading && articles.length === 0 && (
                  <div className="text-sm text-muted-foreground py-12 text-center">本文を読み込み中…</div>
                )}
                {articlesEmpty && (
                  <Card>
                    <CardContent className="p-6 space-y-3">
                      <div className="text-sm">この法令は構造化された条文 (MainProvision/Article) を持たないため、本ビューでは表示できません。</div>
                      <div className="text-xs text-muted-foreground">
                        旧法 (太政官布告) や条文構造の特殊な法令で発生します。生 JSON または e-Gov 公式ページを参照してください。
                      </div>
                      <div className="flex gap-2">
                        <Button variant="outline" size="sm" asChild>
                          <a href={`./laws/${law.law_id}/current.json`} target="_blank" rel="noreferrer">
                            <Download className="size-4" /> 生 JSON
                          </a>
                        </Button>
                        <Button variant="outline" size="sm" asChild>
                          <a href={`https://laws.e-gov.go.jp/law/${law.law_id}`} target="_blank" rel="noreferrer">
                            <ExternalLink className="size-4" /> e-Gov で開く
                          </a>
                        </Button>
                      </div>
                    </CardContent>
                  </Card>
                )}
                {articles.map(a => {
                  const out = outgoingByArt.get(a.article_id) ?? [];
                  const inc = incomingByArt.get(a.article_id) ?? [];
                  return (
                    <article key={a.article_id} id={a.article_id} className="scroll-mt-4 transition-shadow">
                      <header className="mb-3">
                        <div className="flex items-baseline gap-3">
                          <h2 className="text-lg">{a.article_no}</h2>
                          {a.caption && <span className="text-sm text-muted-foreground">{a.caption}</span>}
                        </div>
                        {inc.length > 0 && (
                          <div className="mt-2 flex flex-wrap gap-1.5 text-xs">
                            <span className="text-muted-foreground">被参照:</span>
                            {inc.map((r, i) => (
                              <a
                                key={i}
                                href={`#${r.from_article_id}`}
                                onClick={(ev) => { ev.preventDefault(); scrollToArticle(r.from_article_id); }}
                                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded border border-border hover:bg-accent transition-colors"
                              >
                                <ArrowUpRight className="size-3" />
                                {articleNoById.get(r.from_article_id) ?? r.from_article_id}
                              </a>
                            ))}
                          </div>
                        )}
                      </header>
                      <div className="space-y-2 text-sm leading-relaxed">
                        {a.paragraphs.map(p => (
                          <p key={p.paragraph_no} className="flex gap-3">
                            <span className="text-muted-foreground tabular-nums shrink-0 w-6">{p.paragraph_no}</span>
                            <span>{linkifyText(p.text, out, navigate, law.law_id)}</span>
                          </p>
                        ))}
                      </div>
                    </article>
                  );
                })}
              </div>
            </ScrollArea>
          </div>
        </TabsContent>

        <TabsContent value="timeline" className="flex-1 min-h-0 m-0 p-6 overflow-auto">
          <div className="max-w-3xl">
            <div className="space-y-4">
              {useLiveTimeline ? (
                liveEvents.map((e, i) => {
                  // event_type → ラベルとカラー。
                  const typeLabel =
                    e.event_type === "enactment" ? "制定"
                    : e.event_type === "amendment" ? "改正"
                    : e.event_type === "repeal" ? "廃止"
                    : e.event_type === "snapshot" ? "snapshot"
                    : e.event_type;
                  // status → 表示テキスト + dot 色。e-Gov v2 由来。
                  const statusInfo =
                    e.status === "CurrentEnforced" ? { label: "現行", dot: "bg-emerald-500" }
                    : e.status === "PreviousEnforced" ? { label: "旧版", dot: "bg-zinc-400" }
                    : e.status === "UnEnforced" ? { label: "施行待ち", dot: "bg-amber-500" }
                    : e.status === "Repealed" ? { label: "廃止済", dot: "bg-red-500" }
                    : e.status === "Enacted" ? { label: "制定", dot: "bg-emerald-500" }
                    : { label: e.status || "snapshot", dot: "bg-emerald-500" };
                  return (
                  <div key={e.event_id} className="flex gap-4">
                    <div className="flex flex-col items-center">
                      <div className={`size-3 rounded-full ${statusInfo.dot} ring-4 ring-background`} />
                      {i < liveEvents.length - 1 && <div className="w-px flex-1 bg-border mt-1" />}
                    </div>
                    <Card className="flex-1 mb-2">
                      <CardContent className="p-4">
                        <div className="flex items-center justify-between mb-2">
                          <div className="flex items-center gap-2">
                            <Badge variant="secondary">{statusInfo.label}</Badge>
                            <span className="text-xs text-muted-foreground">{typeLabel}</span>
                            {e.mission && <span className="text-xs text-muted-foreground">{e.mission === "New" ? "新規/全部" : "一部"}</span>}
                          </div>
                          {e.kanpo?.linked && (
                            <Badge variant="outline" className="text-xs">
                              官報リンク済 (conf {e.kanpo.confidence?.toFixed(2)})
                            </Badge>
                          )}
                        </div>
                        {e.amending_law_title && (
                          <div className="text-sm">{e.amending_law_title}</div>
                        )}
                        {e.amending_law_num && (
                          <div className="text-xs text-muted-foreground mt-1">{e.amending_law_num}</div>
                        )}
                        <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground mt-2">
                          {e.promulgation_date && <span>公布 {e.promulgation_date}</span>}
                          {e.effective_date && <span>施行 {e.effective_date}</span>}
                          {!e.effective_date && e.scheduled_enforcement_date && (
                            <span>施行予定 {e.scheduled_enforcement_date}</span>
                          )}
                        </div>
                        {e.enforcement_comment && (
                          <div className="text-xs text-muted-foreground mt-1">{e.enforcement_comment}</div>
                        )}
                        {(e.kanpo?.amend_text || e.kanpo?.pdf_url) && (
                          <div className="mt-3 border-t pt-2">
                            <div className="flex items-center gap-2 mb-1">
                              <span className="text-xs font-medium">改め文</span>
                              {e.kanpo.amend_format && (
                                <Badge variant="secondary" className="text-xs">
                                  {e.kanpo.amend_format === "shinkyu" ? "新旧対照表" : e.kanpo.amend_format === "prose" ? "散文" : "本文"}
                                </Badge>
                              )}
                              {e.kanpo.pdf_url && (
                                <a
                                  className="text-xs underline text-muted-foreground ml-auto"
                                  href={e.kanpo.pdf_url}
                                  target="_blank"
                                  rel="noreferrer"
                                >
                                  官報PDF{e.kanpo.page ? `（p.${e.kanpo.page}）` : ""}
                                </a>
                              )}
                            </div>
                            {e.kanpo.amend_text && (() => {
                              const doc = e.kanpo.amend_document;
                              const shinkyu = !doc && e.kanpo.amend_format === "shinkyu" ? parseShinkyu(e.kanpo.amend_text) : null;
                              return (
                                <details>
                                  <summary className="text-xs text-muted-foreground cursor-pointer select-none">本文を表示</summary>
                                  {doc ? (
                                    <AmendDocumentView doc={doc} />
                                  ) : shinkyu ? (
                                    <div className="mt-1 max-h-96 overflow-auto rounded bg-muted/40 p-2 space-y-2">
                                      {shinkyu.preamble && (
                                        <pre className="text-xs whitespace-pre-wrap font-sans leading-relaxed">{shinkyu.preamble}</pre>
                                      )}
                                      <table className="w-full text-xs border-collapse table-fixed">
                                        <thead>
                                          <tr>
                                            <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正後</th>
                                            <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正前</th>
                                          </tr>
                                        </thead>
                                        <tbody>
                                          {shinkyu.rows.map((r, ri) => (
                                            <tr key={ri} className="align-top">
                                              <td className="border border-border px-2 py-1"><pre className="whitespace-pre-wrap font-sans leading-relaxed">{r.after}</pre></td>
                                              <td className="border border-border px-2 py-1"><pre className="whitespace-pre-wrap font-sans leading-relaxed">{r.before}</pre></td>
                                            </tr>
                                          ))}
                                        </tbody>
                                      </table>
                                    </div>
                                  ) : (
                                    <pre className="mt-1 text-xs whitespace-pre-wrap font-sans leading-relaxed max-h-80 overflow-auto rounded bg-muted/40 p-2">{e.kanpo.amend_text}</pre>
                                  )}
                                </details>
                              );
                            })()}
                          </div>
                        )}
                      </CardContent>
                    </Card>
                  </div>
                  );
                })
              ) : (
                <>
                  {mockEvents.length === 0 && <div className="text-sm text-muted-foreground">改正履歴はありません</div>}
                  {mockEvents.map((e, i) => (
                    <div key={e.event_id} className="flex gap-4">
                      <div className="flex flex-col items-center">
                        <div className={`size-3 rounded-full ${e.status === "effective" ? "bg-emerald-500" : "bg-amber-500"} ring-4 ring-background`} />
                        {i < mockEvents.length - 1 && <div className="w-px flex-1 bg-border mt-1" />}
                      </div>
                      <Card className="flex-1 mb-2">
                        <CardContent className="p-4">
                          <div className="flex items-center justify-between mb-2">
                            <div className="flex items-center gap-2">
                              <Badge variant={e.status === "effective" ? "secondary" : "default"}>
                                {e.status === "effective" ? "施行済" : "施行待ち"}
                              </Badge>
                              <span className="text-xs text-muted-foreground">{e.event_type}</span>
                            </div>
                            {e.kanpo_linked && <Badge variant="outline" className="text-xs">官報リンク済</Badge>}
                          </div>
                          <div className="text-sm">{e.summary}</div>
                          <div className="text-xs text-muted-foreground mt-2">{e.amending_law_num}</div>
                          <div className="flex gap-4 text-xs text-muted-foreground mt-2">
                            <span>公布 {e.promulgation_date}</span>
                            <span>施行 {e.effective_date}</span>
                          </div>
                        </CardContent>
                      </Card>
                    </div>
                  ))}
                </>
              )}
            </div>
          </div>
        </TabsContent>

        <TabsContent value="versions" className="flex-1 m-0 p-6 overflow-auto">
          {detail.versions ? (
            <div className="max-w-3xl space-y-2">
              <div className="text-xs text-muted-foreground">
                current: {detail.versions.current_revision_id ?? "(未指定)"}
              </div>
              {detail.versions.versions.map(v => (
                <Card key={v.revision_id}>
                  <CardContent className="p-4 flex items-center gap-4 text-sm">
                    <Badge variant={v.revision_id === detail.versions?.current_revision_id ? "default" : "outline"}>
                      {v.revision_id}
                    </Badge>
                    <div className="flex-1 grid grid-cols-3 gap-2 text-xs text-muted-foreground">
                      <span>公布 {v.promulgation_date ?? "—"}</span>
                      <span>施行 {v.effective_date ?? "—"}</span>
                      <span>取込 {v.source_update_date ?? "—"}</span>
                    </div>
                    <a className="text-xs underline" href={"./" + v.path} target="_blank" rel="noreferrer">JSON</a>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">バージョン情報を取得できませんでした</div>
          )}
        </TabsContent>
        <TabsContent value="meta" className="flex-1 m-0 p-6 overflow-auto">
          <pre className="text-xs bg-muted rounded-md p-4 overflow-auto max-w-3xl">
{JSON.stringify(detail.doc ?? { law_id: law.law_id, law_num: law.law_num, title: law.title }, null, 2)}
          </pre>
        </TabsContent>
      </Tabs>
    </div>
  );
}
