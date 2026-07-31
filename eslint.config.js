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

// Permanent exemptions, not legacy: `extension-host.ts` and
// `extension-surface.ts` are their own Vite entries — lean, React-free pages for
// the hidden supervisor and the sandboxed surface wrapper. `commands` is a
// single object literal, so importing it retains all ~204 wrappers; that is a
// real cost to pay on two pages that make five calls between them.
const STANDALONE_ENTRIES = ["src/extension-host.ts", "src/extension-surface.ts"];

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
    // else. Everything reachable this way is type-checked end to end, so a
    // renamed command or a changed payload breaks the build instead of a user's
    // click.
    files: ["src/**/*.{ts,tsx}"],
    ignores: [...LEGACY_RAW_INVOKE, ...STANDALONE_ENTRIES, "src/bindings.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/api/core",
              importNames: ["invoke"],
              message:
                "Call the backend through the generated bindings: import { commands } from '@/bindings'. Raw invoke() is untyped — a renamed command fails silently at runtime. (docs/UI 2.0/PLAN.md §5.2)",
            },
          ],
        },
      ],
    },
  },
];
