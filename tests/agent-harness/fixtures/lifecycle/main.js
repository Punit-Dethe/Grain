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
});
