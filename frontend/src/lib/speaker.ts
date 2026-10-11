/**
 * Fork: map a transcript segment's speaker to a display label.
 * "mic" is the local microphone (you); "system" is everything the computer
 * played, i.e. the other people on the call; "Speaker N" comes from speaker
 * identification. `names` holds the names the user gave speakers in this
 * meeting (raw label -> name) and wins over the defaults.
 */
export type SpeakerNames = Record<string, string>;

export function defaultSpeakerLabel(speaker?: string | null): string {
  switch (speaker) {
    case 'mic':
      return 'Me';
    case 'system':
      return 'Them';
    case undefined:
    case null:
    case '':
    case 'Audio': // upstream's placeholder for a single mixed track
      return '';
    default:
      return speaker;
  }
}

export function speakerLabel(speaker?: string | null, names?: SpeakerNames): string {
  const named = speaker ? names?.[speaker]?.trim() : undefined;
  return named || defaultSpeakerLabel(speaker);
}

export function withSpeakerLabel(
  speaker: string | null | undefined,
  text: string,
  names?: SpeakerNames,
): string {
  const label = speakerLabel(speaker, names);
  return label ? `${label}: ${text}` : text;
}
