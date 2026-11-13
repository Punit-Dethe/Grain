import { useTranslation } from "react-i18next";
import { ArrowLeft, Bell, Check, ExternalLink } from "lucide-react";
import { commands, type Reminder } from "@/bindings";
import {
  dateLabel,
  isMissed,
  isoDay,
  linkLabel,
  relativeDay,
  timeLabel,
} from "@/reminders/reminderFormat";

interface Props {
  /** Ringing reminders, oldest due first; the first one is shown. */
  reminders: Reminder[];
  onBack: () => void;
  onDone: (id: string) => void;
}

/** The notch's reminder view: one ringing reminder at a time. */
export function AgentReminder({ reminders, onBack, onDone }: Props) {
  const { t, i18n } = useTranslation();
  const reminder = reminders[0];
  if (!reminder) return null;

  const now = new Date();
  const today = isoDay(now);
  const near = relativeDay(reminder.date, today);
  const day =
    near === 0
      ? ""
      : near === 1
        ? t("agent.reminderTomorrow")
        : near === -1
          ? t("agent.reminderYesterday")
          : dateLabel(reminder.date, today, i18n.language);
  const time = reminder.time
    ? timeLabel(reminder.time, i18n.language)
    : t("agent.reminderAllDay");
  const when = [day, time].filter(Boolean).join(" · ");
  const missed = isMissed(reminder, now);

  return (
    <div className="agrm">
      <div className="agrm-top">
        <button type="button" className="agrm-back" onClick={onBack}>
          <ArrowLeft size={13} />
          <span>{t("agent.reminderBack")}</span>
        </button>
        {reminders.length > 1 && (
          <span className="agrm-count">
            {t("agent.reminderCount", { index: 1, count: reminders.length })}
          </span>
        )}
      </div>

      <div className="agrm-body" key={reminder.id}>
        <div className="agrm-when" data-missed={missed ? "" : undefined}>
          <span className="agrm-bell" aria-hidden="true">
            <Bell size={12} />
          </span>
          <span>{missed ? t("agent.reminderMissed", { when }) : when}</span>
        </div>
        <h2 className="agrm-title">{reminder.title}</h2>
        {reminder.details && <p className="agrm-details">{reminder.details}</p>}
      </div>

      <div className="agrm-actions">
        {reminder.links.map((link, index) => (
          <button
            type="button"
            key={link.url}
            className="agrm-open"
            title={link.url}
            onClick={() =>
              void commands
                .remindersOpenLink(reminder.id, index)
                .catch(console.error)
            }
          >
            <ExternalLink size={13} />
            <span>{t("agent.reminderOpen")}</span>
            <span className="agrm-host">{linkLabel(link.url)}</span>
          </button>
        ))}
        <button
          type="button"
          className="agrm-done"
          onClick={() => onDone(reminder.id)}
        >
          <Check size={14} />
          <span>{t("agent.reminderDone")}</span>
        </button>
      </div>
    </div>
  );
}
