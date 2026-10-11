import { invoke } from '@tauri-apps/api/core';
import type { SpeakerNames } from './speaker';

/**
 * Fork: per-meeting speaker names. The transcript keeps raw labels ("mic",
 * "system", "Speaker 1"); names are stored separately and applied on display,
 * copy and summary. See database/repositories/speaker_name.rs.
 */
export interface MeetingSpeaker {
  speaker: string;
  display_name: string | null;
  line_count: number;
  sample: string | null;
}

export function getMeetingSpeakers(meetingId: string): Promise<MeetingSpeaker[]> {
  return invoke<MeetingSpeaker[]>('api_get_meeting_speakers', { meetingId });
}

export function speakerNamesFrom(speakers: MeetingSpeaker[]): SpeakerNames {
  const names: SpeakerNames = {};
  for (const s of speakers) {
    if (s.display_name?.trim()) names[s.speaker] = s.display_name.trim();
  }
  return names;
}

export async function loadSpeakerNames(meetingId: string): Promise<SpeakerNames> {
  return speakerNamesFrom(await getMeetingSpeakers(meetingId));
}

/** Speakers whose name in `edited` differs from what is stored. */
export function changedNames(speakers: MeetingSpeaker[], edited: Record<string, string>): [string, string][] {
  return speakers
    .filter((s) => s.speaker in edited)
    .map((s): [string, string] => [s.speaker, edited[s.speaker].trim()])
    .filter(([speaker, name]) => name !== (speakers.find((s) => s.speaker === speaker)?.display_name ?? ''));
}

export async function renameSpeakers(meetingId: string, changes: [string, string][]): Promise<void> {
  for (const [speaker, displayName] of changes) {
    await invoke('api_rename_speaker', { meetingId, speaker, displayName });
  }
}
