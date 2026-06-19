import { useMemo, useState } from "react";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { MessageSquare, Search, BookOpen, ExternalLink, FileDown } from "lucide-react";
import { usePubcommentIndex, usePubcommentCase, type PubcommentCaseMeta } from "../../data/use-pubcomment";
import { useNavigate } from "react-router";

// ── 一覧アイテム ──────────────────────────────────────────────────

function CaseListItem({ meta, selected, onClick }: {
  meta: PubcommentCaseMeta;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={[
        "w-full text-left px-4 py-3 border-b border-border transition-colors",
        selected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
      ].join(" ")}
    >
      <div className="text-sm font-medium line-clamp-2">{meta.title}</div>
      <div className="flex items-center gap-2 mt-1 flex-wrap">
        {meta.ministry && (
          <span className="text-xs text-muted-foreground">{meta.ministry}</span>
        )}
        {meta.result_published && (
          <span className="text-xs text-muted-foreground">公示 {meta.result_published}</span>
        )}
        {meta.related_law_name && (
          <span className="text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground flex items-center gap-0.5">
            <BookOpen className="size-3" />{meta.related_law_name}
          </span>
        )}
      </div>
    </button>
  );
}

// ── 意見ひとつ（意見 / 府省の考え方）───────────────────────────────

function OpinionCard({ opinion, query }: {
  opinion: { item: string; opinion: string; ministry_response: string };
  query: string;
}) {
  const hit = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      opinion.item.toLowerCase().includes(q) ||
      opinion.opinion.toLowerCase().includes(q) ||
      opinion.ministry_response.toLowerCase().includes(q)
    );
  }, [opinion, query]);
  if (!hit) return null;

  return (
    <div className="py-3 border-b border-border last:border-0">
      {opinion.item && (
        <div className="text-xs font-semibold text-foreground mb-1.5">{opinion.item}</div>
      )}
      <div className="grid gap-2 md:grid-cols-2">
        <div className="rounded-md bg-muted/40 p-2.5">
          <div className="text-[10px] font-semibold text-muted-foreground mb-1">寄せられた意見</div>
          <p className="text-sm text-foreground/90 leading-relaxed whitespace-pre-wrap">{opinion.opinion}</p>
        </div>
        <div className="rounded-md bg-primary/5 p-2.5">
          <div className="text-[10px] font-semibold text-muted-foreground mb-1">府省の考え方</div>
          <p className="text-sm text-foreground/90 leading-relaxed whitespace-pre-wrap">{opinion.ministry_response}</p>
        </div>
      </div>
    </div>
  );
}

// ── 案件詳細パネル ────────────────────────────────────────────────

function CaseDetail({ caseId, onLawClick }: {
  caseId: string;
  onLawClick: (lawName: string) => void;
}) {
  const { data, loading } = usePubcommentCase(caseId);
  const [opinionQuery, setOpinionQuery] = useState("");

  const filtered = useMemo(() => {
    if (!data) return [];
    const q = opinionQuery.trim().toLowerCase();
    if (!q) return data.opinions;
    return data.opinions.filter(o =>
      o.item.toLowerCase().includes(q) ||
      o.opinion.toLowerCase().includes(q) ||
      o.ministry_response.toLowerCase().includes(q)
    );
  }, [data, opinionQuery]);

  if (loading) {
    return (
      <div className="p-6 space-y-3">
        {[...Array(5)].map((_, i) => <Skeleton key={i} className="h-16 w-full" />)}
      </div>
    );
  }
  if (!data) return <div className="p-6 text-sm text-muted-foreground">読み込めませんでした</div>;

  return (
    <div className="flex flex-col h-full">
      {/* ヘッダー */}
      <div className="px-5 py-4 border-b border-border shrink-0">
        <h2 className="text-base font-semibold leading-snug">{data.title}</h2>
        <div className="text-xs text-muted-foreground mt-1.5 flex flex-wrap gap-x-3 gap-y-1 items-center">
          {data.category && (
            <span className="px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{data.category}</span>
          )}
          {data.ministry && <span>{data.ministry}</span>}
          {data.reception_start && data.reception_end && (
            <span>募集 {data.reception_start} 〜 {data.reception_end}</span>
          )}
          {data.result_published && <span>結果公示 {data.result_published}</span>}
          {typeof data.opinion_count === "number" && data.opinion_count > 0 && (
            <span>提出意見 {data.opinion_count}件</span>
          )}
        </div>

        {/* 関連法令 */}
        {data.related_law_name && (
          <div className="mt-2.5">
            <button
              onClick={() => onLawClick(data.related_law_name!)}
              title={data.legal_basis ?? undefined}
              className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors max-w-full"
            >
              <BookOpen className="size-3 shrink-0" /><span className="truncate">{data.related_law_name}</span>
              <ExternalLink className="size-2.5 opacity-50 shrink-0" />
            </button>
          </div>
        )}

        {/* 添付ファイル（意見と府省の考え方の本文 PDF など） */}
        {data.attachments && data.attachments.length > 0 && (
          <div className="mt-2.5 flex flex-wrap gap-1.5">
            {data.attachments.map((a, i) => (
              <a
                key={i}
                href={a.url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors"
              >
                <FileDown className="size-3 shrink-0" />{a.name}
              </a>
            ))}
          </div>
        )}

        {/* 意見内検索 */}
        <div className="relative mt-3">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input
            value={opinionQuery}
            onChange={e => setOpinionQuery(e.target.value)}
            placeholder="意見・考え方を検索…"
            className="pl-8 h-8 text-sm"
          />
        </div>
        <div className="text-xs text-muted-foreground mt-1.5 flex items-center gap-3">
          <span>{filtered.length} / {data.opinions.length} 件の意見</span>
          {data.source?.detail_url && (
            <a
              href={data.source.detail_url}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-0.5 hover:text-primary"
            >
              e-Gov 原文 <ExternalLink className="size-2.5" />
            </a>
          )}
        </div>
      </div>

      {/* 意見リスト */}
      <ScrollArea className="flex-1">
        <div className="px-5">
          {filtered.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              {data.opinions.length === 0
                ? (data.attachments && data.attachments.length > 0
                    ? "意見と府省の考え方は上部の添付ファイル（PDF）で公開されています"
                    : "意見の概要が公開されていません")
                : "該当する意見がありません"}
            </p>
          ) : (
            filtered.map((o, i) => (
              <OpinionCard key={i} opinion={o} query={opinionQuery} />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

// ── メインビュー ──────────────────────────────────────────────────

export function PubcommentView({
  caseId,
  onSelectCase,
}: {
  caseId: string | null;
  onSelectCase: (id: string | null) => void;
}) {
  const { data, loading } = usePubcommentIndex();
  const navigate = useNavigate();

  const [query, setQuery] = useState("");
  const [ministryFilter, setMinistryFilter] = useState<string>("all");

  const ministries = useMemo(() => {
    if (!data) return [];
    const set = new Set(data.cases.map(c => c.ministry).filter((m): m is string => !!m));
    return [...set].sort();
  }, [data]);

  const filtered = useMemo(() => {
    if (!data) return [];
    const q = query.trim().toLowerCase();
    return data.cases.filter(c => {
      if (ministryFilter !== "all" && c.ministry !== ministryFilter) return false;
      if (q) {
        return (
          c.title.toLowerCase().includes(q) ||
          (c.ministry ?? "").toLowerCase().includes(q) ||
          (c.related_law_name ?? "").toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [data, query, ministryFilter]);

  return (
    <div className="flex h-full">
      {/* 左: 一覧 */}
      <div className="w-80 shrink-0 border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1">パブリックコメント</h2>
            {data && <span className="text-xs text-muted-foreground">{filtered.length}件</span>}
          </div>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="案件名・法令名…"
              className="pl-8 h-8 text-sm"
            />
          </div>
          <Select value={ministryFilter} onValueChange={setMinistryFilter}>
            <SelectTrigger className="h-7 text-xs">
              <SelectValue placeholder="府省" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全府省</SelectItem>
              {ministries.map(m => (
                <SelectItem key={m} value={m}>{m}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <ScrollArea className="flex-1">
          {loading ? (
            <div className="p-4 space-y-2">
              {[...Array(8)].map((_, i) => <Skeleton key={i} className="h-16 w-full" />)}
            </div>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">
              {data ? "該当する案件がありません" : "データがありません"}
            </p>
          ) : (
            filtered.map(c => (
              <CaseListItem
                key={c.case_id}
                meta={c}
                selected={c.case_id === caseId}
                onClick={() => onSelectCase(c.case_id)}
              />
            ))
          )}
        </ScrollArea>
      </div>

      {/* 右: 詳細 */}
      <div className="flex-1 flex flex-col min-w-0">
        {caseId ? (
          <CaseDetail
            caseId={caseId}
            onLawClick={name => navigate(`/search?q=${encodeURIComponent(name)}`)}
          />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <MessageSquare className="size-10 opacity-30" />
            <p className="text-sm">案件を選択すると意見と府省の考え方が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
