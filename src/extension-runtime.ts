// [GRAIN] The worker-side runtime shim (SPEC §3.1, §7.1) — Phase 2.
//
// This string is prepended to an extension's `entry_source` and run inside a
// dedicated Web Worker (one per extension). It is the ONLY code between the
// extension and the wire: it opens the extension's own WebSocket, authenticates
// with the extension's own token, and exposes the `grain` global the extension
// calls. The worker reaches Rust exclusively through this socket — never Tauri
// IPC (which isn't available in worker scope anyway).
//
// The supervisor injects four consts ABOVE this shim before running it:
//   const __GRAIN_EXT_ID__   = "com.example.ext";
//   const __GRAIN_TOKEN__    = "<per-worker secret>";
//   const __GRAIN_CAPS__     = ["storage", "llm", ...];
//   const __GRAIN_ACTIVATION__ = { "TranscriptionComplete": {...} } | null;
//
// Authored in plain ES2017 with no template literals or `${}` so it embeds
// cleanly in the backtick string below.

export const GRAIN_RUNTIME_JS = `(function () {
  var EXT_ID = __GRAIN_EXT_ID__;
  var TOKEN = __GRAIN_TOKEN__;
  var CAPS = __GRAIN_CAPS__;
  var ACTIVATION = __GRAIN_ACTIVATION__;

  var ws = new WebSocket("ws://127.0.0.1:7124");
  var reqSeq = 0;
  var pending = new Map();     // request id -> { resolve, reject }
  var handlers = {};           // "transform" | "sessionResult" -> fn
  var sessionAbort = null;     // AbortController for the one active slow stage
  var onEventFn = null;
  var outbox = [];             // frames queued until the socket opens
  var open = false;

  function fatal(reason) {
    var message = (reason && reason.message ? String(reason.message) : String(reason)).slice(0, 65536);
    var stack = reason && reason.stack ? String(reason.stack).slice(0, 65536) : undefined;
    try { self.postMessage({ type: "fatal", reason: message, stack: stack }); } catch (e) {}
  }

  function send(obj) {
    var s = JSON.stringify(obj);
    if (open) ws.send(s); else outbox.push(s);
  }

  ws.onopen = function () {
    open = true;
    // The hello MUST be the first frame (SPEC §7.1); send it directly, then
    // flush anything the extension queued synchronously before open.
    ws.send(JSON.stringify({ token: TOKEN, client: EXT_ID, grain_api: "1.0" }));
    for (var i = 0; i < outbox.length; i++) ws.send(outbox[i]);
    outbox.length = 0;
  };
  ws.onclose = function () { open = false; fatal("socket closed"); };
  ws.onerror = function () { fatal("socket error"); };

  ws.onmessage = function (e) {
    var msg;
    try { msg = JSON.parse(e.data); } catch (err) { return; }
    if (typeof msg === "string") { deliverEvent(msg); return; }  // unit-variant event
    if (msg.res) { resolveReq(msg.res); return; }
    if (msg.call) { onHostCall(msg.call); return; }
    if (msg.grain_api !== undefined) { return; }                 // welcome
    deliverEvent(msg);                                           // struct-variant event
  };

  function resolveReq(res) {
    var p = pending.get(res.id);
    if (!p) return;
    pending.delete(res.id);
    if (res.err != null) p.reject(asGrainError(res.err));
    else p.resolve(res.ok);
  }

  function asGrainError(raw) {
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

  function req(method, params) {
    return new Promise(function (resolve, reject) {
      var id = ++reqSeq;
      pending.set(id, { resolve: resolve, reject: reject });
      send({ req: { id: id, method: method, params: params || {} } });
    });
  }

  function onHostCall(call) {
    if (call.method === "memory.sample") {
      // Chromium/WebView2 exposes this engine estimate. It is intentionally
      // sampled inside the worker realm; unsupported engines report that fact
      // and are never treated as over-budget.
      var memory = self.performance && self.performance.memory;
      var used = memory && Number(memory.usedJSHeapSize);
      var supported = Number.isFinite(used) && used >= 0;
      send({ callres: { call_id: call.call_id, ok: {
        supported: supported,
        usedBytes: supported ? Math.floor(used) : null
      } } });
      return;
    }
    if (call.method === "session.cancel") {
      if (sessionAbort) sessionAbort.abort();
      if (call.call_id) send({ callres: { call_id: call.call_id, ok: null } });
      return;
    }
    var handler = handlers[call.method];
    if (!handler) {
      send({ callres: { call_id: call.call_id, err: "no handler for " + call.method } });
      return;
    }
    Promise.resolve()
      .then(function () { return handler(call.params || {}); })
      .then(function (out) {
        send({ callres: { call_id: call.call_id, ok: out === undefined ? null : out } });
      })
      .catch(function (err) {
        send({ callres: { call_id: call.call_id, err: String((err && err.message) || err) } });
      });
  }

  function deliverEvent(ev) {
    if (typeof onEventFn === "function") {
      try { onEventFn(ev); } catch (e) { fatal(e); }
    }
  }

  // A cold worker woken by a shortcut press carries the press as its
  // activation. It is NOT a DaemonEvent, so onEvent must not replay it.
  function activationShortcut() {
    var s = ACTIVATION && ACTIVATION.Shortcut;
    return s && s.id ? String(s.id) : null;
  }

  var grain = {
    activation: ACTIVATION,
    caps: CAPS,
    extId: EXT_ID,
    log: {
      info: function (m) { return req("log.info", { msg: String(m) }); },
      warn: function (m) { return req("log.warn", { msg: String(m) }); }
    },
    storage: {
      get: function (k) { return req("storage.get", { key: String(k) }); },
      set: function (k, v) { return req("storage.set", { key: String(k), value: v }); },
      "delete": function (k) { return req("storage.delete", { key: String(k) }); }
    },
    // A document store: one file per key (SPEC 3.4), for collections a KV blob
    // would be the wrong shape for — notes, records, anything that grows.
    doc: {
      get: function (k) { return req("doc.get", { key: String(k) }); },
      put: function (k, v) { return req("doc.put", { key: String(k), value: v }); },
      "delete": function (k) { return req("doc.delete", { key: String(k) }); },
      list: function () { return req("doc.list", {}).then(function (r) { return r && r.keys != null ? r.keys : r; }); }
    },
    // Read the user's current selection (needs the capture:selection grant).
    // Resolves to the selected text, or null when nothing is selected.
    captureSelection: function () {
      return req("capture.selection", {}).then(function (r) { return r && r.text != null ? r.text : null; });
    },
    settings: {
      get: function (k) { return req("settings.get", { key: String(k) }); },
      set: function (k, v) { return req("settings.set", { key: String(k), value: v }); }
    },
    llm: {
      complete: function (prompt) {
        return req("llm.complete", { prompt: String(prompt) }).then(function (r) {
          return r && r.text != null ? r.text : r;
        });
      }
    },
    // Network access is always host-proxied and requires an exact net:<host>
    // grant. The worker itself never receives a browser fetch capability.
    net: {
      fetch: function (url, options) {
        options = options || {};
        var request = {
          url: String(url),
          method: options.method == null ? "GET" : String(options.method),
          headers: options.headers == null ? {} : options.headers
        };
        if (options.body != null) request.body = String(options.body);
        if (options.secret != null) request.secret = options.secret;
        return req("net.fetch", request);
      }
    },
    // On-device embeddings (the same BGE model Grain Space uses). Resolves to an
    // array of vectors, one per input text.
    embed: function (texts) {
      return req("embed", { texts: texts }).then(function (r) {
        return r && r.vectors != null ? r.vectors : r;
      });
    },
    // The extension asks for ITS OWN workspace surface (SPEC §1.2) — there is
    // no id to pass, because the host derives which extension is calling from
    // the channel, not from an argument. The payload reaches the surface UI on
    // mount (and an already-open surface via its payload event).
    workspace: {
      open: function (payload) { return req("workspace.open", { payload: payload == null ? null : payload }); },
      close: function () { return req("workspace.close", {}); }
    },
    // A transient HUD (SPEC 1.2). Host-budgeted in size and lifetime — it
    // auto-dismisses, so an extension cannot leave one on screen.
    overlay: {
      show: function (payload) { return req("overlay.show", { payload: payload == null ? null : payload }); },
      dismiss: function () { return req("overlay.dismiss", {}); }
    },
    session: {
      start: function (options) {
        return req("session.start", { mode: String(options && options.mode || "") });
      }
    },
    // A transform returns the rewritten text (a string); an empty string
    // suppresses the paste (SPEC §3.3).
    onTransform: function (fn) { handlers.transform = function (p) { return fn(p.text); }; },
    onSessionStage: function (fn) {
      handlers.sessionStage = function (p) {
        var controller = new AbortController();
        sessionAbort = controller;
        return Promise.resolve(fn(p.text, { mode: String(p.mode || ""), signal: controller.signal }))
          .then(function (out) {
            if (sessionAbort === controller) sessionAbort = null;
            return out;
          }, function (err) {
            if (sessionAbort === controller) sessionAbort = null;
            throw err;
          });
      };
    },
    onSessionResult: function (fn) {
      grain.onSessionStage(function (text) { return fn(text); });
    },
    // A shortcut press is acknowledged on RECEIPT, not on completion: the
    // handler runs detached so an extension that opens an LLM call from a
    // hotkey is never mistaken for an unresponsive one.
    onShortcut: function (fn) {
      handlers.shortcut = function (p) {
        Promise.resolve()
          .then(function () { return fn(String(p && p.id)); })
          .catch(function (e) {
            grain.log.warn("shortcut handler failed: " + ((e && e.message) || e));
          });
        return null;
      };
      var woke = activationShortcut();
      if (woke != null) {
        Promise.resolve().then(function () { handlers.shortcut({ id: woke }); });
      }
    },
    onEvent: function (fn) {
      onEventFn = fn;
      if (ACTIVATION != null && activationShortcut() == null) {
        // Fire once for the event that woke this worker — the broadcast is
        // already past, so its payload travels in the injected activation.
        Promise.resolve().then(function () { deliverEvent(ACTIVATION); });
      }
    }
  };
  self.grain = grain;
})();
`;
