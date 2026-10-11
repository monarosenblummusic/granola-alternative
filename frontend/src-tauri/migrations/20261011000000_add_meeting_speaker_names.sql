-- Fork addition: per-meeting display names for transcript speakers.
-- transcripts.speaker keeps the raw label ("mic", "system", "Speaker 1");
-- this table maps it to a name the user chose. Renaming never rewrites the
-- transcript, so the echo filter and speaker identification keep working.
CREATE TABLE IF NOT EXISTS meeting_speaker_names (
    meeting_id TEXT NOT NULL,
    speaker TEXT NOT NULL,
    display_name TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
