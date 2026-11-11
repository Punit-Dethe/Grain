import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, RotateCcw, X, Check } from "lucide-react";
import {
  commands,
  type AgentNotchStatus,
  type AgentMessage,
  type AgentConfirm,
  type AgentReply,
} from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";
import { CaptureWaveform } from "../../overlay/CaptureWaveform";
import { AgentWorkingLabel } from "./AgentWorkingLabel";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  // A host-held extension/MCP call. Only the token displayed here can resume it.
  confirmAction?: AgentConfirm | null;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;
function resizeComposer(input: HTMLTextAreaElement) {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 76)}px`;
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

  // The first reply can be retried until the conversation has a follow-up.
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [firstReply, setFirstReply] = useState<AgentReply | null>(null);
  const [hasFollowups, setHasFollowups] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flashedId, setFlashedId] = useState<string | null>(null);
  const [quoteOpen, setQuoteOpen] = useState(false);
  const [revealed, setRevealed] = useState(false);
  // True once the shell has finished growing; the compact status (and its
  // phrase timer) stays mounted until then so it can fade out with the growth.
  const [grown, setGrown] = useState(false);
  const revealedRef = useRef(false);
  const [capture, setCapture] = useState<AgentNotchStatus | null>(null);
  const generationRef = useRef<number | null>(null);
  const contextRef = useRef<string | null>(null);
  const instructionRef = useRef<string>("");
  const messagesRef = useRef<ChatMessage[]>([]);
  const firstReplyRef = useRef<AgentReply | null>(null);
  const hasFollowupsRef = useRef(false);
  const busyRef = useRef(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | undefined>(undefined);
  // Guards the first LLM run so the mount-take and the `agent-instruction`
  // event (whichever wins the race) only trigger it once.
  const firstRunStartedRef = useRef(false);
  const firstTurnGenerationRef = useRef<number | null>(null);
  messagesRef.current = messages;
  firstReplyRef.current = firstReply;
  hasFollowupsRef.current = hasFollowups;
  busyRef.current = busy;

  const reveal = useCallback(() => {
    revealedRef.current = true;
    setRevealed(true);
  }, []);

  /** Run the FIRST instruction (or a retry of it) — compact stage. */
  const runFirst = useCallback(
    async (instruction: string) => {
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
          setFirstReply(res.data);
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

  /** Run the conversation with its existing turns. */
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

  const retry = useCallback(() => {
    if (
      busyRef.current ||
      hasFollowupsRef.current ||
      !instructionRef.current ||
      firstReplyRef.current?.confirm_action
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
      firstRunStartedRef.current = true;
      contextRef.current = await commands.agentGetContext().catch(() => null);
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
    let listeningGeneration: number | null = null;
    let workingGeneration: number | null = null;
    void Promise.all([
      win
        .listen<number>("agent-listening", ({ payload }) => {
          listeningGeneration = payload;
          setCapture((current) =>
            current?.generation === payload
              ? { ...current, listening: true }
              : current,
          );
        })
        .then(track),
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
      win
        .listen("agent-reveal", () => {
          // Give the WebView a compact paint after the native bounds change.
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (!dead) reveal();
            });
          });
        })
        .then(track),
      win
        .listen<string>("agent-panel-dictation", ({ payload }) => {
          const dictated = payload.trim();
          if (!dictated || !revealedRef.current) return;
          const input = composerRef.current;
          if (!input) return;
          const separator = input.value && !/\s$/.test(input.value) ? " " : "";
          input.value += separator + dictated;
          resizeComposer(input);
          input.focus();
        })
        .then(track),
    ])
      .then(async () => {
        const status = await commands.agentNotchStatus();
        if (dead) return;
        generationRef.current = status.generation;
        setCapture({
          ...status,
          listening:
            status.listening || listeningGeneration === status.generation,
          working: status.working || workingGeneration === status.generation,
        });
        await startFirstIfQueued();
      })
      .catch(console.error);
    return () => {
      dead = true;
      unlisten.forEach((release) => release());
    };
  }, [reveal, startFirstIfQueued, win]);

  // Reveal only after reply/error state has committed. The host verifies that
  // this first turn still owns the waiting pill, so Stop cannot reopen a card.
  useEffect(() => {
    const generation = firstTurnGenerationRef.current;
    if (!busy && !revealed && generation !== null && (firstReply || error)) {
      void commands.agentPresentReply(generation).catch(console.error);
    }
  }, [busy, error, revealed, firstReply]);

  useEffect(() => {
    if (!revealed) return;
    // Matches --notch-grow in agent.css (and NOTCH_GROW in agent.rs).
    const timer = window.setTimeout(() => setGrown(true), 460);
    return () => window.clearTimeout(timer);
  }, [revealed]);

  useEffect(() => {
    if (revealed && hasFollowups)
      endRef.current?.scrollIntoView({ block: "end" });
  }, [messages, hasFollowups, busy, revealed]);

  const runFollowup = useCallback(
    async (text: string) => {
      if (!text.trim() || busyRef.current) return;
      let base = messagesRef.current;
      if (!hasFollowupsRef.current) {
        const seed: ChatMessage[] = [];
        if (instructionRef.current) {
          seed.push({
            id: rid(),
            role: "user",
            content: instructionRef.current,
          });
        }
        const reply = firstReplyRef.current;
        if (reply?.text) {
          seed.push({
            id: rid(),
            role: "assistant",
            content: reply.text,
            confirmAction: reply.confirm_action,
          });
        }
        base = seed;
        setHasFollowups(true);
      }
      const next: ChatMessage[] = [
        ...base,
        { id: rid(), role: "user", content: text },
      ];
      setMessages(next);
      await runConversation(next);
    },
    [runConversation],
  );

  const onComposerKey = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (
        event.key !== "Enter" ||
        event.shiftKey ||
        event.nativeEvent.isComposing
      )
        return;
      event.preventDefault();
      const input = event.currentTarget;
      const text = input.value.trim();
      if (!text || busyRef.current) return;
      input.value = "";
      resizeComposer(input);
      void runFollowup(text);
    },
    [runFollowup],
  );

  /** Copy one specific answer (the per-reply copy affordance). Only the copied
   * reply's icon flashes — the flash is keyed by turn id, not global. */
  const copyOne = useCallback((id: string, text: string) => {
    if (!text.trim()) return;
    commands
      .agentCopy(text)
      .then((result) => {
        if (result.status !== "ok") return;
        setFlashedId(id);
        window.clearTimeout(flashTimer.current);
        flashTimer.current = window.setTimeout(() => setFlashedId(null), 1600);
      })
      .catch(() => {});
  }, []);

  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  useEffect(() => {
    if (!revealed) return;
    const focusComposer = () => {
      if (document.hasFocus() && document.activeElement === document.body) {
        composerRef.current?.focus();
      }
    };
    focusComposer();
    window.addEventListener("focus", focusComposer);
    return () => window.removeEventListener("focus", focusComposer);
  }, [revealed]);

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
        setFirstReply((current) =>
          current?.confirm_action?.token === pending.token ? reply : current,
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
  const thread: ChatMessage[] = hasFollowups
    ? messages
    : [
        ...messages,
        ...(!busy && firstReply?.text
          ? [
              {
                id: "a0",
                role: "assistant" as const,
                content: firstReply.text,
                confirmAction: firstReply.confirm_action,
              },
            ]
          : []),
      ];
  const canRetry = !hasFollowups && !busy && !firstReply?.confirm_action;

  return (
    <div className="agent-panel-root">
      <section
        className={revealed ? "agent-notch is-reply" : "agent-notch"}
        aria-label={t("agent.brand")}
      >
        {/* One header for both states: the compact status crossfades into the
            brand while the shell grows, so nothing pops in or out. */}
        <header className="agent-notch-header">
          <span className="agent-notch-indicator" aria-hidden="true" />
          <span className="agent-notch-label">
            {!grown && (
              <span className="agent-notch-status" aria-hidden={revealed}>
                {capture?.working ? (
                  <AgentWorkingLabel />
                ) : (
                  <span role="status">
                    {t(
                      capture?.listening
                        ? "agent.notchListening"
                        : "agent.notchStarting",
                    )}
                  </span>
                )}
              </span>
            )}
            <span className="agent-notch-brand" aria-hidden={!revealed}>
              {t("agent.brand")}
            </span>
          </span>
          {!revealed && capture?.listening && !capture.working && (
            <div className="agent-notch-wave">
              <CaptureWaveform ready session={capture?.generation ?? 0} />
            </div>
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
                void commands.agentCancelNotch(capture.generation);
            }}
          >
            <X size={14} />
          </button>
        </header>

        {revealed && (
          <div className="agent-notch-reply">
            <div className="agc-c-scroll">
              {quoteText && !hasFollowups && (
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
                      {m.content.trim() && (
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
                          {canRetry && (
                            <button
                              type="button"
                              className="agc-c-tool"
                              onClick={retry}
                              aria-label={t("agent.retry")}
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

            <div className="agc-c-compose">
              <textarea
                ref={composerRef}
                rows={1}
                className="agc-c-textarea"
                aria-label={t("agent.placeholderAsk")}
                placeholder={t("agent.placeholderAsk")}
                onInput={(event) => resizeComposer(event.currentTarget)}
                onKeyDown={onComposerKey}
              />
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
