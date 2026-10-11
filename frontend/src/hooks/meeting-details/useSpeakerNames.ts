import { useEffect, useState } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { SpeakerNames } from '@/lib/speaker';
import { loadSpeakerNames } from '@/lib/speakerNames';

/**
 * Fork: the names the user gave this meeting's speakers, kept current when
 * they are renamed or speaker identification re-runs (which clears the names
 * of the other people on the call).
 */
export function useSpeakerNames(meetingId?: string): SpeakerNames {
  const [names, setNames] = useState<SpeakerNames>({});

  useEffect(() => {
    setNames({});
    if (!meetingId) return;
    let cancelled = false;
    const reload = () => {
      loadSpeakerNames(meetingId)
        .then((n) => { if (!cancelled) setNames(n); })
        .catch((err) => console.warn('Failed to load speaker names:', err));
    };
    reload();

    const onEvent = (e: { payload: { meeting_id?: string } }) => {
      if (e.payload?.meeting_id === meetingId) reload();
    };
    const unlisteners: Promise<UnlistenFn>[] = [
      listen('speaker-names-changed', onEvent),
      listen('speaker-identification-complete', onEvent),
    ];
    return () => {
      cancelled = true;
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [meetingId]);

  return names;
}
