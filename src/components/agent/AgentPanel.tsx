import { useCallback, useEffect, useRef, useState } from "react";
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
  type AgentReply,
  type AgentSource,
} from "@/bindings";
import "./agent.css";

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
  const win = getCurrentWindow();

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
  const [quoteOpen, setQuoteOpen] = useState(false);
  const [followupShortcut, setFollowupShortcut] = useState<string>("");

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
        commands.agentCopy(reply).then(flashCopied).catch(() => {});
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
    setMessages(seed);
    setExpanded(true);
    void commands.agentSetPanelMode(true).catch(() => {});
    window.setTimeout(() => followupRef.current?.focus(), 60);
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
    commands.agentCopy(text).then(flashCopied).catch(() => {});
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
    const uns: Array<() => void> = [];
    // The core queued the first instruction after we mounted → run it.
    void win.listen("agent-instruction", () => {
      void startFirstIfQueued();
    }).then((fn) => uns.push(fn));
    // Reveal-in-loading handshake: the window was just shown; keep the loading
    // state until the first reply (or an error) lands.
    void win.listen("agent-loading", () => {
      if (!firstRunStartedRef.current && !expandedRef.current) setBusy(true);
    }).then((fn) => uns.push(fn));
    // A backend-side failure (STT/LLM) with no reply to show.
    void win.listen<string>("agent-error", (e) => {
      firstRunStartedRef.current = true;
      setBusy(false);
      setError(e.payload || t("agent.error"));
    }).then((fn) => uns.push(fn));
    // Follow-up offer opened the warm hidden panel → seed the conversation.
    void win.listen("agent-followup-open", () => {
      void openRetainedConversation();
    }).then((fn) => uns.push(fn));
    // [GRAIN] Dictation routed INTO the panel (the user used the app's STT while
    // the expanded conversation was focused). Append the transcript to the
    // follow-up field instead of it being OS-pasted (which would paste the
    // auto-copied AI reply). Handled here, not by the OS clipboard.
    void win.listen<string>("agent-panel-dictation", (e) => {
      const el = followupRef.current;
      const dictated = (e.payload || "").trim();
      if (!el || !dictated || busyRef.current) return;
      const sep = el.value && !el.value.endsWith(" ") ? " " : "";
      el.value = el.value + sep + dictated;
      el.focus();
    }).then((fn) => uns.push(fn));
    return () => uns.forEach((u) => u());
  }, [openRetainedConversation, startFirstIfQueued, t, win]);

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
    let unEnter: (() => void) | undefined;
    let unFollow: (() => void) | undefined;
    void win
      .listen("agent-global-enter", () => {
        if (!expandedRef.current) confirm();
      })
      .then((fn) => {
        unEnter = fn;
      });
    void win
      .listen("agent-followup", () => {
        expand();
      })
      .then((fn) => {
        unFollow = fn;
      });
    return () => {
      unEnter?.();
      unFollow?.();
    };
  }, [confirm, expand, win]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
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

  /** Open the memory browser — on a specific note (source chip) or unfocused
   * (the not-found escape hatch). Both go through the existing overlay command. */
  const openNote = useCallback((noteId: string | null) => {
    void commands.grainSpaceOpenWindow(noteId).catch(() => {});
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

  // ── COMPACT: the reference reply card ─────────────────────────────────────
  if (!expanded) {
    return (
      <div className="agent-panel-root">
        <div className="agc-card">
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
              <span className="agc-quote-text">“{quoteText}</span>
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
                <div className="agc-reply">{displayedReply}</div>
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
    <div className="agent-panel-root">
      <div className="agc-card agc-card--expanded">
        {/* Header (draggable) */}
        <div className="agc-head" data-tauri-drag-region>
          <span
            className={`agent-dot-status ${busy ? "is-busy" : "is-ready"}`}
          />
          <span className="agc-brand">{t("agent.brand")}</span>
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

        {/* Conversation */}
        <div className="agc-scroll">
          {messages.map((m) => (
            <div key={m.id} className="agc-turn">
              <div
                className={`agc-turn-label ${m.role === "user" ? "is-user" : "is-grain"}`}
              >
                {m.role === "user" ? t("agent.you") : t("agent.grain")}
              </div>
              <div className={`agc-turn-body agc-turn-body--${m.role}`}>
                {m.content}
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
              <div className="agc-turn-label is-grain">{t("agent.grain")}</div>
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
