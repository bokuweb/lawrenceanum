import { useEffect, useMemo, useState } from "react";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Users, Search, ExternalLink, FileText, FileDown } from "lucide-react";
import { api, type ShingikaiIndex, type ShingikaiMeeting, type ShingikaiAttachment } from "../../data/api";

const MINISTRY_LABELS: Record<string, string> = {
  moj: "法務省",
  cao: "内閣府",
  mlit: "国土交通省",
  mhlw: "厚生労働省",
};

const ministryLabel = (m: string) => MINISTRY_LABELS[m] ?? m;

// 議事録本文は数万字になるため、既定では先頭だけ表示する。
const MINUTES_PREVIEW_CHARS = 3000;

function useShingikaiIndex() {
  const [data, setData] = useState<ShingikaiIndex | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    api.shingikaiIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);
  return { data, loading };
}

function StatusBadge({ status }: { status: string }) {
  if (status !== "scheduled") return null;
  return (
    <span className="text-xs px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300 font-medium shrink-0">開催予定</span>
  );
}

function AttachmentRow({ a }: { a: ShingikaiAttachment }) {
  const Icon = a.kind === "material" ? FileDown : FileText;
  return (
    <a href={a.source_url} target="_blank" rel="noreferrer"
      className="flex items-center gap-2 px-3 py-2 rounded-md border border-border hover:border-primary hover:text-primary transition-colors text-sm">
      <Icon className="size-3.5 shrink-0" />
      <span className="truncate flex-1">{a.label || a.attachment_id}</span>
      {a.kind !== "material" && <span className="text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0">議事録</span>}
      {typeof a.bytes === "number" && <span className="text-xs text-muted-foreground tabular-nums shrink-0">{Math.max(1, Math.round(a.bytes / 1024)).toLocaleString()} KB</span>}
    </a>
  );
}

function MeetingDetail({ ministry, minutesId }: { ministry: string; minutesId: string }) {
  const [meeting, setMeeting] = useState<ShingikaiMeeting | null>(null);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setMeeting(null); setExpanded(false);
    api.shingikaiMeeting(ministry, minutesId)
      .then(d => { if (!cancelled) { setMeeting(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [ministry, minutesId]);

  if (loading) return <div className="p-6 space-y-3">{[...Array(6)].map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (!meeting) return <div className="p-6 text-sm text-muted-foreground">読み込めませんでした</div>;

  const minutes = meeting.minutes_text?.trim() || "";
  const truncated = !expanded && minutes.length > MINUTES_PREVIEW_CHARS;
  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="px-5 py-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2 mb-1 flex-wrap">
          <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{ministryLabel(meeting.ministry)}</span>
          <span className="text-xs text-muted-foreground">{meeting.committee}</span>
          <StatusBadge status={meeting.status} />
        </div>
        <h2 className="text-base font-semibold leading-snug">{meeting.title}</h2>
        {meeting.date && <div className="text-xs text-muted-foreground mt-1.5 tabular-nums">開催日 {meeting.date}</div>}
        {meeting.source?.detail_url && (
          <a href={meeting.source.detail_url} target="_blank" rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs mt-2 px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors">
            {ministryLabel(meeting.ministry)} 会議ページ <ExternalLink className="size-2.5" />
          </a>
        )}
      </div>
      <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
        <div className="px-5 py-4 space-y-5">
          {meeting.agenda && (
            <section>
              <h3 className="text-xs font-semibold text-muted-foreground mb-1.5">議題</h3>
              <p className="text-sm whitespace-pre-wrap leading-relaxed">{meeting.agenda}</p>
            </section>
          )}
          {meeting.summary && (
            <section>
              <h3 className="text-xs font-semibold text-muted-foreground mb-1.5">議事概要</h3>
              <p className="text-sm whitespace-pre-wrap leading-relaxed">{meeting.summary}</p>
            </section>
          )}
          {meeting.attachments.length > 0 && (
            <section>
              <h3 className="text-xs font-semibold text-muted-foreground mb-1.5">議事録・配布資料（{meeting.attachments.length}）</h3>
              <div className="space-y-1.5">
                {meeting.attachments.map(a => <AttachmentRow key={a.attachment_id} a={a} />)}
              </div>
            </section>
          )}
          <section>
            <h3 className="text-xs font-semibold text-muted-foreground mb-1.5">議事録本文</h3>
            {minutes ? (
              <>
                <p className="text-sm whitespace-pre-wrap leading-relaxed">
                  {truncated ? minutes.slice(0, MINUTES_PREVIEW_CHARS) + "…" : minutes}
                </p>
                {truncated && (
                  <button onClick={() => setExpanded(true)} className="mt-2 text-xs text-primary hover:underline">
                    全文を表示（{minutes.length.toLocaleString()} 字）
                  </button>
                )}
              </>
            ) : (
              <p className="text-sm text-muted-foreground">
                {meeting.status === "scheduled" ? "開催前の会議です" : "議事録はまだ公開されていません"}
              </p>
            )}
          </section>
        </div>
      </ScrollArea>
    </div>
  );
}

export function ShingikaiView({ meetingRef, onSelect }: {
  meetingRef: { ministry: string; minutesId: string } | null;
  onSelect: (ministry: string, minutesId: string) => void;
}) {
  const { data, loading } = useShingikaiIndex();
  const [query, setQuery] = useState("");
  const [ministryFilter, setMinistryFilter] = useState("all");

  const ministries = useMemo(
    () => [...new Set((data?.minutes ?? []).map(m => m.ministry))].sort(),
    [data],
  );

  const filtered = useMemo(() => {
    const q = query.trim();
    return (data?.minutes ?? []).filter(m => {
      if (ministryFilter !== "all" && m.ministry !== ministryFilter) return false;
      if (q) return m.title.includes(q) || m.committee.includes(q);
      return true;
    });
  }, [data, query, ministryFilter]);

  return (
    <div className="flex h-full">
      <div className="w-96 shrink-0 border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1">審議会議事録</h2>
            {data && <span className="text-xs text-muted-foreground">{filtered.length}会議</span>}
          </div>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input value={query} onChange={e => setQuery(e.target.value)} placeholder="会議名・審議会…" className="pl-8 h-8 text-sm" />
          </div>
          <Select value={ministryFilter} onValueChange={setMinistryFilter}>
            <SelectTrigger className="h-7 text-xs"><SelectValue placeholder="府省" /></SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全府省</SelectItem>
              {ministries.map(m => <SelectItem key={m} value={m}>{ministryLabel(m)}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <ScrollArea className="flex-1 min-h-0 [&>[data-slot=scroll-area-viewport]>div]:!block">
          {loading ? (
            <div className="p-4 space-y-2">{[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}</div>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">{data ? "該当する会議がありません" : "データがありません"}</p>
          ) : (
            filtered.map(m => {
              const selected = meetingRef?.ministry === m.ministry && meetingRef?.minutesId === m.minutes_id;
              return (
                <button key={`${m.ministry}/${m.minutes_id}`} onClick={() => onSelect(m.ministry, m.minutes_id)}
                  className={["w-full text-left px-4 py-3 border-b border-border transition-colors", selected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"].join(" ")}>
                  <div className="flex items-center gap-2 mb-0.5">
                    <span className="text-xs font-bold px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0">{ministryLabel(m.ministry)}</span>
                    {m.date && <span className="text-xs text-muted-foreground tabular-nums">{m.date}</span>}
                    <StatusBadge status={m.status} />
                  </div>
                  <div className="text-sm font-medium line-clamp-2 [overflow-wrap:anywhere]">{m.title}</div>
                  <div className="flex items-center gap-2 mt-0.5 flex-wrap">
                    <span className="text-xs text-muted-foreground truncate">{m.committee}</span>
                    {m.has_minutes && <span className="text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground">議事録あり</span>}
                    {m.attachment_count > 0 && <span className="text-xs text-muted-foreground">資料 {m.attachment_count}</span>}
                  </div>
                </button>
              );
            })
          )}
        </ScrollArea>
      </div>
      <div className="flex-1 flex flex-col min-w-0">
        {meetingRef ? (
          <MeetingDetail ministry={meetingRef.ministry} minutesId={meetingRef.minutesId} />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <Users className="size-10 opacity-30" />
            <p className="text-sm">会議を選択すると議題・議事録が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
