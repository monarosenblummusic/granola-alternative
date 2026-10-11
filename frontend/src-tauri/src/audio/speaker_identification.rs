// audio/speaker_identification.rs
//
// Fork addition: tell the other people on a call apart after the meeting.
//
// The live transcript only knows two sources: "mic" (you) and "system"
// (everyone else). After the meeting, the saved recording is sent to Deepgram
// once with diarization on. Deepgram labels every utterance with a speaker
// number. The speaker whose talk overlaps your mic lines the most is you;
// everyone else becomes "Speaker 1", "Speaker 2", ... in order of first
// appearance and replaces the generic "system" lines. Your own mic lines are
// kept exactly as they were.

use crate::api::TranscriptSegment;
use crate::database::models::Transcript;
use crate::database::repositories::setting::SettingsRepository;
use crate::database::repositories::speaker_name::{MeetingSpeaker, SpeakerNameRepository};
use crate::state::AppState;
use super::common::write_transcripts_json;
use super::echo_filter::{echo_indices, Timed};
use anyhow::{anyhow, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

const DEEPGRAM_LISTEN_URL: &str = "https://api.deepgram.com/v1/listen";
/// A Deepgram speaker counts as "you" when at least this share of their
/// speaking time overlaps your microphone lines...
const SELF_MIN_OVERLAP_RATIO: f64 = 0.5;
/// ...and the overlap is at least this long, so a stray cough cannot match.
const SELF_MIN_OVERLAP_SECONDS: f64 = 2.0;

static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct Guard;
impl Guard {
    fn acquire() -> Result<Self, String> {
        IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Guard)
            .map_err(|_| "Speaker identification is already running".to_string())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Pure logic (unit tested)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Utterance {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub speaker: u32,
    #[serde(default)]
    pub transcript: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct DiarizedResponse {
    results: Option<DiarizedResults>,
}

#[derive(Debug, Deserialize)]
struct DiarizedResults {
    #[serde(default)]
    utterances: Vec<Utterance>,
}

pub(crate) fn parse_utterances(body: &str) -> Result<Vec<Utterance>> {
    let parsed: DiarizedResponse =
        serde_json::from_str(body).map_err(|e| anyhow!("Unexpected Deepgram response: {}", e))?;
    Ok(parsed
        .results
        .map(|r| r.utterances)
        .unwrap_or_default()
        .into_iter()
        .filter(|u| !u.transcript.trim().is_empty() && u.end > u.start)
        .collect())
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Which Deepgram speaker number is the local user, judged by overlap with
/// the mic lines. None when nobody matches clearly (e.g. you never spoke).
pub(crate) fn identify_self_speaker(utterances: &[Utterance], mic_lines: &[Line]) -> Option<u32> {
    let mut talk: HashMap<u32, f64> = HashMap::new();
    let mut shared: HashMap<u32, f64> = HashMap::new();
    for u in utterances {
        *talk.entry(u.speaker).or_default() += u.end - u.start;
        let o: f64 = mic_lines.iter().map(|m| overlap(u.start, u.end, m.start, m.end)).sum();
        *shared.entry(u.speaker).or_default() += o.min(u.end - u.start);
    }
    shared
        .into_iter()
        .filter(|(spk, o)| {
            let total = talk.get(spk).copied().unwrap_or(0.0);
            *o >= SELF_MIN_OVERLAP_SECONDS && total > 0.0 && o / total >= SELF_MIN_OVERLAP_RATIO
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(spk, _)| spk)
}

/// Keep the mic lines, replace everything else with diarized utterances.
fn timed(l: &Line) -> Timed<'_> {
    Timed { start: l.start, end: l.end, text: l.text.as_str() }
}

pub(crate) fn merge(existing: &[Line], utterances: &[Utterance]) -> Vec<Line> {
    let mic_all: Vec<&Line> = existing
        .iter()
        .filter(|l| l.speaker.as_deref() == Some("mic"))
        .collect();
    // Drop echoed mic lines (remote speech re-heard through the speakers) so
    // they can neither be kept as "Me" nor make a remote voice look like you.
    // Compare against the non-mic lines, which never contain your voice.
    let others: Vec<Timed> = existing
        .iter()
        .filter(|l| l.speaker.as_deref() != Some("mic"))
        .map(timed)
        .collect();
    let echoes = echo_indices(&mic_all.iter().map(|l| timed(l)).collect::<Vec<_>>(), &others);
    let mic: Vec<Line> = mic_all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !echoes.contains(i))
        .map(|(_, l)| l.clone())
        .collect();
    let me = identify_self_speaker(utterances, &mic);

    let mut names: HashMap<u32, String> = HashMap::new();
    let mut out = mic;
    for u in utterances {
        if Some(u.speaker) == me {
            continue; // already covered by the mic lines
        }
        let next = names.len() + 1;
        let name = names
            .entry(u.speaker)
            .or_insert_with(|| format!("Speaker {}", next))
            .clone();
        out.push(Line {
            start: u.start,
            end: u.end,
            speaker: Some(name),
            text: u.transcript.trim().to_string(),
        });
    }
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub(crate) fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("webm") => "audio/webm",
        _ => "audio/mp4", // .mp4 / .m4a, the recorder's own format
    }
}

// ---------------------------------------------------------------------------
// Deepgram call + persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerIdentificationResult {
    pub meeting_id: String,
    pub speakers: usize,
    pub lines: usize,
}

async fn request_diarization(api_key: &str, model: &str, audio_path: &Path) -> Result<Vec<Utterance>> {
    let bytes = tokio::fs::read(audio_path)
        .await
        .map_err(|e| anyhow!("Could not read recording {}: {}", audio_path.display(), e))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15 * 60))
        .build()?;
    let response = client
        .post(DEEPGRAM_LISTEN_URL)
        .query(&[
            ("model", model),
            ("diarize", "true"),
            ("utterances", "true"),
            ("smart_format", "true"),
            ("punctuate", "true"),
            ("detect_language", "true"),
        ])
        .header("Authorization", format!("Token {}", api_key))
        .header("Content-Type", content_type_for(audio_path))
        .body(bytes)
        .send()
        .await
        .map_err(|e| anyhow!("Could not reach Deepgram: {}", e))?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(super::transcription::deepgram_provider::describe_http_error(
            status.as_u16(),
            &body
        )));
    }
    parse_utterances(&body)
}

fn to_line(t: &Transcript) -> Line {
    let start = t.audio_start_time.unwrap_or(0.0);
    Line {
        start,
        end: t.audio_end_time.unwrap_or(start),
        speaker: t.speaker.clone(),
        text: t.transcript.clone(),
    }
}

async fn run<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, folder: &Path) -> Result<SpeakerIdentificationResult> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;
    let pool = state.db_manager.pool();

    let api_key = SettingsRepository::get_transcript_api_key(pool, "deepgram")
        .await?
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| anyhow!("Add a Deepgram API key in Settings > Transcription to identify speakers."))?;

    let audio_path: PathBuf = super::retranscription::find_audio_file(folder)?;
    let existing: Vec<Line> = sqlx::query_as::<_, Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(to_line)
    .collect();

    let _ = app.emit("speaker-identification-progress", serde_json::json!({
        "meeting_id": meeting_id, "message": "Sending the recording to Deepgram…"
    }));
    let utterances = request_diarization(&api_key, super::transcription::deepgram_provider::DEFAULT_DEEPGRAM_MODEL, &audio_path).await?;
    if utterances.is_empty() {
        return Err(anyhow!("Deepgram found no speech in the recording; the transcript was left unchanged."));
    }

    let merged = merge(&existing, &utterances);
    let speakers = merged
        .iter()
        .filter_map(|l| l.speaker.as_deref())
        .filter(|s| s.starts_with("Speaker "))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let now = chrono::Utc::now().to_rfc3339();
    let segments: Vec<TranscriptSegment> = merged
        .iter()
        .map(|l| TranscriptSegment {
            id: format!("transcript-{}", Uuid::new_v4()),
            text: l.text.clone(),
            timestamp: now.clone(),
            audio_start_time: Some(l.start),
            audio_end_time: Some(l.end),
            duration: Some(l.end - l.start),
            speaker: l.speaker.clone(),
        })
        .collect();

    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    // "Speaker 1" may be a different person now, so its old name would be wrong.
    SpeakerNameRepository::clear_remote_names(&mut *tx, meeting_id).await?;
    for s in &segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.id)
        .bind(meeting_id)
        .bind(&s.text)
        .bind(&s.timestamp)
        .bind(s.audio_start_time)
        .bind(s.audio_end_time)
        .bind(s.duration)
        .bind(&s.speaker)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    if let Err(e) = write_transcripts_json(folder, &segments) {
        warn!("Failed to write transcripts.json after speaker identification: {}", e);
    }
    info!("Speaker identification for {}: {} speakers, {} lines", meeting_id, speakers, segments.len());
    Ok(SpeakerIdentificationResult { meeting_id: meeting_id.to_string(), speakers, lines: segments.len() })
}

#[tauri::command]
pub async fn identify_speakers_command<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
) -> Result<(), String> {
    let guard = Guard::acquire()?;
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        match run(&app, &meeting_id, Path::new(&meeting_folder_path)).await {
            Ok(result) => {
                let _ = app.emit("speaker-identification-complete", &result);
                crate::final_notes::auto_export(&app, &meeting_id, crate::final_notes::Trigger::Changed);
            }
            Err(e) => {
                warn!("Speaker identification failed for {}: {}", meeting_id, e);
                let _ = app.emit("speaker-identification-error", serde_json::json!({
                    "meeting_id": meeting_id, "error": e.to_string()
                }));
            }
        }
    });
    Ok(())
}

/// Speakers in a meeting's transcript with their current names.
#[tauri::command]
pub async fn api_get_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    SpeakerNameRepository::list_speakers(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| format!("Could not load speakers: {}", e))
}

/// Name a speaker in one meeting. An empty name restores "Me"/"Them"/"Speaker N".
#[tauri::command]
pub async fn api_rename_speaker<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    speaker: String,
    display_name: String,
) -> Result<Option<String>, String> {
    if speaker.trim().is_empty() {
        return Err("Speaker label is missing.".to_string());
    }
    let name = SpeakerNameRepository::rename(state.db_manager.pool(), &meeting_id, &speaker, &display_name)
        .await
        .map_err(|e| format!("Could not rename speaker: {}", e))?;
    let _ = app.emit("speaker-names-changed", serde_json::json!({ "meeting_id": meeting_id }));
    crate::final_notes::auto_export(&app, &meeting_id, crate::final_notes::Trigger::Changed);
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(start: f64, end: f64, speaker: u32, t: &str) -> Utterance {
        Utterance { start, end, speaker, transcript: t.into() }
    }
    fn mic(start: f64, end: f64, t: &str) -> Line {
        Line { start, end, speaker: Some("mic".into()), text: t.into() }
    }
    fn sys(start: f64, end: f64, t: &str) -> Line {
        Line { start, end, speaker: Some("system".into()), text: t.into() }
    }

    #[test]
    fn parses_utterances_and_drops_empty_ones() {
        let body = r#"{"results":{"channels":[],"utterances":[
            {"start":0.5,"end":2.0,"speaker":0,"transcript":"Hi all","confidence":0.9},
            {"start":2.0,"end":2.0,"speaker":1,"transcript":"x"},
            {"start":3.0,"end":4.0,"speaker":1,"transcript":"  "}]}}"#;
        assert_eq!(parse_utterances(body).unwrap(), vec![u(0.5, 2.0, 0, "Hi all")]);
        assert!(parse_utterances(r#"{"results":{}}"#).unwrap().is_empty());
        assert!(parse_utterances("nope").is_err());
    }

    #[test]
    fn finds_self_by_mic_overlap() {
        let utts = [u(0.0, 5.0, 0, "me talking"), u(5.0, 9.0, 1, "alice"), u(9.0, 12.0, 2, "bob")];
        let mics = [mic(0.2, 4.8, "me talking")];
        assert_eq!(identify_self_speaker(&utts, &mics), Some(0));
    }

    #[test]
    fn no_self_when_overlap_too_small_or_absent() {
        let utts = [u(0.0, 10.0, 0, "alice"), u(10.0, 20.0, 1, "bob")];
        assert_eq!(identify_self_speaker(&utts, &[]), None);
        // a 1-second cough during Alice's 10 seconds is not enough
        assert_eq!(identify_self_speaker(&utts, &[mic(3.0, 4.0, "hm")]), None);
    }

    #[test]
    fn merge_keeps_mic_lines_and_numbers_others_by_first_appearance() {
        let existing = [mic(0.0, 4.0, "Let's start."), sys(4.5, 9.0, "generic them line")];
        let utts = [
            u(0.0, 4.0, 3, "Let's start."),   // me
            u(4.5, 6.0, 7, "Thanks."),        // first remote voice
            u(6.0, 9.0, 2, "Agenda first."),  // second remote voice
            u(9.5, 11.0, 7, "Agreed."),
        ];
        let out = merge(&existing, &utts);
        let got: Vec<(Option<&str>, &str)> =
            out.iter().map(|l| (l.speaker.as_deref(), l.text.as_str())).collect();
        assert_eq!(got, vec![
            (Some("mic"), "Let's start."),
            (Some("Speaker 1"), "Thanks."),
            (Some("Speaker 2"), "Agenda first."),
            (Some("Speaker 1"), "Agreed."),
        ]);
    }

    #[test]
    fn merge_without_mic_keeps_every_speaker() {
        let out = merge(&[sys(0.0, 3.0, "x")], &[u(0.0, 3.0, 0, "a"), u(3.0, 6.0, 1, "b")]);
        let speakers: Vec<_> = out.iter().map(|l| l.speaker.clone().unwrap()).collect();
        assert_eq!(speakers, vec!["Speaker 1", "Speaker 2"]);
    }

    #[test]
    fn echoed_mic_lines_do_not_turn_a_remote_voice_into_me() {
        // No headset: Alice's words come out of the speakers and are picked up
        // by the mic, so they appear on both tracks. My real line is 20-24 s.
        let existing = [
            sys(0.0, 6.0, "Can everyone see the revenue chart for October?"),
            mic(0.2, 6.1, "can everyone see the revenue chart for October"),
            sys(8.0, 14.0, "Margins dropped because shipping costs went up."),
            mic(8.1, 14.2, "margins dropped because shipping costs went"),
            mic(20.0, 24.0, "Yes, and we should renegotiate the courier contract."),
        ];
        let utts = [
            u(0.0, 6.0, 0, "Can everyone see the revenue chart for October?"),
            u(8.0, 14.0, 0, "Margins dropped because shipping costs went up."),
            u(20.0, 24.0, 1, "Yes, and we should renegotiate the courier contract."),
        ];
        let out = merge(&existing, &utts);
        let got: Vec<(Option<&str>, &str)> =
            out.iter().map(|l| (l.speaker.as_deref(), l.text.as_str())).collect();
        assert_eq!(got, vec![
            (Some("Speaker 1"), "Can everyone see the revenue chart for October?"),
            (Some("Speaker 1"), "Margins dropped because shipping costs went up."),
            (Some("mic"), "Yes, and we should renegotiate the courier contract."),
        ]);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type_for(Path::new("a/audio.mp4")), "audio/mp4");
        assert_eq!(content_type_for(Path::new("a/audio.M4A")), "audio/mp4");
        assert_eq!(content_type_for(Path::new("a/audio.wav")), "audio/wav");
        assert_eq!(content_type_for(Path::new("a/audio.mp3")), "audio/mpeg");
    }
}
