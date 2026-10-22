import { useEffect, useState } from "react";
import { api, type MeetingMeta, type Meeting, type ProceedingsIndex } from "./api";

export function useProceedingsIndex() {
  const [data, setData] = useState<ProceedingsIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    api.proceedingsIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(e => { if (!cancelled) { setError(String(e)); setLoading(false); } });
    return () => { cancelled = true; };
  }, [attempt]);

  return { data, loading, error, retry: () => setAttempt(n => n + 1) };
}

export function useMeeting(meetingId: string | null) {
  const [data, setData] = useState<Meeting | null>(null);
  const [loading, setLoading] = useState(Boolean(meetingId));
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!meetingId) { setData(null); setLoading(false); return; }
    let cancelled = false;
    setLoading(true);
    setData(null);
    api.meeting(meetingId)
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [meetingId, attempt]);

  return { data, loading, retry: () => setAttempt(n => n + 1) };
}

export type { MeetingMeta };
