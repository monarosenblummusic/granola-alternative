import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalCore = { ...await import('@tauri-apps/api/core') };
let provider: string | null = 'deepgram';
const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>) => {
  calls.push({ command, args });
  if (command === 'api_get_transcript_config') return provider ? { provider } : null;
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { autoIdentifySpeakers } = await import('../../src/lib/speakerIdentification');
afterAll(() => mock.module('@tauri-apps/api/core', () => originalCore));

beforeEach(() => {
  calls.length = 0;
});

describe('autoIdentifySpeakers', () => {
  test('starts identification when Deepgram is the provider', async () => {
    provider = 'deepgram';
    expect(await autoIdentifySpeakers('meeting-1', '/rec/m1')).toBe(true);
    expect(calls[1]).toEqual({
      command: 'identify_speakers_command',
      args: { meetingId: 'meeting-1', meetingFolderPath: '/rec/m1' },
    });
  });

  test('does nothing for local engines or missing config', async () => {
    for (const p of ['parakeet', 'localWhisper', null]) {
      provider = p;
      calls.length = 0;
      expect(await autoIdentifySpeakers('m', '/f')).toBe(false);
      expect(calls.map((c) => c.command)).toEqual(['api_get_transcript_config']);
    }
  });
});
