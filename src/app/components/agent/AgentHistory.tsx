import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { MessagesSquare, Trash2 } from "lucide-react";
import type { AgentChatSummary } from "@/bindings";

const DAY_MS = 86_400_000;

type Group = "today" | "yesterday" | "week" | "older";
const GROUP_LABEL: Record<Group, string> = {
  today: "agent.historyToday",
  yesterday: "agent.historyYesterday",
  week: "agent.historyWeek",
  older: "agent.historyOlder",
};

function groupOf(at: number, todayStart: number): Group {
  if (at >= todayStart) return "today";
  if (at >= todayStart - DAY_MS) return "yesterday";
  if (at >= todayStart - 6 * DAY_MS) return "week";
  return "older";
}

interface Props {
  /** `null` while the list loads. */
  chats: AgentChatSummary[] | null;
  current: string | null;
  /** A turn is running: switching or deleting waits for it. */
  busy: boolean;
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
}

/** The notch's chats, newest first, grouped by day. */
export function AgentHistory({
  chats,
  current,
  busy,
  onOpen,
  onDelete,
}: Props) {
  const { t, i18n } = useTranslation();
  // The row whose delete asks for a second click.
  const [confirming, setConfirming] = useState<string | null>(null);

  const groups = useMemo(() => {
    const now = new Date();
    const todayStart = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate(),
    ).getTime();
    const clock = new Intl.DateTimeFormat(i18n.language, {
      hour: "numeric",
      minute: "2-digit",
    });
    const weekday = new Intl.DateTimeFormat(i18n.language, {
      weekday: "short",
    });
    const date = new Intl.DateTimeFormat(i18n.language, {
      month: "short",
      day: "numeric",
    });
    const out: Array<{
      group: Group;
      rows: Array<AgentChatSummary & { when: string }>;
    }> = [];
    for (const chat of chats ?? []) {
      const group = groupOf(chat.updated_unix_ms, todayStart);
      const format =
        group === "today" || group === "yesterday"
          ? clock
          : group === "week"
            ? weekday
            : date;
      const row = { ...chat, when: format.format(chat.updated_unix_ms) };
      const last = out[out.length - 1];
      if (last?.group === group) last.rows.push(row);
      else out.push({ group, rows: [row] });
    }
    return out;
  }, [chats, i18n.language]);

  if (!chats) return <div className="agh" />;

  if (!chats.length)
    return (
      <div className="agh">
        <div className="agh-empty">
          <MessagesSquare size={18} strokeWidth={1.75} />
          <span>{t("agent.historyEmpty")}</span>
        </div>
      </div>
    );

  return (
    <div className="agh">
      <div className="agh-scroll">
        {groups.map(({ group, rows }) => (
          <section key={group} className="agh-group">
            <h3 className="agh-group-label">{t(GROUP_LABEL[group])}</h3>
            <ul className="agh-list">
              {rows.map((chat) => {
                const asking = confirming === chat.id;
                return (
                  <li
                    key={chat.id}
                    className={`agh-row${asking ? " is-confirming" : ""}`}
                    aria-current={chat.id === current ? "true" : undefined}
                    onPointerLeave={() => asking && setConfirming(null)}
                  >
                    <button
                      type="button"
                      className="agh-open"
                      disabled={busy}
                      onClick={() => onOpen(chat.id)}
                    >
                      <span className="agh-title">
                        {chat.title || t("agent.historyUntitled")}
                      </span>
                      <span className="agh-when">{chat.when}</span>
                    </button>
                    <button
                      type="button"
                      className="agh-delete"
                      disabled={busy}
                      title={t("agent.historyDelete")}
                      aria-label={
                        asking
                          ? t("agent.historyDeleteConfirm")
                          : t("agent.historyDelete")
                      }
                      onBlur={() => asking && setConfirming(null)}
                      onClick={() => {
                        if (!asking) {
                          setConfirming(chat.id);
                          return;
                        }
                        setConfirming(null);
                        onDelete(chat.id);
                      }}
                    >
                      {asking ? (
                        t("agent.historyDeleteConfirm")
                      ) : (
                        <Trash2 size={13} />
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
