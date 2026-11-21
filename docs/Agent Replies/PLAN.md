# Agent replies: a short spoken line and a result card

Branch: `feat/agent-reply-voice-and-result-card` (from `main`, 2026-11-21). Plan only; nothing is built yet.

## The problem

1. **Talk-back reads everything.** `grain_tts::speak` reads `speakable(reply)` up to 3000 characters (`MAX_SPOKEN_CHARS`). A long answer, a list, or a drafted email is read out word by word. The streamed path (`speak_feed`) starts talking before the reply is complete, so it cannot know the length in advance.
2. **Rewrites have nowhere to go.** "Turn this into an email", "make this more formal", "summarise this paragraph": the result is plain chat text. In compact voice mode (`agent_compact_voice`), which most people use, the notch stays small and the voice reads the email aloud. `UI_RULE` in `grain_agent_canvas.rs` tells the model *not* to draw a view for "a rewrite or transformation of selected text", so Dynamic UI does not help today either.

What the user wants: the voice says something short ("Done, here's your email. Want it more formal?"), and the rewritten text shows up in a decent-sized card with a clear Copy button. Copy and you're done.

## The idea in one line

Every reply has two parts: **what Grain says** (always short, unless you asked to hear it all) and **what Grain shows** (the full answer, a view, or a result card). The model writes both in the same call. Grain enforces the length limit itself, so a reply is never read out at full length by accident.

No second AI call, no background worker, no new model. These are string rules in the host and one new card in the notch.

---

## Part 1: what gets spoken

### Reply format (one LLM call)

When talk-back is on, the model may start a reply with one line:

```
SAY: Here's your email. Want it more formal?
<the full reply, a result block, or a view program>
```

- The `SAY:` line is first, so the streamed speech (`speak_feed`) speaks it straight away and stops where the body starts. The fast first audio we have today is kept.
- The model writes a `SAY:` line only when the reply is long, structured, or a result. A short conversational reply is spoken whole, with no tag and no extra tokens.
- `split_ui` already tolerates a leftover `SAY:` tag. That parsing moves into one shared splitter (see Part 3).
- The prompt addition is sent **only when `agent_talk_back` is on**, and is kept to about 60 tokens (it is paid on every turn).

### Grain enforces the limit (host fallback)

A pure function in `grain_tts.rs`, `spoken_plan(reply, say) -> Spoken`, decides what is said:

| Case | What is spoken |
|---|---|
| Model wrote `SAY:` | That line (still capped at 3000 characters) |
| No `SAY:`, short plain reply (≤ ~220 chars, ≤ 2 sentences, no list/heading/table/code) | The whole reply |
| No `SAY:`, long or structured | The first sentence if it is ≤ ~160 chars, then "The rest is on screen." Otherwise just "It's on screen." |
| Only code | "The code is in the reply." (existing) |

About 220 characters is roughly 15 seconds of speech. It is one constant, `SPEAK_WHOLE_MAX_CHARS`, and not a setting (open question 1).

**Streaming:** `SpeechFeed` gets the same rule. It speaks the `SAY:` line if there is one. If not, it speaks whole sentences until the text passes the limit, finishes the current sentence, adds "The rest is on screen." and stops. That is the fallback above, done while the reply streams in.

### Situations

| # | Situation | Example | Spoken | Shown |
|---|---|---|---|---|
| 1 | Small talk / short fact | "What's 15% of 80?" | Whole reply: "Twelve." | Chat |
| 2 | Long explanation | "How does a heat pump work?" | Gist: "It moves heat instead of making it, like a fridge in reverse. The details are on screen." | Chat, notch opens |
| 3 | Tool task done | "Remind me at 5 to call Mum" | "Done. I'll remind you at 5." | Chat / receipt |
| 4 | Long tool task | "Clean up my inbox" | Existing first-step line ("Checking your inbox."), then a short result: "Archived 34 emails." | Chat |
| 5 | **Rewrite / transform** | "Turn this into an email" | "Here's your email. Want it more formal?" | **Result card** |
| 6 | Refine a result | "More formal" | "Made it more formal." | Result card, replaced in place |
| 7 | Summarise selection | "Summarise this" | "Here's the summary, three points." (open question 5) | Result card |
| 8 | List / steps / options | "Steps to reset my router" | "Five steps. Start by unplugging it for 30 seconds." | Chat or view |
| 9 | View reply | Weather | Existing voice line ("14 degrees, rain until 3.") | View |
| 10 | Question back | "Which Priya?" | Whole question: you need it to answer | Chat |
| 11 | Risky action confirmation | "Send this to 40 people?" | **Whole** confirmation text, never shortened, because you must hear what you are approving | Confirm panel |
| 12 | Error | Provider down | Short reason: "I couldn't reach the AI. Check your connection." | Chat |
| 13 | **Asked to hear it** | "Read this email to me" | Full text: the model puts it all in `SAY:`. The length limit applies only to the fallback, never to the model's own `SAY:` | Chat |
| 14 | Code | "Write a regex for emails" | "The regex is in the reply." | Chat / result card |

Rules that come out of this table:
- Questions back (10) and confirmations (11) are always spoken whole. The host checks `confirm_action` and skips shortening there.
- The final spoken line must not repeat the first-step line already said (4).
- A refine offer ("Want it more formal?") goes on about one result in three, not every reply. The prompt says "sometimes", and the card's chips carry the offer anyway.

### Compact voice mode

Today a long reply can be read in full while the notch stays compact. New rule: **if the spoken line is shorter than the shown answer** (cases 2, 5–8, 14), the notch opens to show it, the same way it already does for views (`first_turn = None` in `agent_run`). Only replies that are spoken whole (1, 3, 10, 12) stay compact.

---

## Part 2: the result card

### Why not an OpenUI view

The model could draw the email as an OpenUI `Text` block, but:
- the whole body would have to be escaped inside an OpenUI Lang string, which is fragile for long text with quotes and newlines;
- it would only work with Dynamic UI on;
- a document is copied, not looked at: it needs a fixed, readable layout, not a model-composed one.

So a result is its own **host-native reply kind**, next to `text` and `ui`.

### Format

The model wraps the transformed text in one block after the optional `SAY:` line:

```
SAY: Here's your email. Want it more formal?
<result kind="email" title="Follow-up on Thursday's call" subject="Thursday follow-up">
Hi Sam,
...
</result>
```

- `kind`: `email`, `message`, `rewrite`, `summary`, `list`, `code`, `translation` (only picks the card's layout and chips; unknown kinds fall back to `rewrite`).
- `subject` only for `email` (shown as its own copyable field).
- The prompt rule: when the user asks to rewrite, convert, reformat, translate, summarise or draft text (with or without a selection), put the result, and only the result, in the block: no preamble and no "Here's your email" inside it. When a selection was captured at summon, the system message says so, which makes this the likely path. The model decides; there is no separate classifier.
- One reply has a view **or** a result, never both.

### Host

- `AgentReply` gains `result: Option<AgentResult { kind, title, subject, body }>`, and `AgentHistoryMessage` gains the same, so the chat keeps it.
- The latest result is sent back with the next turn (like the latest view), so "more formal", "shorter" or "in Spanish" refines that text. Older results are not resent.
- A reply with a result is never presented compact (Part 1).

### The card (notch frontend)

Sized for the content, inside the existing fixed 760×620 reply window (live window resizes stay off the table; the shell is clipped with `agent_notch_shape` like views):

| Variant | When | Size (CSS px) |
|---|---|---|
| **Short** | ≤ 3 lines (a rewritten sentence, a message) | 440 wide, height fits, min 160 |
| **Document** | email, summary, rewrite, list, translation | 560 wide (≈ 70 characters a line, comfortable to read), height fits up to 560 |
| **Code** | `code` | 600 wide, monospace, height fits up to 560 |

Layout, top to bottom:
1. **Header row**: kind label + title (quiet, small), close X.
2. **Subject field** (email only), with its own small copy icon.
3. **Body**: the text in a readable size, markdown rendered for `list`/`summary`. It streams into the card as it arrives.
4. **Action row** (bottom, always visible):
   - **Copy**: the primary, large button. Turns into a check and "Copied" for 1.5 s.
   - **Replace**: shown only when a selection was captured. It pastes the result over the original selection in the source app (focus goes back to the captured window, then Grain's normal paste path), then closes the notch.
   - **Refine chips**, two or three per kind: email → *More formal · Shorter · Friendlier*; summary → *Shorter · Bullet points*; rewrite → *Simpler · More formal*. A chip sends that follow-up; the card updates in place.
   - Speaking a follow-up ("make it shorter") works through the existing follow-up dictation.

Long bodies: the views rule is "never scroll", but text you are about to copy cannot be cut off or shrunk to unreadable. Proposal: the **body only** scrolls past 560 px, while the header and action row stay fixed (open question 2).

Copy format: plain text for every kind. For `list`/`summary`/`email` also write HTML to the clipboard, so pasting into Gmail or Docs keeps bullets and bold (Phase 4, needs a small clipboard addition, since `agent_copy` writes text only today).

After Copy the card stays open, so you can still refine. Escape or X closes it (open question 3).

---

## Part 3: code layout

All Grain-owned; nothing touches `src-tauri/src/handy/`.

| Where | Change |
|---|---|
| `src-tauri/src/grain_agent_reply.rs` (new, small) | One splitter for a reply: `SAY:` line → `say`, `<result …>` block → `result`, `root = …` → `ui` (moves `split_ui` here from `grain_agent_canvas.rs`, which keeps the prompts). Pure functions + tests for every row of the situations table. |
| `src-tauri/src/grain_tts.rs` | `spoken_plan`, `SPEAK_WHOLE_MAX_CHARS`, the "rest is on screen" fallback, the `SpeechFeed` limit, speaking only `say` while streaming. |
| `src-tauri/src/agent.rs` | `agent_run`: use the splitter; speak `spoken_plan`; open the notch when spoken < shown; never shorten confirmations; send the latest result back; prompt rules (`SAY` only with talk-back on, result rule always). `StreamTurn` streams the body to the panel and only the `say` part to speech. |
| `src-tauri/src/grain_agent_history.rs` | `result` field on recorded messages. |
| `src/app/components/agent/ResultCard.tsx` (new) + `agent.css` | The card. Mounted only while a result is open. |
| `AgentPanel.tsx` | Show `reply.result` as a card (like `reply.ui`), chips send follow-ups, Replace command. |
| `src/app/bindings.ts` | Regenerated (`AgentResult`, new command). |
| new command `agent_replace_selection(text)` | Restores focus to the window captured at summon and pastes through the existing paste path. |

Resource cost: zero while idle. A few string scans per reply, one React component while a card is open, no new threads, no extra AI calls.

---

## Phases

| Phase | Scope | Done when |
|---|---|---|
| **1. Spoken line** | `spoken_plan` + streaming limit + `SAY:` prompt rule + "open the notch when shortened" | A long answer is spoken as a gist, short replies unchanged, confirmations and questions spoken whole, "read it to me" reads it all. Rust tests per situation. |
| **2. Result reply kind** | `<result>` parsing, `AgentReply.result`, history, latest result sent back, prompt rule, bindings | Host returns `result` for transforms; refines replace it; tests. |
| **3. Result card** | `ResultCard`, sizes, Copy, refine chips, streaming into the card, Replace | User visual approval in the real app (UI protocol). |
| **4. Polish** | Rich copy (HTML + text), chip wording per kind, harness acceptance runs | `tests/agent-harness/` scenarios for the situations table pass. |

Each phase: `cargo test` (lib), `cargo clippy`, `bunx tsc`, lint on touched files only (no repo-wide formatters), commit + push per verified chunk. The user does the visual check for Phase 3 with the real `bun run tauri dev`.

## Open questions for the user

1. **Length limit**: speak a reply whole up to about 220 characters (≈ 15 s) and shorten beyond that? Fixed constant, or a "Short / Full" talk-back choice in settings?
2. **Long results**: let only the card's body scroll past 560 px (recommended), or keep "never scroll" and cap the length?
3. **After Copy**: keep the card open so you can still refine (recommended), or close the notch automatically?
4. **Replace button**: include "Replace" (paste back over the original selection)? It is a one-click finish, but it acts on another app.
5. **Summaries**: read a short summary aloud (it is often what you want to hear), or treat it like any other result (card + one-line voice)?
