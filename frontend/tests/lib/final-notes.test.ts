import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalCore = { ...await import('@tauri-apps/api/core') };
const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>) => {
  calls.push({ command, args });
  if (command === 'api_export_final_notes') return 'C:\\Users\\me\\Documents\\FinalNotes\\2026-10-11 1405 Standup [1a2b3c4d].md';
  if (command === 'api_save_final_notes_settings') return { folder: args?.folder, auto_export: args?.autoExport };
  return undefined;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const finalNotes = await import('../../src/lib/finalNotes');
afterAll(() => mock.module('@tauri-apps/api/core', () => originalCore));

beforeEach(() => {
  calls.length = 0;
});

describe('FinalNotes', () => {
  test('export sends the meeting id and returns the path', async () => {
    const path = await finalNotes.exportFinalNotes('m1');
    expect(calls).toEqual([{ command: 'api_export_final_notes', args: { meetingId: 'm1' } }]);
    expect(finalNotes.fileNameOf(path)).toBe('2026-10-11 1405 Standup [1a2b3c4d].md');
  });

  test('settings use the command argument names Tauri expects', async () => {
    const saved = await finalNotes.saveFinalNotesSettings({ folder: 'D:\\Notes', auto_export: false });
    expect(calls[0]).toEqual({
      command: 'api_save_final_notes_settings',
      args: { folder: 'D:\\Notes', autoExport: false },
    });
    expect(saved).toEqual({ folder: 'D:\\Notes', auto_export: false });
  });

  test('file names come out of Windows and Unix paths', () => {
    expect(finalNotes.fileNameOf('/home/me/FinalNotes/a.md')).toBe('a.md');
    expect(finalNotes.fileNameOf('a.md')).toBe('a.md');
  });
});

describe('Claude extension', () => {
  test('connect opens the bundled extension through the app', async () => {
    await finalNotes.installClaudeExtension();
    expect(calls).toEqual([{ command: 'api_install_claude_extension', args: undefined }]);
  });
});
