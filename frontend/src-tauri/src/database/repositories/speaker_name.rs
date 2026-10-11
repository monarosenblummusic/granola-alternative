// Fork addition: per-meeting speaker names ("Speaker 1" -> "Alice").
//
// The transcript keeps its raw speaker labels; names live in
// meeting_speaker_names and are applied when a transcript is shown, copied or
// summarised.

use serde::Serialize;
use sqlx::{Error as SqlxError, Sqlite, SqliteConnection, SqlitePool};

/// Longest name we store; anything longer is almost certainly a paste mistake.
const MAX_NAME_CHARS: usize = 60;

/// One speaker in a meeting's transcript, for the rename dialog.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, PartialEq)]
pub struct MeetingSpeaker {
    /// Raw label stored in transcripts.speaker.
    pub speaker: String,
    pub display_name: Option<String>,
    pub line_count: i64,
    /// The speaker's longest line, to help recognise who it is.
    pub sample: Option<String>,
}

pub struct SpeakerNameRepository;

/// Trim, collapse inner whitespace and cap the length. Empty means "no name".
pub fn normalize_name(name: &str) -> Option<String> {
    let collapsed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_NAME_CHARS).collect();
    let capped = capped.trim_end().to_string();
    (!capped.is_empty()).then_some(capped)
}

impl SpeakerNameRepository {
    /// Every labelled speaker in the meeting, "mic" first, then by first appearance.
    pub async fn list_speakers(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        sqlx::query_as::<_, MeetingSpeaker>(
            "SELECT t.speaker AS speaker,
                    n.display_name AS display_name,
                    COUNT(*) AS line_count,
                    (SELECT t2.transcript FROM transcripts t2
                      WHERE t2.meeting_id = t.meeting_id AND t2.speaker = t.speaker
                      ORDER BY length(t2.transcript) DESC LIMIT 1) AS sample
               FROM transcripts t
               LEFT JOIN meeting_speaker_names n
                 ON n.meeting_id = t.meeting_id AND n.speaker = t.speaker
              WHERE t.meeting_id = ? AND t.speaker IS NOT NULL AND t.speaker NOT IN ('', 'Audio')
              GROUP BY t.speaker
              ORDER BY t.speaker <> 'mic', MIN(COALESCE(t.audio_start_time, 0))",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Set a speaker's name; an empty name restores the default label.
    pub async fn rename(pool: &SqlitePool, meeting_id: &str, speaker: &str, name: &str) -> Result<Option<String>, SqlxError> {
        let name = normalize_name(name);
        match &name {
            Some(n) => {
                sqlx::query(
                    "INSERT INTO meeting_speaker_names (meeting_id, speaker, display_name) VALUES (?, ?, ?)
                     ON CONFLICT(meeting_id, speaker) DO UPDATE SET display_name = excluded.display_name",
                )
                .bind(meeting_id)
                .bind(speaker)
                .bind(n)
                .execute(pool)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM meeting_speaker_names WHERE meeting_id = ? AND speaker = ?")
                    .bind(meeting_id)
                    .bind(speaker)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(name)
    }

    /// Forget names of the other people on the call. Used when speaker
    /// identification re-runs, because "Speaker 1" may now be someone else.
    /// The local user's name ("mic") is kept.
    pub async fn clear_remote_names(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), SqlxError> {
        sqlx::query::<Sqlite>("DELETE FROM meeting_speaker_names WHERE meeting_id = ? AND speaker <> 'mic'")
            .bind(meeting_id)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn delete_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), SqlxError> {
        sqlx::query::<Sqlite>("DELETE FROM meeting_speaker_names WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(conn)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_meeting() -> SqlitePool {
        // One connection: every connection to sqlite::memory: is its own database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Standup', 'now', 'now')")
            .execute(&pool)
            .await
            .unwrap();
        let lines = [
            ("a", 0.0, "Speaker 2", "Morning all."),
            ("b", 1.0, "mic", "Hi."),
            ("c", 2.0, "Speaker 1", "Quick update on the release."),
            ("d", 3.0, "Speaker 2", "Go ahead, I'm listening to the whole thing."),
            ("e", 4.0, "Audio", "upstream placeholder"),
        ];
        for (id, start, speaker, text) in lines {
            sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, speaker)
                 VALUES (?, 'm1', ?, 'now', ?, ?)",
            )
            .bind(id)
            .bind(text)
            .bind(start)
            .bind(speaker)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    #[test]
    fn names_are_normalized() {
        assert_eq!(normalize_name("  Alice   Smith "), Some("Alice Smith".to_string()));
        assert_eq!(normalize_name(" \t "), None);
        assert_eq!(normalize_name(&"x".repeat(100)).unwrap().chars().count(), MAX_NAME_CHARS);
    }

    #[tokio::test]
    async fn lists_speakers_me_first_then_by_first_appearance() {
        let pool = pool_with_meeting().await;
        let speakers = SpeakerNameRepository::list_speakers(&pool, "m1").await.unwrap();
        let keys: Vec<_> = speakers.iter().map(|s| s.speaker.as_str()).collect();
        assert_eq!(keys, ["mic", "Speaker 2", "Speaker 1"]);
        let s2 = &speakers[1];
        assert_eq!(s2.line_count, 2);
        assert_eq!(s2.sample.as_deref(), Some("Go ahead, I'm listening to the whole thing."));
        assert_eq!(s2.display_name, None);
    }

    #[tokio::test]
    async fn rename_sets_updates_and_clears() {
        let pool = pool_with_meeting().await;
        let name = |pool: SqlitePool| async move {
            SpeakerNameRepository::list_speakers(&pool, "m1").await.unwrap()
                .into_iter().find(|s| s.speaker == "Speaker 1").unwrap().display_name
        };

        assert_eq!(SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", " Dana ").await.unwrap(), Some("Dana".into()));
        assert_eq!(name(pool.clone()).await.as_deref(), Some("Dana"));
        SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", "Dana Levi").await.unwrap();
        assert_eq!(name(pool.clone()).await.as_deref(), Some("Dana Levi"));
        assert_eq!(SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", "  ").await.unwrap(), None);
        assert_eq!(name(pool.clone()).await, None);
    }

    #[tokio::test]
    async fn re_identification_keeps_my_name_only() {
        let pool = pool_with_meeting().await;
        SpeakerNameRepository::rename(&pool, "m1", "mic", "Moshe").await.unwrap();
        SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", "Dana").await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        SpeakerNameRepository::clear_remote_names(&mut conn, "m1").await.unwrap();
        drop(conn);
        let named: Vec<_> = SpeakerNameRepository::list_speakers(&pool, "m1").await.unwrap()
            .into_iter().filter_map(|s| s.display_name).collect();
        assert_eq!(named, ["Moshe"]);
    }

    #[tokio::test]
    async fn names_are_per_meeting_and_removed_with_the_meeting() {
        let pool = pool_with_meeting().await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m2', 'Other', 'now', 'now')")
            .execute(&pool).await.unwrap();
        SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", "Dana").await.unwrap();
        SpeakerNameRepository::rename(&pool, "m2", "Speaker 1", "Eli").await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        SpeakerNameRepository::delete_for_meeting(&mut conn, "m1").await.unwrap();
        drop(conn);
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT meeting_id, display_name FROM meeting_speaker_names")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(rows, [("m2".to_string(), "Eli".to_string())]);
    }
}
