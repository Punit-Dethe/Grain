import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUp, Sparkles } from "lucide-react";
import type { AskTurn, MeetingSummary } from "@/bindings";
import { Markdown } from "@/components/markdown/Markdown";
import "@/components/markdown/markdown.css";
import { AiRequired, errorMessage } from "./SummaryTab";
import { useMeetAiAvailable, type MeetController } from "./useMeetController";

const SUGGESTIONS = ["decisions", "actions", "wants"] as const;

/** Ask questions about one meeting; answers come from its transcript only.
 * The conversation lives with the open meeting and is not saved. */
export function AskTab({
  meet,
  meeting,
}: {
  meet: MeetController;
  meeting: MeetingSummary;
}) {
  const { t } = useTranslation();
  const ai = useMeetAiAvailable();
  const [messages, setMessages] = useState<AskTurn[]>([]);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const { ask } = meet;

  useEffect(() => {
    const reduce = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    endRef.current?.scrollIntoView({
      block: "end",
      behavior: reduce ? "auto" : "smooth",
    });
  }, [messages, pending]);

  const send = async (text: string) => {
    const question = text.trim();
    if (!question || pending) return;
    const history = messages;
    setMessages([...history, { role: "user", content: question }]);
    setDraft("");
    setPending(true);
    setError(null);
    try {
      const answer = await ask(meeting.id, question, history);
      setMessages((current) => [
        ...current,
        { role: "assistant", content: answer },
      ]);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    void send(draft);
  };

  if (ai === null) return null;
  if (ai === false) return <AiRequired />;

  return (
    <section className="meet-ask" aria-label={t("ui2.meet.tabs.ask")}>
      {messages.length === 0 ? (
        <div className="meet-ask-intro">
          <span className="meet-pane-icon" aria-hidden="true">
            <Sparkles size={22} strokeWidth={1.6} />
          </span>
          <h2>{t("ui2.meet.ask.title")}</h2>
          <p>{t("ui2.meet.ask.body")}</p>
          <ul className="meet-ask-suggestions">
            {SUGGESTIONS.map((key) => (
              <li key={key}>
                <button
                  type="button"
                  disabled={pending}
                  onClick={() =>
                    void send(t(`ui2.meet.ask.suggestions.${key}`))
                  }
                >
                  {t(`ui2.meet.ask.suggestions.${key}`)}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <ol className="meet-ask-thread">
          {messages.map((message, index) => (
            <li key={index} className={`meet-ask-message is-${message.role}`}>
              {message.role === "user" ? (
                <p>{message.content}</p>
              ) : (
                <Markdown markdown={message.content} className="gm-markdown" />
              )}
            </li>
          ))}
          {pending && (
            <li className="meet-ask-message is-assistant" role="status">
              <span className="meet-spinner" aria-hidden="true" />
              <span className="meet-ask-thinking">
                {t("ui2.meet.ask.thinking")}
              </span>
            </li>
          )}
        </ol>
      )}
      {error && (
        <p className="meet-inline-error" role="alert">
          {error}
        </p>
      )}
      <div className="meet-ask-dock">
        <form className="meet-ask-composer" onSubmit={submit}>
          <textarea
            rows={1}
            value={draft}
            placeholder={t("ui2.meet.ask.placeholder")}
            aria-label={t("ui2.meet.ask.placeholder")}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (
                event.key === "Enter" &&
                !event.shiftKey &&
                !event.nativeEvent.isComposing
              ) {
                event.preventDefault();
                void send(draft);
              }
            }}
          />
          <button
            className="meet-ask-send"
            type="submit"
            disabled={pending || draft.trim() === ""}
            aria-label={t("ui2.meet.ask.send")}
            title={t("ui2.meet.ask.send")}
          >
            <ArrowUp className="icon sm" aria-hidden="true" />
          </button>
        </form>
      </div>
      <div ref={endRef} className="meet-ask-end" />
    </section>
  );
}
