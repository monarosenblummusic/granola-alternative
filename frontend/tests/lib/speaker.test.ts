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

describe('speaker names', () => {
  const names = { mic: 'Moshe', 'Speaker 1': 'Dana', 'Speaker 2': '  ' };

  test('a chosen name replaces the default label', () => {
    expect(withSpeakerLabel('mic', 'hi', names)).toBe('Moshe: hi');
    expect(withSpeakerLabel('Speaker 1', 'hi', names)).toBe('Dana: hi');
  });

  test('unnamed or blank-named speakers keep their default label', () => {
    expect(speakerLabel('Speaker 2', names)).toBe('Speaker 2');
    expect(speakerLabel('system', names)).toBe('Them');
    expect(speakerLabel(undefined, names)).toBe('');
  });

  test('names never label lines that have no speaker', () => {
    expect(withSpeakerLabel('', 'hi', { '': 'Ghost' })).toBe('hi');
  });
});
