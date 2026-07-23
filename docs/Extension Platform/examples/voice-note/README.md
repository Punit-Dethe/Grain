# Voice Note extension

This scripted developer extension proves the complete extension-owned session
path. Its contributed shortcut starts Grain's host-owned recorder in `note`
mode. The slow stage structures the transcript through `grain.llm`, stores the
result in the extension's document namespace, and returns `handled: true` so
Grain does not also paste the text.

Start from a normal `grain-ext init` scaffold, replace its manifest and
`src/main.ts` with these files, then run:

```powershell
npm install
npm run build
grain-ext doctor
grain-ext dev
```

Enable the extension and press `Ctrl+Shift+N` to start and stop the session.
The pill identifies Voice Note as the owner while recording. The structured
note is stored under a `voice-note-*` document key.

If the worker crashes, reloads, times out, or returns an invalid stage result,
Grain falls back to the untouched transcript. Extension-owned stages may change
the words, but they cannot lose them.
