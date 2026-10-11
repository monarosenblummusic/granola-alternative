// audio/echo_filter.rs
//
// Fork addition: drop "Me" lines that are really the other people coming out
// of the speakers and back into the microphone.
//
// Without a headset the mic hears the call, so remote speech is transcribed
// twice: correctly from the system track and again from the mic. The system
// track never contains the local user's voice (call apps do not play you back
// to yourself), so a mic line whose words were also said on the system track
// at the same moment is an echo.
//
// Matching is on words and timing, not audio, so it works regardless of
// speaker volume or microphone gain.

use std::collections::{HashMap, HashSet};

/// How far around a mic line to look for the same words on the system track.
/// Live segments from the two tracks are cut independently, so their
/// boundaries can differ by a second or more.
const WINDOW_SECONDS: f64 = 2.0;
/// Share of a mic line's words that must appear on the system track.
const MIN_CONTAINMENT: f64 = 0.6;
/// Lines this short ("yes", "okay") only count as echo on an exact match that
/// also overlaps in time, because both people may genuinely say them.
const SHORT_LINE_WORDS: usize = 2;

#[derive(Debug, Clone, Copy)]
pub struct Timed<'a> {
    pub start: f64,
    pub end: f64,
    pub text: &'a str,
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn overlaps(a: &Timed, b: &Timed, slack: f64) -> bool {
    a.start - slack < b.end && b.start < a.end + slack
}

/// Indices of `mic` lines that are echoes of `system` lines.
pub fn echo_indices(mic: &[Timed], system: &[Timed]) -> HashSet<usize> {
    let mut echoes = HashSet::new();
    for (i, m) in mic.iter().enumerate() {
        let mine = words(m.text);
        if mine.is_empty() {
            continue;
        }
        let short = mine.len() <= SHORT_LINE_WORDS;
        let slack = if short { 0.5 } else { WINDOW_SECONDS };

        // Multiset of words spoken on the system track around this line.
        let mut heard: HashMap<String, usize> = HashMap::new();
        for s in system.iter().filter(|s| overlaps(m, s, slack)) {
            for w in words(s.text) {
                *heard.entry(w).or_default() += 1;
            }
        }
        if heard.is_empty() {
            continue;
        }

        let mut matched = 0usize;
        for w in &mine {
            if let Some(n) = heard.get_mut(w) {
                if *n > 0 {
                    *n -= 1;
                    matched += 1;
                }
            }
        }
        let containment = matched as f64 / mine.len() as f64;
        let is_echo = if short { matched == mine.len() } else { containment >= MIN_CONTAINMENT };
        if is_echo {
            echoes.insert(i);
        }
    }
    echoes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(start: f64, end: f64, text: &str) -> Timed<'_> {
        Timed { start, end, text }
    }

    #[test]
    fn exact_echo_is_detected() {
        let sys = [t(10.0, 14.0, "We should ship the release on Friday.")];
        let mic = [t(10.1, 14.2, "we should ship the release on friday")];
        assert_eq!(echo_indices(&mic, &sys), HashSet::from([0]));
    }

    #[test]
    fn imperfect_echo_transcription_still_matches() {
        // The echo is quieter, so the mic transcript drops and garbles words.
        let sys = [t(10.0, 14.0, "Let's move the budget review to Thursday morning.")];
        let mic = [t(10.3, 13.8, "move the budget review to Thursday warning")];
        assert!(echo_indices(&mic, &sys).contains(&0));
    }

    #[test]
    fn echo_split_across_system_segments_matches() {
        let sys = [t(5.0, 6.5, "I sent the forecast"), t(6.6, 8.0, "to finance yesterday.")];
        let mic = [t(5.2, 8.1, "I sent the forecast to finance yesterday")];
        assert!(echo_indices(&mic, &sys).contains(&0));
    }

    #[test]
    fn my_own_speech_is_kept() {
        let sys = [t(0.0, 4.0, "What do you think about the numbers?")];
        let mic = [t(4.5, 8.0, "I think they look fine but the margin is thin")];
        assert!(echo_indices(&mic, &sys).is_empty());
    }

    #[test]
    fn same_words_at_a_different_time_are_not_echo() {
        let sys = [t(0.0, 3.0, "We should ship on Friday")];
        let mic = [t(60.0, 63.0, "We should ship on Friday")];
        assert!(echo_indices(&mic, &sys).is_empty());
    }

    #[test]
    fn short_replies_need_an_exact_simultaneous_match() {
        let sys = [t(10.0, 10.6, "Yes."), t(20.0, 21.0, "Okay, thanks.")];
        // exact and simultaneous: echo
        assert!(echo_indices(&[t(10.1, 10.7, "yes")], &sys).contains(&0));
        // "yes" two seconds later is a real reply, not an echo
        assert!(echo_indices(&[t(12.5, 13.0, "yes")], &sys).is_empty());
        // one of two words: not echo
        assert!(echo_indices(&[t(20.0, 21.0, "okay great")], &sys).is_empty());
    }

    #[test]
    fn repeated_words_are_counted_once_each() {
        let sys = [t(0.0, 3.0, "no")];
        let mic = [t(0.0, 3.0, "no no no no no")];
        assert!(echo_indices(&mic, &sys).is_empty());
    }

    #[test]
    fn empty_inputs() {
        assert!(echo_indices(&[], &[t(0.0, 1.0, "hi")]).is_empty());
        assert!(echo_indices(&[t(0.0, 1.0, "hi there")], &[]).is_empty());
        assert!(echo_indices(&[t(0.0, 1.0, " ... ")], &[t(0.0, 1.0, "hi")]).is_empty());
    }
}
