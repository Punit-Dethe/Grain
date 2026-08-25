# Recommendation Lab

The Recommendation Lab installs synthetic extensions through Grain's normal
load-unpacked path. It does not provide a mock renderer or alternate ranking
pipeline. Every fixture uses the same packaged 512 px Grain icon, requests zero
permissions, and calls no external service.

## Run

1. Start the real app with `bun run dev:asr`.
2. In Settings, enable **Experimentation** and **Developer mode**.
3. Open **Extensions → Developer → Recommendation Lab**.
4. Choose **Core 6** for focused interaction testing or **Stress 24** for the
   full ranking and scrolling corpus.
5. Press `Alt+Shift+Enter`, speak a request, then press the shortcut again to
   stop capture (or release it when push-to-talk is enabled).
6. Use **Remove lab** when finished. Turning off Developer mode also unloads
   the fixtures and removes their generated files.

## Core matrix

| Goal                   | Example request                                   | Expected fixture/path                                                  |
| ---------------------- | ------------------------------------------------- | ---------------------------------------------------------------------- |
| Close topical overlap  | “Play my downloaded jazz album”                   | Stream Music and Music Library compete; chooser remains usable         |
| Close work overlap     | “Create a repository issue for the login crash”   | Code Host and Issue Tracker compete                                    |
| Named match            | “Use Calendar Planner to schedule lunch tomorrow” | Calendar Planner is a named recommendation; editable confirmation form |
| Internal translation   | “Translator, identify this language”              | Translator opens its same-window internal command diagnostic           |
| Internal issue command | “Create an urgent issue for the broken login”     | Issue Tracker ranks its own eight commands after hand-off              |
| Internal music command | “Stream Music, play my focus playlist”            | Stream Music ranks its own seven commands after hand-off               |

## Internal command matrix

The first five Core fixtures rank deliberately overlapping internal commands
with both `grain.match.lexical` and `grain.match.semantic`. The same extension
window then reports the top five scores and one of these outcomes:

- **Suggested**: multiple commands cleared the semantic floor but remain within
  the 8-point decision margin. Up to three Grain-owned actions resolve the
  ambiguity in-place.
- **Executed**: one command is a clear winner. Execution is simulated; the lab
  never touches an external service.
- **Auto-send**: added only when the winning command is explicitly safe and is
  independently a semantic winner by at least 15 points. Lexical evidence can
  never enable this badge.
- **Unresolved / Semantic unavailable**: no command executes. The score table
  remains available for diagnosis.

Use these as adversarial starting points; the displayed score and margin are
the result to inspect, not a hard-coded expected value:

| Goal                     | Extension     | Example request                                                          |
| ------------------------ | ------------- | ------------------------------------------------------------------------ |
| Play versus queue        | Stream Music  | “Play this one, but maybe make it the next thing after the current song” |
| Playlist versus library  | Music Library | “Save this track with my focus music”                                    |
| Create versus find issue | Issue Tracker | “Check whether there is a login bug, and open one if there is not”       |
| Review versus merge      | Code Host     | “Go through these approved changes before they land on main”             |
| Translate versus define  | Translator    | “Tell me what this French phrase means in context”                       |
| Semantic-only paraphrase | Stream Music  | “Put on the collection I made for dinner”                                |
| Safe Auto-send candidate | Translator    | “Identify the language without translating the paragraph”                |

To verify honest degradation, temporarily remove the semantic model and repeat
a named command such as “Stream Music, skip track”. The view must report
**Semantic unavailable**, retain the lexical score, and execute nothing.

## Stress paths

Install **Stress 24**, then cover these additional outcomes:

| Goal                       | Example request                              | Expected fixture/path                                           |
| -------------------------- | -------------------------------------------- | --------------------------------------------------------------- |
| Long result + text actions | “Meeting Notes, summarize these decisions”   | Long result; test Copy, Insert, and Replace selection           |
| No-message completion      | “Timer, start a timer for twenty minutes”    | Completion result with no extension message                     |
| Destructive treatment      | “File Organizer, delete duplicate downloads” | Grain-owned danger confirmation                                 |
| Terminal failure           | “File Organizer, move this protected file”   | Error result; app remains dismissible with Escape               |
| Decline and reroute        | “Travel Planner, navigate to the airport”    | Travel Planner declines; chooser reopens without it             |
| Long name and list         | Search for “knowledge”                       | Long extension name wraps/truncates safely among 24 entries     |
| Per-extension Auto-send    | Open Translator's extension details          | Beta toggle persists; global switch remains in Capture settings |

For Insert/Replace, focus a normal text editor before starting capture. Select
text before capture to verify Replace; leave a caret with no selection to verify
Insert. Auto-send still requires the on-device semantic model and a sufficiently
clear topical match; that model/performance gate is intentionally outside this
fixture setup.
