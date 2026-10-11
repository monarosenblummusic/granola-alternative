/**
 * Fork: map a transcript segment's source track to a display label.
 * "mic" is the local microphone (you); "system" is everything the computer
 * played, i.e. the other people on the call.
 */
export function speakerLabel(speaker?: string | null): string {
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

export function withSpeakerLabel(speaker: string | null | undefined, text: string): string {
  const label = speakerLabel(speaker);
  return label ? `${label}: ${text}` : text;
}
