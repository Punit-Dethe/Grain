# Agent replies: a short spoken version and a result card

Branch: `feat/agent-reply-voice-and-result-card` (from `main`, 2026-11-21).

**Status:** Phase 1 (spoken version) built, not yet tried in the real app. Phases 2–4 not started.

Phase 1 as built: `grain_agent_reply.rs` (`SAY_RULE`, `split_say`, `stream_say`, `confirm_line`); `grain_tts::reply_speech` + `fit_sentences` (fallback 500 chars, whole sentences only; the feed reads all of a SAY line); `agent.rs` sends `SAY_RULE` only with talk-back on, speaks the SAY line or the approval line, keeps the SAY line out of the chat and the stream, keeps it in front through approval continuations, and strips it from first-step lines.

Revision 2 (2026-11-21) folds in the user's review of revision 1.

## How people use this

The Agent is used **while multitasking**. You ask for something, carry on with your work, and the answer comes back by voice. So:

- The voice tells you, briefly and in whole sentences, what happened. That should be enough to move on.
- **Nothing opens on top of your work by itself.** If the short version makes you curious, you open the notch and read the detail when it suits you.
- The one exception is a request whose whole point is to get text back to use: "turn this into an email", "make this more formal". There the card with the text *is* the answer, so it appears, you copy, and it closes.

## The problem today

1. **Talk-back reads everything.** `grain_tts::speak` reads `speakable(reply)` up to 3000 characters. A one-page story, or a detailed report of five actions, is read out in full.
2. **Rewrites have nowhere to go.** "Convert this into an email": the email is plain chat text and is read aloud. `UI_RULE` in `grain_agent_canvas.rs` tells the model *not* to draw a view for "a rewrite or transformation of selected text".

## The rule, in one line

**If the reply is long, the AI also writes a short spoken version, and that is what is read aloud, in full.** A short reply is read as it is. A text transform goes into a result card, and the voice just says it's done.

The same rule applies to every kind of reply (stories, explanations, action reports, approval requests), which makes it consistent and easy to enforce. No second AI call, no background worker.

---

## Part 1: what gets spoken

### The short spoken version

When talk-back is on and the reply is long (more than about three sentences, or any list, table, code or long text), the model starts its reply with one line:

```
SAY: I've written you a short story about a lighthouse keeper and the storm that changes his mind.
<the full reply>
```

- **The SAY line is spoken whole.** There is no hard time cap and no cutting mid-sentence. It is short because the model wrote it short.
- It describes, it doesn't read out: "I've written you a poem about autumn, four verses, it ends on a hopeful note", not the first verse.
- It is first in the reply, so streamed speech (`speak_feed`) can speak it immediately and stops where the body starts. The fast first audio we have today is kept.
- Short replies get no SAY line and are spoken whole, as today.
- The chat shows the full reply. The SAY line itself is not shown.

**Prompt cost:** one compact paragraph, sent only when `agent_talk_back` is on. It needs to be clear enough that the model follows it reliably, and no more. Draft:

> SPOKEN VERSION: your reply is read aloud. If it is longer than about three sentences, or has a list, table, code or a long text, begin with one line `SAY: …`: one or two natural sentences that tell the user what you did or found, in plain speech, as if telling a friend. Describe long content rather than reading it ("I've written you a poem about autumn; it ends on a hopeful note"). For actions, say what was done in short ("Filed the bug on GitHub, added it to Linear and wrote it up in Notion"). Then the full reply on the next line. Short replies need no SAY line.

### If the model forgets the SAY line

A safety net, not the main path: whole sentences are spoken up to a generous budget (about 500 characters) and speech always ends at a sentence boundary, never mid-word or mid-sentence. The existing 3000-character cap goes. A missing SAY line is logged, so we can see how often the prompt fails while testing.

### Situations

| # | Situation | Example | Spoken | Shown (only if you open the notch) |
|---|---|---|---|---|
| 1 | Short answer | "What's 15% of 80?" | Whole reply: "Twelve." | Chat |
| 2 | Long creative text | "Write me a one-page story" | "I've written you a story about a lighthouse keeper; it's in the notch when you want it." | Full story |
| 3 | Long explanation | "How does a heat pump work?" | "It moves heat instead of making it, like a fridge running backwards." | Full explanation |
| 4 | Simple action | "Remind me at 5 to call Mum" | "Done, I'll remind you at 5." | Chat |
| 5 | Multi-step action | "File this bug, track it and document it" | "Filed it on GitHub, added it to Linear and wrote it up in Notion." | Detailed report (links, IDs), or a view |
| 6 | **Text transform** | Selected text + "convert this into an email" | "Done, here's your email." (sometimes "Want it more formal?") | **Result card opens** (Part 2) |
| 7 | Refine a result | "More formal" | "Made it more formal." | Card updates in place |
| 8 | Summary of selection | "Summarise this" | **The summary itself**, read aloud | Chat. No card: a summary is something you listen to |
| 9 | List / steps | "Steps to reset my router" | "Five steps, starting with unplugging it for 30 seconds." | Full list / view |
| 10 | View reply | Weather | Existing short voice line | View (unchanged) |
| 11 | Question back | "Which Priya, work or family?" | Whole question (it's short) | Chat |
| 12 | Approval needed | "Publish the build, tag it, post the notes" | Short version: "Before I publish the build and post the notes, I need your OK." | Confirmation details |
| 13 | Asked to hear it | "Read this email to me" | The full text: the model puts it all in SAY | Chat |
| 14 | Error | Provider down | "I couldn't reach the AI, check your connection." | Chat |

What "transform" means (6, 7): you selected text and asked for **that text back, changed** (email, more formal, better format, fix grammar, translate, shorten). Selected text plus "create a GitHub issue about this" is an action (5), and "summarise this" is a spoken answer (8). Neither one gets a card.

### Approval requests (12)

Today `confirm_action` replies are spoken from the confirmation's markdown. New: the confirmation gets the same treatment as everything else. Details stay on screen; the spoken line is short ("I need your OK to publish the build and post the release notes"). The host builds it from the confirmation's title and summary, since the model isn't asked to write a SAY line for host-gated approvals. Approving still happens on screen or by saying "yes", as today.

### The notch stays where you left it

Unchanged from today, and now a design rule: a long reply in compact voice mode **stays compact** and speaks the short version. Revision 1's "open the notch when the reply was shortened" is dropped. Only two things open the notch by themselves: a generated view (existing behaviour, unchanged) and a result card (Part 2).

---

## Part 2: the result card (text transforms only)

### Why not an OpenUI view

The email would have to be escaped inside an OpenUI Lang string (fragile for long text), it would only work with Dynamic UI on, and text you copy needs a fixed, readable layout, not one the model composes. So a result is its own **host-native reply kind**, next to `text` and `ui`.

### Format

```
SAY: Done, here's your email.
<result kind="email" subject="Thursday follow-up">
Hi Sam,
...
</result>
```

- `kind`: `email`, `message`, `rewrite`, `list`, `code`, `translation` (only picks layout and refine chips; unknown → `rewrite`). `subject` only for email.
- Prompt rule (short): when the user asks to change, rewrite, reformat or translate text and wants that text back, put only the new text in the `<result>` block, with no preamble inside it. Summaries and actions about the text are not results. When a selection was captured, the system message says so.
- A reply has a view **or** a result, never both. A result reply always has a SAY line (fallback when missing: "Done.").

### Host

- `AgentReply.result: Option<AgentResult { kind, subject, body }>`; the same on `AgentHistoryMessage`, so the chat keeps it.
- The latest result is sent back with the next turn (like the latest view), so "more formal" or "shorter" refines it.
- A result reply opens the notch to the card, even in compact voice mode.

### The card

Experimental sizing, to be tuned in the real app (the reply window stays a fixed 760×620 and the shell is clipped with `agent_notch_shape`, as views are):

| Variant | When | Size (CSS px) |
|---|---|---|
| Short | ≤ 3 lines (a rewritten sentence, a message) | 440 wide, height fits (min 160) |
| Document | email, rewrite, list, translation | 560 wide (≈ 70 characters a line), height fits up to 560 |
| Code | `code` | 600 wide, monospace, up to 560 |

Layout:
1. **Header**: kind label (quiet), close X.
2. **Subject** (email only), with its own small copy icon.
3. **Body**: readable size, markdown rendered for `list`. Streams in as it arrives. **Scrolls when it is longer than the card.** This is the one exception to the views' "never scroll" rule, because text you are about to copy must never be cut off or shrunk. Header and action row stay fixed.
4. **Action row**:
   - **Copy**: primary and large. On click: copy, a brief check, then **the notch smoothly closes**. Done.
   - **Refine chips**, two or three per kind (email: *More formal · Shorter · Friendlier*). The card updates in place. Speaking a follow-up works too.

Copy writes plain text. Rich copy (HTML for bullets and bold) comes later if pasting into Gmail or Docs needs it.

### Later, not in this branch

- **Replace**: paste the result back over the original selection. Planned separately, after this has passed testing.

---

## Code layout

All Grain-owned; nothing in `src-tauri/src/handy/`.

| Where | Change |
|---|---|
| `src-tauri/src/grain_agent_reply.rs` (new) | Pure splitter: `SAY:` line → `say`, `<result …>` → `result`, the rest → text (then `split_ui` for a view, as today). Tests for each row of the situations table. |
| `src-tauri/src/grain_tts.rs` | Speak `say` when present; sentence-boundary fallback budget instead of the 3000 cap; `SpeechFeed` waits for the first line, speaks only the SAY line if there is one, never speaks a result body. |
| `src-tauri/src/agent.rs` | `agent_run` uses the splitter, the SAY prompt (talk-back on only), the result prompt, latest result sent back, a short spoken line for confirmations, a result opens the notch. `StreamTurn` keeps the SAY line out of the visible stream and streams result bodies to the card. |
| `src-tauri/src/grain_agent_canvas.rs` | `UI_RULE` points transforms to the result block instead of "skip". |
| `src-tauri/src/grain_agent_history.rs` | `result` on recorded messages. |
| `src/app/components/agent/ResultCard.tsx` (new) + `agent.css` | The card. Mounted only while a result is open. |
| `AgentPanel.tsx` | Show `reply.result`; Copy → copy then close the notch; chips send follow-ups. |
| `src/app/bindings.ts` | `AgentResult` type and fields. |

Resource cost: zero while idle. A few string scans per reply and one component while a card is open.

## Phases

| Phase | Scope | Done when |
|---|---|---|
| **1. Spoken version** | SAY parsing, TTS changes, streaming, prompt, confirmation line | A long story is spoken as one or two sentences; short replies, questions and summaries are read whole; nothing opens by itself; "read it to me" reads it all. Rust tests per situation. |
| **2. Result reply kind** | `<result>` parsing, `AgentReply.result`, history, refine round-trip, prompt | Transforms come back as results; summaries and actions don't. |
| **3. Result card** | Card, sizes, scrolling body, Copy-and-close, chips | User visual approval in the real app. |
| **4. Tuning** | Prompt wording against real replies, card sizes, harness scenarios for the table | `tests/agent-harness/` scenarios pass. |

Each phase: Rust lib tests, clippy, `tsc`, lint on touched files only, commit and push per verified chunk. The user checks Phase 3 visually in the real app.

## Decisions (user, 2026-11-21)

- Long replies get a model-written short version that is spoken in full. No hard time cap, no mid-sentence cuts.
- Short summaries are read aloud, not put in a card.
- Nothing opens on its own for long replies or actions; the user opens the notch when curious.
- Result cards only for "give me this text back, changed".
- Result card body scrolls; after Copy the notch closes smoothly.
- Card sizing is experimental, tuned in the real app.
- Replace (paste back over the selection) comes later, after testing.
