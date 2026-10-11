import { invoke } from '@tauri-apps/api/core';

/**
 * Fork: after a recording is saved, start Deepgram speaker identification
 * automatically when Deepgram is the transcription provider. Local engines
 * cannot diarize, so nothing happens for them. Failures are reported through
 * the speaker-identification-error event and never block saving.
 */
export async function autoIdentifySpeakers(meetingId: string, meetingFolderPath: string): Promise<boolean> {
  try {
    const config = await invoke<{ provider?: string } | null>('api_get_transcript_config');
    if (config?.provider !== 'deepgram') return false;
    await invoke('identify_speakers_command', { meetingId, meetingFolderPath });
    return true;
  } catch (err) {
    console.warn('Automatic speaker identification did not start:', err);
    return false;
  }
}
