// final_notes.rs
//
// Fork addition: FinalNotes export. Every meeting becomes one Markdown file in
// a FinalNotes folder (default: Documents\FinalNotes) holding the AI notes,
// the user's own notes and the transcript with speaker names. Claude Desktop
// reads that folder through the MCP filesystem server.
//
// Files are named "<date> <time> <title> [<tag>].md". The tag is derived from
// the meeting id, so a re-export replaces the meeting's previous file even
// after the title changes. Files are written to a temporary name and renamed
// into place, so a reader never sees a half-written file.

use crate::database::models::Transcript;
use crate::database::repositories::{
    meeting::MeetingsRepository, meeting_notes::MeetingNotesRepository,
    speaker_name::SpeakerNameRepository, summary::SummaryProcessesRepository,
};
use crate::state::AppState;
use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, Local, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

const STORE_FILE: &str = "final_notes.json";
const STORE_KEY: &str = "settings";
const MAX_TITLE_CHARS: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinalNotesSettings {
    pub folder: PathBuf,
    /// Export after every summary and keep exported files up to date.
    #[serde(default = "default_true")]
    pub auto_export: bool,
}

fn default_true() -> bool {
    true
}

pub fn default_folder() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FinalNotes")
}

impl Default for FinalNotesSettings {
    fn default() -> Self {
        Self { folder: default_folder(), auto_export: true }
    }
}

pub fn load_settings<R: Runtime>(app: &AppHandle<R>) -> FinalNotesSettings {
    app.store(STORE_FILE)
        .ok()
        .and_then(|store| store.get(STORE_KEY))
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn save_settings<R: Runtime>(app: &AppHandle<R>, settings: &FinalNotesSettings) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("Could not open settings: {}", e))?;
    store.set(STORE_KEY, serde_json::to_value(settings)?);
    store.save().map_err(|e| anyhow!("Could not save settings: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Rendering (pure, unit tested)
// ---------------------------------------------------------------------------

pub struct ExportLine {
    pub start: Option<f64>,
    /// Display label ("Me", "Dana", "Speaker 2"); empty when unknown.
    pub speaker: String,
    pub text: String,
}

pub struct ExportMeeting {
    pub id: String,
    pub title: String,
    pub started: DateTime<FixedOffset>,
    pub duration_seconds: Option<f64>,
    pub participants: Vec<String>,
    pub ai_notes: Option<String>,
    pub my_notes: Option<String>,
    pub lines: Vec<ExportLine>,
}

/// Short, stable tag that identifies a meeting's file.
pub fn meeting_tag(meeting_id: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in meeting_id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", hash as u32)
}

/// A title that is safe as a file name on Windows, macOS and Linux.
pub fn safe_title(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
    let trimmed = capped.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() { "Meeting".to_string() } else { trimmed }
}

pub fn file_name(meeting: &ExportMeeting) -> String {
    format!(
        "{} {} [{}].md",
        meeting.started.format("%Y-%m-%d %H%M"),
        safe_title(&meeting.title),
        meeting_tag(&meeting.id)
    )
}

/// The same labels the app shows: the user's names first, then Me/Them.
pub fn speaker_label(speaker: Option<&str>, names: &HashMap<String, String>) -> String {
    match speaker {
        None | Some("") | Some("Audio") => String::new(),
        Some(s) => match names.get(s) {
            Some(name) => name.clone(),
            None => match s {
                "mic" => "Me".to_string(),
                "system" => "Them".to_string(),
                other => other.to_string(),
            },
        },
    }
}

/// Shift headings so the shallowest one becomes `top` (e.g. 3 under a "##" section).
fn nest_headings(markdown: &str, top: usize) -> String {
    let level = |line: &str| {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
    };
    let mut in_code = false;
    let mut min_level = usize::MAX;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        } else if !in_code && level(line) {
            min_level = min_level.min(line.chars().take_while(|c| *c == '#').count());
        }
    }
    if min_level == usize::MAX || min_level >= top {
        return markdown.trim().to_string();
    }
    let shift = top - min_level;
    in_code = false;
    markdown
        .trim()
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
                return line.to_string();
            }
            if !in_code && level(line) {
                let hashes = line.chars().take_while(|c| *c == '#').count();
                let new_level = (hashes + shift).min(6);
                format!("{}{}", "#".repeat(new_level), &line[hashes..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("[{}:{:02}:{:02}]", h, m, s) } else { format!("[{:02}:{:02}]", m, s) }
}

fn quoted(text: &str) -> String {
    // A JSON string is a valid YAML double-quoted scalar.
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

pub fn render_markdown(meeting: &ExportMeeting, exported_at: DateTime<FixedOffset>) -> String {
    let duration_minutes = meeting.duration_seconds.map(|s| (s / 60.0).round().max(1.0) as u64);
    let mut out = String::new();

    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", quoted(&meeting.title)));
    out.push_str(&format!("date: {}\n", meeting.started.to_rfc3339()));
    if let Some(m) = duration_minutes {
        out.push_str(&format!("duration_minutes: {}\n", m));
    }
    let participants: Vec<String> = meeting.participants.iter().map(|p| quoted(p)).collect();
    out.push_str(&format!("participants: [{}]\n", participants.join(", ")));
    out.push_str(&format!("meeting_id: {}\n", quoted(&meeting.id)));
    out.push_str(&format!("exported_at: {}\n", exported_at.to_rfc3339()));
    out.push_str("---\n\n");

    out.push_str(&format!("# {}\n\n", meeting.title.trim()));
    let mut facts = vec![format!("**Date:** {}", meeting.started.format("%a %-d %b %Y, %H:%M"))];
    if let Some(m) = duration_minutes {
        facts.push(format!("**Duration:** {} min", m));
    }
    if !meeting.participants.is_empty() {
        facts.push(format!("**Participants:** {}", meeting.participants.join(", ")));
    }
    out.push_str(&facts.join(" · "));
    out.push_str("\n\n");

    if let Some(notes) = meeting.ai_notes.as_deref().filter(|n| !n.trim().is_empty()) {
        out.push_str("## AI notes\n\n");
        out.push_str(&nest_headings(notes, 3));
        out.push_str("\n\n");
    }
    if let Some(notes) = meeting.my_notes.as_deref().filter(|n| !n.trim().is_empty()) {
        out.push_str("## My notes\n\n");
        out.push_str(&nest_headings(notes, 3));
        out.push_str("\n\n");
    }

    out.push_str("## Transcript\n\n");
    if meeting.lines.is_empty() {
        out.push_str("No transcript.\n");
    }
    for line in &meeting.lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(start) = line.start {
            out.push_str(&timestamp(start));
            out.push(' ');
        }
        if !line.speaker.is_empty() {
            out.push_str(&line.speaker);
            out.push_str(": ");
        }
        out.push_str(text);
        out.push_str("  \n");
    }
    out
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

fn tag_suffix(meeting_id: &str) -> String {
    format!(" [{}].md", meeting_tag(meeting_id))
}

/// Files in `folder` that belong to the meeting.
fn existing_files(folder: &Path, meeting_id: &str) -> Vec<PathBuf> {
    let suffix = tag_suffix(meeting_id);
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&suffix)))
        .collect()
}

/// Write the file atomically and remove the meeting's older files.
pub fn write_export(folder: &Path, meeting: &ExportMeeting, exported_at: DateTime<FixedOffset>) -> Result<PathBuf> {
    fs::create_dir_all(folder).map_err(|e| anyhow!("Could not create {}: {}", folder.display(), e))?;
    let path = folder.join(file_name(meeting));
    let tmp = folder.join(format!(".{}.{}.tmp", meeting_tag(&meeting.id), Uuid::new_v4()));
    fs::write(&tmp, render_markdown(meeting, exported_at))
        .map_err(|e| anyhow!("Could not write {}: {}", tmp.display(), e))?;
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!("Could not write {}: {}", path.display(), e));
    }
    for old in existing_files(folder, &meeting.id).into_iter().filter(|p| *p != path) {
        if let Err(e) = fs::remove_file(&old) {
            warn!("Could not remove old export {}: {}", old.display(), e);
        }
    }
    Ok(path)
}

pub fn remove_export_files(folder: &Path, meeting_id: &str) {
    for path in existing_files(folder, meeting_id) {
        if let Err(e) = fs::remove_file(&path) {
            warn!("Could not remove export {}: {}", path.display(), e);
        }
    }
}

// ---------------------------------------------------------------------------
// Database -> export
// ---------------------------------------------------------------------------

fn summary_markdown(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value.get("markdown")?.as_str().map(str::to_string).filter(|m| !m.trim().is_empty())
}

async fn load_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<ExportMeeting> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await?
        .ok_or_else(|| anyhow!("Meeting {} not found", meeting_id))?;

    let transcripts: Vec<Transcript> = sqlx::query_as::<_, Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let speakers = SpeakerNameRepository::list_speakers(pool, meeting_id).await?;
    let names: HashMap<String, String> = speakers
        .iter()
        .filter_map(|s| s.display_name.clone().map(|n| (s.speaker.clone(), n)))
        .collect();
    let mut participants: Vec<String> = Vec::new();
    for s in &speakers {
        let label = speaker_label(Some(&s.speaker), &names);
        if !label.is_empty() && !participants.contains(&label) {
            participants.push(label);
        }
    }

    let ai_notes = SummaryProcessesRepository::get_summary_data(pool, meeting_id)
        .await?
        .and_then(|p| p.result)
        .and_then(|raw| summary_markdown(&raw));
    let my_notes = MeetingNotesRepository::get(pool, meeting_id).await?;

    let duration_seconds = transcripts
        .iter()
        .filter_map(|t| t.audio_end_time.or(t.audio_start_time))
        .fold(None, |max: Option<f64>, v| Some(max.map_or(v, |m| m.max(v))));

    Ok(ExportMeeting {
        id: meeting.id,
        title: meeting.title,
        started: meeting.created_at.0.with_timezone(&Local).fixed_offset(),
        duration_seconds,
        participants,
        ai_notes,
        my_notes,
        lines: transcripts
            .iter()
            .map(|t| ExportLine {
                start: t.audio_start_time,
                speaker: speaker_label(t.speaker.as_deref(), &names),
                text: t.transcript.clone(),
            })
            .collect(),
    })
}

pub async fn export_meeting(pool: &SqlitePool, meeting_id: &str, folder: &Path) -> Result<PathBuf> {
    let meeting = load_meeting(pool, meeting_id).await?;
    let path = write_export(folder, &meeting, Utc::now().with_timezone(&Local).fixed_offset())?;
    info!("Exported meeting {} to {}", meeting_id, path.display());
    Ok(path)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Trigger {
    /// A summary was just generated: create or replace the file.
    SummaryReady,
    /// Something in an already exported meeting changed: refresh the file only.
    Changed,
}

/// Background export used by the rest of the app; respects the auto-export
/// setting and reports the result through events.
pub fn auto_export<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, trigger: Trigger) {
    let settings = load_settings(app);
    if !settings.auto_export {
        return;
    }
    if trigger == Trigger::Changed && existing_files(&settings.folder, meeting_id).is_empty() {
        return;
    }
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else { return };
        match export_meeting(state.db_manager.pool(), &meeting_id, &settings.folder).await {
            Ok(path) => {
                let _ = app.emit("final-notes-exported", serde_json::json!({
                    "meeting_id": meeting_id, "path": path.to_string_lossy()
                }));
            }
            Err(e) => {
                warn!("FinalNotes export failed for {}: {}", meeting_id, e);
                let _ = app.emit("final-notes-export-error", serde_json::json!({
                    "meeting_id": meeting_id, "error": e.to_string()
                }));
            }
        }
    });
}

/// A deleted meeting's export is deleted too.
pub fn remove_export<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    remove_export_files(&load_settings(app).folder, meeting_id);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn api_get_final_notes_settings<R: Runtime>(app: AppHandle<R>) -> Result<FinalNotesSettings, String> {
    Ok(load_settings(&app))
}

#[tauri::command]
pub async fn api_save_final_notes_settings<R: Runtime>(
    app: AppHandle<R>,
    folder: String,
    auto_export: bool,
) -> Result<FinalNotesSettings, String> {
    let folder = PathBuf::from(folder.trim());
    if !folder.is_absolute() {
        return Err("Choose a full folder path for FinalNotes.".to_string());
    }
    fs::create_dir_all(&folder).map_err(|e| format!("Could not create {}: {}", folder.display(), e))?;
    let settings = FinalNotesSettings { folder, auto_export };
    save_settings(&app, &settings).map_err(|e| e.to_string())?;
    Ok(settings)
}

/// Export one meeting now, whatever the auto-export setting. Returns the file path.
#[tauri::command]
pub async fn api_export_final_notes<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    let folder = load_settings(&app).folder;
    export_meeting(state.db_manager.pool(), &meeting_id, &folder)
        .await
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("FinalNotes export failed: {}", e))
}

/// Folder picker for the FinalNotes setting. None when cancelled.
#[tauri::command]
pub async fn api_choose_final_notes_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let current = load_settings(&app).folder;
    let mut picker = app.dialog().file().set_title("Choose the FinalNotes folder");
    if current.exists() {
        picker = picker.set_directory(&current);
    }
    Ok(picker.blocking_pick_folder().map(|p| p.to_string()))
}

/// Open a folder, or a file with its default app.
fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(path)
        .spawn()
        .map_err(|e| format!("Could not open {}: {}", path.display(), e))?;
    Ok(())
}

#[tauri::command]
pub async fn api_open_final_notes_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let folder = load_settings(&app).folder;
    fs::create_dir_all(&folder).map_err(|e| format!("Could not create {}: {}", folder.display(), e))?;
    open_path(&folder)
}

#[derive(Serialize)]
pub struct ClaudeExtension {
    pub extension_path: String,
    pub folder: String,
}

/// Open the bundled Claude Desktop extension (built from
/// mcp/finalnotes-readonly) so Claude Desktop offers to install it. The
/// extension gives Claude read-only access to the FinalNotes folder.
#[tauri::command]
pub async fn api_install_claude_extension<R: Runtime>(app: AppHandle<R>) -> Result<ClaudeExtension, String> {
    use tauri::path::BaseDirectory;
    let extension = app
        .path()
        .resolve("resources/finalnotes-readonly.mcpb", BaseDirectory::Resource)
        .map_err(|e| format!("Could not find the Claude extension: {}", e))?;
    if !extension.exists() {
        return Err("This build does not include the Claude extension.".to_string());
    }
    let folder = load_settings(&app).folder;
    fs::create_dir_all(&folder).map_err(|e| format!("Could not create {}: {}", folder.display(), e))?;
    open_path(&extension)?;
    Ok(ClaudeExtension {
        extension_path: extension.to_string_lossy().to_string(),
        folder: folder.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn when() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-11T14:05:00+03:00").unwrap()
    }

    fn meeting() -> ExportMeeting {
        ExportMeeting {
            id: "meeting-1234".into(),
            title: "Q4 budget: review / sign-off?".into(),
            started: when(),
            duration_seconds: Some(41.0 * 60.0 + 20.0),
            participants: vec!["Moshe".into(), "Dana".into(), "Speaker 2".into()],
            ai_notes: Some("## Decisions\n- Budget approved\n### Detail\n```\n# not a heading\n```".into()),
            my_notes: Some("- budget\n- hire".into()),
            lines: vec![
                ExportLine { start: Some(5.0), speaker: "Moshe".into(), text: "Let's start.".into() },
                ExportLine { start: Some(3725.0), speaker: "Dana".into(), text: "Approved.".into() },
                ExportLine { start: None, speaker: String::new(), text: "  ".into() },
            ],
        }
    }

    #[test]
    fn file_names_are_safe_dated_and_tagged() {
        let name = file_name(&meeting());
        assert_eq!(name, format!("2026-10-11 1405 Q4 budget review sign-off [{}].md", meeting_tag("meeting-1234")));
        assert_eq!(safe_title(" ..CON.. "), "..CON");
        assert_eq!(safe_title("???"), "Meeting");
        assert_eq!(safe_title(&"a".repeat(200)).len(), MAX_TITLE_CHARS);
        assert_ne!(meeting_tag("meeting-1"), meeting_tag("meeting-2"));
    }

    #[test]
    fn labels_match_the_app() {
        let names = HashMap::from([("Speaker 1".to_string(), "Dana".to_string()), ("mic".to_string(), "Moshe".to_string())]);
        assert_eq!(speaker_label(Some("mic"), &names), "Moshe");
        assert_eq!(speaker_label(Some("Speaker 1"), &names), "Dana");
        assert_eq!(speaker_label(Some("system"), &names), "Them");
        assert_eq!(speaker_label(Some("Speaker 2"), &names), "Speaker 2");
        assert_eq!(speaker_label(Some("Audio"), &names), "");
        assert_eq!(speaker_label(None, &names), "");
    }

    #[test]
    fn renders_front_matter_sections_and_transcript() {
        let md = render_markdown(&meeting(), when());
        assert!(md.starts_with("---\ntitle: \"Q4 budget: review / sign-off?\"\ndate: 2026-10-11T14:05:00+03:00\nduration_minutes: 41\n"));
        assert!(md.contains("participants: [\"Moshe\", \"Dana\", \"Speaker 2\"]\nmeeting_id: \"meeting-1234\"\n"));
        assert!(md.contains("# Q4 budget: review / sign-off?\n\n**Date:** Sun 11 Oct 2026, 14:05 · **Duration:** 41 min · **Participants:** Moshe, Dana, Speaker 2\n"));
        // AI note headings sit under "## AI notes"; code blocks are left alone.
        assert!(md.contains("## AI notes\n\n### Decisions\n- Budget approved\n#### Detail\n```\n# not a heading\n```\n"));
        assert!(md.contains("## My notes\n\n- budget\n- hire\n"));
        assert!(md.contains("## Transcript\n\n[00:05] Moshe: Let's start.  \n[1:02:05] Dana: Approved.  \n"));
        let order = ["## AI notes", "## My notes", "## Transcript"].map(|h| md.find(h).unwrap());
        assert!(order[0] < order[1] && order[1] < order[2]);
    }

    #[test]
    fn missing_parts_are_left_out() {
        let mut m = meeting();
        m.ai_notes = None;
        m.my_notes = Some("  ".into());
        m.duration_seconds = None;
        m.lines.clear();
        let md = render_markdown(&m, when());
        assert!(!md.contains("## AI notes") && !md.contains("## My notes") && !md.contains("duration"));
        assert!(md.contains("## Transcript\n\nNo transcript.\n"));
    }

    #[test]
    fn summary_markdown_is_read_from_the_result_json() {
        assert_eq!(summary_markdown(r###"{"markdown":"## A\n- b","english_cache":{}}"###).as_deref(), Some("## A\n- b"));
        assert_eq!(summary_markdown(r#"{"summary_json":[]}"#), None);
        assert_eq!(summary_markdown("not json"), None);
    }

    #[test]
    fn export_replaces_the_previous_file_and_leaves_others_alone() {
        let dir = std::env::temp_dir().join(format!("finalnotes-test-{}", Uuid::new_v4()));
        let other = dir.join("2026-01-01 0900 Other [deadbeef].md");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&other, "keep").unwrap();

        let mut m = meeting();
        let first = write_export(&dir, &m, when()).unwrap();
        m.title = "Renamed".into();
        let second = write_export(&dir, &m, when()).unwrap();

        assert!(!first.exists());
        assert!(fs::read_to_string(&second).unwrap().contains("# Renamed"));
        assert_eq!(fs::read_to_string(&other).unwrap(), "keep");
        let leftovers: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".tmp")).collect();
        assert!(leftovers.is_empty());

        remove_export_files(&dir, &m.id);
        assert!(!second.exists());
        assert!(other.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn exports_a_meeting_from_the_database() {
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Standup', '2026-10-11T11:00:00Z', '2026-10-11T11:00:00Z')")
            .execute(&pool).await.unwrap();
        for (id, start, end, speaker, text) in [
            ("a", 0.0, 2.0, "mic", "Morning."),
            ("b", 2.5, 6.0, "Speaker 1", "Release is on track."),
        ] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, speaker) VALUES (?, 'm1', ?, 'now', ?, ?, ?)")
                .bind(id).bind(text).bind(start).bind(end).bind(speaker)
                .execute(&pool).await.unwrap();
        }
        SpeakerNameRepository::rename(&pool, "m1", "Speaker 1", "Dana").await.unwrap();
        MeetingNotesRepository::save(&pool, "m1", "- release date?").await.unwrap();
        sqlx::query("INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result) VALUES ('m1', 'completed', '2026-10-11T12:00:00Z', '2026-10-11T12:00:00Z', ?)")
            .bind(r###"{"markdown":"## Status\n- On track"}"###)
            .execute(&pool).await.unwrap();

        let dir = std::env::temp_dir().join(format!("finalnotes-db-{}", Uuid::new_v4()));
        let path = export_meeting(&pool, "m1", &dir).await.unwrap();
        let md = fs::read_to_string(&path).unwrap();
        assert!(md.contains("participants: [\"Me\", \"Dana\"]"));
        assert!(md.contains("duration_minutes: 1\n"));
        assert!(md.contains("## AI notes\n\n### Status\n- On track"));
        assert!(md.contains("## My notes\n\n- release date?"));
        assert!(md.contains("[00:00] Me: Morning.  \n[00:02] Dana: Release is on track.  \n"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
