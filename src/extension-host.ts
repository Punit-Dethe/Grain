// [GRAIN] The extension-host supervisor (SPEC §3.1, §7.1) — Phase 2.
//
// This is GRAIN's own code, running in the hidden `extension-host` webview. It
// hosts one Web Worker per extension; NO extension code ever runs in this global
// (each extension's source runs only inside its own Worker). The Rust
// `extension_host` module drives it over Tauri events:
//   Rust → here:  ext-host://spawn { ext_id, token, entry_source, caps, activation }
//                 ext-host://kill  { ext_id }
//   here → Rust:  ext-host://ready (once, when listeners are live)
//                 ext-host://died  { ext_id, reason }
//
// The security wall is the Rust WebSocket boundary — this supervisor only
// assembles and terminates workers; it holds no capability of its own.

import { listen, emit } from "@tauri-apps/api/event";
import { GRAIN_RUNTIME_JS } from "./extension-runtime";

interface SpawnPayload {
  ext_id: string;
  token: string;
  entry_source: string;
  caps?: string[];
  activation?: unknown;
}

interface WorkerHandle {
  worker: Worker;
  url: string;
}

const workers = new Map<string, WorkerHandle>();
const MAX_WORKER_ERROR_CHARS = 64 * 1024;

interface WorkerErrorDetail {
  stack?: string;
  worker_url?: string;
  entry_line_offset?: number;
  line?: number;
  column?: number;
}

function died(ext_id: string, reason: string, detail: WorkerErrorDetail = {}) {
  const boundedReason = reason.slice(0, MAX_WORKER_ERROR_CHARS);
  const boundedStack = detail.stack?.slice(0, MAX_WORKER_ERROR_CHARS);
  void emit("ext-host://died", {
    ext_id,
    ...detail,
    reason: boundedReason,
    stack: boundedStack,
  });
}

function spawnWorker(p: SpawnPayload) {
  if (workers.has(p.ext_id)) return; // one worker per extension (SPEC §7.1)

  // Inject the four consts the shim reads, ABOVE the shim, then the extension's
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
    ";" +
    "const __GRAIN_ACTIVATION__=" +
    JSON.stringify(p.activation ?? null) +
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
    died(p.ext_id, "worker construction failed: " + String(e));
    return;
  }

  worker.onerror = (ev) => {
    const error = ev && (ev.error as { stack?: unknown } | undefined);
    died(p.ext_id, String((ev && ev.message) || "worker error"), {
      stack: error && error.stack ? String(error.stack) : undefined,
      worker_url: url,
      entry_line_offset: entryLineOffset,
      line: ev && ev.lineno ? ev.lineno : undefined,
      column: ev && ev.colno ? ev.colno : undefined,
    });
    killWorker(p.ext_id);
  };
  worker.onmessage = (ev) => {
    // The shim posts { type: "fatal", reason } on an unrecoverable error.
    const m = ev.data as {
      type?: string;
      reason?: string;
      stack?: string;
    } | null;
    if (m && m.type === "fatal") {
      died(p.ext_id, String(m.reason || "fatal"), {
        stack: m.stack ? String(m.stack) : undefined,
        worker_url: url,
        entry_line_offset: entryLineOffset,
      });
      killWorker(p.ext_id);
    }
  };

  workers.set(p.ext_id, { worker, url });
}

function killWorker(ext_id: string) {
  const h = workers.get(ext_id);
  if (!h) return;
  workers.delete(ext_id);
  try {
    h.worker.terminate();
  } catch {
    /* already gone */
  }
  URL.revokeObjectURL(h.url); // free the blob source — "destroy if not in use"
}

async function main() {
  await listen<SpawnPayload>("ext-host://spawn", (e) => spawnWorker(e.payload));
  await listen<{ ext_id: string }>("ext-host://kill", (e) =>
    killWorker(e.payload.ext_id),
  );
  // Signal the host that our listeners are live so it can flush queued spawns
  // (Tauri events aren't buffered — a spawn emitted before this would be lost).
  await emit("ext-host://ready", {});
}

void main();
