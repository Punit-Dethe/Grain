/**
 * [GRAIN] The adapter is the seam the whole note-UI port rests on
 * (NOTE-UI-EXTENSION-PLAN.md): the same components must run inside the app and
 * inside an extension surface, and only this module knows which. A mistake here
 * is invisible on one side and total on the other, so it is tested against a
 * stub bridge rather than discovered when the window is empty.
 */

import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";

// A PLAIN object, not a Proxy: the adapter spreads `...tauri`, and spread copies
// own enumerable keys — which a Proxy without an ownKeys trap does not have. The
// real generated `commands` is an object literal, so the stub has to be one too
// or the test measures the stub instead of the adapter.
//
// The helper lives INSIDE the factory because `vi.mock` is hoisted above every
// top-level binding in the file.
vi.mock("../../bindings", () => {
  const tauriStub =
    (name: string) =>
    (...args: unknown[]) =>
      Promise.resolve({ status: "ok", data: { via: "tauri", name, args } });
  return {
    commands: {
      grainSpaceListCards: tauriStub("grainSpaceListCards"),
      grainSpaceGetNote: tauriStub("grainSpaceGetNote"),
      grainSpaceSearchNotes: tauriStub("grainSpaceSearchNotes"),
      grainSpaceCreateNote: tauriStub("grainSpaceCreateNote"),
      grainSpaceSaveNote: tauriStub("grainSpaceSaveNote"),
      grainSpaceDeleteNote: tauriStub("grainSpaceDeleteNote"),
      grainSpaceSetPinned: tauriStub("grainSpaceSetPinned"),
      grainSpaceMoveNote: tauriStub("grainSpaceMoveNote"),
      grainSpaceArmReminder: tauriStub("grainSpaceArmReminder"),
      grainSpaceDismissReminder: tauriStub("grainSpaceDismissReminder"),
      grainSpaceCloseWindow: tauriStub("grainSpaceCloseWindow"),
      // Not overridden by the adapter — the pass-through case.
      grainSpaceRecallTurn: tauriStub("grainSpaceRecallTurn"),
    },
  };
});

import { commands, inExtension } from "./hostAdapter";

type Call = { method: string; args: unknown[] };

function stubBridge(calls: Call[]) {
  const record =
    (method: string, result: unknown = null) =>
    (...args: unknown[]) => {
      calls.push({ method, args });
      return Promise.resolve(result);
    };
  (globalThis as Record<string, unknown>).grain = {
    notes: {
      cards: record("cards", [{ id: "a", title: "One" }]),
      get: record("get", { id: "a", title: "One", body: "b" }),
      search: record("search", [{ id: "a" }]),
      save: record("save", { id: "new-1" }),
      update: record("update"),
      delete: record("delete"),
      move: record("move"),
      pin: record("pin"),
      reminder: record("reminder"),
    },
    workspace: { close: record("workspace.close") },
  };
}

describe("hostAdapter", () => {
  let calls: Call[];

  beforeEach(() => {
    calls = [];
  });
  afterEach(() => {
    delete (globalThis as Record<string, unknown>).grain;
  });

  it("uses Tauri when there is no bridge", async () => {
    expect(inExtension()).toBe(false);
    const res = await commands.grainSpaceListCards();
    expect(res.status).toBe("ok");
    expect((res as unknown as { data: { via: string } }).data.via).toBe("tauri");
  });

  it("uses the bridge when there is one", async () => {
    stubBridge(calls);
    expect(inExtension()).toBe(true);
    const res = await commands.grainSpaceListCards();
    expect(res).toEqual({ status: "ok", data: [{ id: "a", title: "One" }] });
    expect(calls.map((c) => c.method)).toEqual(["cards"]);
  });

  it("passes through methods it does not override, so the port is one import", async () => {
    stubBridge(calls);
    // Recall stays behind: it needs the `llm` grant the note window does not
    // ask for. It must still RESOLVE rather than throw, so the components that
    // call it fail visibly at their own boundary instead of here.
    const res = await commands.grainSpaceRecallTurn([]);
    expect(res.status).toBe("ok");
    expect(calls).toHaveLength(0);
  });

  it("returns the saved note, not just an id, because callers want the note", async () => {
    stubBridge(calls);
    const res = await commands.grainSpaceCreateNote("hello");
    expect(res.status).toBe("ok");
    // save → get: the bridge answers with an id and the component wants a note.
    expect(calls.map((c) => c.method)).toEqual(["save", "get"]);
    expect(calls[1].args[0]).toBe("new-1");
  });

  it("turns a rejection into an error VALUE, matching the Tauri shape", async () => {
    (globalThis as Record<string, unknown>).grain = {
      notes: { cards: () => Promise.reject(new Error("Grain Space is switched off.")) },
      workspace: {},
    };
    const res = await commands.grainSpaceListCards();
    // The components branch on `status`; a thrown error would take down the
    // render instead of showing the message.
    expect(res).toEqual({
      status: "error",
      error: "Grain Space is switched off.",
    });
  });

  it("maps dismissing a reminder to a null fire time", async () => {
    stubBridge(calls);
    await commands.grainSpaceDismissReminder("a");
    expect(calls[0]).toEqual({ method: "reminder", args: ["a", null] });
  });

  it("closes the host's window through the workspace channel", async () => {
    stubBridge(calls);
    await commands.grainSpaceCloseWindow();
    expect(calls.map((c) => c.method)).toEqual(["workspace.close"]);
  });
});
