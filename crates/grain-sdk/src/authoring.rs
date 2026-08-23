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

/** Grain-rendered standard extension UI. Authors control structure and state;
 * Grain owns every actual element, style, focus rule, and trusted action bar. */
export type GrainViewGap = "xs" | "sm" | "md" | "lg";
export type GrainViewTone =
  | "neutral"
  | "muted"
  | "info"
  | "success"
  | "warning"
  | "danger";
export type GrainViewNode =
  | { type: "stack"; gap?: GrainViewGap; children?: GrainViewNode[] }
  | {
      type: "inline";
      gap?: GrainViewGap;
      align?: "start" | "center" | "end" | "stretch" | "space_between";
      wrap?: boolean;
      children?: GrainViewNode[];
    }
  | { type: "grid"; columns: 1 | 2 | 3; gap?: GrainViewGap; children?: GrainViewNode[] }
  | { type: "section"; title?: string; children?: GrainViewNode[] }
  | { type: "divider" }
  | { type: "heading"; text: string; level?: "one" | "two" | "three" }
  | { type: "text"; text: string; tone?: GrainViewTone }
  | { type: "badge"; text: string; tone?: GrainViewTone }
  | { type: "metadata"; label: string; value: string }
  | {
      type: "text_field";
      id: string;
      label: string;
      value?: string;
      placeholder?: string;
      required?: boolean;
      disabled?: boolean;
      maxLength?: number;
    }
  | {
      type: "text_area";
      id: string;
      label: string;
      value?: string;
      placeholder?: string;
      rows?: number;
      required?: boolean;
      disabled?: boolean;
      maxLength?: number;
    }
  | {
      type: "select";
      id: string;
      label: string;
      value?: string;
      options: readonly { value: string; label: string }[];
      required?: boolean;
      disabled?: boolean;
    }
  | {
      type: "checkbox";
      id: string;
      label: string;
      checked?: boolean;
      required?: boolean;
      disabled?: boolean;
    };

export interface GrainView {
  version?: 1;
  title: string;
  description?: string;
  root: GrainViewNode;
  actions?: readonly {
    id: string;
    label: string;
    intent?: "primary" | "secondary" | "danger" | "cancel";
    kind?: "submit" | "cancel";
    disabled?: boolean;
  }[];
}

export type GrainViewValue = string | boolean;
export type GrainViewEvent =
  | {
      kind: "change";
      target: string;
      value: GrainViewValue;
      values: Readonly<Record<string, GrainViewValue>>;
    }
  | {
      kind: "submit";
      target: string;
      values: Readonly<Record<string, GrainViewValue>>;
    }
  | { kind: "cancel" };

export type GrainViewReply =
  | { view: GrainView }
  | { message?: string }
  | { decline: string }
  | { error: string };

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
  /** The foreground window's visible text from its accessibility tree — never a
   * screenshot (needs capture:screen-text). Null when nothing is readable. */
  screenText(): Promise<string | null>;
  /** A PNG of the foreground window (needs capture:screen-image), or null.
   * Grain's own features never capture one — this exists so an extension the
   * user installed deliberately can. Feed it straight to `llm.complete`. */
  screenImage(): Promise<{
    mime: string;
    width: number;
    height: number;
    base64: string;
  } | null>;
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
    /** Complete a prompt. Pass `image` (from `screenImage()`) to ask about a
     * picture; a model that cannot read images is retried without it by the
     * host, so this still resolves to an answer. */
    complete(
      prompt: string,
      image?: { base64: string; mime?: string },
    ): Promise<string>;
  };
  readonly net: {
    fetch(
      url: string,
      options?: {
        method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
        headers?: Record<string, string>;
        body?: string;
        secret?: { key: string; header: string; prefix?: string };
        /** Manifest authentication id. Grain attaches its vaulted Bearer token;
         * the token itself is never returned to extension code. Mutually
         * exclusive with `secret`. */
        auth?: string;
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
    status(id: string): Promise<GrainAuthConnection | null>;
    connect(id: string): Promise<GrainAuthConnection>;
    disconnect(id: string): Promise<unknown>;
  };
  embed(texts: string[]): Promise<number[][]>;
  /** Rank this extension's OWN commands against a request (Extensions V1 §4).
   * Conveniences over the same machinery Grain uses to rank extensions — call
   * them in any order, or none: an extension may instead pass the request
   * straight to `llm.complete` with its own tool schema. `match.semantic` needs
   * the on-device model (declare `needs: ["semantic"]`) but no capability. */
  readonly match: {
    /** Fast lexical rank over declared phrasings. Strong for names/verbs, weak
     * for paraphrase — reach for `semantic` when wording varies. */
    lexical(
      text: string,
      candidates: readonly { id: string; phrases: readonly string[] }[],
    ): Promise<{ id: string; score: number }[]>;
    /** Semantic rank over declared examples. Understands paraphrase; loads the
     * embedding model on demand. `margin` is the gap to the next candidate. */
    semantic(
      text: string,
      candidates: readonly { id: string; examples: readonly string[] }[],
    ): Promise<{ id: string; score: number; margin: number }[]>;
    /** Turn a ranking into a decision. `minConfidence` is the floor to act at
     * all; `margin` is how far the best must lead to be picked outright — within
     * it, the top candidates are `ambiguous`. There is no universal threshold;
     * measure with `grain-ext eval` and set these. */
    decide(
      candidates: readonly { id: string; score: number }[],
      policy?: { minConfidence?: number; margin?: number },
    ): Promise<{ pick: string } | { ambiguous: string[] } | { none: true }>;
  };
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
  readonly ui: {
    /** Handle stable-id events from Grain's standard Extension Surface. Return
     * a replacement tree to update it, or a finite outcome to close it. */
    onEvent(
      handler: (
        event: GrainViewEvent,
      ) => GrainViewReply | void | Promise<GrainViewReply | void>,
    ): void;
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
  /** The user accepted this extension in Extension Mode (Extensions V1 §3): the
   * WHOLE request is handed over, verbatim. The extension owns what happens next
   * — interpret it with `match.*` or `llm.complete` and its own tool schema, ask
   * or confirm as needed, and return an optional short result. Return
   * `{ decline }` when this extension is not the right owner so Grain can reopen
   * the chooser without making the user repeat the request; reserve `{ error }`
   * for a request this extension owned but failed to complete. */
  onRequest(
    handler: (
      request: string,
    ) =>
      | void
      | { view: GrainView }
      | { message?: string }
      | { decline: string }
      | { error: string }
      | Promise<
          | void
          | { view: GrainView }
          | { message?: string }
          | { decline: string }
          | { error: string }
        >,
  ): void;
}

export interface GrainAuthConnection {
  id: string;
  provider_name: string;
  authorization_host: string;
  token_host: string;
  scopes: string[];
  api_hosts: string[];
  /** `default` in API 1.x; retained so a later API can expose multiple accounts. */
  connection_id: string;
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
