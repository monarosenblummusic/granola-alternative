import { invoke } from '@tauri-apps/api/core';

/**
 * Fork: FinalNotes export. Each meeting is written as one Markdown file
 * (AI notes, your notes, transcript) to a folder Claude can read.
 * See src-tauri/src/final_notes.rs.
 */
export interface FinalNotesSettings {
  folder: string;
  auto_export: boolean;
}

export function getFinalNotesSettings(): Promise<FinalNotesSettings> {
  return invoke<FinalNotesSettings>('api_get_final_notes_settings');
}

export function saveFinalNotesSettings(settings: FinalNotesSettings): Promise<FinalNotesSettings> {
  return invoke<FinalNotesSettings>('api_save_final_notes_settings', {
    folder: settings.folder,
    autoExport: settings.auto_export,
  });
}

/** Opens a folder picker; null when cancelled. */
export function chooseFinalNotesFolder(): Promise<string | null> {
  return invoke<string | null>('api_choose_final_notes_folder');
}

export function openFinalNotesFolder(): Promise<void> {
  return invoke('api_open_final_notes_folder');
}

/** Export one meeting now. Resolves to the written file's path. */
export function exportFinalNotes(meetingId: string): Promise<string> {
  return invoke<string>('api_export_final_notes', { meetingId });
}

/** File name from a full path, for short messages. */
export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}
