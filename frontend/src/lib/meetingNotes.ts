import { invoke } from '@tauri-apps/api/core';

/**
 * Fork: the notepad. While recording there is no meeting yet, so notes are
 * kept as a draft in localStorage (survives navigation, restarts and crashes)
 * and attached to the meeting when it is saved. After that they live in the
 * meeting_notes table and the summary model receives them ("Enhance").
 */
export const NOTES_DRAFT_KEY = 'meetingNotes.draft';
/** Fired on window when the draft was moved onto a meeting. */
export const NOTES_DRAFT_CLEARED_EVENT = 'meeting-notes-draft-cleared';

export function readNotesDraft(): string {
  try {
    return localStorage.getItem(NOTES_DRAFT_KEY) ?? '';
  } catch {
    return '';
  }
}

export function writeNotesDraft(text: string): void {
  try {
    if (text.trim()) localStorage.setItem(NOTES_DRAFT_KEY, text);
    else localStorage.removeItem(NOTES_DRAFT_KEY);
  } catch {
    // Storage unavailable: the notepad still works for this session.
  }
}

export function clearNotesDraft(): void {
  writeNotesDraft('');
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(NOTES_DRAFT_CLEARED_EVENT));
}

export async function getMeetingNotes(meetingId: string): Promise<string> {
  return (await invoke<string | null>('api_get_meeting_notes', { meetingId })) ?? '';
}

export function saveMeetingNotes(meetingId: string, notes: string): Promise<void> {
  return invoke('api_save_meeting_notes', { meetingId, notes });
}

/**
 * Move the notepad draft onto a just-saved meeting. The draft is kept if the
 * save fails, so notes are never lost. Never throws.
 */
export async function attachNotesDraft(meetingId: string): Promise<boolean> {
  const draft = readNotesDraft();
  if (!draft.trim()) return false;
  try {
    await saveMeetingNotes(meetingId, draft);
    clearNotesDraft();
    return true;
  } catch (err) {
    console.warn('Could not attach notes to the meeting; the draft was kept:', err);
    return false;
  }
}
