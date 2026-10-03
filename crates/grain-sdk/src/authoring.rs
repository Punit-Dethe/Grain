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
/// the tool-only capability union, never the internal daemon/pill protocol.
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

/** Only arguments validated against this tool's manifest are passed in. */
export type GrainToolArguments = { [key: string]: JsonValue };

export interface GrainToolContext {
  /** Opaque key for this prepared write, or null for a read. Forward it only
   * when your service supports idempotency. It does not authorize a call,
   * guarantee deduplication, or permit automatic retries. */
  readonly idempotencyKey: string | null;
}

/** Bounded display data. Grain owns provenance, receipts and rendering.
 * Additional scalar fields can be shown as details; arbitrary nested JSON
 * is not currently retained as structured model output. */
export interface GrainToolDisplay {
  title?: string;
  body?: string;
  details?: { label: string; value: string }[];
}

export interface GrainToolData extends GrainToolDisplay {
  [key: string]: JsonValue | undefined;
}

export type GrainToolErrorClass =
  | "auth"
  | "network"
  | "invalid_argument"
  | "not_found"
  | "rate_limited"
  | "cancelled"
  | "internal";

/** Return exactly one envelope. Plain display data/string/null also work for
 * compatibility. Error messages are not shown verbatim by the host. Auth
 * errors do not automatically open sign-in. Extension-authored follow-up
 * interactions are unsupported and intentionally absent here. */
export type GrainToolResult =
  | (GrainToolDisplay & { ok?: never; error?: never; needsInteraction?: never })
  | string
  | null
  | { ok: GrainToolData | string | null; error?: never; needsInteraction?: never }
  | {
      error: { class?: GrainToolErrorClass; message?: string };
      ok?: never;
      needsInteraction?: never;
    };

export type GrainToolHandler = (
  args: GrainToolArguments,
  context: GrainToolContext,
) => GrainToolResult | Promise<GrainToolResult>;

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
  actions(handlers: Record<string, GrainToolHandler>): void;
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
