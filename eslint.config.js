import i18next from "eslint-plugin-i18next";
import tsParser from "@typescript-eslint/parser";

// [GRAIN] Files still calling the backend by raw string instead of the generated
// `commands.*` bindings. Untyped: a renamed command fails silently at runtime,
// in whatever screen nobody happened to open. See docs/UI 2.0/PLAN.md §5.2.
//
// This list may SHRINK, never grow. It is not a backlog to work through — every
// file on it is legacy UI that the rewrite deletes, so converting it now would
// be effort spent on code with a known expiry. The rule exists to stop the NEW
// tree from inheriting the habit.
const LEGACY_RAW_INVOKE = [
  "src/components/settings/experimentations/OverviewSection.tsx",
  "src/components/settings/experimentations/ExtensionSettings.tsx",
  "src/components/settings/experimentations/ExtensionDetail.tsx",
  "src/components/settings/experimentations/ExperimentationsSettings.tsx",
  "src/components/settings/experimentations/DeveloperSection.tsx",
  "src/components/settings/experimentations/AgentSection.tsx",
  "src/components/settings/grain-space/McpBridge.tsx",
  "src/components/settings/post-processing/PostProcessingSettings.tsx",
];

// Files still subscribing to backend events by hand-typed string. Same rule as
// above, same reason: a renamed event fails silently at runtime. The eleven
// events the main window consumes are now typed (src-tauri/src/grain_events.rs)
// and reachable as `events.modelStateChanged.listen(...)`.
//
// Shrink-only, and mostly retired by deletion rather than conversion. Not all
// of these are legacy: `LiveLogViewer` listens to `log://log`, and the
// grain-space pair to `grain-space://*` — names no Rust type can spell, so they
// are staying on the raw API until those channels get a typed home. Exact paths
// only, no directory globs: a glob would silently re-exempt files added later.
const LEGACY_RAW_LISTEN = [
  "src/App.tsx",
  "src/components/grain-space/GrainSpaceOverlay.tsx",
  "src/components/model-selector/ModelSelector.tsx",
  "src/components/quick-panel/useSystemStatus.ts",
  "src/components/settings/HandyKeysShortcutInput.tsx",
  "src/components/settings/debug/LiveLogViewer.tsx",
  "src/components/settings/grain-space/GrainSpaceSettings.tsx",
  "src/components/update-checker/UpdateChecker.tsx",
  "src/stores/modelStore.ts",
  "src/stores/settingsStore.ts",
];

// Permanent exemptions, not legacy: `extension-host.ts` and
// `extension-surface.ts` are their own Vite entries — lean, React-free pages for
// the hidden supervisor and the sandboxed surface wrapper. `commands` and
// `events` live in the same module, so importing either retains all ~204 command
// wrappers; that is a real cost to pay on two pages that make five calls
// between them. `bindings.ts` is the generated file the rules point everyone at.
const STANDALONE_ENTRIES = ["src/extension-host.ts", "src/extension-surface.ts"];
const GENERATED = [...STANDALONE_ENTRIES, "src/bindings.ts"];

const NO_RAW_INVOKE = {
  name: "@tauri-apps/api/core",
  importNames: ["invoke"],
  message:
    "Call the backend through the generated bindings: import { commands } from '@/bindings'. Raw invoke() is untyped — a renamed command fails silently at runtime. (docs/UI 2.0/PLAN.md §5.2)",
};

const NO_RAW_LISTEN = {
  name: "@tauri-apps/api/event",
  importNames: ["listen"],
  message:
    "Subscribe through the generated bindings: import { events } from '@/bindings', then events.modelStateChanged.listen(...). Raw listen() is untyped — a renamed event silently never fires. (docs/UI 2.0/PLAN.md §5.2)",
};

// Flat config REPLACES a rule's options rather than merging them, so the last
// block matching a file decides its whole restricted-import set. The two
// allowlists are not the same list, which means each file needs exactly one
// block carrying every restriction that still applies to it — hence three
// blocks rather than two independent bans.
const restrict = (...paths) => ({
  "no-restricted-imports": ["error", { paths }],
});

export default [
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaFeatures: {
          jsx: true,
        },
      },
    },
    plugins: {
      i18next,
    },
    rules: {
      // Catch text in JSX that should be translated
      "i18next/no-literal-string": [
        "error",
        {
          markupOnly: true, // Only check JSX content, not all strings
          ignoreAttribute: [
            "className",
            "style",
            "type",
            "id",
            "name",
            "key",
            "data-*",
            "aria-*",
          ], // Ignore common non-translatable attributes
        },
      ],
    },
  },
  {
    // [GRAIN] The frontend→backend contract is the GENERATED bindings, nothing
    // else — commands out, events in. Everything reachable this way is
    // type-checked end to end, so a renamed command or a changed payload breaks
    // the build instead of a user's click.
    files: ["src/**/*.{ts,tsx}"],
    ignores: [...LEGACY_RAW_INVOKE, ...LEGACY_RAW_LISTEN, ...GENERATED],
    rules: restrict(NO_RAW_INVOKE, NO_RAW_LISTEN),
  },
  {
    // Legacy on invoke only — still held to the event contract.
    files: LEGACY_RAW_INVOKE,
    ignores: [...LEGACY_RAW_LISTEN, ...GENERATED],
    rules: restrict(NO_RAW_LISTEN),
  },
  {
    // Legacy on listen only — still held to the command contract.
    files: LEGACY_RAW_LISTEN,
    ignores: [...LEGACY_RAW_INVOKE, ...GENERATED],
    rules: restrict(NO_RAW_INVOKE),
  },
];
