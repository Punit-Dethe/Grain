# Extension Settings & UI Integration

Companion to [PLAN.md](PLAN.md). Decides where extension settings live, how
the UI changes when extensions are installed/enabled/updated/removed, and
pre-resolves the edge-case ledger. Written after studying the four reference
implementations, because each proves a different point.

> **Context change:** the Quick Panel is being retired / re-imagined. Nothing
> in this document (or the revised PLAN.md) depends on it. Everything below
> lives in the **settings window**, which survives any shell redesign — an app
> has a settings surface no matter what the shell becomes.

---

## Part 0 — What the reference apps prove

| App | Model | What it proves |
|---|---|---|
| **VS Code** — `contributes.configuration` | Declarative JSON schema → host-generated, **searchable**, namespaced settings UI | Declarative scales: thousands of extensions, zero settings-UI code, no key conflicts (forced `extension.key` namespacing), global search works across every extension |
| **Raycast** — manifest `preferences` | Declarative per-extension/per-command prefs, native rendering; types incl. `password`, `file`, `directory`, `appPicker` | A small, well-chosen **type vocabulary** covers ~95% of real extensions; secrets are a first-class *type*, not an afterthought |
| **Zen Mods** — `preferences.json` | Declarative prefs rendered natively; string prefs bind directly to CSS variables | The **theme-as-tokens** model works in production — mods restyle the browser with zero code, which is exactly Grain's pill-theme plan |
| **Obsidian** — `PluginSettingTab` | Code-rendered settings, one sidebar entry per plugin | The cautionary tale, twice: (1) per-plugin sidebar entries scale so badly the community built a plugin (*Settings Sidebar Organizer*) just to fold them; (2) code-rendered settings are invisible to search |

Conclusion: **declarative-first, custom-UI as a gated escalation, and no
per-extension entries in any top-level navigation.** Obsidian's flexibility
is real, but Grain gets it through the escalation path instead of as the
default.

---

## Part 1 — Where settings appear (the layout decision)

**One fixed entry point: "Extensions" in the settings sidebar.** The core
sidebar never grows or shrinks when extensions come and go — no
per-extension tabs at the top level, ever (the Obsidian lesson). Inside that
entry, a **master–detail** layout:

```
Settings sidebar          Extensions (master)             Detail (per extension)
┌─────────────┐   ┌──────────────────────────┐   ┌────────────────────────────┐
│ General     │   │ [search installed…]      │   │ Spaces          v0.3.1     │
│ Models      │   │                          │   │ by @author  ● verified     │
│ Post-proc.  │   │ ▢ Spaces        ● on     │   │ [Enable ▣]  [Open] [⋯]    │
│ …           │   │ ▢ Focus Pill    ● on     │   │──────────────────────────  │
│ Extensions ←│   │ ▢ Zh Prompts    ○ off    │   │ Overview                   │
│ Debug       │   │                          │   │ Settings         ← groups  │
└─────────────┘   │ [Browse marketplace…]    │   │ Permissions                │
                  └──────────────────────────┘   │ Data & Advanced            │
                                                 └────────────────────────────┘
```

- **Master list**: every installed extension as a row/card — icon, name,
  enable toggle, trust badge, one-line status ("2 shortcuts · uses AI"), and
  an **Open** affordance if it declares a workspace/overlay surface (this is
  also the launcher now that the Quick Panel rail is gone).
- **Detail view** (replaces the current per-extension tabs idea — same
  instinct, better structure): four stable sections, vertically scrolled,
  with in-page anchors instead of app-level tabs:
  1. **Overview** — description, version, links, update button, changelog.
  2. **Settings** — the rendered schema (below). If the extension declares
     multiple settings *groups*, each group is a titled section with an
     anchor rail on the right (like VS Code's settings TOC), **not** another
     tab bar. Tabs multiply chrome; anchors keep one scrollable, searchable
     page.
  3. **Permissions** — every granted capability in plain words, with
     per-capability revoke where feasible (revoking may auto-disable
     features; the row says so).
  4. **Data & Advanced** — storage used, reset-to-defaults, export/import
     settings, uninstall (with the keep/purge choice), and for tier-B/C the
     runtime state (asleep/awake, last wake, resource notes).

Why master–detail and not the existing "each extension is a tab inside the
Extensions page": tabs are a fixed-width, order-significant metaphor that
breaks at ~7 items and hides everything behind opaque labels. A searchable
master list scales to hundreds, sorts by enabled/recent, and gives each
extension a full-width detail canvas. The current implementation's instinct
(all extension UI inside one Extensions area) is **correct** — only the
inner navigation changes from tabs to list→detail.

**Global settings search** (when the settings window gains one — worth it
regardless of extensions): declarative settings index automatically;
results deep-link to `Extensions → <ext> → Settings → <group>#<key>`.
Custom-UI panels contribute only their declared `searchTerms`, and the
result deep-links to the panel. This is the VS Code advantage and the
second reason declarative is the default.

---

## Part 2 — The three levels of settings a manifest can declare

**Level 1 — Schema settings (default; tier A capability — no code, no runtime).**

```jsonc
"settings": {
  "groups": [
    { "id": "capture", "title": "Capture",
      "items": [
        { "key": "autoFile",  "type": "boolean", "default": true,
          "title": "Auto-file new notes",
          "description": "Let AI pick the folder for captured notes." },
        { "key": "vaultDir",  "type": "directory", "title": "Vault location" },
        { "key": "hotword",   "type": "keybind", "default": "Ctrl+Shift+S" },
        { "key": "provider",  "type": "enum", "options": ["local", "cloud"],
          "default": "local" },
        { "key": "apiKey",    "type": "secret", "title": "Service API key" }
      ] }
  ]
}
```

Type vocabulary v1 (Raycast-informed, Grain-flavored): `boolean`, `string`,
`number` (min/max/step), `enum`, `multi-enum`, `keybind` (routes through the
binding registry, so conflict detection is inherited), `directory`, `file`,
`secret` (stored in grain-core's secrets file, masked in UI, readable by the
extension only at call time, never in `settings.get` bulk reads), `color`,
`slider`. Anything fancier is Level 3.

Host renders these with Grain's own components — one look everywhere, zero
extension code executed, works while the extension is **asleep** (rendering
settings must never wake a runtime — a page of toggles cannot cost 60 MB).

**Level 2 — Groups + conditional visibility (still declarative).**
`"when": "capture.autoFile == true"` visibility clauses and group ordering.
Covers the "extension hosts multiple sets of settings" case: multiple
groups, one anchor rail, no tabs.

**Level 3 — Custom settings panel (escalation; needs `surface:settings-panel`).**
A sandboxed iframe inside the detail view's Settings section, loading the
extension's own HTML/JS via Grain's custom asset protocol, talking only
through the capability-checked bridge (Firefox renders extension option
pages inline exactly this way). For the Grain Space class: folder pickers
with previews, category editors, embedding stats. Rules: it lives *below*
any schema groups (an extension may mix both), it exists only while the
detail view is open (created on scroll-into-view, destroyed on navigate
away), and it must declare `searchTerms`. The marketplace badge shows
"custom settings UI" so reviewers look at it.

---

## Part 3 — The settings data model (why conflicts are impossible)

- **Namespace**: every value lives under `ext.<extension-id>.<group>.<key>`.
  Extension ids are unique in the index; two extensions **cannot** collide,
  and no extension can name a core setting — the store is physically
  separate from `AppSettings` (own JSON per extension under the registry
  dir, secrets in the existing secrets file under the same namespacing).
- **Core settings are read-only to extensions**, and only through explicit
  host getters for the few things extensions legitimately need (app
  language, theme, overlay position). No write path exists, so "extension
  changes a Grain setting" is not an edge case — it is unrepresentable.
- **Reads/writes** go through `settings.get/set` (bridge, capability
  `settings`), which validate against the schema (type, range, enum) and
  emit `settings.onChange` both ways (UI edits notify the extension;
  extension writes update any open UI — last-write-wins, no locks; both
  writers are the same user).
- **AppContext stays clean**: the registry store is *not* part of
  grain-core's `AppSettings` schema, so core settings migrations never touch
  extension data and vice versa.

---

## Part 4 — Lifecycle: what the UI does at every transition

| Transition | Store | UI change |
|---|---|---|
| **Install** (marketplace or `.grainpack`) | manifest cached; no values written | Row appears in master list, toggle off. Nothing else in the app changes. |
| **First enable** | grants recorded; defaults NOT materialized (values written only when a user or the extension first sets them — keeps "reset to default" meaningful and updates cheap) | Permission sheet (plain-language, per-capability) → on accept: toggle on, Settings/Permissions sections appear, contributed shortcuts register (conflicts surfaced immediately — see ledger #7), pill slots/surfaces become available. |
| **Disable** | values retained; grants retained | Toggle off; shortcuts unregister; surfaces close; pill slots vanish; detail view stays fully browsable (settings visible but inert-grayed, editable — users pre-configure before re-enabling). |
| **Update, same permissions** | schema diff applied: new keys appear with defaults; removed keys → values quarantined (kept invisibly for one version for downgrade, then pruned); renamed keys migrated via manifest `renames` map; type-changed values re-validated, invalid → default + a notice row | Changelog badge on the row; Settings section re-renders from the new schema. |
| **Update, NEW permissions** | update installs but extension is **held disabled** until the new grants are approved (Chrome's model) | Row shows "needs review — new permissions"; sheet shows only the *diff*. |
| **Uninstall** | dialog: "Remove settings and data too?" — default **keep** (accidental uninstall is recoverable; reinstalling finds everything) with an explicit purge checkbox | Row disappears; if kept, the orphaned data is listed under Settings → Extensions → Data & Advanced → "Orphaned extension data (2)" with per-item purge — orphans are visible, never silent (the VS Code settings.json failure, fixed). |
| **Crash/broken manifest** | untouched | Row renders in error state with the reason; detail shows raw manifest; nothing else degrades — a broken extension can never take the settings window down. |

---

## Part 5 — The edge-case ledger

Resolved now, so they're implementation details later instead of design
crises:

1. **Two extensions want the same shortcut** — all extension keybinds go
   through the existing binding registry; a conflict renders both rows in
   warning state with "conflicts with <other>"; the *later* registrant stays
   inactive until rebound. Core bindings always win over extension bindings.
2. **Two extensions hook the transcript** — transforms run in a
   deterministic, **user-visible pipeline**: Settings → Dictation → "Pipeline"
   shows core steps (fixed) and extension steps (drag-reorderable, with
   per-step timing shown after each run). No hidden ordering, no
   priority-integer wars.
3. **Transform misbehaves** — per-call timeout (pass-through on breach) +
   3-strike auto-disable with a pill notice chip (already in PLAN.md Part 6);
   the strike state is visible in that extension's Data & Advanced.
4. **Extension asleep when its settings open** — Level 1/2 render from
   schema, no wake. Level 3 wakes only its iframe, never the extension-host
   webview, and dies on navigate-away.
5. **Setting changed while asleep** — values live in the host store; the
   extension reads current values on next wake. No sync problem exists
   because the extension never holds the master copy.
6. **Secrets** — `secret` type: stored in the secrets file, masked input,
   excluded from settings export by default ("include secrets" checkbox with
   a warning), never readable via bulk `settings.get`, redacted from any
   diagnostic dump.
7. **Schema invalid / value corrupt** — validation on load; corrupt values →
   default + counted notice ("2 settings were reset"); invalid schema →
   error card (Part 4, last row).
8. **Extension writes settings in a loop** — write rate-limit per extension
   (e.g. 10/s sustained) → strikes like #3. Cheap to add on the bridge,
   impossible to retrofit socially later.
9. **Uninstall/reinstall churn** — keep-by-default (Part 4) makes reinstall
   lossless; purge is explicit; orphans are enumerated, never silent.
10. **Portable mode** — the registry store + extension data dirs live under
    the same portable data root (`portable.rs` already defines it); nothing
    extension-related ever writes outside it.
11. **Export/import** — per-extension settings export (JSON, secrets opt-in)
    from Data & Advanced; a full-app export includes the registry. This also
    covers "sync" until real sync exists — deliberately deferred.
12. **Two extensions, same *name*** — identity is the reverse-dns id;
    the master list disambiguates duplicate display names with the author
    handle. Ids are unique in the index by construction.
13. **Extension depends on another extension** — `dependencies` is reserved
    in the manifest but **rejected at install time in v1** ("requires X,
    which isn't supported yet") rather than half-supported. Revisit with
    real demand.
14. **Locale** — schema `title`/`description` accept either a string or a
    `{ "en": …, "de": … }` map; falls back to `en`; extension locales never
    merge into Grain's i18n files.
15. **The pill must stay sacred** — pill slots are capped (one action chip
    per extension, N total, overflow goes to a "…" chip) and the *user*
    can hide any extension's chip from that extension's settings — layout
    control never belongs to the extension.
16. **Settings UI while the marketplace lists an update** — update is a
    banner inside the detail view, not a modal; never interrupts editing.
17. **Enable at startup vs `resident`** — enabled ≠ running. An enabled
    tier-B extension is *woken by its subscriptions*; only `resident` keeps
    it warm, and that word appears on the permission sheet.

---

## Part 6 — Revisions this forces in PLAN.md

1. **`panel` surface redefined**: was "a route inside the Quick Panel"; now
   the only in-settings surface is the Level-3 **settings panel** (iframe in
   the detail view). App-class UI is `workspace`; transient UI is `overlay`.
   No extension UI ships inside whatever replaces the Quick Panel until that
   shell exists — the surface catalog gains a slot then, additively.
2. **Launcher**: was "Quick Panel extensions rail"; now master-list **Open**
   buttons + contributed shortcuts + tray "Extensions ▸" + pill chips. When
   the new shell lands, it can add a richer launcher without any manifest
   change (surfaces are already declared, not coded).
3. **Phase 3 exit criteria** now include the master–detail Extensions UI and
   Level 1/2 schema rendering; Level 3 iframes move to Phase 4 (they need
   the asset protocol + bridge hardening from the native-tier work anyway).

---

*Sources: [VS Code contribution points](https://code.visualstudio.com/api/references/contribution-points)
· [VS Code settings UX guidance](https://code.visualstudio.com/api/ux-guidelines/settings)
· [Raycast manifest/preferences](https://developers.raycast.com/information/manifest)
· [Zen Mods preferences](https://docs.zen-browser.app/themes-store/themes-marketplace-preferences)
· [Obsidian Settings Sidebar Organizer](https://community.obsidian.md/plugins/settings-sidebar-organizer)
(the existence proof that per-plugin sidebar entries don't scale).*
