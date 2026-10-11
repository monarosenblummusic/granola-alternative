// Fork addition: the notepad. Notes typed during or after a meeting are stored
// in meeting_notes (table from upstream, previously unused) and given to the
// summary model so "Enhance" builds on what the user wrote.

use chrono::Utc;
use sqlx::{Error as SqlxError, Sqlite, SqliteConnection, SqlitePool};

pub struct MeetingNotesRepository;

impl MeetingNotesRepository {
    /// The meeting's notes, or None when there are none.
    pub async fn get(pool: &SqlitePool, meeting_id: &str) -> Result<Option<String>, SqlxError> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT notes_markdown FROM meeting_notes WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await?;
        Ok(row.and_then(|(notes,)| notes).filter(|n| !n.trim().is_empty()))
    }

    /// Save the notes; blank notes delete the row.
    pub async fn save(pool: &SqlitePool, meeting_id: &str, notes: &str) -> Result<(), SqlxError> {
        if notes.trim().is_empty() {
            sqlx::query("DELETE FROM meeting_notes WHERE meeting_id = ?")
                .bind(meeting_id)
                .execute(pool)
                .await?;
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO meeting_notes (meeting_id, notes_markdown, created_at, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(meeting_id) DO UPDATE SET notes_markdown = excluded.notes_markdown, updated_at = excluded.updated_at",
        )
        .bind(meeting_id)
        .bind(notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), SqlxError> {
        sqlx::query::<Sqlite>("DELETE FROM meeting_notes WHERE meeting_id = ?")
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
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Standup', 'now', 'now')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn save_update_and_read_back() {
        let pool = pool_with_meeting().await;
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap(), None);
        MeetingNotesRepository::save(&pool, "m1", "- budget\n- hire").await.unwrap();
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap().as_deref(), Some("- budget\n- hire"));
        MeetingNotesRepository::save(&pool, "m1", "- budget: approved").await.unwrap();
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap().as_deref(), Some("- budget: approved"));
    }

    #[tokio::test]
    async fn blank_notes_remove_the_row() {
        let pool = pool_with_meeting().await;
        MeetingNotesRepository::save(&pool, "m1", "x").await.unwrap();
        MeetingNotesRepository::save(&pool, "m1", " \n ").await.unwrap();
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap(), None);
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_notes").fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn notes_are_deleted_with_the_meeting() {
        let pool = pool_with_meeting().await;
        MeetingNotesRepository::save(&pool, "m1", "x").await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        MeetingNotesRepository::delete_for_meeting(&mut conn, "m1").await.unwrap();
        drop(conn);
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap(), None);
    }
}
