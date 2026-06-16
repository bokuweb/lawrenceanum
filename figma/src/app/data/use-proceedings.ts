import { useEffect, useState } from "react";
import { api, type MeetingMeta, type Meeting, type ProceedingsIndex } from "./api";

export function useProceedingsIndex() {
  const [data, setData] = useState<ProceedingsIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.proceedingsIndex()
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(e => { if (!cancelled) { setError(String(e)); setLoading(false); } });
    return () => { cancelled = true; };
  }, []);

  return { data, loading, error };
}

export function useMeeting(meetingId: string | null) {
  const [data, setData] = useState<Meeting | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!meetingId) { setData(null); return; }
    let cancelled = false;
    setLoading(true);
    api.meeting(meetingId)
      .then(d => { if (!cancelled) { setData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [meetingId]);

  return { data, loading };
}

export type { MeetingMeta };
