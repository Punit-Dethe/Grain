import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type AgentMessage } from "@/bindings";
import "./agent.css";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;
// Glyph constants (kept out of JSX so the i18n lint doesn't treat them as copy).
const CLOSE_X = "×";
const SEND_ARROW = "↵";
const SEP = "·";

/**
 * [GRAIN] The Agent panel — the right-side conversation.
 *
 * Seeded with the instruction the palette handed off, it runs the request, shows
 * the reply (auto-copied), and offers a follow-up input. The selection captured
 * at summon is the LLM context (never shown verbatim). Esc closes (destroys) it.
 */
export function AgentPanel() {
  const { t } = useTranslation();
  const win = getCurrentWindow();

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copyFlash, setCopyFlash] = useState(false);

  const contextRef = useRef<string | null>(null);
  const messagesRef = useRef<ChatMessage[]>([]);
  const followupRef = useRef<HTMLInputElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const hasAutoCopied = useRef(false);
  const flashTimer = useRef<number | undefined>(undefined);
  const startedRef = useRef(false);
  messagesRef.current = messages;

  const lastReply =
    [...messages].reverse().find((m) => m.role === "assistant")?.content ?? "";

  const flashCopied = useCallback(() => {
    setCopyFlash(true);
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setCopyFlash(false), 1600);
  }, []);

  const run = useCallback(
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
            { id: rid(), role: "assistant", content: reply },
          ]);
          if (!hasAutoCopied.current && reply.trim()) {
            hasAutoCopied.current = true;
            commands.agentCopy(reply).then(flashCopied).catch(() => {});
          }
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
    [flashCopied, t],
  );

  // Mount: pull context + the first instruction the palette handed off, run it.
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    (async () => {
      try {
        contextRef.current = await commands.agentGetContext();
      } catch {
        /* no context is fine */
      }
      let instruction: string | null = null;
      try {
        instruction = await commands.agentTakeInstruction();
      } catch {
        /* nothing queued */
      }
      if (instruction && instruction.trim()) {
        const seed: ChatMessage[] = [
          { id: rid(), role: "user", content: instruction.trim() },
        ];
        setMessages(seed);
        await run(seed);
      }
    })();
  }, [run]);

  // Esc closes — global so it works even when the field isn't focused.
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

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, busy]);

  const sendFollowup = useCallback(async () => {
    const el = followupRef.current;
    const text = el?.value.trim() ?? "";
    if (!text || busy) return;
    if (el) el.value = "";
    const next: ChatMessage[] = [
      ...messagesRef.current,
      { id: rid(), role: "user", content: text },
    ];
    setMessages(next);
    await run(next);
  }, [busy, run]);

  const copyReply = useCallback(() => {
    if (!lastReply) return;
    commands.agentCopy(lastReply).then(flashCopied).catch(() => {});
  }, [lastReply, flashCopied]);

  return (
    <div className="agent-panel-root">
      <div className="agent-card agent-card--panel">
        {/* Header (draggable) */}
        <div className="agent-pan-head" data-tauri-drag-region>
          <span className={`agent-dot-status ${busy ? "is-busy" : "is-ready"}`} />
          <span className="agent-brand">{t("agent.brand")}</span>
          {busy && (
            <span className="agent-pan-processing">
              {SEP} {t("agent.processing")}
            </span>
          )}
          <span className="agent-spacer" />
          <button
            type="button"
            className="agent-close"
            title={t("agent.escCue")}
            onClick={() => void win.close()}
          >
            {CLOSE_X}
          </button>
        </div>

        <div className="agent-divider" />

        {/* Conversation */}
        <div className="agent-pan-scroll">
          {messages.map((m) => (
            <div key={m.id} className="agent-turn">
              <div
                className={`agent-turn-label ${m.role === "user" ? "is-user" : "is-grain"}`}
              >
                {m.role === "user" ? t("agent.you") : t("agent.grain")}
              </div>
              <div className={`agent-turn-body agent-turn-body--${m.role}`}>
                {m.content}
              </div>
            </div>
          ))}

          {busy && (
            <div className="agent-turn">
              <div className="agent-turn-label is-grain">{t("agent.grain")}</div>
              <div className="agent-typing" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
            </div>
          )}

          {error && <div className="agent-error">{error}</div>}
          <div ref={endRef} />
        </div>

        {/* Copy row (only once there's a reply and no error) */}
        {!error && lastReply && (
          <>
            <div className="agent-divider" />
            <div className="agent-copyrow">
              <button
                type="button"
                className={`agent-copybtn ${copyFlash ? "is-flash" : ""}`}
                onClick={copyReply}
              >
                {copyFlash ? t("agent.copied") : t("agent.copyReply")}
              </button>
              {copyFlash && (
                <span className="agent-copyhint">{t("agent.autoCopied")}</span>
              )}
              <span className="agent-spacer" />
              <span className="agent-cue">{t("agent.escCue")}</span>
            </div>
          </>
        )}

        {/* Follow-up input */}
        <div className={`agent-followup ${busy ? "is-busy" : ""}`}>
          <input
            ref={followupRef}
            type="text"
            className="agent-followup-field"
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
            className="agent-followup-send"
            disabled={busy}
            title={t("agent.followupPlaceholder")}
            onClick={() => void sendFollowup()}
          >
            {SEND_ARROW}
          </button>
        </div>
      </div>
    </div>
  );
}
