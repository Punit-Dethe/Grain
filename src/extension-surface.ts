// [GRAIN] Wrapper for an extension's workspace UI (SPEC §1.2, §7.1).
//
// This file is GRAIN's code running in the surface window. It holds the surface
// token and owns the socket; the extension's markup runs inside a SANDBOXED
// iframe with no `allow-same-origin`, which means:
//   - an opaque origin, so it cannot touch this document, its globals, or the
//     Tauri IPC that the sleep/wake acks go through,
//   - no shared realm with any other extension's surface,
//   - identity bound to this page's socket, so extension code cannot assert an
//     identity in a payload and be believed.
//
// The extension talks to the host by postMessage to this page, which forwards
// as a normal host-call frame. Every capability check still happens in Rust —
// this bridge adds no authority, it only carries messages.
//
// Lifecycle mirrors Grain Space: on sleep the iframe is REMOVED (the extension's
// DOM and JS heap become collectable, which is the whole reason the sleeping
// window is cheap), and on revive it is recreated.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type SurfaceInit = {
  extensionId: string;
  token: string;
  uiSource: string;
  sleepEvent: string;
  reviveEvent: string;
  payloadEvent: string;
  reloadEvent: string;
};

/** Injected ahead of the extension's markup: the `grain` API, as a postMessage
 *  proxy. It deliberately mirrors the worker runtime's shape so an author moves
 *  between the two without relearning anything. */
const BRIDGE = `<script>(function(){
  var seq = 0, pending = {};
  function call(method, params){
    return new Promise(function(resolve, reject){
      var id = ++seq;
      pending[id] = { resolve: resolve, reject: reject };
      parent.postMessage({ __grain: 1, id: id, method: method, params: params || {} }, "*");
    });
  }
  var listeners = [];
  function asGrainError(raw){
    var info = raw && typeof raw === "object" ? raw : {
      code: "E_INTERNAL",
      message: String(raw),
      hint: "Retry the call and copy the Developer log if it keeps failing.",
      docs: ""
    };
    var error = new Error(String(info.message || "Host call failed"));
    error.name = "GrainError";
    error.code = String(info.code || "E_INTERNAL");
    error.hint = String(info.hint || "");
    error.docs = String(info.docs || "");
    if (info.capability != null) error.capability = String(info.capability);
    return error;
  }
  window.addEventListener("message", function(e){
    var d = e.data;
    if (!d) return;
    if (d.__grainres === 1) {
      var p = pending[d.id];
      if (!p) return;
      delete pending[d.id];
      if (d.err != null) p.reject(asGrainError(d.err)); else p.resolve(d.ok);
      return;
    }
    if (d.__grainevent === 1) {
      for (var i = 0; i < listeners.length; i++) {
        try { listeners[i](d.event); } catch (err) {}
      }
    }
  });
  window.grain = {
    log: {
      info: function(m){ return call("log.info", { msg: String(m) }); },
      warn: function(m){ return call("log.warn", { msg: String(m) }); }
    },
    storage: {
      get: function(k){ return call("storage.get", { key: k }); },
      set: function(k, v){ return call("storage.set", { key: k, value: v }); },
      "delete": function(k){ return call("storage.delete", { key: k }); }
    },
    doc: {
      get: function(k){ return call("doc.get", { key: k }); },
      put: function(k, v){ return call("doc.put", { key: k, value: v }); },
      "delete": function(k){ return call("doc.delete", { key: k }); },
      list: function(){ return call("doc.list", {}).then(function(r){ return r && r.keys != null ? r.keys : r; }); }
    },
    embed: function(texts){ return call("embed", { texts: texts }).then(function(r){ return r && r.vectors != null ? r.vectors : r; }); },
    settings: {
      get: function(k){ return call("settings.get", { key: k }); },
      set: function(k, v){ return call("settings.set", { key: k, value: v }); }
    },
    llm: { complete: function(p){ return call("llm.complete", { prompt: String(p) }); } },
    net: {
      fetch: function(url, options){
        options = options || {};
        var request = { url: String(url), method: options.method == null ? "GET" : String(options.method), headers: options.headers == null ? {} : options.headers };
        if (options.body != null) request.body = String(options.body);
        if (options.secret != null) request.secret = options.secret;
        return call("net.fetch", request);
      }
    },
    workspace: { close: function(){ return call("workspace.close", {}); } },
    overlay: { dismiss: function(){ return call("overlay.dismiss", {}); } },
    open: {
      url: function(u){ return call("open.url", { url: String(u) }); },
      app: function(p){ return call("open.app", { path: String(p) }); },
      pickApp: function(){ return call("open.pickApp", {}).then(function(r){ return r && r.path != null ? r.path : null; }); }
    },
    onEvent: function(fn){ listeners.push(fn); },
    call: call
  };
})();<\/script>`;

let init: SurfaceInit | null = null;
let ws: WebSocket | null = null;
let frame: HTMLIFrameElement | null = null;
let socketOpen = false;
const outbox: string[] = [];
/** iframe request id -> the frame that asked, so a reply goes back to the
 *  window that is still mounted (a sleep mid-flight must not resurrect one). */
let reqSeq = 0;
const inflight = new Map<number, { frameReq: number; source: Window }>();

function fail(message: string) {
  const el = document.getElementById("fallback");
  if (el) {
    el.textContent = message;
    el.classList.add("show");
  }
  console.error("[grain] surface:", message);
}

function send(obj: unknown) {
  const s = JSON.stringify(obj);
  if (socketOpen && ws) ws.send(s);
  else outbox.push(s);
}

/** Mount the extension's UI. `srcdoc` + `sandbox` without `allow-same-origin`
 *  is what produces the opaque origin — do not add it. */
function mount() {
  if (frame || !init) return;
  document.getElementById("fallback")?.classList.remove("show");
  const el = document.createElement("iframe");
  el.id = "frame";
  el.setAttribute("sandbox", "allow-scripts");
  el.srcdoc = BRIDGE + init.uiSource;
  document.body.appendChild(el);
  frame = el;
}

function unmount() {
  if (frame) {
    frame.remove();
    frame = null;
  }
  inflight.clear();
}

/** Hand the iframe the payload the surface was opened with, once it is up. A
 *  fresh build (and a wake after sleep) has no live listener yet, so the host
 *  parks the payload and we collect it here — the workspace/overlay opening
 *  argument would otherwise never arrive. */
async function deliverOpeningPayload() {
  try {
    const payload = await invoke<unknown>("extension_surface_payload");
    if (payload != null) {
      frame?.contentWindow?.postMessage(
        { __grainevent: 1, event: payload },
        "*",
      );
    }
  } catch {
    /* no payload is the common case — not an error */
  }
}

function connect(cfg: SurfaceInit) {
  ws = new WebSocket("ws://127.0.0.1:7124");
  ws.onopen = () => {
    socketOpen = true;
    // The hello MUST be the first frame (SPEC §7.1).
    ws!.send(
      JSON.stringify({
        token: cfg.token,
        client: cfg.extensionId,
        grain_api: "1.0",
      }),
    );
    for (const s of outbox) ws!.send(s);
    outbox.length = 0;
  };
  ws.onclose = () => {
    socketOpen = false;
  };
  ws.onmessage = (e) => {
    let msg: any;
    try {
      msg = JSON.parse(e.data);
    } catch {
      return;
    }
    if (msg && msg.res) {
      const waiting = inflight.get(msg.res.id);
      if (!waiting) return;
      inflight.delete(msg.res.id);
      waiting.source.postMessage(
        {
          __grainres: 1,
          id: waiting.frameReq,
          ok: msg.res.ok,
          err: msg.res.err,
        },
        "*",
      );
      return;
    }
    if (msg && msg.grain_api !== undefined) return; // welcome
    // Anything else is a DaemonEvent the server already filtered to our grants.
    frame?.contentWindow?.postMessage({ __grainevent: 1, event: msg }, "*");
  };
}

window.addEventListener("message", (e) => {
  const d = e.data;
  if (!d || d.__grain !== 1 || typeof d.method !== "string") return;
  // The ONLY accepted sender is the mounted extension frame. Without this a
  // stale frame — or anything else that got a handle to this window — could
  // spend the surface's capabilities.
  if (!frame || e.source !== frame.contentWindow) return;
  const id = ++reqSeq;
  inflight.set(id, { frameReq: d.id, source: e.source as Window });
  send({ req: { id, method: d.method, params: d.params ?? {} } });
});

async function boot() {
  const cfg =
    (await invoke<SurfaceInit | null>("extension_surface_init")) ?? null;
  if (!cfg) {
    fail("This surface has no extension attached.");
    return;
  }
  init = cfg;
  document.title = cfg.extensionId;
  connect(cfg);

  // Sleep: drop the extension's whole realm, THEN ack — the host hides and
  // suspends only once the DOM is actually gone, which is what makes a sleeping
  // workspace cost almost nothing.
  await listen(cfg.sleepEvent, async () => {
    unmount();
    await invoke("extension_surface_sleep_ready");
  });
  await listen(cfg.reviveEvent, async () => {
    mount();
    await deliverOpeningPayload();
    await invoke("extension_surface_ui_ready");
  });
  await listen(cfg.payloadEvent, (e) => {
    frame?.contentWindow?.postMessage(
      { __grainevent: 1, event: e.payload },
      "*",
    );
  });
  await listen<{
    workspaceUiSource?: string | null;
    overlayUiSource?: string | null;
  }>(cfg.reloadEvent, (e) => {
    if (!init) return;
    const overlay = cfg.sleepEvent.startsWith("ext-overlay://");
    const source = overlay
      ? e.payload.overlayUiSource
      : e.payload.workspaceUiSource;
    unmount();
    if (source == null) {
      fail("This surface was removed by the latest extension build.");
      return;
    }
    init.uiSource = source;
    mount();
  });

  mount();
  await deliverOpeningPayload();
  await invoke("extension_surface_ui_ready");
}

boot().catch((e) => fail(String(e)));
