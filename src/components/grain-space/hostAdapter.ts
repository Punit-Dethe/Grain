/**
 * [GRAIN] The note UI's data layer, in the shape the components already call.
 *
 * The components were written against `commands.grainSpaceX(...)` — Tauri
 * commands, because they were Grain's own frontend. They are moving into an
 * extension surface, where the only channel is `window.grain` (a postMessage
 * proxy to Grain's page, which relays to the host over the local port, where
 * every method is capability-checked).
 *
 * Rather than rewrite eight components, this module exports a `commands` object
 * with the SAME method names and the same `Result` shape. The port becomes one
 * changed import line per file, which is a diff a person can actually review.
 *
 * ## Which side it runs on
 *
 * Both, deliberately. When `window.grain` is absent it falls through to the real
 * Tauri commands, so the components work unchanged inside the app during the
 * move (NOTE-UI-EXTENSION-PLAN.md phases the port so there is never a commit
 * where Grain Space has no viewer). When it is present, calls go over the
 * bridge. Nothing above this line has to know which.
 *
 * ## What is NOT here
 *
 * Recall (`grainSpaceRecallTurn`/`Reset`), the embedding-model downloads, and
 * "open in Obsidian". They need grants the note window does not ask for — `llm`
 * for Recall, and a custom URL scheme the host's allowlist rightly refuses —
 * and they all have a home in Grain's own settings. A viewer that quietly
 * needed the AI grant would be a worse trade than a viewer without a chat rail.
 */

import { commands as tauri } from "../../bindings";
import type { Note, NoteCard } from "../../bindings";

type Ok<T> = { status: "ok"; data: T };
type Err = { status: "error"; error: string };
type Res<T> = Ok<T> | Err;

/** The surface bridge, when this is running inside an extension window. */
type NotesBridge = {
  cards(): Promise<NoteCard[]>;
  search(query: string, limit?: number): Promise<unknown[]>;
  get(id: string): Promise<Note>;
  save(body: string, title?: string): Promise<{ id: string }>;
  update(id: string, fields: { title?: string; body?: string }): Promise<void>;
  delete(id: string): Promise<void>;
  move(id: string, folder: string | null): Promise<void>;
  pin(id: string, pinned: boolean): Promise<void>;
  reminder(id: string, fireAt: number | null): Promise<void>;
};

type GrainSurface = {
  notes: NotesBridge;
  workspace: { close(): Promise<void> };
};

const bridge = (): GrainSurface | null =>
  (globalThis as { grain?: GrainSurface }).grain ?? null;

/** Wrap a bridge promise in the `Result` shape the components branch on, so an
 *  error is a value here exactly as it is on the Tauri side. */
async function wrap<T>(run: () => Promise<T>): Promise<Res<T>> {
  try {
    return { status: "ok", data: await run() };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { status: "error", error: message };
  }
}

export const commands = {
  // Everything not overridden below passes straight through, so this object is
  // a drop-in superset of the generated bindings and the port is ONE changed
  // import line. The pass-throughs are Tauri calls that only work inside the
  // app — Recall, the model downloads, "open in Obsidian" — and the components
  // that use them check `inExtension()` and hide themselves instead.
  ...tauri,

  grainSpaceListCards(): Promise<Res<NoteCard[]>> {
    const g = bridge();
    return g ? wrap(() => g.notes.cards()) : tauri.grainSpaceListCards();
  },

  grainSpaceGetNote(id: string): Promise<Res<Note>> {
    const g = bridge();
    return g ? wrap(() => g.notes.get(id)) : tauri.grainSpaceGetNote(id);
  },

  grainSpaceSearchNotes(query: string): Promise<Res<Note[]>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceSearchNotes(query);
    return wrap(async () => (await g.notes.search(query)) as Note[]);
  },

  grainSpaceCreateNote(body: string): Promise<Res<Note>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceCreateNote(body);
    // The bridge answers with an id; the components want the note, and reading
    // it back is one extra call on a path a person takes once.
    return wrap(async () => {
      const { id } = await g.notes.save(body);
      return g.notes.get(id);
    });
  },

  grainSpaceSaveNote(note: Note): Promise<Res<null>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceSaveNote(note);
    return wrap(async () => {
      await g.notes.update(note.id, { title: note.title, body: note.body });
      return null;
    });
  },

  grainSpaceDeleteNote(id: string): Promise<Res<null>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceDeleteNote(id);
    return wrap(async () => {
      await g.notes.delete(id);
      return null;
    });
  },

  grainSpaceSetPinned(id: string, pinned: boolean): Promise<Res<Note>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceSetPinned(id, pinned);
    return wrap(async () => {
      await g.notes.pin(id, pinned);
      return g.notes.get(id);
    });
  },

  grainSpaceMoveNote(id: string, folder: string | null): Promise<Res<Note>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceMoveNote(id, folder);
    return wrap(async () => {
      await g.notes.move(id, folder);
      return g.notes.get(id);
    });
  },

  grainSpaceArmReminder(id: string, fireAt: number): Promise<Res<Note>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceArmReminder(id, fireAt);
    return wrap(async () => {
      await g.notes.reminder(id, fireAt);
      return g.notes.get(id);
    });
  },

  grainSpaceDismissReminder(id: string): Promise<Res<Note>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceDismissReminder(id);
    return wrap(async () => {
      await g.notes.reminder(id, null);
      return g.notes.get(id);
    });
  },

  grainSpaceCloseWindow(): Promise<Res<null>> {
    const g = bridge();
    if (!g) return tauri.grainSpaceCloseWindow();
    return wrap(async () => {
      await g.workspace.close();
      return null;
    });
  },
};

/** True when running inside an extension surface rather than inside the app.
 *  The few places that must differ — Recall, the model downloads, "open in
 *  Obsidian" — check this and hide themselves rather than failing at click. */
export const inExtension = (): boolean => bridge() != null;
