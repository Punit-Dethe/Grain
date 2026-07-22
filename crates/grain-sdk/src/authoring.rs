//! Author-facing extension project contract.
//!
//! Installed Phase-1 packs embed their JavaScript in `entry_source`. Developer
//! projects keep source in a real file so bundlers can produce useful source
//! maps. `grain-ext` reads this wrapper, builds [`entry`], then hands the host
//! the existing [`ExtensionManifest`] contract.

use serde::{Deserialize, Serialize};

use crate::ExtensionManifest;

/// `manifest.json` at the root of an unpacked extension project.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionProjectManifest {
    #[serde(flatten)]
    pub manifest: ExtensionManifest,
    /// Project-relative TypeScript/JavaScript entry file.
    pub entry: String,
}

/// The author-facing `grain` global. `grain-ext` combines this declaration with
/// event types reflected from `grain-sdk` and the current capability union.
/// Keeping the API declaration here makes the SDK the source copied into every
/// scaffold instead of letting a CLI template drift from the wire contract.
pub const GRAIN_API_TYPESCRIPT: &str = r#"export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export type GrainActivation = DaemonEvent | { Shortcut: { id: string } };

export interface GrainApi {
  readonly activation: GrainActivation | null;
  readonly caps: readonly GrainCapability[];
  readonly extId: string;

  readonly log: {
    info(message: string): Promise<unknown>;
    warn(message: string): Promise<unknown>;
  };
  readonly storage: {
    get<T extends JsonValue = JsonValue>(key: string): Promise<T | null>;
    set(key: string, value: JsonValue): Promise<unknown>;
    delete(key: string): Promise<unknown>;
  };
  readonly doc: {
    get<T extends JsonValue = JsonValue>(key: string): Promise<T | null>;
    put(key: string, value: JsonValue): Promise<unknown>;
    delete(key: string): Promise<unknown>;
    list(): Promise<string[]>;
  };
  captureSelection(): Promise<string | null>;
  readonly settings: {
    get<T extends JsonValue = JsonValue>(key: string): Promise<T | null>;
    set(key: string, value: JsonValue): Promise<unknown>;
  };
  readonly llm: {
    complete(prompt: string): Promise<string>;
  };
  embed(texts: string[]): Promise<number[][]>;
  readonly workspace: {
    open(payload?: JsonValue): Promise<unknown>;
    close(): Promise<unknown>;
  };
  readonly overlay: {
    show(payload?: JsonValue): Promise<unknown>;
    dismiss(): Promise<unknown>;
  };

  onTransform(handler: (text: string) => string | Promise<string>): void;
  onSessionResult(handler: (text: string) => void | Promise<void>): void;
  onShortcut(handler: (id: string) => void | Promise<void>): void;
  onEvent(handler: (event: DaemonEvent) => void): void;
}

declare global {
  const grain: GrainApi;
}
"#;
