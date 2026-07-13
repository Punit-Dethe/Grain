import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUp, MessageSquare, Plus, Sparkles } from "lucide-react";
import { commands, type AgentMessage, type AgentSource } from "@/bindings";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  /** Grain Recall evidence footer (RECALL-PLAN §6): empty for a plain answer. */
  sources?: AgentSource[];
  notFound?: boolean;
  /** A `forget` turn hands us the memory to confirm before deletion (§7.2). */
  confirmDelete?: AgentSource | null;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;

/**
 * [GRAIN] Chat rail — Grain Recall, in-window. This is the SAME conversational
 * memory brain the voice Recall pill drives (`recall.rs` → `run_turn`): hybrid
 * retrieve over the user's saved notes, a memories block with stable `[Mn]`
 * ids, a bounded `search_memory` tool loop, and an answer with evidence /
 * conversational writes (remember · update · complete · forget). Here it is fed
 * by typed messages instead of the summon panel, via `grainSpaceRecallTurn`.
 *
 * The chat owns its own thread; each turn sends the whole history (the shape the
 * backend expects) and `grainSpaceRecallReset` clears the shared M-id registry
 * at the start of a fresh conversation. Source chips select the cited note in
 * this same window (`onOpenNote`) rather than opening a new one.
 *
 * The clip shell animates width 0 ↔ 320px so it slides; the inner pane is
 * fixed-width so its content never reflows mid-slide.
 */
export function ChatRail({
  open,
  onOpenNote,
}: {
  open: boolean;
  onOpenNote: (noteId: string) => void;
}) {
  const { t } = useTranslation();

  const [tab, setTab] = useState<"chat" | "headsUp">("chat");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleteResolved, setDeleteResolved] = useState<
    Record<string, "deleted" | "cancelled">
  >({});

  const messagesRef = useRef<ChatMessage[]>([]);
  const busyRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  messagesRef.current = messages;
  busyRef.current = busy;

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, busy]);

  // Focus the field when the rail slides open.
  useEffect(() => {
    if (open && tab === "chat") {
      window.setTimeout(() => inputRef.current?.focus(), 260);
    }
  }, [open, tab]);

  /** Run one Recall turn for `text`, appending the answer (or an error). */
  const send = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || busyRef.current) return;

      // A fresh conversation starts with a clean M-id registry (mirrors the
      // reset the voice pill does on each summon).
      if (messagesRef.current.length === 0) {
        await commands.grainSpaceRecallReset().catch(() => {});
      }

      const next: ChatMessage[] = [
        ...messagesRef.current,
        { id: rid(), role: "user", content: trimmed },
      ];
      setMessages(next);
      setInput("");
      setBusy(true);
      setError(null);

      const payload: AgentMessage[] = next.map((m) => ({
        role: m.role,
        content: m.content,
      }));
      try {
        const res = await commands.grainSpaceRecallTurn(payload);
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
        } else {
          setError(res.error || t("grainSpaceOverlay.chatError"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("grainSpaceOverlay.chatError"));
      } finally {
        setBusy(false);
        inputRef.current?.focus();
      }
    },
    [t],
  );

  /** Clear the thread and the shared M-id registry — a fresh conversation. */
  const newChat = useCallback(() => {
    if (busyRef.current) return;
    void commands.grainSpaceRecallReset().catch(() => {});
    setMessages([]);
    setError(null);
    setDeleteResolved({});
    setInput("");
    inputRef.current?.focus();
  }, []);

  const confirmForget = useCallback((noteId: string) => {
    void commands
      .grainSpaceDeleteNote(noteId)
      .then(() => setDeleteResolved((p) => ({ ...p, [noteId]: "deleted" })))
      .catch(() => {});
  }, []);
  const cancelForget = useCallback((noteId: string) => {
    setDeleteResolved((p) => ({ ...p, [noteId]: "cancelled" }));
  }, []);

  /** In-panel delete confirmation for a `forget` turn (RECALL-PLAN §7.2). */
  const renderConfirmDelete = (src: AgentSource) => {
    const title = src.title.trim() || t("grainSpaceOverlay.untitled");
    const state = deleteResolved[src.note_id];
    if (state === "cancelled") return null;
    if (state === "deleted") {
      return (
        <div className="gs-chat-evidence">
          <span className="gs-chat-forget-done">
            {t("grainSpaceOverlay.chatForgetDone", { title })}
          </span>
        </div>
      );
    }
    return (
      <div className="gs-chat-evidence gs-chat-confirm">
        <span className="gs-chat-confirm-q">
          {t("grainSpaceOverlay.chatForgetConfirm", { title })}
        </span>
        <div className="gs-chat-confirm-actions">
          <button
            type="button"
            className="gs-chat-forget-btn"
            onClick={() => confirmForget(src.note_id)}
          >
            {t("grainSpaceOverlay.chatForgetDelete")}
          </button>
          <button
            type="button"
            className="gs-chat-keep-btn"
            onClick={() => cancelForget(src.note_id)}
          >
            {t("grainSpaceOverlay.chatForgetKeep")}
          </button>
        </div>
      </div>
    );
  };

  /** Source chips (click → select the cited note here) or the not-found hint. */
  const renderEvidence = (sources: AgentSource[], notFound: boolean) => {
    if (notFound) {
      return (
        <div className="gs-chat-evidence">
          <span className="gs-chat-notfound">
            {t("grainSpaceOverlay.chatNotFound")}
          </span>
        </div>
      );
    }
    if (sources.length === 0) return null;
    return (
      <div className="gs-chat-evidence gs-chat-sources">
        {sources.map((s) => (
          <button
            key={s.note_id}
            type="button"
            className="gs-chat-source"
            title={s.title.trim() || t("grainSpaceOverlay.untitled")}
            onClick={() => onOpenNote(s.note_id)}
          >
            {s.title.trim() || t("grainSpaceOverlay.untitled")}
          </button>
        ))}
      </div>
    );
  };

  const empty = messages.length === 0 && !busy && !error;

  return (
    <div
      className={`gs-chat-clip${open ? " gs-chat-clip--open" : ""}`}
      aria-hidden={!open}
    >
      <div className="gs-chat">
        <div className="gs-chat-tabs">
          <button
            type="button"
            className={`gs-chat-tab${tab === "headsUp" ? " gs-chat-tab--on" : ""}`}
            onClick={() => setTab("headsUp")}
          >
            <Sparkles width={13} height={13} />
            {t("grainSpaceOverlay.headsUp")}
          </button>
          <button
            type="button"
            className={`gs-chat-tab${tab === "chat" ? " gs-chat-tab--on" : ""}`}
            onClick={() => setTab("chat")}
          >
            <MessageSquare width={13} height={13} />
            {t("grainSpaceOverlay.chat")}
          </button>
          <span className="gs-chat-tabs-spacer" />
          {tab === "chat" && messages.length > 0 && (
            <button
              type="button"
              className="gs-chat-new"
              title={t("grainSpaceOverlay.chatNewChat")}
              onClick={newChat}
            >
              <Plus width={15} height={15} />
            </button>
          )}
        </div>

        {tab === "headsUp" ? (
          <div className="gs-chat-body gs-chat-body--center">
            <div className="gs-chat-welcome">
              {t("grainSpaceOverlay.headsUpTitle")}
            </div>
            <div className="gs-chat-card gs-chat-card--static">
              <div className="gs-chat-card-sub">
                {t("grainSpaceOverlay.headsUpSub")}
              </div>
            </div>
          </div>
        ) : empty ? (
          <div className="gs-chat-body gs-chat-body--center">
            <div className="gs-chat-welcome">
              {t("grainSpaceOverlay.chatWelcome")}
            </div>
            <button
              type="button"
              className="gs-chat-card"
              onClick={() => void send(t("grainSpaceOverlay.chatCard1Prompt"))}
            >
              <div className="gs-chat-card-title">
                {t("grainSpaceOverlay.chatCard1Title")}
              </div>
              <div className="gs-chat-card-sub">
                {t("grainSpaceOverlay.chatCard1Prompt")}
              </div>
            </button>
            <button
              type="button"
              className="gs-chat-card"
              onClick={() => void send(t("grainSpaceOverlay.chatCard2Prompt"))}
            >
              <div className="gs-chat-card-title">
                {t("grainSpaceOverlay.chatCard2Title")}
              </div>
              <div className="gs-chat-card-sub">
                {t("grainSpaceOverlay.chatCard2Prompt")}
              </div>
            </button>
          </div>
        ) : (
          <div className="gs-chat-scroll">
            {messages.map((m) => (
              <div key={m.id} className="gs-chat-turn">
                <div
                  className={`gs-chat-turn-label${m.role === "user" ? " is-user" : " is-grain"}`}
                >
                  {m.role === "user"
                    ? t("grainSpaceOverlay.chatYou")
                    : t("grainSpaceOverlay.chatGrain")}
                </div>
                <div className={`gs-chat-bubble gs-chat-bubble--${m.role}`}>
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
              <div className="gs-chat-turn">
                <div className="gs-chat-turn-label is-grain">
                  {t("grainSpaceOverlay.chatGrain")}
                </div>
                <div className="gs-chat-typing" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </div>
              </div>
            )}
            {error && <div className="gs-chat-error">{error}</div>}
            <div ref={endRef} />
          </div>
        )}

        {tab === "chat" && (
          <div className="gs-chat-foot">
            <div className="gs-chat-input">
              <input
                ref={inputRef}
                value={input}
                disabled={busy}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void send(input);
                  }
                }}
                placeholder={t("grainSpaceOverlay.chatPlaceholder")}
                spellCheck={false}
              />
              <button
                type="button"
                className="gs-chat-send"
                disabled={busy || !input.trim()}
                title={t("grainSpaceOverlay.chatPlaceholder")}
                onClick={() => void send(input)}
              >
                <ArrowUp width={13} height={13} />
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
