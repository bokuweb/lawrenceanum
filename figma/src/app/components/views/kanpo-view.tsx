// 官報の一覧と項目の表示。左に日付を選んでその日の号・項目（目次）を並べ、右に選んだ項目を出す。
// 本文は標題を問わず全項目から抽出しており、改め文（新旧対照表を含む）は HTML で組む。
// 本文が取れなかった項目は標題と官報 PDF への導線だけを出す。
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { BookOpen, ExternalLink, Newspaper, Scale, Search } from "lucide-react";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { AmendDocumentView } from "../amend-document-view";
import type { KanpoItem, KanpoLawRef } from "../../data/api";
import { findKanpoItem, kanpoIssueLabel, useKanpoDates, useKanpoDay } from "../../data/use-kanpo";

/** 選択中の項目（その日の何号目の何項目目か）。 */
export type KanpoItemRef = { issue: number; item: number };
/** 検索結果からの遷移で、並び順の代わりに号・頁・標題で項目を指すとき。 */
export type KanpoItemLookup = { issueNo: string | null; page: number | null; title: string };

function formatLabel(format: string | undefined): string | null {
  if (format === "shinkyu") return "新旧対照表";
  if (format === "prose") return "改め文";
  return null;
}

function ItemListRow({ item, selected, onClick }: { item: KanpoItem; selected: boolean; onClick: () => void }) {
  const law = item.linked_laws?.[0];
  const format = formatLabel(item.amend_format);
  return (
    <button
      onClick={onClick}
      aria-current={selected ? "true" : undefined}
      className={[
        "w-full text-left px-4 py-2.5 border-b border-border transition-colors",
        selected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
      ].join(" ")}
    >
      <div className="text-sm line-clamp-2">{item.title}</div>
      <div className="flex items-center gap-2 mt-1 flex-wrap text-xs text-muted-foreground">
        <span className="tabular-nums">{item.page}頁</span>
        {/* 本文は全項目から取るので「本文あり」は出さず、改め文の形式だけを示す。 */}
        {format && <span className="px-1.5 py-0.5 rounded bg-muted">{format}</span>}
        {law?.title && (
          <span className="flex items-center gap-0.5 min-w-0">
            <BookOpen className="size-3 shrink-0" />
            <span className="truncate max-w-[12rem]">{law.title}</span>
          </span>
        )}
      </div>
    </button>
  );
}

function LawChips({ label, laws, onOpen }: { label: string; laws: KanpoLawRef[]; onOpen: (lawId: string) => void }) {
  return (
    <div className="flex items-start gap-2 flex-wrap">
      <span className="text-xs text-muted-foreground mt-1 shrink-0">{label}</span>
      {laws.map(l => (
        <button
          key={l.law_id}
          onClick={() => onOpen(l.law_id)}
          className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors max-w-full"
        >
          <BookOpen className="size-3 shrink-0" />
          <span className="truncate">{l.title || l.law_id}</span>
        </button>
      ))}
    </div>
  );
}

function ItemDetail({ date, issueLabel, item }: { date: string; issueLabel: string; item: KanpoItem }) {
  const navigate = useNavigate();
  const format = formatLabel(item.amend_format);
  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="px-5 py-4 border-b border-border shrink-0 space-y-2.5">
        <h2 className="text-base font-semibold leading-snug">{item.title}</h2>
        <div className="text-xs text-muted-foreground flex flex-wrap gap-x-3 gap-y-1 items-center">
          <span className="tabular-nums">{date}</span>
          <span>{issueLabel}</span>
          <span className="tabular-nums">{item.page}頁</span>
          {item.agency_hint && <span>{item.agency_hint}</span>}
          {format && <span className="px-1.5 py-0.5 rounded bg-muted">{format}</span>}
          {item.pdf_url && (
            <a
              href={item.pdf_url}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 underline hover:text-primary"
            >
              <ExternalLink className="size-3" />官報PDF（{item.page}頁）
            </a>
          )}
        </div>
        {!!item.linked_laws?.length && (
          <LawChips label="改正対象" laws={item.linked_laws} onOpen={id => navigate(`/laws/${id}`)} />
        )}
        {!!item.basis_laws?.length && (
          <LawChips label="根拠法令" laws={item.basis_laws} onOpen={id => navigate(`/laws/${id}`)} />
        )}
      </div>
      <ScrollArea className="flex-1 min-h-0">
        <div className="px-5 py-4">
          {item.amend_document ? (
            // 既定の max-h-96（法令閲覧の改正履歴に埋め込む用）を外し、右の欄の高さいっぱいに使う。
            // スクロールは右の欄の ScrollArea だけにする（入れ子のスクロールを作らない）。
            <AmendDocumentView doc={item.amend_document} className="" />
          ) : item.amend_text ? (
            <pre className="text-sm whitespace-pre-wrap font-sans leading-relaxed">{item.amend_text}</pre>
          ) : (
            <div className="text-sm text-muted-foreground space-y-1.5">
              <p>この項目の本文は取り出せませんでした。</p>
              <p className="text-xs">
                内容は官報 PDF で確認してください（官報の公式サイトでの無料閲覧は掲載から 90 日間です）。
              </p>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

export function KanpoView({
  date,
  selected,
  lookup,
  onSelect,
}: {
  date: string | null;
  selected: KanpoItemRef | null;
  /** 検索結果からの遷移。日付の中身を読んだら selected に置き換える（onSelect を replace で呼ぶ）。 */
  lookup: KanpoItemLookup | null;
  onSelect: (date: string, item: KanpoItemRef | null, opts?: { replace?: boolean }) => void;
}) {
  const { data: dates, loading: datesLoading } = useKanpoDates();
  const activeDate = date ?? dates?.dates[0]?.date ?? null;
  const { data: day, loading: dayLoading } = useKanpoDay(activeDate);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (!lookup || !day || !activeDate || day.date !== activeDate) return;
    onSelect(activeDate, findKanpoItem(day, lookup), { replace: true });
  }, [lookup, day, activeDate, onSelect]);

  const groups = useMemo(() => {
    if (!day) return [];
    const q = query.trim().toLowerCase();
    return day.issues
      .map((issue, i) => ({
        issue,
        index: i,
        items: issue.items
          .map((item, j) => ({ item, index: j }))
          .filter(({ item }) => !q
            || item.title.toLowerCase().includes(q)
            || (item.agency_hint ?? "").toLowerCase().includes(q)
            || (item.linked_laws ?? []).some(l => l.title.toLowerCase().includes(q))),
      }))
      .filter(g => g.items.length > 0);
  }, [day, query]);
  const shown = groups.reduce((n, g) => n + g.items.length, 0);

  const current = day && selected ? day.issues[selected.issue]?.items[selected.item] : undefined;
  const currentIssue = day && selected ? day.issues[selected.issue] : undefined;

  // 一覧に無い日付（URL で直に指定した古い日付など）も選べるように残す。
  const dateOptions = useMemo(() => {
    const list = dates?.dates.map(d => d.date) ?? [];
    return activeDate && !list.includes(activeDate) ? [activeDate, ...list] : list;
  }, [dates, activeDate]);

  return (
    <div className="flex h-full min-h-0">
      <div className="w-96 shrink-0 border-r border-border flex flex-col min-h-0">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1 flex items-center gap-1.5"><Newspaper className="size-4" />官報</h2>
            {day && <span role="status" aria-label="表示中の項目数" className="text-xs text-muted-foreground">{shown}件</span>}
          </div>
          <Select
            value={activeDate ?? undefined}
            onValueChange={d => onSelect(d, null)}
            disabled={dateOptions.length === 0}
          >
            <SelectTrigger className="h-8 text-sm" aria-label="発行日">
              <SelectValue placeholder={datesLoading ? "読み込み中…" : "発行日"} />
            </SelectTrigger>
            <SelectContent>
              {dateOptions.map(d => {
                const meta = dates?.dates.find(x => x.date === d);
                return (
                  <SelectItem key={d} value={d}>
                    <span className="tabular-nums">{d}</span>
                    {meta && <span className="text-muted-foreground ml-2">{meta.items}件</span>}
                  </SelectItem>
                );
              })}
            </SelectContent>
          </Select>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="標題・府省・法令名で絞り込み…"
              aria-label="この日の項目を絞り込む"
              className="pl-8 h-8 text-sm"
            />
          </div>
        </div>

        <ScrollArea className="flex-1 min-h-0">
          {datesLoading || dayLoading ? (
            <div className="p-4 space-y-2">
              {[...Array(8)].map((_, i) => <Skeleton key={i} className="h-12 w-full" />)}
            </div>
          ) : !activeDate || !day ? (
            <p className="p-6 text-center text-sm text-muted-foreground">官報のデータがありません</p>
          ) : groups.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">該当する項目がありません</p>
          ) : (
            groups.map(g => (
              <section key={g.index} aria-label={kanpoIssueLabel(g.issue)}>
                <div className="px-4 py-1.5 text-xs font-medium bg-muted border-b border-border sticky top-0 z-10 flex items-center gap-2">
                  <span>{kanpoIssueLabel(g.issue)}</span>
                  {g.issue.page_count ? <span className="text-muted-foreground tabular-nums">{g.issue.page_count}頁</span> : null}
                </div>
                {g.items.map(({ item, index }) => (
                  <ItemListRow
                    key={index}
                    item={item}
                    selected={selected?.issue === g.index && selected?.item === index}
                    onClick={() => onSelect(activeDate, { issue: g.index, item: index })}
                  />
                ))}
              </section>
            ))
          )}
        </ScrollArea>
      </div>

      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        {current && currentIssue && activeDate ? (
          <ItemDetail date={activeDate} issueLabel={kanpoIssueLabel(currentIssue)} item={current} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <Scale className="size-10 opacity-30" />
            <p className="text-sm">項目を選択すると内容が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
