// [GRAIN] The extension-host supervisor (SPEC §3.1, §7.1) — Phase 2.
//
// This is GRAIN's own code, running in the hidden `extension-host` webview. It
// hosts one Web Worker per extension; NO extension code ever runs in this global
// (each extension's source runs only inside its own Worker). The Rust
// `extension_host` module drives it over Tauri events:
//   Rust → here:  ext-host://spawn { ext_id, token, entry_source, caps }
//                 ext-host://kill  { ext_id, token }
//   here → Rust:  ext-host://ready / failed { generation, reason? }
//                 ext-host://died  { ext_id, token, reason }
//
// The security wall is the Rust WebSocket boundary — this supervisor only
// assembles and terminates workers; it holds no capability of its own.

import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import { GRAIN_RUNTIME_JS } from "./extension-runtime";

interface SpawnPayload {
  ext_id: string;
  token: string;
  entry_source: string;
  caps?: string[];
}

interface WorkerHandle {
  worker: Worker;
  url: string;
  token: string;
}

const workers = new Map<string, WorkerHandle>();
const generation = (
  window as Window & { __GRAIN_SUPERVISOR_GENERATION__?: number }
).__GRAIN_SUPERVISOR_GENERATION__;
const unlisteners: UnlistenFn[] = [];
let stopped = false;
const MAX_WORKER_ERROR_CHARS = 64 * 1024;

interface WorkerErrorDetail {
  stack?: string;
  worker_url?: string;
  entry_line_offset?: number;
  line?: number;
  column?: number;
}

function died(
  ext_id: string,
  token: string,
  reason: string,
  detail: WorkerErrorDetail = {},
) {
  const boundedReason = reason.slice(0, MAX_WORKER_ERROR_CHARS);
  const boundedStack = detail.stack?.slice(0, MAX_WORKER_ERROR_CHARS);
  void emit("ext-host://died", {
    ext_id,
    token,
    ...detail,
    reason: boundedReason,
    stack: boundedStack,
  });
}

function spawnWorker(p: SpawnPayload) {
  if (stopped) return;
  const current = workers.get(p.ext_id);
  if (current?.token === p.token) return;
  if (current) killWorker(p.ext_id, current.token);

  // Inject the three consts the shim reads, ABOVE the shim, then the extension's
  // own source. JSON.stringify is the injection boundary — values are data, so
  // an extension id/token can't break out into code.
  const header =
    "const __GRAIN_EXT_ID__=" +
    JSON.stringify(p.ext_id) +
    ";" +
    "const __GRAIN_TOKEN__=" +
    JSON.stringify(p.token) +
    ";" +
    "const __GRAIN_CAPS__=" +
    JSON.stringify(p.caps || []) +
    ";\n";
  const prefix = header + GRAIN_RUNTIME_JS + "\n";
  const entryLineOffset = (prefix.match(/\n/g) || []).length;
  const src = prefix + p.entry_source;

  const url = URL.createObjectURL(new Blob([src], { type: "text/javascript" }));
  let worker: Worker;
  try {
    worker = new Worker(url);
  } catch (e) {
    URL.revokeObjectURL(url);
    died(p.ext_id, p.token, "worker construction failed: " + String(e));
    return;
  }

  worker.onerror = (ev) => {
    if (workers.get(p.ext_id)?.worker !== worker) return;
    const error = ev && (ev.error as { stack?: unknown } | undefined);
    died(p.ext_id, p.token, String((ev && ev.message) || "worker error"), {
      stack: error && error.stack ? String(error.stack) : undefined,
      worker_url: url,
      entry_line_offset: entryLineOffset,
      line: ev && ev.lineno ? ev.lineno : undefined,
      column: ev && ev.colno ? ev.colno : undefined,
    });
    killWorker(p.ext_id, p.token);
  };
  worker.onmessage = (ev) => {
    if (workers.get(p.ext_id)?.worker !== worker) return;
    // The shim posts { type: "fatal", reason } on an unrecoverable error.
    const m = ev.data as {
      type?: string;
      reason?: string;
      stack?: string;
    } | null;
    if (m && m.type === "fatal") {
      died(p.ext_id, p.token, String(m.reason || "fatal"), {
        stack: m.stack ? String(m.stack) : undefined,
        worker_url: url,
        entry_line_offset: entryLineOffset,
      });
      killWorker(p.ext_id, p.token);
    }
  };

  workers.set(p.ext_id, { worker, url, token: p.token });
}

function killWorker(ext_id: string, token: string) {
  const h = workers.get(ext_id);
  if (!h || h.token !== token) return;
  workers.delete(ext_id);
  h.worker.onerror = null;
  h.worker.onmessage = null;
  try {
    h.worker.terminate();
  } catch {
    /* already gone */
  }
  URL.revokeObjectURL(h.url); // free the blob source — "destroy if not in use"
}

function dispose() {
  if (stopped) return;
  stopped = true;
  window.removeEventListener("pagehide", dispose);
  for (const unlisten of unlisteners.splice(0)) unlisten();
  for (const [id, handle] of workers) killWorker(id, handle.token);
}

async function register<T>(name: string, callback: (payload: T) => void) {
  const unlisten = await listen<T>(name, (event) => {
    if (!stopped) callback(event.payload);
  });
  // A listener can finish registration after the page has started closing.
  if (stopped) unlisten();
  else unlisteners.push(unlisten);
}

async function main() {
  window.addEventListener("pagehide", dispose, { once: true });
  try {
    if (!Number.isSafeInteger(generation) || !generation || generation < 1) {
      throw new Error("Missing supervisor generation");
    }
    await register<SpawnPayload>("ext-host://spawn", spawnWorker);
    if (stopped) return;
    await register<{ ext_id: string; token: string }>(
      "ext-host://kill",
      (payload) => killWorker(payload.ext_id, payload.token),
    );
    if (stopped) return;
    await emit("ext-host://ready", { generation });
  } catch (error) {
    dispose();
    await emit("ext-host://failed", {
      generation,
      reason: String(error).slice(0, MAX_WORKER_ERROR_CHARS),
    }).catch(() => undefined);
  }
}

void main();
