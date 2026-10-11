use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

/// Indices (into `segments`) of mic lines that echo a system line.
fn echo_segment_indices(segments: &[TranscriptSegment]) -> std::collections::HashSet<usize> {
    use crate::audio::echo_filter::{echo_indices, Timed};
    fn timed(s: &TranscriptSegment) -> Timed<'_> {
        let start = s.audio_start_time.unwrap_or(0.0);
        Timed { start, end: s.audio_end_time.unwrap_or(start), text: s.text.as_str() }
    }
    let (mut mic_idx, mut mic, mut system) = (Vec::new(), Vec::new(), Vec::new());
    for (i, s) in segments.iter().enumerate() {
        match s.speaker.as_deref() {
            Some("mic") => {
                mic_idx.push(i);
                mic.push(timed(s));
            }
            Some("system") => system.push(timed(s)),
            _ => {}
        }
    }
    echo_indices(&mic, &system).into_iter().map(|i| mic_idx[i]).collect()
}

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting '{}': {}", meeting_title, e);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // Fork: without a headset the mic re-hears the call. Drop mic lines that
        // repeat what the system track said at the same moment.
        let echoes = echo_segment_indices(transcripts);
        if !echoes.is_empty() {
            info!("Dropping {} echoed mic line(s) for meeting {}", echoes.len(), meeting_id);
        }

        // 2. Save each transcript segment with audio timing fields
        for (index, segment) in transcripts.iter().enumerate() {
            if echoes.contains(&index) {
                continue;
            }
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.speaker)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod echo_tests {
    use super::*;

    fn seg(start: f64, end: f64, speaker: Option<&str>, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("{}", start),
            text: text.to_string(),
            timestamp: String::new(),
            audio_start_time: Some(start),
            audio_end_time: Some(end),
            duration: Some(end - start),
            speaker: speaker.map(str::to_string),
        }
    }

    #[test]
    fn only_echoed_mic_lines_are_selected() {
        let segments = [
            seg(0.0, 4.0, Some("system"), "Please send me the contract by Monday."),
            seg(0.1, 4.1, Some("mic"), "please send me the contract by Monday"), // echo
            seg(5.0, 7.0, Some("mic"), "Sure, I will do that today."),           // real
            seg(8.0, 9.0, None, "please send me the contract by Monday"),        // legacy, untouched
        ];
        assert_eq!(echo_segment_indices(&segments), std::collections::HashSet::from([1]));
    }
}
