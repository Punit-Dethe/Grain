// Compatibility types for routes/bindings pending their final removal.
// Extensions no longer contribute settings, shortcuts or core-feature shelves.
import type React from "react";
import type { StoreEntry } from "@/bindings";

type SettingKindName =
  | "bool"
  | "string"
  | "secret"
  | "number"
  | "select"
  | "shortcut"
  | "color"
  | "slider"
  | "app_path"
  | "url"
  | "list"
  | "unsupported";

/** The SCHEMA of one field (no value) — mirror of Rust `ExtensionSettingField`.
 * Recursive: a `list` field carries its own `fields`. */
export interface SettingField {
  key: string;
  label: string;
  description: string;
  kind: SettingKindName;
  min: number | null;
  max: number | null;
  step: number | null;
  options: { value: string; label: string }[];
  fields: SettingField[];
  item_label: string | null;
}

/** Mirror of the Rust `ExtensionSettingRow` (grain_commands.rs). Local type
 * until the next dev run regenerates bindings.ts — never hand-edit bindings. */
export interface SettingRow {
  key: string;
  label: string;
  description: string;
  kind: SettingKindName;
  anchor: string | null;
  order: number;
  value: unknown;
  notice: string | null;
  min: number | null;
  max: number | null;
  step: number | null;
  options: { value: string; label: string }[];
  fields: SettingField[];
  item_label: string | null;
}

export interface SettingsSection {
  id: string;
  name: string;
  rows: SettingRow[];
}

/** Anchors this build renders (SPEC §4.3 v1, mirroring grain-sdk's `ANCHORS`).
 * A row whose anchor is absent from this list is NOT an error — it falls back
 * to the extension's own section, because settings are never lost. */
export const ANCHORS = [
  "snippets.after",
  "dictation.pipeline.after",
  "context.after",
  "agent.after",
  "models.after",
] as const;

export type Anchor = (typeof ANCHORS)[number];

export interface ExtensionAnchorSnapshot {
  installedCount: number;
  loading: boolean;
  error: string | null;
}

// No listeners, requests or retained state for retired contribution surfaces.
export const ExtensionSettings: React.FC<{
  section: SettingsSection;
  rows?: SettingRow[];
  onChanged?: () => void;
}> = () => null;

export const ExtensionShortcuts: React.FC<{ id: string }> = () => null;

export const ExtensionAnchor: React.FC<{
  anchor: Anchor;
  catalogueEntries?: StoreEntry[];
  onSnapshot?: (snapshot: ExtensionAnchorSnapshot) => void;
}> = () => null;
