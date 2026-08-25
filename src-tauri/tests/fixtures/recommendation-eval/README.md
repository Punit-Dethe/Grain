# Recommendation eval fixture

`v1/golden.json` is the first cross-extension recommendation corpus. It is
deliberately small enough for source review but contains close competitors,
explicit names, ambiguous acceptable sets, ASR-like spellings, and true
out-of-scope requests.

Run the real headless app path from `src-tauri` after the current semantic model
has been installed through Grain:

```powershell
cargo run -- --eval tests/fixtures/recommendation-eval/v1/golden.json --json
```

The file uses `semanticMode: "required"`, so CI or a developer machine cannot
silently record a name-only result as the semantic baseline. The accepted modes
are `required` (the default), `optional`, and `disabled`; use `disabled` only for
an intentional alias-routing test.

Its `qualityGates` encode the recorded RQ0 baseline. A regression still prints
the human or JSON report, then exits with code 2 so CI can fail without a second
comparison tool.

`v1/name-only-smoke.json` is the checked-in exception: it exercises CLI
dispatch, manifest loading, implicit display-name aliases, and no-match behavior
without requiring the optional semantic model.

Manifest `recommend.examples` are index data. Golden `cases` are held-out
phrases and must not be copied into those examples.
