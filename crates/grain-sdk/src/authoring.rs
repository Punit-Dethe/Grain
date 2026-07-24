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
    #[serde(default)]
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

export type GrainActivation =
  | DaemonEvent
  | { Shortcut: { id: string } }
  | { Session: { mode: string } };

export type GrainErrorCode =
  | "E_CAPABILITY_DENIED"
  | "E_TIMEOUT"
  | "E_SESSION_BUSY"
  | "E_QUOTA"
  | "E_RESPONSE_TOO_LARGE"
  | "E_INVALID_MANIFEST"
  | "E_INVALID_ARGUMENT"
  | "E_NOT_IMPLEMENTED"
  | "E_UNKNOWN_METHOD"
  | "E_UNAVAILABLE"
  | "E_INTERNAL";

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
  /** The foreground app right now (needs capture:app), or null. */
  focusedApp(): Promise<{
    name: string;
    exe: string | null;
    exePath: string | null;
    urlHost: string | null;
  } | null>;
  readonly settings: {
    get<T extends JsonValue = JsonValue>(key: string): Promise<T | null>;
    set(key: string, value: JsonValue): Promise<unknown>;
  };
  readonly llm: {
    complete(prompt: string): Promise<string>;
  };
  readonly net: {
    fetch(
      url: string,
      options?: {
        method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
        headers?: Record<string, string>;
        body?: string;
        secret?: { key: string; header: string; prefix?: string };
      },
    ): Promise<{
      status: number;
      ok: boolean;
      headers: Record<string, string>;
      body: string;
      url: string;
    }>;
  };
  embed(texts: string[]): Promise<number[][]>;
  readonly open: {
    /** Open a link in the user's browser. Host allows only http/https/mailto/tel. */
    url(url: string): Promise<unknown>;
    /** Launch a user-approved application by path (see pickApp). */
    app(path: string): Promise<unknown>;
    /** Ask the user to choose an application; resolves to its path (approved for
     * this extension) or null if cancelled. The only way to make a path launchable. */
    pickApp(): Promise<string | null>;
  };
  readonly workspace: {
    open(payload?: JsonValue): Promise<unknown>;
    close(): Promise<unknown>;
  };
  readonly overlay: {
    show(payload?: JsonValue): Promise<unknown>;
    dismiss(): Promise<unknown>;
  };
  readonly session: {
    start(options: { mode: string }): Promise<unknown>;
  };

  onTransform(handler: (text: string) => string | Promise<string>): void;
  onSessionStage(
    handler: (
      text: string,
      context: { readonly mode: string; readonly signal: AbortSignal },
    ) =>
      | string
      | { text?: string; handled?: boolean }
      | Promise<string | { text?: string; handled?: boolean }>,
  ): void;
  /** @deprecated Use onSessionStage. */
  onSessionResult(
    handler: (text: string) =>
      | string
      | { text?: string; handled?: boolean }
      | Promise<string | { text?: string; handled?: boolean }>,
  ): void;
  onShortcut(handler: (id: string) => void | Promise<void>): void;
  onEvent(handler: (event: DaemonEvent) => void): void;
}

declare global {
  interface GrainError extends Error {
    readonly name: "GrainError";
    readonly code: GrainErrorCode;
    readonly hint: string;
    readonly docs: string;
    readonly capability?: string;
  }
  const grain: GrainApi;
}
"#;
