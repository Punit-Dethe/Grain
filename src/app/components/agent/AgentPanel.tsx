import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowUp, AudioLines, Copy, RotateCcw, X, Check } from "lucide-react";
import {
  commands,
  type AgentMessage,
  type AgentConfirm,
  type AgentReply,
} from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";
import { CaptureWaveform } from "../../overlay/CaptureWaveform";
import { AgentWorkingLabel } from "./AgentWorkingLabel";
import "./agent.css";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  // A host-held extension/MCP call. Only the token displayed here can resume it.
  confirmAction?: AgentConfirm | null;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;
const ENTER_GLYPH = "⏎";

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
  const [versions, setVersions] = useState<AgentReply[]>([]);
  const [versionIdx, setVersionIdx] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flashedId, setFlashedId] = useState<string | null>(null);
  const [quoteOpen, setQuoteOpen] = useState(false);
  const [revealed, setRevealed] = useState(false);
  const revealedRef = useRef(false);
  const [capture, setCapture] = useState<{
    generation: number;
    working: boolean;
  } | null>(null);
  const generationRef = useRef<number | null>(null);
  const [followupShortcut, setFollowupShortcut] = useState("");
  const [composing, setComposing] = useState(false);
  const composingRef = useRef(false);
  composingRef.current = composing;

  const contextRef = useRef<string | null>(null);
  const instructionRef = useRef<string>("");
  const messagesRef = useRef<ChatMessage[]>([]);
  const versionsRef = useRef<AgentReply[]>([]);
  const versionIdxRef = useRef(0);
  const expandedRef = useRef(false);
  const busyRef = useRef(false);
  const centerInputRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | undefined>(undefined);
  // Guards the first LLM run so the mount-take and the `agent-instruction`
  // event (whichever wins the race) only trigger it once.
  const firstRunStartedRef = useRef(false);
  const firstTurnGenerationRef = useRef<number | null>(null);
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
  const compactConfirmAction = versions[versionIdx]?.confirm_action ?? null;
  const expandedConfirmAction = [...messages]
    .reverse()
    .find((message) => message.role === "assistant")?.confirmAction;
  const displayedConfirmAction = expanded
    ? expandedConfirmAction
    : compactConfirmAction;

  const reveal = useCallback(() => {
    revealedRef.current = true;
    setRevealed(true);
  }, []);

  /** Run the FIRST instruction (or a retry of it) — compact stage. */
  const runFirst = useCallback(
    async (instruction: string) => {
      firstRunStartedRef.current = true;
      // The panel may already be busy while speech is being transcribed. Seed
      // visible state now: updating instructionRef or busy=true alone need not
      // trigger another render before the model finishes.
      setMessages([{ id: "u0", role: "user", content: instruction }]);
      setBusy(true);
      setError(null);
      try {
        const payload: AgentMessage[] = [
          { role: "user", content: instruction },
        ];
        const res = await commands.agentRun(
          payload,
          contextRef.current,
          revealedRef.current ? null : firstTurnGenerationRef.current,
        );
        if (res.status === "ok") {
          const reply = res.data;
          setVersions((prev) => {
            const next = [...prev, reply];
            setVersionIdx(next.length - 1);
            return next;
          });
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
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
        const res = await commands.agentRun(payload, contextRef.current, null);
        if (res.status === "ok") {
          const reply = res.data;
          setMessages((prev) => [
            // Any new turn after a pending action either resolved or cancelled
            // that host token. Keep historical text, but never leave a stale
            // approval button active in an older message.
            ...prev.map((message) =>
              message.confirmAction
                ? { ...message, confirmAction: null }
                : message,
            ),
            {
              id: rid(),
              role: "assistant",
              content: reply.text,
              confirmAction: reply.confirm_action,
            },
          ]);
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  /** Open the CENTER follow-up field (button click or the continuation
   * shortcut). Once open it stays open for the session. */
  const startCompose = useCallback(() => {
    setComposing(true);
    requestAnimationFrame(() => centerInputRef.current?.focus());
  }, []);

  /** Confirm: paste the displayed reply back into the source app (backend
   * closes this window, refocuses the target, and pastes). */
  const confirm = useCallback(() => {
    const pending = expandedRef.current
      ? [...messagesRef.current]
          .reverse()
          .find((message) => message.role === "assistant")?.confirmAction
      : versionsRef.current[versionIdxRef.current]?.confirm_action;
    if (pending) return;
    const text = expandedRef.current
      ? lastReplyOf(messagesRef.current)
      : (versionsRef.current[versionIdxRef.current]?.text ?? "");
    if (!text.trim() || busyRef.current) return;
    void commands.agentConfirmPaste(text).catch(() => {});
  }, []);

  const retry = useCallback(() => {
    if (
      busyRef.current ||
      expandedRef.current ||
      !instructionRef.current ||
      versionsRef.current[versionIdxRef.current]?.confirm_action
    )
      return;
    void runFirst(instructionRef.current);
  }, [runFirst]);

  /** Take the queued first instruction and run it — guarded so the mount-take
   * and the `agent-instruction` event fire it at most once. */
  const startFirstIfQueued = useCallback(async () => {
    const generation = generationRef.current;
    if (generation === null || firstRunStartedRef.current) return;
    let instruction: Awaited<ReturnType<typeof commands.agentTakeInstruction>> =
      null;
    try {
      instruction = await commands.agentTakeInstruction(generation);
    } catch {
      /* nothing queued yet */
    }
    if (instruction?.text.trim() && !firstRunStartedRef.current) {
      firstTurnGenerationRef.current = instruction.generation;
      if (instruction.is_error) {
        firstRunStartedRef.current = true;
        setError(instruction.text);
        setBusy(false);
        return;
      }
      instructionRef.current = instruction.text.trim();
      await runFirst(instructionRef.current);
    }
  }, [runFirst]);

  useEffect(() => {
    let dead = false;
    const unlisten: Array<() => void> = [];
    const track = (release: () => void) => {
      if (dead) release();
      else unlisten.push(release);
    };
    void (async () => {
      const settings = await commands.getAppSettings();
      if (!dead && settings.status === "ok") {
        setFollowupShortcut(
          settings.data.bindings.agent_followup?.current_binding ?? "",
        );
      }
    })().catch(console.error);

    let workingGeneration: number | null = null;
    void Promise.all([
      win
        .listen<number>("agent-working", ({ payload }) => {
          workingGeneration = payload;
          setCapture((current) =>
            current?.generation === payload
              ? { ...current, working: true }
              : current,
          );
        })
        .then(track),
      win
        .listen("agent-instruction", () => void startFirstIfQueued())
        .then(track),
      win.listen("agent-reveal", reveal).then(track),
      win.listen("agent-followup", startCompose).then(track),
      win
        .listen<string>("agent-panel-dictation", ({ payload }) => {
          const dictated = payload.trim();
          if (!dictated || !revealedRef.current) return;
          const append = () => {
            const el = centerInputRef.current;
            if (dead || !el) return;
            const sep = el.value && !el.value.endsWith(" ") ? " " : "";
            el.value += sep + dictated;
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 132)}px`;
            el.focus();
          };
          if (composingRef.current) append();
          else {
            startCompose();
            requestAnimationFrame(append);
          }
        })
        .then(track),
    ])
      .then(async () => {
        const [status, context] = await Promise.all([
          commands.agentNotchStatus(),
          commands.agentGetContext(),
        ]);
        if (dead) return;
        generationRef.current = status.generation;
        contextRef.current = context;
        setCapture({
          ...status,
          working: status.working || workingGeneration === status.generation,
        });
        await startFirstIfQueued();
      })
      .catch(console.error);
    return () => {
      dead = true;
      unlisten.forEach((release) => release());
    };
  }, [reveal, startCompose, startFirstIfQueued, win]);

  // Reveal only after reply/error state has committed. The host verifies that
  // this first turn still owns the waiting pill, so Stop cannot reopen a card.
  useEffect(() => {
    const generation = firstTurnGenerationRef.current;
    if (
      !busy &&
      !revealed &&
      generation !== null &&
      (versions.length > 0 || error)
    ) {
      void commands.agentPresentReply(generation).catch(console.error);
    }
  }, [busy, error, revealed, versions.length]);

  useEffect(() => {
    if (revealed && expanded) endRef.current?.scrollIntoView({ block: "end" });
  }, [messages, expanded, busy, revealed]);

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
            confirmAction: reply.confirm_action,
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
      centerInputRef.current?.focus();
    },
    [runConversation],
  );

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
        if (busyRef.current) return;
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

  // CENTER: "just start typing" opens the follow-up field, seeded with the key —
  // the same type-to-expand feel as the summon pill. Only while the panel is
  // focused (global keydown), not composing, and idle.
  useEffect(() => {
    if (!revealed) return;
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
  }, [revealed, startCompose]);

  /** Resume exactly the host-held action shown in this reply. The model is not
   * called again and cannot substitute a provider, tool, or argument. */
  const resolveAction = useCallback(
    async (pending: AgentConfirm, approve: boolean) => {
      if (busyRef.current) return;
      setBusy(true);
      setError(null);
      try {
        const response = await commands.agentConfirmAction(
          pending.token,
          approve,
        );
        if (response.status !== "ok") {
          throw new Error(response.error || t("agent.error"));
        }
        const reply = response.data;
        setVersions((current) =>
          current.map((version) =>
            version.confirm_action?.token === pending.token ? reply : version,
          ),
        );
        setMessages((current) =>
          current.map((message) =>
            message.confirmAction?.token === pending.token
              ? {
                  ...message,
                  content: reply.text,
                  confirmAction: reply.confirm_action,
                }
              : message,
          ),
        );
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  const renderConfirmAction = (pending: AgentConfirm) => (
    <div className="agc-evidence agc-confirm-action">
      <span className="agc-confirm-q">{pending.summary}</span>
      <div className="agc-confirm-actions">
        <button
          type="button"
          className="agc-action-btn"
          disabled={busy}
          onClick={() => void resolveAction(pending, true)}
        >
          {t("agent.confirm")}
        </button>
        <button
          type="button"
          className="agc-cancel-btn"
          disabled={busy}
          onClick={() => void resolveAction(pending, false)}
        >
          {t("tray.cancel")}
        </button>
      </div>
    </div>
  );

  // Selected source text is separate from the submitted message below.
  const quoteText = contextRef.current?.trim() || "";
  const shortcutParts = followupShortcut
    ? followupShortcut.split("+").map(keycapLabel)
    : [];
  const canConfirm =
    !busy && !displayedConfirmAction && displayedReply.trim().length > 0;
  const thread: ChatMessage[] = expanded
    ? messages
    : [
        ...messages,
        ...(!busy && !error && displayedReply
          ? [
              {
                id: "a0",
                role: "assistant" as const,
                content: displayedReply,
                confirmAction: compactConfirmAction,
              },
            ]
          : []),
      ];
  const hasAnswer = thread.some((m) => m.role === "assistant");
  // Redo re-runs the first instruction only (there are no versions once the
  // conversation branches), so it lives under the sole pre-follow-up answer.
  const canRedo =
    !expanded && !busy && !displayedConfirmAction && versions.length > 0;
  // The shortcut reads as one unit, e.g. "Alt Q" — never split into chips.
  const shortcutLabel = shortcutParts.join(" ");

  return (
    <div className="agent-panel-root">
      <section
        className={revealed ? "agent-notch is-reply" : "agent-notch"}
        aria-label={t("agent.brand")}
      >
        <header className="agent-notch-header">
          <AudioLines
            size={16}
            className="agent-notch-mark"
            aria-hidden="true"
          />
          <span className="agent-notch-label">
            {revealed ? (
              t("agent.brand")
            ) : capture?.working ? (
              <AgentWorkingLabel />
            ) : (
              <span role="status">{t("agent.notchListening")}</span>
            )}
          </span>
          {!revealed && !capture?.working && (
            <div className="agent-notch-wave">
              <CaptureWaveform ready session={capture?.generation ?? 0} />
            </div>
          )}
          {!revealed && capture && !capture.working && (
            <button
              type="button"
              className="agent-notch-control agent-notch-submit"
              title={t("agent.notchSubmit")}
              aria-label={t("agent.notchSubmit")}
              onClick={() =>
                void commands.agentNotchAction(capture.generation, true)
              }
            >
              <ArrowUp size={15} />
            </button>
          )}
          <button
            type="button"
            className="agent-notch-control agent-notch-close"
            title={revealed ? t("agent.notchClose") : t("agent.notchCancel")}
            aria-label={
              revealed ? t("agent.notchClose") : t("agent.notchCancel")
            }
            disabled={!revealed && !capture}
            onClick={() => {
              if (revealed) void win.close();
              else if (capture)
                void commands.agentNotchAction(capture.generation, false);
            }}
          >
            <X size={15} />
          </button>
        </header>

        {revealed && (
          <div className="agent-notch-reply">
            <div className="agc-c-scroll">
              {quoteText && !expanded && (
                <button
                  className={`agc-c-prompt ${quoteOpen ? "is-open" : ""}`}
                  onClick={() => setQuoteOpen((v) => !v)}
                  type="button"
                  aria-expanded={quoteOpen}
                >
                  {quoteText}
                </button>
              )}

              {thread.map((m) => (
                <div key={m.id} className={`agc-c-turn is-${m.role}`}>
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      <div className="agc-c-answer">
                        <AgentMarkdown markdown={m.content} />
                      </div>
                      {m.confirmAction && renderConfirmAction(m.confirmAction)}
                      {expanded && (
                        <div className="agc-c-tools">
                          <button
                            type="button"
                            className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                            onClick={() => copyOne(m.id, m.content)}
                            aria-label={t("agent.copyReply")}
                            title={t("agent.copyReply")}
                          >
                            {flashedId === m.id ? (
                              <Check size={13} />
                            ) : (
                              <Copy size={13} />
                            )}
                          </button>
                        </div>
                      )}
                    </>
                  )}
                </div>
              ))}

              {busy && (
                <div className="agent-typing" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </div>
              )}
              {error && <div className="agc-error">{error}</div>}
              <div ref={endRef} />
            </div>

            {/* Bottom: JUST the text field once composing; otherwise the quiet
              follow-up + shortcut (left) and Confirm on the first reply (right). */}
            {composing ? (
              <div className="agc-c-compose">
                {/* Enabled while busy, so users can draft a follow-up. */}
                <textarea
                  ref={centerInputRef}
                  rows={1}
                  className="agc-c-textarea"
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
                        aria-label={t("agent.retry")}
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
        )}
      </section>
    </div>
  );
}
