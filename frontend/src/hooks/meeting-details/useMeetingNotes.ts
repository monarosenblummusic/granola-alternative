import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { getMeetingNotes, saveMeetingNotes } from '@/lib/meetingNotes';

export type NotesSaveState = 'idle' | 'saving' | 'saved' | 'error';

const SAVE_DELAY_MS = 800;

/**
 * Fork: a meeting's notepad text with autosave. `flush` writes pending edits
 * immediately; Enhance calls it so the summary sees the latest notes.
 */
export function useMeetingNotes(meetingId: string) {
  const [notes, setNotesState] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [saveState, setSaveState] = useState<NotesSaveState>('idle');
  const latest = useRef('');
  const pending = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const meetingRef = useRef(meetingId);

  const save = useCallback(async (id: string, text: string) => {
    pending.current = false;
    if (meetingRef.current === id) setSaveState('saving');
    try {
      await saveMeetingNotes(id, text);
      if (meetingRef.current === id && !pending.current) setSaveState('saved');
    } catch (err) {
      if (meetingRef.current === id) setSaveState('error');
      toast.error(`Notes were not saved: ${err}`);
    }
  }, []);

  const flush = useCallback(async () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    if (pending.current) await save(meetingRef.current, latest.current);
  }, [save]);

  useEffect(() => {
    meetingRef.current = meetingId;
    let cancelled = false;
    setLoaded(false);
    setSaveState('idle');
    latest.current = '';
    setNotesState('');
    getMeetingNotes(meetingId)
      .then((text) => {
        if (cancelled || pending.current) return;
        latest.current = text;
        setNotesState(text);
      })
      .catch((err) => console.warn('Failed to load notes:', err))
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
      // Leaving the meeting: save what was typed.
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      if (pending.current) void save(meetingId, latest.current);
    };
  }, [meetingId, save]);

  const setNotes = useCallback((text: string) => {
    latest.current = text;
    pending.current = true;
    setNotesState(text);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      void save(meetingRef.current, latest.current);
    }, SAVE_DELAY_MS);
  }, [save]);

  return { notes, setNotes, loaded, saveState, flush };
}
