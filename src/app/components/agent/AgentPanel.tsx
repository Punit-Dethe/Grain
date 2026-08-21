import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ChevronLeft,
  ChevronRight,
  Copy,
  RotateCcw,
  X,
  Check,
} from "lucide-react";
import {
  commands,
  type AgentMessage,
  type AgentAutocopy,
  type AgentPanelPosition,
  type AgentReply,
  type AgentSource,
} from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";
import "./agent.css";

/** [GRAIN] CENTER layout geometry that must agree with the backend
 * (`agent.rs`): the panel's top is pinned this far below the work-area top and
 * keeps this gap at the bottom, so the frontend can cap its own max-height to
 * match what the window will actually clamp to. */
const CENTER_TOP_OFFSET = 76;
const CENTER_BOTTOM_GAP = 52;

/** Duration of the card's opening growth — must match `agc-expand` in
 * `agent.css`. Nothing on the backend races it any more: the window no longer
 * waits a guessed interval before showing, because the panel paints nothing at
 * all until `agent-reveal` arrives. */
const REVEAL_MS = 440;
/** Shared opening curve. Decelerating, no overshoot. */
const EXPAND_EASE = "cubic-bezier(0.16, 0.84, 0.3, 1)";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  // Grain Recall evidence footer (RECALL-PLAN §6): empty/false for Assist.
  sources?: AgentSource[];
  notFound?: boolean;
  // A `forget` turn hands us the memory to confirm before deletion (§7.2).
  confirmDelete?: AgentSource | null;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;
// Glyph constants (kept out of JSX so the i18n lint doesn't treat them as copy).
const SEND_ARROW = "↵";
const ENTER_GLYPH = "⏎";

/** Compact relative age for a source chip ("3d ago", "yesterday"). Symbols
 * only, so no i18n copy — matches the hardcoded keycap glyphs above. */
function relDate(ms: number): string {
  const diff = Math.max(0, Date.now() - ms);
  const mins = Math.floor(diff / 60_000);
  const hours = Math.floor(diff / 3_600_000);
  const days = Math.floor(diff / 86_400_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days === 1) return "yesterday";
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}

/** Pretty-print one part of a shortcut binding for the keycap chips. */
function keycapLabel(part: string): string {
  const p = part.trim().toLowerCase();
  switch (p) {
    case "ctrl":
    case "control":
      return "Ctrl";
    case "alt":
      return "Alt";
    case "option":
      return "⌥";
    case "shift":
      return "⇧";
    case "meta":
    case "cmd":
    case "command":
      return "⌘";
    case "enter":
      return ENTER_GLYPH;
    case "space":
      return "Space";
    case "escape":
      return "Esc";
    default:
      return p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1);
  }
}

/**
 * [GRAIN] The Agent panel — the bottom-right reply surface, in two stages:
 *
 *   COMPACT (the reference card): retry pager (‹ 1/N ›) top-left, ✕ top-right,
 *   the captured text (quote, expandable via "More"), the reply, and a bottom
 *   bar — Ask follow up (+ its configurable shortcut as keycaps) · copy ·
 *   retry · Confirm ⏎ (pastes the displayed reply into the source app).
 *
 *   EXPANDED (the conversation): grows to the sidebar footprint when the user
 *   asks a follow-up (button, shortcut, or the Quick-Agent pill offer). Retry
 *   and the version pager disappear once a follow-up exists — versions belong
 *   to the first reply only. Esc closes either stage.
 *
 * Auto-copy honors the `agent_autocopy` setting: off / first reply / all.
 */
export function AgentPanel() {
  const { t } = useTranslation();
  // ONE handle for the component's life. `getCurrentWindow()` mints a fresh
  // object on every call, so calling it inline made `win` a new identity each
  // render — every effect that depends on it tore down and re-subscribed
  // constantly, and `listen()` is async, so any subscription still in flight
  // when its own cleanup ran survived forever. Those orphans stacked up and
  // each one handled the same event again (one dictation pasted N times).
  const win = useMemo(() => getCurrentWindow(), []);

  // Conversation (expanded stage). In the compact stage this holds only the
  // first user turn; the assistant replies live in `versions`.
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  // Retry versions of the FIRST reply (compact stage), and which one is shown.
  // Each version carries its Recall evidence (sources / not-found) alongside
  // the text; Assist versions have empty sources so no footer renders.
  const [versions, setVersions] = useState<AgentReply[]>([]);
  const [versionIdx, setVersionIdx] = useState(0);
  const [expanded, setExpanded] = useState(false);
  // The panel is revealed the instant the user submits — BEFORE the transcript
  // and reply exist — so it opens in the busy (loading) state.
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copyFlash, setCopyFlash] = useState(false);
  // CENTER: which reply's copy icon is currently flashing (per-turn, not global).
  const [flashedId, setFlashedId] = useState<string | null>(null);
  const [quoteOpen, setQuoteOpen] = useState(false);
  // Bumped on every reveal signal so the card replays its opening animation.
  // The window is warm and pre-created, so mounting is NOT the reveal.
  const [appearNonce, setAppearNonce] = useState(0);
  // Nothing renders until the window is actually being shown. Showing a window
  // and arming an animation inside it are independent, so which one wins is a
  // race — and the losing order paints the card at rest for a frame before it
  // snaps small and grows, which is what read as the panel opening twice. With
  // nothing to paint until the entrance is armed, that order stops mattering.
  const [revealed, setRevealed] = useState(false);
  const revealedRef = useRef(false);
  const revealingRef = useRef(false);
  // Set by `expand()` to the box the card occupied BEFORE the window grew. Its
  // presence tells the reveal effect to animate the card's box open from there
  // instead of playing the plain opening growth. Consumed once.
  const growFromRef = useRef<{ w: number; h: number } | null>(null);
  const [followupShortcut, setFollowupShortcut] = useState<string>("");
  // Which reply surface this session is rendering — loaded at mount. `side` is
  // the original bottom-right card; `center` is the sleek center-top panel.
  const [position, setPosition] = useState<AgentPanelPosition>("side");
  const positionRef = useRef<AgentPanelPosition>("side");
  positionRef.current = position;
  // CENTER only: false until the user opens a follow-up. While false the bottom
  // shows the follow-up + Confirm bar; once true it's just the text field — and
  // it stays true for the rest of the session (the bar never returns).
  const [composing, setComposing] = useState(false);
  const composingRef = useRef(false);
  composingRef.current = composing;

  const contextRef = useRef<string | null>(null);
  const instructionRef = useRef<string>("");
  const autocopyRef = useRef<AgentAutocopy>("first");
  const firstCopyDoneRef = useRef(false);
  const messagesRef = useRef<ChatMessage[]>([]);
  const versionsRef = useRef<AgentReply[]>([]);
  const versionIdxRef = useRef(0);
  const expandedRef = useRef(false);
  const busyRef = useRef(false);
  const followupRef = useRef<HTMLInputElement>(null);
  // CENTER layout: the auto-growing follow-up textarea. `cardRef` is the card
  // itself — every layout uses it for the reveal animation, and CENTER also
  // reports its height to the backend so the window hugs its content.
  const centerInputRef = useRef<HTMLTextAreaElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const lastReportedH = useRef(0);
  const endRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | undefined>(undefined);
  const startedRef = useRef(false);
  // Guards the first LLM run so the mount-take and the `agent-instruction`
  // event (whichever wins the race) only trigger it once.
  const firstRunStartedRef = useRef(false);
  messagesRef.current = messages;
  versionsRef.current = versions;
  versionIdxRef.current = versionIdx;
  expandedRef.current = expanded;
  busyRef.current = busy;

  const lastReplyOf = (msgs: ChatMessage[]) =>
    [...msgs].reverse().find((m) => m.role === "assistant")?.content ?? "";

  /** The reply the surface currently presents (pager-aware in compact). */
  const displayedReply = expanded
    ? lastReplyOf(messages)
    : (versions[versionIdx]?.text ?? "");
  /** Evidence footer for the compact card's paged version (empty for Assist).
   * Expanded renders a footer per assistant turn instead. */
  const compactSources = versions[versionIdx]?.sources ?? [];
  const compactNotFound = versions[versionIdx]?.not_found ?? false;
  const compactConfirmDelete = versions[versionIdx]?.confirm_delete ?? null;

  /** The window is on screen — render, and play the entrance. Idempotent: the
   * backend announces the reveal AND the panel checks for itself on mount, so
   * whichever arrives first wins and the second is a no-op. One window is only
   * ever revealed once (Esc destroys it rather than hiding it). */
  const reveal = useCallback(() => {
    if (revealedRef.current) return;
    revealedRef.current = true;
    setRevealed(true);
    setAppearNonce((n) => n + 1);
  }, []);

  const flashCopied = useCallback(() => {
    setCopyFlash(true);
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setCopyFlash(false), 1600);
  }, []);

  /** Auto-copy per the user's policy (off / first / all). */
  const maybeAutoCopy = useCallback(
    (reply: string) => {
      if (!reply.trim()) return;
      const policy = autocopyRef.current;
      const shouldCopy =
        policy === "all" || (policy === "first" && !firstCopyDoneRef.current);
      firstCopyDoneRef.current = true;
      if (shouldCopy) {
        commands
          .agentCopy(reply)
          .then(flashCopied)
          .catch(() => {});
      }
    },
    [flashCopied],
  );

  /** Run the FIRST instruction (or a retry of it) — compact stage. */
  const runFirst = useCallback(
    async (instruction: string) => {
      firstRunStartedRef.current = true;
      setBusy(true);
      setError(null);
      try {
        const payload: AgentMessage[] = [
          { role: "user", content: instruction },
        ];
        const res = await commands.agentRun(payload, contextRef.current);
        if (res.status === "ok") {
          const reply = res.data;
          setVersions((prev) => {
            const next = [...prev, reply];
            setVersionIdx(next.length - 1);
            return next;
          });
          maybeAutoCopy(reply.text);
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [maybeAutoCopy, t],
  );

  /** Run the whole conversation — expanded stage. */
  const runConversation = useCallback(
    async (history: ChatMessage[]) => {
      setBusy(true);
      setError(null);
      try {
        const payload: AgentMessage[] = history.map((m) => ({
          role: m.role,
          content: m.content,
        }));
        const res = await commands.agentRun(payload, contextRef.current);
        if (res.status === "ok") {
          const reply = res.data;
          setMessages((prev) => [
            ...prev,
            {
              id: rid(),
              role: "assistant",
              content: reply.text,
              sources: reply.sources,
              notFound: reply.not_found,
              confirmDelete: reply.confirm_delete,
            },
          ]);
          maybeAutoCopy(reply.text);
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
        followupRef.current?.focus();
      }
    },
    [maybeAutoCopy, t],
  );

  /** Expand into the conversation stage (button / shortcut / pill offer). */
  const expand = useCallback(() => {
    if (expandedRef.current) {
      followupRef.current?.focus();
      return;
    }
    // The first run is still in flight — expanding now would strand its reply
    // in the (hidden) version list. The button is disabled; this also covers
    // the global follow-up shortcut.
    if (busyRef.current || versionsRef.current.length === 0) return;
    // Freeze the displayed version into the conversation history (evidence and
    // not-found carried through so the footer persists after expanding).
    const reply = versionsRef.current[versionIdxRef.current];
    const seed: ChatMessage[] = [];
    if (instructionRef.current) {
      seed.push({ id: rid(), role: "user", content: instructionRef.current });
    }
    if (reply && reply.text) {
      seed.push({
        id: rid(),
        role: "assistant",
        content: reply.text,
        sources: reply.sources,
        notFound: reply.not_found,
        confirmDelete: reply.confirm_delete,
      });
    }
    // ONE window, and it does not move. The window is already the conversation's
    // footprint (`side_envelope` in `agent.rs`) — the compact card is just a box
    // in its bottom-right corner — so expanding is that box growing to fill a
    // window that never resizes. No native step to hide, race, or wait for.
    //
    // Measure the CARD, not the viewport: the viewport is the target, and it has
    // been the target the whole time. Reading the window for the start box was
    // only ever correct while the window itself was being resized, and when that
    // read lost its race it returned the target — collapsing the growth to
    // nothing and falling through to the entrance animation instead, which
    // played scale(0.86)→1 on the full-size conversation. That is the "big card
    // compacts, then the expanded appears".
    // `offsetWidth/Height`, not the bounding rect: the rect includes transforms,
    // so expanding while the entrance is still scaling would read a shrunken box
    // and grow from the wrong size. Same reason `reportHeight` uses it.
    const card = cardRef.current;
    const from = {
      w: card?.offsetWidth || window.innerWidth,
      h: card?.offsetHeight || window.innerHeight,
    };
    if (card) {
      card.style.width = `${from.w}px`;
      card.style.height = `${from.h}px`;
    }
    growFromRef.current = from;
    setMessages(seed);
    setExpanded(true);
    setAppearNonce((n) => n + 1);
    window.setTimeout(() => followupRef.current?.focus(), 60);
    // State only now (which brain owns Enter, whether dictation routes into the
    // panel). It no longer touches the window, so nothing waits on it.
    void commands.agentSetPanelMode(true).catch(() => {});
  }, []);

  /** Open the CENTER follow-up field (button click or the continuation
   * shortcut). Once open it stays open for the session. */
  const startCompose = useCallback(() => {
    setComposing(true);
    requestAnimationFrame(() => centerInputRef.current?.focus());
  }, []);

  /** Confirm: paste the displayed reply back into the source app (backend
   * closes this window, refocuses the target, and pastes). */
  const confirm = useCallback(() => {
    const text = expandedRef.current
      ? lastReplyOf(messagesRef.current)
      : (versionsRef.current[versionIdxRef.current]?.text ?? "");
    if (!text.trim() || busyRef.current) return;
    void commands.agentConfirmPaste(text).catch(() => {});
  }, []);

  const retry = useCallback(() => {
    if (busyRef.current || expandedRef.current || !instructionRef.current)
      return;
    void runFirst(instructionRef.current);
  }, [runFirst]);

  const copyReply = useCallback(() => {
    const text = expandedRef.current
      ? lastReplyOf(messagesRef.current)
      : (versionsRef.current[versionIdxRef.current]?.text ?? "");
    if (!text) return;
    commands
      .agentCopy(text)
      .then(flashCopied)
      .catch(() => {});
  }, [flashCopied]);

  /** Take the queued first instruction and run it — guarded so the mount-take
   * and the `agent-instruction` event fire it at most once. */
  const startFirstIfQueued = useCallback(async () => {
    if (firstRunStartedRef.current) return;
    let instruction: string | null = null;
    try {
      instruction = await commands.agentTakeInstruction();
    } catch {
      /* nothing queued yet */
    }
    if (instruction && instruction.trim() && !firstRunStartedRef.current) {
      instructionRef.current = instruction.trim();
      await runFirst(instructionRef.current);
    }
  }, [runFirst]);

  /** Seed the EXPANDED conversation from the retained Quick-Agent history
   * (reopen from a follow-up offer). No-op when there's nothing retained. */
  const openRetainedConversation = useCallback(async () => {
    if (firstRunStartedRef.current || expandedRef.current) return false;
    let retained: AgentMessage[] = [];
    try {
      retained = await commands.agentTakeConversation();
    } catch {
      return false;
    }
    if (retained.length === 0) return false;
    firstRunStartedRef.current = true;
    instructionRef.current =
      retained.find((m) => m.role === "user")?.content ?? "";
    // Replies already delivered (pasted) count against the "first"-copy policy.
    firstCopyDoneRef.current = retained.some((m) => m.role === "assistant");
    setMessages(
      retained.map((m) => ({
        id: rid(),
        role: m.role === "assistant" ? "assistant" : "user",
        content: m.content,
      })),
    );
    setBusy(false);
    setExpanded(true);
    void commands.agentSetPanelMode(true).catch(() => {});
    window.setTimeout(() => followupRef.current?.focus(), 60);
    return true;
  }, []);

  // Mount: load settings + the summon context. The panel is pre-created HIDDEN,
  // so mount runs BEFORE the user submits — the first instruction usually
  // arrives later via the `agent-instruction` event (below). We still try a
  // take here in case the instruction beat the webview to the punch.
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    (async () => {
      try {
        const res = await commands.getAppSettings();
        if (res.status === "ok") {
          autocopyRef.current = res.data.agent_autocopy ?? "first";
          setPosition(res.data.agent_panel_position ?? "side");
          const b = res.data.bindings["agent_followup"];
          if (b) setFollowupShortcut(b.current_binding);
        }
      } catch {
        /* defaults hold */
      }
      try {
        contextRef.current = await commands.agentGetContext();
      } catch {
        /* no context is fine */
      }
      // Quick-Agent reopen wins; otherwise pick up an already-queued instruction.
      if (await openRetainedConversation()) return;
      await startFirstIfQueued();
    })();
  }, [openRetainedConversation, startFirstIfQueued]);

  // Backend → panel signals for the pre-created (warm) window lifecycle.
  useEffect(() => {
    // `listen()` resolves asynchronously, so a subscription can land AFTER this
    // effect has been torn down. Retiring it on arrival is what guarantees no
    // orphan outlives the effect — "destroy if not in use", and the reason a
    // single event can never be handled twice.
    let dead = false;
    const uns: Array<() => void> = [];
    const track = (un: () => void) => (dead ? un() : uns.push(un));
    // The core queued the first instruction after we mounted → run it.
    void win
      .listen("agent-instruction", () => {
        void startFirstIfQueued();
      })
      .then(track);
    // The ONE entrance signal, emitted by every backend path that shows this
    // window (`reveal_panel` in `agent.rs`) and always after the state event
    // below, so the card is already showing the right thing when it appears.
    // The state events do NOT arm the entrance themselves — two of them can
    // land for a single reveal, and that played the animation twice.
    void win.listen("agent-reveal", reveal).then(track);
    // Reveal-in-loading handshake: the window is about to be shown; keep the
    // loading state until the first reply (or an error) lands.
    void win
      .listen("agent-loading", () => {
        if (!firstRunStartedRef.current && !expandedRef.current) setBusy(true);
      })
      .then(track);
    // A backend-side failure (STT/LLM) with no reply to show.
    void win
      .listen<string>("agent-error", (e) => {
        firstRunStartedRef.current = true;
        setBusy(false);
        setError(e.payload || t("agent.error"));
      })
      .then(track);
    // Follow-up offer opened the warm hidden panel → seed the conversation.
    void win
      .listen("agent-followup-open", () => {
        void openRetainedConversation();
      })
      .then(track);
    // [GRAIN] Dictation routed INTO the panel (the user used the app's STT while
    // the expanded conversation was focused). Append the transcript to the
    // follow-up field instead of it being OS-pasted (which would paste the
    // auto-copied AI reply). Handled here, not by the OS clipboard.
    void win
      .listen<string>("agent-panel-dictation", (e) => {
        const dictated = (e.payload || "").trim();
        if (!dictated || busyRef.current) return;
        const append = (el: HTMLTextAreaElement | HTMLInputElement | null) => {
          if (!el) return;
          const sep = el.value && !el.value.endsWith(" ") ? " " : "";
          el.value = el.value + sep + dictated;
          if (el instanceof HTMLTextAreaElement) {
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 132)}px`;
          }
          el.focus();
        };
        if (positionRef.current !== "center") {
          append(followupRef.current);
          return;
        }
        // CENTER: the field only exists once composing, but the backend has
        // already suppressed the OS paste to route the transcript here — so a
        // dictation while the quiet bar is showing would be dropped outright.
        // Dictating IS the request to compose: open the field, then append.
        if (composingRef.current) {
          append(centerInputRef.current);
          return;
        }
        startCompose();
        requestAnimationFrame(() => append(centerInputRef.current));
      })
      .then(track);
    // A window BUILT already-visible (the windowless follow-up rebuild) emits
    // its reveal while this webview is still loading, so that event is gone
    // before anything can hear it. Asking closes that hole: whichever of the
    // two answers first reveals, and `reveal` makes the loser a no-op. Without
    // this, one path would render a permanently blank window.
    void win
      .isVisible()
      .then((v) => {
        if (v) reveal();
      })
      .catch(() => {});
    return () => {
      dead = true;
      uns.forEach((u) => u());
    };
  }, [
    openRetainedConversation,
    reveal,
    startCompose,
    startFirstIfQueued,
    t,
    win,
  ]);

  // Replay the opening animation on each reveal. Restarted imperatively rather
  // than by remounting: the card carries refs (height reporting, scroll) that
  // must survive, and re-adding the class after a reflow is what re-arms a CSS
  // animation on an element that is already in the tree.
  //
  // LAYOUT effect, not a passive one: on expand the conversation card mounts in
  // the same commit that arms the animation, and a passive effect would let it
  // paint once at full size first — a visible pop before the growth begins.
  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!el || appearNonce === 0) return;
    revealingRef.current = true;
    const from = growFromRef.current;
    growFromRef.current = null;

    const to = { w: window.innerWidth, h: window.innerHeight };
    let anim: Animation | undefined;
    if (from) {
      // Expanding: grow the card's own box out to fill the window.
      //
      // `from` being set IS the signal, with no second opinion about whether the
      // box actually got bigger. That extra test used to decide this, and when
      // it read equal sizes it fell through to the ENTRANCE animation — which
      // shrinks the full-size card to 0.86 and grows it back. A growth that
      // measures as no growth must be a no-op, never a different animation.
      //
      // The inline size is the START box, so the first painted frame is right
      // by construction — never dependent on when the animation's own first
      // keyframe lands. Setting it to the TARGET and leaning on `fill:
      // backwards` instead is what flashed the full-size card for one frame at
      // the start of the growth. `fill: forwards` then holds the end box until
      // the finish handler hands the card back to the stylesheet.
      el.classList.add("is-growing");
      el.style.width = `${from.w}px`;
      el.style.height = `${from.h}px`;
      anim = el.animate(
        [
          { width: `${from.w}px`, height: `${from.h}px` },
          { width: `${to.w}px`, height: `${to.h}px` },
        ],
        { duration: REVEAL_MS, easing: EXPAND_EASE, fill: "forwards" },
      );
      const done = anim;
      void done.finished
        .then(() => {
          // Clear the pin FIRST (the stylesheet's 100%/100% is the same box the
          // fill is holding), then release the fill — so no frame falls between
          // the two and snaps the card shut.
          el.style.width = "";
          el.style.height = "";
          done.cancel();
        })
        .catch(() => {});
    } else {
      el.classList.remove("is-appearing");
      void el.offsetWidth; // force reflow so the animation restarts
      el.classList.add("is-appearing");
    }

    // Hand the card back to the stylesheet once it has played out, so nothing
    // (a pinned box, `will-change`) outlives the animation that needed it.
    const timer = window.setTimeout(() => {
      el.classList.remove("is-appearing", "is-growing");
      el.style.width = "";
      el.style.height = "";
      revealingRef.current = false;
    }, REVEAL_MS + 60);
    return () => {
      window.clearTimeout(timer);
      anim?.cancel();
    };
  }, [appearNonce]);

  // Esc closes — global so it works even when no field is focused.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void win.close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [win]);

  // Backend bridges: the transient global Enter (compact → confirm) and the
  // follow-up shortcut / pill click (→ expand).
  useEffect(() => {
    // Same late-resolve guard as the lifecycle listeners above.
    let dead = false;
    const uns: Array<() => void> = [];
    const track = (un: () => void) => (dead ? un() : uns.push(un));
    void win
      .listen("agent-global-enter", () => {
        if (!expandedRef.current) confirm();
      })
      .then(track);
    void win
      .listen("agent-followup", () => {
        // CENTER: the continuation shortcut opens (and focuses) the follow-up
        // field. SIDE keeps its discrete expand step.
        if (positionRef.current === "center") {
          startCompose();
        } else {
          expand();
        }
      })
      .then(track);
    return () => {
      dead = true;
      uns.forEach((u) => u());
    };
  }, [confirm, expand, startCompose, win]);

  useEffect(() => {
    // A smooth scroll running at the same time as the reveal is two animations
    // moving the same content on different curves — it reads as stutter. While
    // the card is opening, jump instead; the user has not seen the old position
    // anyway.
    endRef.current?.scrollIntoView({
      behavior: revealingRef.current ? "auto" : "smooth",
      block: "end",
    });
  }, [messages, versions, versionIdx, busy]);

  const sendFollowup = useCallback(async () => {
    const el = followupRef.current;
    const text = el?.value.trim() ?? "";
    if (!text || busyRef.current) return;
    if (el) el.value = "";
    const next: ChatMessage[] = [
      ...messagesRef.current,
      { id: rid(), role: "user", content: text },
    ];
    setMessages(next);
    await runConversation(next);
  }, [runConversation]);

  // ── CENTER layout: unified follow-up + auto-grow plumbing ─────────────────
  // Send a follow-up in the CENTER surface. Unlike the side card there is no
  // discrete "expand" step — the surface is one continuously-growing thread — so
  // the first follow-up materialises the opening exchange (instruction + the
  // shown reply) into the conversation before appending the new turn.
  const runFollowup = useCallback(
    async (text: string) => {
      if (!text.trim() || busyRef.current) return;
      let base = messagesRef.current;
      if (!expandedRef.current) {
        const seed: ChatMessage[] = [];
        if (instructionRef.current) {
          seed.push({
            id: rid(),
            role: "user",
            content: instructionRef.current,
          });
        }
        const reply = versionsRef.current[versionIdxRef.current];
        if (reply?.text) {
          seed.push({
            id: rid(),
            role: "assistant",
            content: reply.text,
            sources: reply.sources,
            notFound: reply.not_found,
            confirmDelete: reply.confirm_delete,
          });
        }
        base = seed;
        setMessages(seed);
        setExpanded(true);
      }
      const next: ChatMessage[] = [
        ...base,
        { id: rid(), role: "user", content: text },
      ];
      setMessages(next);
      await runConversation(next);
      // runConversation returns focus to the SIDE input; the center textarea
      // is a different element, so return focus to it here.
      centerInputRef.current?.focus();
    },
    [runConversation],
  );

  /** Report the card's natural height to the backend so the window hugs it. The
   * CSS max-height caps the measurement, so the window stops growing and the
   * thread scrolls internally past that point. `offsetHeight` (not the bounding
   * rect) so the reveal animation's transform never leaks into the size the
   * window is driven to. */
  const reportHeight = useCallback(() => {
    const el = cardRef.current;
    if (!el) return;
    const h = el.offsetHeight;
    if (h > 0 && Math.abs(h - lastReportedH.current) >= 2) {
      lastReportedH.current = h;
      void commands.agentResizePanel(h).catch(() => {});
    }
  }, []);

  /** Grow the follow-up textarea with its content (capped, then it scrolls). */
  const autoGrowTextarea = useCallback(() => {
    const el = centerInputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 132)}px`;
  }, []);

  const onCenterKey = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        const el = centerInputRef.current;
        const text = el?.value.trim() ?? "";
        if (text) {
          if (el) {
            el.value = "";
            el.style.height = "auto";
          }
          void runFollowup(text);
        } else {
          // Enter on an empty field = insert the shown answer into the app.
          confirm();
        }
      }
    },
    [runFollowup, confirm],
  );

  /** Cap the center panel's own max-height to what the window will clamp to, so
   * the internal scroll kicks in exactly when the window stops growing. */
  const centerMax = useMemo(() => {
    const avail =
      (typeof window !== "undefined" && window.screen?.availHeight) ||
      window.innerHeight ||
      800;
    return Math.max(
      220,
      Math.round(avail - CENTER_TOP_OFFSET - CENTER_BOTTOM_GAP),
    );
  }, []);

  /** Copy one specific answer (the per-reply copy affordance). Only the copied
   * reply's icon flashes — the flash is keyed by turn id, not global. */
  const copyOne = useCallback((id: string, text: string) => {
    if (!text.trim()) return;
    commands
      .agentCopy(text)
      .then(() => {
        setFlashedId(id);
        window.clearTimeout(flashTimer.current);
        flashTimer.current = window.setTimeout(() => setFlashedId(null), 1600);
      })
      .catch(() => {});
  }, []);

  // CENTER: drive the window height from the card's content. A ResizeObserver
  // catches every source of growth (reply lands, follow-up added, textarea
  // grows, quote expands) and reports the new height on the next frame.
  useEffect(() => {
    if (position !== "center") return;
    const el = cardRef.current;
    if (!el) return;
    let raf = 0;
    const measure = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(reportHeight);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [position, reportHeight]);

  // CENTER: "just start typing" opens the follow-up field, seeded with the key —
  // the same type-to-expand feel as the summon pill. Only while the panel is
  // focused (global keydown), not composing, and idle.
  useEffect(() => {
    if (position !== "center") return;
    const onKey = (e: KeyboardEvent) => {
      if (composingRef.current || busyRef.current) return;
      if (e.metaKey || e.ctrlKey || e.altKey || e.key.length !== 1) return;
      const target = e.target as HTMLElement | null;
      if (
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLInputElement
      )
        return;
      e.preventDefault();
      const ch = e.key;
      startCompose();
      requestAnimationFrame(() => {
        const el = centerInputRef.current;
        if (!el) return;
        el.value = ch;
        el.style.height = "auto";
        el.style.height = `${Math.min(el.scrollHeight, 132)}px`;
      });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [position, startCompose]);

  /** Open the notebook — on a specific note (source chip) or unfocused (the
   * not-found escape hatch). Brings Grain forward with the Notes tab selected;
   * there is no separate notes window to summon any more. */
  const openNote = useCallback((noteId: string | null) => {
    void commands.grainSpaceRevealNote(noteId).catch(() => {});
  }, []);

  // Resolution of each `forget` confirmation, keyed by note id — a forget for a
  // given memory surfaces at most once per conversation.
  const [deleteResolved, setDeleteResolved] = useState<
    Record<string, "deleted" | "cancelled">
  >({});
  const confirmForget = useCallback((noteId: string) => {
    void commands
      .grainSpaceDeleteNote(noteId)
      .then(() => setDeleteResolved((p) => ({ ...p, [noteId]: "deleted" })))
      .catch(() => {});
  }, []);
  const cancelForget = useCallback((noteId: string) => {
    setDeleteResolved((p) => ({ ...p, [noteId]: "cancelled" }));
  }, []);

  /** The in-panel delete confirmation for a `forget` turn (RECALL-PLAN §7.2):
   * an explicit Delete / Keep choice — deletion never happens without a click. */
  const renderConfirmDelete = (src: AgentSource) => {
    const title = src.title.trim() || t("agent.untitledNote");
    const state = deleteResolved[src.note_id];
    if (state === "cancelled") return null;
    if (state === "deleted") {
      return (
        <div className="agc-evidence">
          <span className="agc-forget-done">
            {t("agent.forgetDone", { title })}
          </span>
        </div>
      );
    }
    return (
      <div className="agc-evidence agc-confirm-delete">
        <span className="agc-confirm-q">
          {t("agent.forgetConfirm", { title })}
        </span>
        <div className="agc-confirm-actions">
          <button
            type="button"
            className="agc-forget-btn"
            onClick={() => confirmForget(src.note_id)}
          >
            {t("agent.forgetDelete")}
          </button>
          <button
            type="button"
            className="agc-cancel-btn"
            onClick={() => cancelForget(src.note_id)}
          >
            {t("agent.forgetCancel")}
          </button>
        </div>
      </div>
    );
  };

  /** The Grain Recall evidence strip under an answer: source chips (click →
   * overlay focus) or the not-found escape-hatch button. Renders nothing for
   * Assist replies (empty sources, not_found = false). RECALL-PLAN §6. */
  const renderEvidence = (sources: AgentSource[], notFound: boolean) => {
    if (notFound) {
      return (
        <div className="agc-evidence">
          <button
            type="button"
            className="agc-notfound-btn"
            onClick={() => openNote(null)}
          >
            {t("agent.notFoundOpen")}
          </button>
        </div>
      );
    }
    if (sources.length === 0) return null;
    return (
      <div className="agc-evidence">
        <div className="agc-sources">
          {sources.map((s) => (
            <button
              key={s.note_id}
              type="button"
              className="agc-source"
              title={`${s.title.trim() || t("agent.untitledNote")} · ${relDate(s.saved_at)}`}
              onClick={() => openNote(s.note_id)}
            >
              {s.title.trim() || t("agent.untitledNote")}
            </button>
          ))}
        </div>
      </div>
    );
  };

  const quoteText = contextRef.current?.trim() || instructionRef.current;
  const shortcutParts = followupShortcut
    ? followupShortcut.split("+").map(keycapLabel)
    : [];
  const canConfirm = !busy && displayedReply.trim().length > 0;
  // Held back until the window is actually being shown — see `reveal`.
  const rootClass = `agent-panel-root${revealed ? " is-revealed" : ""}`;

  // ══ CENTER: the sleek center-top panel ════════════════════════════════════
  // One continuously-growing surface (no compact/expanded split). Before the
  // first follow-up the thread is just the shown answer; a follow-up
  // materialises the exchange into a threaded conversation.
  if (position === "center") {
    const thread: ChatMessage[] = expanded
      ? messages
      : !busy && displayedReply
        ? [
            {
              id: "a0",
              role: "assistant",
              content: displayedReply,
              sources: compactSources,
              notFound: compactNotFound,
              confirmDelete: compactConfirmDelete,
            },
          ]
        : [];
    const hasAnswer = thread.some((m) => m.role === "assistant");
    // Redo re-runs the first instruction only (there are no versions once the
    // conversation branches), so it lives under the sole pre-follow-up answer.
    const canRedo = !expanded && !busy && versions.length > 0;
    // The shortcut reads as one unit, e.g. "Alt Q" — never split into chips.
    const shortcutLabel = shortcutParts.join(" ");
    const lastIdx = thread.length - 1;

    return (
      <div className={rootClass}>
        <div
          ref={cardRef}
          className="agc-card agc-center"
          style={{ maxHeight: centerMax }}
        >
          {/* Top edge: a fade to black so content dissolves under the rim. */}
          <div className="agc-c-fade" aria-hidden="true" />
          <button
            type="button"
            className="agc-c-x"
            title={t("agent.escCue")}
            onClick={() => void win.close()}
          >
            <X size={12} />
          </button>

          {/* Content — grows with the conversation, scrolls past the max. */}
          <div className="agc-c-scroll">
            {quoteText && !expanded && (
              <div
                className={`agc-c-prompt ${quoteOpen ? "is-open" : ""}`}
                onClick={() => setQuoteOpen((v) => !v)}
                role="button"
                tabIndex={-1}
              >
                {quoteText}
              </div>
            )}

            {error ? (
              <div className="agc-error">{error}</div>
            ) : thread.length === 0 && busy ? (
              <div className="agent-typing" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
            ) : (
              thread.map((m, idx) => (
                <div key={m.id} className={`agc-c-turn is-${m.role}`}>
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      <div className="agc-c-answer">
                        <AgentMarkdown markdown={m.content} />
                      </div>
                      {renderEvidence(m.sources ?? [], m.notFound ?? false)}
                      {m.confirmDelete && renderConfirmDelete(m.confirmDelete)}
                      {expanded && (
                        <div className="agc-c-tools">
                          <button
                            type="button"
                            className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                            onClick={() => copyOne(m.id, m.content)}
                            title={t("agent.copyReply")}
                          >
                            {flashedId === m.id ? (
                              <Check size={13} />
                            ) : (
                              <Copy size={13} />
                            )}
                          </button>
                          {canRedo && idx === lastIdx && (
                            <button
                              type="button"
                              className="agc-c-tool"
                              onClick={retry}
                              title={t("agent.retry")}
                            >
                              <RotateCcw size={13} />
                            </button>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              ))
            )}

            {thread.length > 0 && busy && (
              <div className="agent-typing" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
            )}
            <div ref={endRef} />
          </div>

          {/* Bottom: JUST the text field once composing; otherwise the quiet
              follow-up + shortcut (left) and Confirm on the first reply (right). */}
          {composing ? (
            <div className="agc-c-compose">
              <textarea
                ref={centerInputRef}
                rows={1}
                className="agc-c-textarea"
                disabled={busy}
                placeholder={
                  busy
                    ? t("agent.followupWaiting")
                    : t("agent.followupPlaceholder")
                }
                onInput={autoGrowTextarea}
                onKeyDown={onCenterKey}
              />
            </div>
          ) : hasAnswer ? (
            <div className="agc-c-bar">
              <button
                type="button"
                className="agc-c-followup"
                disabled={busy}
                onClick={startCompose}
              >
                {t("agent.askFollowup")}
                {shortcutLabel && (
                  <span className="agc-c-kbd">{shortcutLabel}</span>
                )}
              </button>
              <span className="agc-spacer" />
              {!expanded && (
                <>
                  <button
                    type="button"
                    className={`agc-c-tool agc-c-bar-tool ${flashedId === "a0" ? "is-flash" : ""}`}
                    onClick={() => copyOne("a0", displayedReply)}
                    title={t("agent.copyReply")}
                  >
                    {flashedId === "a0" ? (
                      <Check size={13} />
                    ) : (
                      <Copy size={13} />
                    )}
                  </button>
                  {canRedo && (
                    <button
                      type="button"
                      className="agc-c-tool agc-c-bar-tool"
                      onClick={retry}
                      title={t("agent.retry")}
                    >
                      <RotateCcw size={13} />
                    </button>
                  )}
                  <button
                    type="button"
                    className="agc-c-insert"
                    disabled={!canConfirm}
                    onClick={confirm}
                    title={t("agent.confirmHint")}
                  >
                    {t("agent.confirm")}
                  </button>
                </>
              )}
            </div>
          ) : null}
        </div>
      </div>
    );
  }

  // ── COMPACT: the reference reply card ─────────────────────────────────────
  if (!expanded) {
    return (
      <div className={rootClass}>
        <div className="agc-card" ref={cardRef}>
          {/* Header: version pager (left) · close (right). Draggable. */}
          <div className="agc-head" data-tauri-drag-region>
            <div className="agc-pager">
              <button
                type="button"
                className="agc-pager-btn"
                disabled={busy || versionIdx <= 0}
                onClick={() => setVersionIdx((i) => Math.max(0, i - 1))}
                title={t("agent.prevVersion")}
              >
                <ChevronLeft size={14} />
              </button>
              <span className="agc-pager-count">
                {Math.min(versionIdx + 1, Math.max(versions.length, 1))}/
                {Math.max(versions.length, 1)}
              </span>
              <button
                type="button"
                className="agc-pager-btn"
                disabled={busy || versionIdx >= versions.length - 1}
                onClick={() =>
                  setVersionIdx((i) => Math.min(versions.length - 1, i + 1))
                }
                title={t("agent.nextVersion")}
              >
                <ChevronRight size={14} />
              </button>
            </div>
            <span className="agc-spacer" />
            <button
              type="button"
              className="agc-close"
              title={t("agent.escCue")}
              onClick={() => void win.close()}
            >
              <X size={15} />
            </button>
          </div>

          {/* The captured text (selection, else the instruction). */}
          {quoteText && (
            <div
              className={`agc-quote ${quoteOpen ? "is-open" : ""}`}
              onClick={() => setQuoteOpen((v) => !v)}
              role="button"
              tabIndex={-1}
            >
              <span className="agc-quote-text">{quoteText}</span>
              {!quoteOpen && quoteText.length > 120 && (
                <span className="agc-quote-more">…{t("agent.more")}</span>
              )}
            </div>
          )}

          {/* Reply */}
          <div className="agc-body">
            {busy ? (
              <div className="agent-typing" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
            ) : error ? (
              <div className="agc-error">{error}</div>
            ) : (
              <>
                <div className="agc-reply">
                  <AgentMarkdown markdown={displayedReply} />
                </div>
                {renderEvidence(compactSources, compactNotFound)}
                {compactConfirmDelete &&
                  renderConfirmDelete(compactConfirmDelete)}
              </>
            )}
            <div ref={endRef} />
          </div>

          {/* Bottom bar: Ask follow up + keycaps · copy · retry · Confirm ⏎ */}
          <div className="agc-foot">
            <button
              type="button"
              className="agc-followup-btn"
              disabled={busy || versions.length === 0}
              onClick={expand}
            >
              {t("agent.askFollowup")}
              {shortcutParts.map((p, i) => (
                <span key={i} className="agc-keycap">
                  {p}
                </span>
              ))}
            </button>
            <span className="agc-spacer" />
            <button
              type="button"
              className={`agc-icon-btn ${copyFlash ? "is-flash" : ""}`}
              disabled={!canConfirm}
              onClick={copyReply}
              title={t("agent.copyReply")}
            >
              {copyFlash ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              type="button"
              className="agc-icon-btn"
              disabled={busy || versions.length === 0}
              onClick={retry}
              title={t("agent.retry")}
            >
              <RotateCcw size={14} />
            </button>
            <button
              type="button"
              className="agc-confirm"
              disabled={!canConfirm}
              onClick={confirm}
              title={t("agent.confirmHint")}
            >
              {t("agent.confirm")}
              <span className="agc-confirm-glyph">{ENTER_GLYPH}</span>
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── EXPANDED: the conversation ─────────────────────────────────────────────
  return (
    <div className={rootClass}>
      <div className="agc-card agc-card--expanded" ref={cardRef}>
        {/* Header (draggable): a quiet wordmark balances the close button. */}
        <div className="agc-head agc-head--expanded" data-tauri-drag-region>
          <span className="agc-title">{t("agent.brand")}</span>
          <span className="agc-spacer" />
          <button
            type="button"
            className="agc-close"
            title={t("agent.escCue")}
            onClick={() => void win.close()}
          >
            <X size={15} />
          </button>
        </div>

        {/* Conversation — the user's turns are bubbles, answers are prose. */}
        <div className="agc-scroll">
          {messages.map((m) => (
            <div key={m.id} className="agc-turn">
              <div className={`agc-turn-body agc-turn-body--${m.role}`}>
                {m.role === "assistant" ? (
                  <AgentMarkdown markdown={m.content} />
                ) : (
                  m.content
                )}
              </div>
              {m.role === "assistant" &&
                renderEvidence(m.sources ?? [], m.notFound ?? false)}
              {m.role === "assistant" &&
                m.confirmDelete &&
                renderConfirmDelete(m.confirmDelete)}
            </div>
          ))}

          {busy && (
            <div className="agc-turn">
              <div className="agent-typing" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
            </div>
          )}

          {error && <div className="agc-error">{error}</div>}
          <div ref={endRef} />
        </div>

        {/* Follow-up input */}
        <div className={`agc-input ${busy ? "is-busy" : ""}`}>
          <input
            ref={followupRef}
            type="text"
            className="agc-input-field"
            disabled={busy}
            placeholder={
              busy ? t("agent.followupWaiting") : t("agent.followupPlaceholder")
            }
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void sendFollowup();
              }
            }}
          />
          <button
            type="button"
            className="agc-send"
            disabled={busy}
            title={t("agent.followupPlaceholder")}
            onClick={() => void sendFollowup()}
          >
            {SEND_ARROW}
          </button>
        </div>

        {/* Bottom bar: copy · Confirm (pastes the latest reply) */}
        <div className="agc-foot agc-foot--expanded">
          <span className="agc-cue">{t("agent.escCue")}</span>
          <span className="agc-spacer" />
          <button
            type="button"
            className={`agc-icon-btn ${copyFlash ? "is-flash" : ""}`}
            disabled={!canConfirm}
            onClick={copyReply}
            title={t("agent.copyReply")}
          >
            {copyFlash ? <Check size={14} /> : <Copy size={14} />}
          </button>
          <button
            type="button"
            className="agc-confirm"
            disabled={!canConfirm}
            onClick={confirm}
            title={t("agent.confirmHint")}
          >
            {t("agent.confirm")}
            <span className="agc-confirm-glyph">{ENTER_GLYPH}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
