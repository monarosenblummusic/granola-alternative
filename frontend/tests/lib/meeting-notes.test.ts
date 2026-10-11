import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

// Minimal localStorage for bun.
const store = new Map<string, string>();
const originalStorage = (globalThis as { localStorage?: unknown }).localStorage;
(globalThis as { localStorage?: unknown }).localStorage = {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, String(v)),
  removeItem: (k: string) => void store.delete(k),
};

const originalCore = { ...await import('@tauri-apps/api/core') };
const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
let failSave = false;
const invoke = mock(async (command: string, args?: Record<string, unknown>) => {
  calls.push({ command, args });
  if (command === 'api_save_meeting_notes' && failSave) throw new Error('db locked');
  if (command === 'api_get_meeting_notes') return null;
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const notes = await import('../../src/lib/meetingNotes');
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  (globalThis as { localStorage?: unknown }).localStorage = originalStorage;
});

beforeEach(() => {
  calls.length = 0;
  store.clear();
  failSave = false;
});

describe('notepad draft', () => {
  test('blank drafts are not stored', () => {
    notes.writeNotesDraft('  \n');
    expect(store.has(notes.NOTES_DRAFT_KEY)).toBe(false);
    notes.writeNotesDraft('- budget');
    expect(notes.readNotesDraft()).toBe('- budget');
  });

  test('the draft moves onto the saved meeting', async () => {
    notes.writeNotesDraft('- budget\n- hire Dana');
    expect(await notes.attachNotesDraft('m1')).toBe(true);
    expect(calls).toEqual([
      { command: 'api_save_meeting_notes', args: { meetingId: 'm1', notes: '- budget\n- hire Dana' } },
    ]);
    expect(notes.readNotesDraft()).toBe('');
  });

  test('no draft means no save', async () => {
    expect(await notes.attachNotesDraft('m1')).toBe(false);
    expect(calls).toEqual([]);
  });

  test('a failed save keeps the draft', async () => {
    failSave = true;
    notes.writeNotesDraft('- keep me');
    expect(await notes.attachNotesDraft('m1')).toBe(false);
    expect(notes.readNotesDraft()).toBe('- keep me');
  });

  test('missing notes load as empty text', async () => {
    expect(await notes.getMeetingNotes('m1')).toBe('');
  });
});
