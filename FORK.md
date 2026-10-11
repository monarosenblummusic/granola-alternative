# Fork notice

This project is a fork of [Meetily](https://github.com/Zackriya-Solutions/meetily)
by Zackriya Solutions, imported at upstream release **v0.4.1**
(commit `a2cb62e827da7ef59f65064c97233efb2313878e`, 2026-09-10) under the MIT
License. The upstream license is kept unchanged in `LICENSE.md`.

Removed from the import: `frontend/vs_buildtools.exe` (Microsoft installer,
download it from Microsoft instead) and two demo GIFs under `docs/`.

## Goal of this fork

A bot-free, Granola-style meeting notes app for Windows:

- Separate "Me" (microphone) and "Them" (system audio) tracks instead of one mixed track
- Deepgram cloud transcription (live + final pass), local engines kept as fallback
- Notepad during the call, then AI "Enhance" that merges notes + transcript
- Speaker separation on the "Them" track, word-level speaker assignment
- Calendar-aware meeting detection, Ctrl+Alt+R toggle
- FinalNotes export and a read-only MCP server so Claude can query meetings
