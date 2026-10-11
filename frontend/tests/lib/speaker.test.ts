import { describe, expect, test } from 'bun:test';
import { speakerLabel, withSpeakerLabel } from '../../src/lib/speaker';

describe('speaker labels', () => {
  test('mic is Me and system is Them', () => {
    expect(speakerLabel('mic')).toBe('Me');
    expect(speakerLabel('system')).toBe('Them');
  });

  test('missing or legacy mixed-track speakers get no label', () => {
    for (const s of [undefined, null, '', 'Audio']) {
      expect(speakerLabel(s)).toBe('');
      expect(withSpeakerLabel(s, 'hello')).toBe('hello');
    }
  });

  test('other speaker names pass through', () => {
    expect(withSpeakerLabel('SPEAKER_01', 'hi')).toBe('SPEAKER_01: hi');
    expect(withSpeakerLabel('mic', 'hi')).toBe('Me: hi');
  });
});
