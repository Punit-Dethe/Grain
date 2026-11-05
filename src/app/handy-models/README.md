# Handy model catalogue

This is the explicitly requested catalogue UI port from Handy
`82dfab9fda18c4bc24e1f6e56d36474d38eca0bc`, using
`@handy-computer/models` 0.2.3. Keep the model table, language search,
ranking, grading and download/selection behavior aligned with that source.
This directory is an intentional, scoped exception to the frontend freeze;
it is not a second ASR engine, model registry or selection store.

The directory mirrors upstream `src/` paths. Imports point here unless they
use Grain's generated bindings or settings/model stores.
Local adaptations:

- `modelJobs.ts` connects the table's lock UI to Grain's existing file-job
  snapshot/events, with cleanup when the catalogue unmounts. Backend admission
  remains authoritative.
- `lib/models/source.ts` retains only the repository join; ONNX migration
  controls and `RetiredModelsNotice` are omitted.
- PreferredLanguages is a searchable control in the table toolbar alongside
  Live preview and Translate, rather than a separate setting row. Preferences rank the
  catalogue; they do not change recognition language or automatically switch
  models. No system-language plugin is installed by this port.
- `ModelPicker` mounts this same catalogue in Settings and onboarding, with
  Grain's error display and a bounded scrolling area. Theme aliases live in
  `app.css`; Grain's global controls are not replaced by upstream controls.
  With a selected model it starts collapsed, showing its name, size, speed and
  first preferred language's accuracy through Handy's existing metric components.
  Expanding mounts the full catalogue; selecting a different model closes it.
  Delete controls are available only in the expanded list.
- Catalogue translations come from Handy; product instructions name Grain
  while benchmark provenance continues to identify Handy's measurements.

See `Upstream/DICTATION-CONVERGENCE-2026-11-06.md` for backend scope and checks.
