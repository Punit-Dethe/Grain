const REVISION = "one";
grain.actions({
  hello: async () => ({
    ok: { title: "Harness Lifecycle", body: `Harness hello (${REVISION})` },
  }),
  slow_hello: async () => {
    await new Promise((resolve) => setTimeout(resolve, 15000));
    return {
      ok: {
        title: "Harness Lifecycle",
        body: `Harness slow hello (${REVISION})`,
      },
    };
  },
  deadline_hello: async () => {
    await new Promise((resolve) => setTimeout(resolve, 25000));
    return { ok: { body: "Harness deadline hello must not succeed" } };
  },
  lost_reply: async () => {
    // Drop the actual reply at the worker's wire boundary. No host seam fakes it.
    const send = WebSocket.prototype.send;
    WebSocket.prototype.send = function (frame) {
      if (JSON.parse(frame).callres) {
        this.close();
        return;
      }
      return send.call(this, frame);
    };
    return { ok: { body: "Harness lost reply must not succeed" } };
  },
  tool_error: async () => ({
    error: { class: "network", message: "HARNESS_PRIVATE_ERROR_MARKER" },
  }),
  thrown_error: async () => {
    throw new Error("HARNESS_PRIVATE_ERROR_MARKER");
  },
  malformed_result: async () => ({
    ok: { body: "Harness malformed success must not appear" },
    error: { message: "HARNESS_PRIVATE_ERROR_MARKER" },
  }),
  oversized_result: async () => ({ ok: { body: "x".repeat(65536) } }),
  oversized_raw: async () => ({ ok: { body: "x".repeat(524288) } }),
  input_echo: async ({ text }) => ({ ok: { body: text } }),
  typed_echo: async (args) => ({
    ok: { body: `Harness typed reply: ${JSON.stringify(args)}` },
  }),
  typed_optional_echo: async (args) => ({
    ok: { body: `Harness typed reply: ${JSON.stringify(args)}` },
  }),
});
