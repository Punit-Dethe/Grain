// [GRAIN] The worker-side runtime shim (SPEC §3.1, §7.1) — Phase 2.
//
// This string is prepended to an extension's `entry_source` and run inside a
// dedicated Web Worker (one per extension). It is the ONLY code between the
// extension and the Grain tool wire: it opens the extension's own WebSocket,
// authenticates with the extension's own token, and exposes the `grain` global.
// The hidden host page's CSP restricts standard worker browser networking;
// Rust still authorizes every tool RPC on the socket independently.
//
// The supervisor injects four consts ABOVE this shim before running it:
//   const __GRAIN_EXT_ID__   = "com.example.ext";
//   const __GRAIN_TOKEN__    = "<per-worker secret>";
//   const __GRAIN_CAPS__     = ["storage", "auth", "net:api.example.com"];
//
// Authored in plain ES2017 with no template literals or `${}` so it embeds
// cleanly in the backtick string below.

export const GRAIN_RUNTIME_JS = `(function () {
  var EXT_ID = __GRAIN_EXT_ID__;
  var TOKEN = __GRAIN_TOKEN__;
  var CAPS = __GRAIN_CAPS__;

  var ws = new WebSocket("ws://127.0.0.1:7124");
  var reqSeq = 0;
  var pending = new Map();     // request id -> { resolve, reject }
  var handlers = Object.create(null); // exact tool calls only
  var closed = false;
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
  ws.onclose = function () {
    open = false;
    closed = true;
    outbox.length = 0;
    pending.forEach(function (p) { p.reject(new Error("tool connection closed")); });
    pending.clear();
    fatal("socket closed");
  };
  ws.onerror = function () { fatal("socket error"); };

  ws.onmessage = function (e) {
    var msg;
    try { msg = JSON.parse(e.data); } catch (err) { return; }
    if (!msg || typeof msg !== "object") return;
    if (msg.res) { resolveReq(msg.res); return; }
    if (msg.call) { onHostCall(msg.call); return; }
    if (msg.grain_api !== undefined) { return; }                 // welcome
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
    if (closed) return Promise.reject(new Error("tool connection closed"));
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
    var handler = call.method === "action" ? handlers.action : null;
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

  var grain = {
    caps: Object.freeze(CAPS.slice()),
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
    // Grain API network access is host-proxied and requires an exact net:<host>
    // grant. The hidden page's inherited CSP blocks direct browser fetches.
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
        if (options.auth === true) request.auth = true;
        return req("net.fetch", request);
      }
    },
    // Grain owns the OAuth browser flow and credential vault. These methods
    // return connection metadata only; raw tokens never enter this worker.
    auth: {
      status: function () { return req("auth.status", {}); },
      connect: function () { return req("auth.connect", {}); },
      disconnect: function () { return req("auth.disconnect", {}); }
    },
    // [GRAIN] Extensions 2.0 (Amendment C): the extension declares its actions
    // here, one handler per declared action id. Grain's Agent chooses the EXACT
    // action and sends validated arguments — the extension never sees the
    // transcript and never learns other extensions exist (eyes + hands; Grain is
    // the brain). Each handler returns structured DATA, never a rendered view;
    // Grain owns all rendering (markdown now, native cards later). Return shapes:
    //   { ...data }               plain result data (wrapped as ok by Grain)
    //   { error: { class?, message } }
    // Extension-authored follow-up is unsupported by the host; do not advertise
    // needsInteraction in the public author API.
    //   grain.actions({ create_issue: async function (args) { return { title, body }; } })
    actions: function (map) {
      // Copy own functions once; later prototype/map mutation cannot introduce
      // a different callable under an approved tool identity.
      var tools = Object.create(null);
      Object.keys(map || {}).forEach(function (key) {
        if (typeof map[key] === "function") tools[key] = map[key];
      });
      handlers.action = function (p) {
        var name = p && p.action;
        var fn = tools[name];
        if (typeof fn !== "function") {
          return { error: { class: "not_found", message: "no such action: " + name } };
        }
        return Promise.resolve()
          .then(function () {
            return fn((p && p.arguments) || {}, {
              idempotencyKey: (p && p.idempotencyKey) || null
            });
          })
          .then(function (out) {
            // Pass through an already-tagged result; wrap plain data as ok.
            if (out && ["error", "needsInteraction", "ok"].some(function (key) {
              return Object.prototype.hasOwnProperty.call(out, key);
            })) return out;
            return { ok: out == null ? {} : out };
          })
          .catch(function (e) {
            return { error: { class: "internal", message: (e && e.message) || String(e) } };
          });
      };
    }
  };
  Object.keys(grain).forEach(function (key) {
    if (grain[key] && typeof grain[key] === "object") Object.freeze(grain[key]);
  });
  self.grain = Object.freeze(grain);
})();
`;
