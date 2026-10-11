import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalCore = { ...await import('@tauri-apps/api/core') };
const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const stored = [
  { speaker: 'mic', display_name: 'Moshe', line_count: 4, sample: 'Hi all' },
  { speaker: 'Speaker 1', display_name: null, line_count: 7, sample: 'Release update' },
  { speaker: 'Speaker 2', display_name: 'Eli', line_count: 2, sample: null },
];
const invoke = mock(async (command: string, args?: Record<string, unknown>) => {
  calls.push({ command, args });
  if (command === 'api_get_meeting_speakers') return stored;
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { changedNames, loadSpeakerNames, renameSpeakers, speakerNamesFrom } = await import('../../src/lib/speakerNames');
afterAll(() => mock.module('@tauri-apps/api/core', () => originalCore));

beforeEach(() => {
  calls.length = 0;
});

describe('speaker names', () => {
  test('only named speakers end up in the names map', async () => {
    expect(speakerNamesFrom(stored)).toEqual({ mic: 'Moshe', 'Speaker 2': 'Eli' });
    expect(await loadSpeakerNames('m1')).toEqual({ mic: 'Moshe', 'Speaker 2': 'Eli' });
    expect(calls[0]).toEqual({ command: 'api_get_meeting_speakers', args: { meetingId: 'm1' } });
  });

  test('only edited names are saved, trimmed, and clearing counts as a change', () => {
    const edited = { mic: 'Moshe', 'Speaker 1': '  Dana ', 'Speaker 2': '' };
    expect(changedNames(stored, edited)).toEqual([
      ['Speaker 1', 'Dana'],
      ['Speaker 2', ''],
    ]);
    expect(changedNames(stored, { mic: ' Moshe ', 'Speaker 1': ' ' })).toEqual([]);
  });

  test('each change is one rename call', async () => {
    await renameSpeakers('m1', [['Speaker 1', 'Dana'], ['Speaker 2', '']]);
    expect(calls).toEqual([
      { command: 'api_rename_speaker', args: { meetingId: 'm1', speaker: 'Speaker 1', displayName: 'Dana' } },
      { command: 'api_rename_speaker', args: { meetingId: 'm1', speaker: 'Speaker 2', displayName: '' } },
    ]);
  });
});
