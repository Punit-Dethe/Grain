# Reminders — plan

Status: v1 built (2026-11-13). Scope was narrowed by the user from a
"calendar + to-dos + notes + reminders" planner to **reminders only**. To-dos,
quick notes and a month calendar are out of scope for now (see §8).

## 1. What it is

A built-in **Reminders** tab in the main window, plus an Agent tool, so the user
can say *"remind me on 25 Nov at 6pm about the design review, here's the Meet
link"* and Grain rings at that time with an **Open** button for the link.

A reminder has four primitives:

| Field   | Required | Notes                                                       |
| ------- | -------- | ----------------------------------------------------------- |
| `title` | yes      | The topic. ≤ 200 chars.                                     |
| `date`  | yes      | Local calendar date, `YYYY-MM-DD`.                          |
| `time`  | no       | Local wall-clock `HH:MM` (24 h). Absent = all-day.          |
| `links` | no       | ≤ 5 links, each checked by Reach's link policy. Shown as "Open". |

Plus optional `details` (≤ 2 000 chars) for anything the Agent captured that is
not a link.

## 2. UI sketch

```
┌ Reminders ───────────────────────────────────────────── [ + New ] ┐
│ Grain rings at the time you set. Ask the Agent, or add one here.   │
│                                                                    │
│ ┌ composer (opens on + New / edit) ───────────────────────────────┐│
│ │ What should Grain remind you about?                             ││
│ │ [ 25 Nov 2026 ] [ 18:00 ] (time optional)                       ││
│ │ Links  [ https://meet.google.com/… ] [+ link]                   ││
│ │                                         [Cancel] [Save reminder]││
│ └─────────────────────────────────────────────────────────────────┘│
│                                                                    │
│ RINGING                                                            │
│  ● Design review          6:00 PM          [Open] [Snooze] [Done]  │
│ TODAY · Fri 13 Nov                                                 │
│  ○ Call the bank          All day                                  │
│ TOMORROW                                                           │
│  ○ Standup                9:30 AM   [Open meet.google.com]         │
│ WED 25 NOV                                                         │
│  ○ Design review          6:00 PM   [Open]          Agent          │
│ EARLIER (past, not done)                                           │
│ DONE (collapsed, "Clear")                                          │
└────────────────────────────────────────────────────────────────────┘
```

* The circle marks a reminder done (undo by clicking again in Done).
* Hover reveals Edit / Delete. Edit reuses the composer inline.
* Empty state: one line and the "+ New" button.
* Visual language: UI 2.0 tokens (`--bg-*`, `--text-*`, `--hairline`,
  `--radius-*`), `.page`/`.page-header`/`.button` shell classes, styles in
  `src/app/reminders/reminders.css`.

The tab sits in the sidebar directly below Personalize.

**Ringing happens in the Agent notch** (user decision 2026-11-13: no separate
overlay window). At the due time the notch opens by hand-open
(`agent::open_notch_for_reminder`) on a **reminder view** that replaces the
Chat/Recall pane:

```
┌ Recall  Chat ─────────────────────────────────────── × ┐
│ ← Back                                         1 of 2 │
│                                                        │
│ (bell) 6:00 PM            (or "Missed · 5:00 PM")      │
│ Design review                                          │
│ note…                                                  │
│                                                        │
│ [ ↗ Open  meet.google.com ]                   [ ✓ Done ] │
└────────────────────────────────────────────────────────┘
```

* **Back** → the notch returns to whatever it was on (Chat or Recall). The
  reminder stays ringing and shows again the next time the notch opens.
* **Done** → the reminder is finished; the next ringing one shows, or the notch
  returns to Chat/Recall.
* **Escape** on the reminder view counts as **Done** (for now). Escape again
  closes the notch as usual.
* Clicking Chat/Recall (or Tab) also steps back.
* A summoned notch (voice → reply) is never taken over by a reminder.

Why not an OS toast: Grain has no notification plugin, and
`tauri-plugin-notification` cannot attach action buttons on desktop, so an
"Open the Meet link" button is impossible there. The notch is the surface the
user already looks at, and it costs no extra window.

## 3. Storage

`<app data>/reminders.json` — one small JSON document, written atomically
(tmp + rename) under a process mutex, read on demand, never cached in memory.
This matches Grain's other backend-owned stores (`agent/notch.json`, Meet
manifests). SQLite (`history.db`) would be overkill for a list of tens of
records and would pin a connection. There is no Notes tab or Grain Space vault
in the tree any more (both were retired), so nothing to reuse.

```jsonc
{
  "reminders": [
    {
      "id": "9f1c…",              // host-minted hex
      "title": "Design review",
      "date": "2026-11-25",
      "time": "18:00",            // or null = all-day
      "links": [{ "url": "https://meet.google.com/abc-defg-hij" }],
      "details": null,
      "kind": "notify",           // §7: "action" later
      "status": "pending",        // pending | ringing | done
      "ring_at_ms": 1795000000000,// next ring instant (UTC ms) or null
      "source": "agent",          // user | agent
      "created_ms": 0, "updated_ms": 0
    }
  ]
}
```

`ring_at_ms` is computed by the host on every save from the local date/time:

* timed → that local instant;
* all-day → 09:00 local on that date (`ALL_DAY_RING_HOUR`);
* if that instant is already past at save time → `null` (it shows in the list
  but does not ring). The Agent tool refuses a timed reminder in the past.
* snooze → `now + minutes`; ringing / done → `null`.

Time zones: the instant is fixed at save time. Travelling does not move it.

## 4. Commands and events

All in `src-tauri/src/grain_reminders.rs`; pure logic in
`crates/grain-core/src/reminders.rs`.

| Command                         | Does                                              |
| ------------------------------- | ------------------------------------------------- |
| `reminders_list()`              | all reminders                                     |
| `reminders_save(input)`         | create (no id) or edit; recomputes `ring_at_ms`   |
| `reminders_delete(id)`          | remove one                                        |
| `reminders_set_done(id, done)`  | done / back to pending                            |
| `reminders_snooze(id, minutes)` | ringing → pending, rings again later              |
| `reminders_clear_done()`        | remove every done reminder                        |
| `reminders_open_link(id, index)`| opens the *stored* link (re-validated) — the webview never hands the host a URL to open |

Event: `reminders-changed` (no payload). The tab and the notch re-read.

## 5. Firing path (one timer, not an engine)

```
save / delete / snooze / done / fire / app start
        └─► rearm(): abort the old timer task, read the file,
                     pick min(ring_at_ms over pending, notch retry if owed)
                     ├─ none → no task exists at all
                     └─ some → spawn ONE task: sleep until due (in ≤ 5 min
                               slices, re-reading the wall clock, so laptop
                               sleep or a clock change cannot make it late),
                               then fire()
fire(): every pending reminder with ring_at_ms ≤ now → status ringing,
        ring_at_ms = null; if any rang (or a retry is due) → present();
        emit reminders-changed; rearm()
present(): on the main loop, agent::open_notch_for_reminder()
        ├─ opened (or already open by hand) → the panel shows its reminder view
        └─ cannot open now (Agent off, dictating, a summon owns the notch)
           → owe a retry in 60 s and rearm()
```

The retry is the same single timer, owed only while something rings and the
notch could not open. At start-up, reminders whose time passed while Grain was
closed ring at once (the view labels them "Missed"), and reminders still
`ringing` from a previous run open the notch again.

## 6. Agent tool

Provider `grain.reminders` ("Reminders"), compiled in beside Reach and MCP
(same hooks: `capability.rs` directory/catalog/enabled, `action_exec.rs`
dispatch). Always on: it only writes Grain's own local file. The directory
description carries today's date, because the Agent prompt has none.

* `create_reminder { title, date, time?, links?[], details? }`
  `date` accepts `YYYY-MM-DD`, `MM-DD` (next occurrence), `today`,
  `tomorrow` or a weekday name (next occurrence). `time` accepts `18:00`,
  `6pm`, `6:30 pm`. The result echoes the resolved day and time so the model
  can confirm it to the user. Links are captured now, only from the
  arguments, and checked by `reach::validate_link`.
* `list_reminders { date? }` — read-only: upcoming reminders (or one day),
  with today's date in the reply.

## 7. Deferred: scheduled actions (plan only — NOT built)

A second reminder kind where, at the due time, a stored instruction is sent to
the Agent, which then acts. Example: *"on 25 Nov at 6pm open my browser and
join this Meet"*.

Data model: the same record with `kind: "action"` and an `instruction` string
(plus the same `links`, which become context the instruction can refer to).
`status` gains `running` and `failed`, and a `last_result` text is kept.

Firing: the same single timer. On fire, a `notify` reminder rings as today; an
`action` reminder instead:

1. opens the notch on the reminder view in a "Running in 10 s — Cancel" state (a short grace
   window; nothing runs without a visible chance to stop it);
2. submits `instruction` to the Agent through the existing summon/submit entry
   (`agent.rs`), exactly as if typed in the notch, with the reminder's links
   appended as context; tools available are the user's enabled ones (e.g.
   Reach's `open_link` for a Meet URL);
3. records the Agent's final reply as `last_result`, status `done` or
   `failed`, and shows it in the notch and the tab.

Rules to settle before building: never fire an action while the user is
dictating (queue until idle); missed actions (app closed) ask instead of
running late; the creator of an action reminder must be the user or a
confirmed Agent call (an Agent-created action needs one explicit approval at
creation, since it runs unattended later); recurrence is out of scope.

## 8. Other future work

* To-dos, quick notes and a month calendar view (the original planner idea).
* Recurring reminders (daily / weekly).
* Calendar providers (Google, Outlook) — read events as reminders, maybe
  write back. Needs OAuth through the extension platform; not started.
* Reminder sound / talk-back voice reading the title.
