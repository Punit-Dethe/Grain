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
  readonly net: {
    fetch(
      url: string,
      options?: {
        method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
        headers?: Record<string, string>;
        body?: string;
        secret?: { key: string; header: string; prefix?: string };
        /** Use this extension's single vaulted account. Grain attaches its Bearer token;
         * the token itself is never returned to extension code. Mutually
         * exclusive with `secret`. */
        auth?: true;
      },
    ): Promise<{
      status: number;
      ok: boolean;
      headers: Record<string, string>;
      body: string;
      url: string;
    }>;
  };
  readonly auth: {
    status(): Promise<GrainAuthConnection | null>;
    connect(): Promise<GrainAuthConnection>;
    disconnect(): Promise<unknown>;
  };
  /** Register exact declared tools. Only explicit validated arguments arrive. */
  actions(handlers: Record<string, (args: { [key: string]: JsonValue }) =>
    { title?: string; body?: string } | Promise<{ title?: string; body?: string }>>): void;
}

export interface GrainAuthConnection {
  provider_name: string;
  authorization_host: string;
  token_host: string;
  scopes: string[];
  api_hosts: string[];
  state:
    | "connected"
    | "needs_reauthorization"
    | "expired"
    | "disconnected"
    | "unavailable";
  granted_scopes: string[];
  expires_at: string | null;
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
