import { useEffect, useMemo, useState } from "react";
import { Badge } from "../ui/badge";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { ScrollArea } from "../ui/scroll-area";
import { Skeleton } from "../ui/skeleton";
import { Separator } from "../ui/separator";
import { MessageSquare, Search, BookOpen, ExternalLink } from "lucide-react";
import { useProceedingsIndex, useMeeting, type MeetingMeta } from "../../data/use-proceedings";
import { api, type MeetingToLaws } from "../../data/api";
import { useNavigate } from "react-router";

// ── ユーティリティ ────────────────────────────────────────────────

const HOUSE_LABEL: Record<string, string> = {
  参議院: "参",
  衆議院: "衆",
  両院: "両",
};

function houseColor(house: string) {
  if (house.includes("衆")) return "bg-blue-100 text-blue-800 dark:bg-blue-900/40 dark:text-blue-300";
  if (house.includes("参")) return "bg-purple-100 text-purple-800 dark:bg-purple-900/40 dark:text-purple-300";
  return "bg-muted text-muted-foreground";
}

function relevanceLabel(r: string) {
  if (r === "amendment_debate") return "改正審議";
  if (r === "reference_only") return "言及";
  return r;
}

// ── 一覧アイテム ──────────────────────────────────────────────────

function MeetingListItem({ meta, selected, onClick }: {
  meta: MeetingMeta;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={[
        "w-full text-left px-4 py-3 border-b border-border transition-colors",
        selected
          ? "bg-accent text-accent-foreground"
          : "hover:bg-accent/50",
      ].join(" ")}
    >
      <div className="flex items-start gap-2">
        <span className={["text-xs font-bold px-1.5 py-0.5 rounded shrink-0 mt-0.5", houseColor(meta.house)].join(" ")}>
          {HOUSE_LABEL[meta.house] ?? meta.house}
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium truncate">
            {meta.committee ?? "本会議"}
          </div>
          <div className="flex items-center gap-2 mt-0.5">
            <span className="text-xs text-muted-foreground">{meta.date}</span>
            <span className="text-xs text-muted-foreground">第{meta.session}回</span>
            <span className="text-xs text-muted-foreground flex items-center gap-0.5">
              <MessageSquare className="size-3" />{meta.speech_count}
            </span>
          </div>
        </div>
      </div>
    </button>
  );
}

// ── 発言ひとつ ────────────────────────────────────────────────────

function SpeechCard({ speech, query }: {
  speech: { speaker: string | null; speaker_group: string | null; speech: string; order: number };
  query: string;
}) {
  const highlighted = useMemo(() => {
    if (!query.trim()) return null;
    const q = query.trim();
    const idx = speech.speech.toLowerCase().indexOf(q.toLowerCase());
    if (idx < 0) return null;
    const start = Math.max(0, idx - 60);
    const end = Math.min(speech.speech.length, idx + q.length + 100);
    const before = (start > 0 ? "…" : "") + speech.speech.slice(start, idx);
    const match = speech.speech.slice(idx, idx + q.length);
    const after = speech.speech.slice(idx + q.length, end) + (end < speech.speech.length ? "…" : "");
    return { before, match, after };
  }, [speech.speech, query]);

  return (
    <div className="py-3 border-b border-border last:border-0">
      <div className="flex items-center gap-2 mb-1">
        <span className="text-xs font-semibold text-foreground">
          {speech.speaker ?? "（不明）"}
        </span>
        {speech.speaker_group && (
          <span className="text-xs text-muted-foreground">{speech.speaker_group}</span>
        )}
      </div>
      {highlighted ? (
        <p className="text-sm text-muted-foreground leading-relaxed">
          {highlighted.before}
          <mark className="bg-yellow-200 dark:bg-yellow-800 text-foreground rounded px-0.5">
            {highlighted.match}
          </mark>
          {highlighted.after}
        </p>
      ) : (
        <p className="text-sm text-muted-foreground leading-relaxed line-clamp-3">
          {speech.speech}
        </p>
      )}
    </div>
  );
}

// ── 会議詳細パネル ────────────────────────────────────────────────

function useLinkedLaws(meetingId: string) {
  const [data, setData] = useState<MeetingToLaws | null>(null);
  useEffect(() => {
    let cancelled = false;
    api.meetingToLaws(meetingId)
      .then(d => { if (!cancelled) setData(d); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [meetingId]);
  return data;
}

function MeetingDetail({ meetingId, query, onLawClick }: {
  meetingId: string;
  query: string;
  onLawClick: (lawId: string) => void;
}) {
  const { data, loading } = useMeeting(meetingId);
  const linkedLaws = useLinkedLaws(meetingId);
  const [speechQuery, setSpeechQuery] = useState("");
  const effectiveQuery = speechQuery || query;

  const filteredSpeeches = useMemo(() => {
    if (!data) return [];
    const q = effectiveQuery.trim().toLowerCase();
    if (!q) return data.speeches;
    return data.speeches.filter(s =>
      s.speech.toLowerCase().includes(q) ||
      (s.speaker ?? "").toLowerCase().includes(q)
    );
  }, [data, effectiveQuery]);

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
        <div className="flex items-center gap-2 mb-1">
          <span className={["text-xs font-bold px-1.5 py-0.5 rounded", houseColor(data.house)].join(" ")}>
            {data.house}
          </span>
          <span className="text-sm font-semibold">{data.committee ?? "本会議"}</span>
        </div>
        <div className="text-xs text-muted-foreground">
          第{data.session}回国会 · {data.date}
          {data.issue && ` · ${data.issue}`}
        </div>
        {/* 発言内検索 */}
        <div className="relative mt-3">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input
            value={speechQuery}
            onChange={e => setSpeechQuery(e.target.value)}
            placeholder="発言を検索…"
            className="pl-8 h-8 text-sm"
          />
        </div>
        <div className="text-xs text-muted-foreground mt-1.5">
          {filteredSpeeches.length} / {data.speeches.length} 発言
        </div>
      </div>

      {/* 関連法令 */}
      {linkedLaws && linkedLaws.linked_laws.length > 0 && (
        <div className="px-5 py-3 border-b border-border shrink-0">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
            <BookOpen className="size-3" />
            関連法令 ({linkedLaws.linked_laws.length})
          </div>
          <div className="flex flex-wrap gap-1.5">
            {linkedLaws.linked_laws.map(l => (
              <button
                key={l.law_id}
                onClick={() => onLawClick(l.law_id)}
                className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-border hover:border-primary hover:text-primary transition-colors"
              >
                {l.relevance === "amendment_debate" && (
                  <span className="size-1.5 rounded-full bg-orange-400 shrink-0" />
                )}
                {l.title}
                <ExternalLink className="size-2.5 opacity-50" />
              </button>
            ))}
          </div>
          <p className="text-[10px] text-muted-foreground mt-1.5">
            <span className="inline-flex items-center gap-1"><span className="size-1.5 rounded-full bg-orange-400 inline-block" />改正審議</span>
            <span className="ml-2 opacity-60">その他は言及のみ</span>
          </p>
        </div>
      )}

      {/* 発言リスト */}
      <ScrollArea className="flex-1">
        <div className="px-5">
          {filteredSpeeches.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">該当する発言がありません</p>
          ) : (
            filteredSpeeches.map(s => (
              <SpeechCard key={s.speech_id} speech={s} query={effectiveQuery} />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

// ── メインビュー ──────────────────────────────────────────────────

export function ProceedingsView({
  meetingId,
  onSelectMeeting,
}: {
  meetingId: string | null;
  onSelectMeeting: (id: string | null) => void;
}) {
  const { data, loading } = useProceedingsIndex();
  const navigate = useNavigate();

  const [query, setQuery] = useState("");
  const [houseFilter, setHouseFilter] = useState<string>("all");
  const [sessionFilter, setSessionFilter] = useState<string>("all");

  const sessions = useMemo(() => {
    if (!data) return [];
    const set = new Set(data.meetings.map(m => m.session));
    return [...set].sort((a, b) => b - a);
  }, [data]);

  const houses = useMemo(() => {
    if (!data) return [];
    const set = new Set(data.meetings.map(m => m.house));
    return [...set].sort();
  }, [data]);

  const filtered = useMemo(() => {
    if (!data) return [];
    const q = query.trim().toLowerCase();
    return data.meetings.filter(m => {
      if (houseFilter !== "all" && m.house !== houseFilter) return false;
      if (sessionFilter !== "all" && String(m.session) !== sessionFilter) return false;
      if (q) {
        return (m.committee ?? "").toLowerCase().includes(q) ||
          m.date.includes(q) ||
          String(m.session).includes(q);
      }
      return true;
    });
  }, [data, query, houseFilter, sessionFilter]);

  return (
    <div className="flex h-full">
      {/* 左: 一覧 */}
      <div className="w-80 shrink-0 border-r border-border flex flex-col">
        <div className="px-4 py-3 border-b border-border shrink-0 space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold flex-1">国会会議録</h2>
            {data && (
              <span className="text-xs text-muted-foreground">{filtered.length}件</span>
            )}
          </div>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
            <Input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="委員会・日付…"
              className="pl-8 h-8 text-sm"
            />
          </div>
          <div className="flex gap-1.5">
            <Select value={houseFilter} onValueChange={setHouseFilter}>
              <SelectTrigger className="h-7 text-xs flex-1">
                <SelectValue placeholder="院" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全院</SelectItem>
                {houses.map(h => (
                  <SelectItem key={h} value={h}>{h}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={sessionFilter} onValueChange={setSessionFilter}>
              <SelectTrigger className="h-7 text-xs flex-1">
                <SelectValue placeholder="会期" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全会期</SelectItem>
                {sessions.map(s => (
                  <SelectItem key={s} value={String(s)}>第{s}回</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <ScrollArea className="flex-1">
          {loading ? (
            <div className="p-4 space-y-2">
              {[...Array(8)].map((_, i) => <Skeleton key={i} className="h-14 w-full" />)}
            </div>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">
              {data ? "該当する会議がありません" : "データがありません"}
            </p>
          ) : (
            filtered.map(m => (
              <MeetingListItem
                key={m.meeting_id}
                meta={m}
                selected={m.meeting_id === meetingId}
                onClick={() => onSelectMeeting(m.meeting_id)}
              />
            ))
          )}
        </ScrollArea>
      </div>

      {/* 右: 詳細 */}
      <div className="flex-1 flex flex-col min-w-0">
        {meetingId ? (
          <MeetingDetail
            meetingId={meetingId}
            query={query}
            onLawClick={id => navigate(`/laws/${id}`)}
          />
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3">
            <MessageSquare className="size-10 opacity-30" />
            <p className="text-sm">会議を選択すると発言が表示されます</p>
          </div>
        )}
      </div>
    </div>
  );
}
