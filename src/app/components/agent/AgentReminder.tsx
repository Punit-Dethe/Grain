import { useEffect, useLayoutEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  AppWindow,
  ArrowUpRight,
  Bell,
  Check,
  Globe,
  Video,
} from "lucide-react";
import { commands, type Reminder } from "@/bindings";
import {
  dateLabel,
  isMissed,
  isoDay,
  linkInfo,
  relativeDay,
  timeLabel,
  type LinkInfo,
} from "@/reminders/reminderFormat";

/** The reminder shell width, in CSS px (`.agent-notch.is-fit` in agent.css). */
export const REMINDER_WIDTH = 440;
/** The notch header above the view (`--notch-reply-header`) plus its edge. */
const HEADER_HEIGHT = 35;
/** Snooze choices, in minutes. */
const SNOOZE_MINUTES = [5, 15, 60] as const;

interface Props {
  /** The ringing reminder on show. */
  reminder: Reminder;
  onDone: (id: string) => void;
  onSnooze: (id: string, minutes: number) => void;
  /** The shell height this view needs, in CSS px. */
  onHeightChange: (height: number) => void;
}

const KIND_ICON = { meeting: Video, web: Globe, app: AppWindow } as const;

function LinkRow({
  reminderId,
  index,
  info,
  url,
  primary,
  autoFocus,
}: {
  reminderId: string;
  index: number;
  info: LinkInfo;
  url: string;
  primary: boolean;
  autoFocus: boolean;
}) {
  const { t } = useTranslation();
  const Icon = KIND_ICON[info.kind];
  return (
    <li>
      <button
        type="button"
        className="agrm-link"
        data-primary={primary ? "" : undefined}
        title={url}
        autoFocus={autoFocus}
        onClick={() =>
          void commands
            .remindersOpenLink(reminderId, index)
            .catch(console.error)
        }
      >
        <span className="agrm-link-icon" aria-hidden="true">
          <Icon size={15} strokeWidth={1.8} />
        </span>
        <span className="agrm-link-text">
          <span className="agrm-link-name">{info.name}</span>
          {info.detail && (
            <span className="agrm-link-detail">{info.detail}</span>
          )}
        </span>
        <span className="agrm-link-go">
          {t(
            info.kind === "meeting"
              ? "agent.reminderJoin"
              : "agent.reminderOpen",
          )}
          <ArrowUpRight size={13} strokeWidth={2} />
        </span>
      </button>
    </li>
  );
}

/** The notch's reminder view: one ringing reminder, sized to its content. */
export function AgentReminder({
  reminder,
  onDone,
  onSnooze,
  onHeightChange,
}: Props) {
  const { t, i18n } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const doneRef = useRef<HTMLButtonElement>(null);

  // Report the height the content needs; the panel sizes and clips the shell.
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const report = () =>
      onHeightChange(
        Math.ceil(root.getBoundingClientRect().height) + HEADER_HEIGHT,
      );
    report();
    const observer = new ResizeObserver(report);
    observer.observe(root);
    return () => observer.disconnect();
  }, [onHeightChange]);

  // Enter acts on the primary control: the first link, or Done without one.
  useEffect(() => {
    if (!reminder.links.length) doneRef.current?.focus();
  }, [reminder.id, reminder.links.length]);

  const now = new Date();
  const today = isoDay(now);
  const near = relativeDay(reminder.date, today);
  const day =
    near === 0
      ? t("agent.reminderToday")
      : near === 1
        ? t("agent.reminderTomorrow")
        : near === -1
          ? t("agent.reminderYesterday")
          : dateLabel(reminder.date, today, i18n.language);
  const time = reminder.time
    ? timeLabel(reminder.time, i18n.language)
    : t("agent.reminderAllDay");
  const missed = isMissed(reminder, now);
  const hasLinks = reminder.links.length > 0;

  return (
    <div className="agrm" ref={rootRef}>
      <div className="agrm-head">
        <h2 className="agrm-title">{reminder.title}</h2>
        <div className="agrm-when">
          <span className="agrm-bell" aria-hidden="true">
            <Bell size={12} strokeWidth={2} />
          </span>
          <span className="agrm-time">{time}</span>
          <span className="agrm-dot" aria-hidden="true" />
          <span>{day}</span>
          {missed && (
            <span className="agrm-missed">{t("agent.reminderMissed")}</span>
          )}
        </div>
        {reminder.details && <p className="agrm-details">{reminder.details}</p>}
      </div>

      {hasLinks && (
        <ul className="agrm-links" aria-label={t("agent.reminderLinks")}>
          {reminder.links.map((link, index) => (
            <LinkRow
              key={link.url}
              reminderId={reminder.id}
              index={index}
              info={linkInfo(link.url)}
              url={link.url}
              primary={index === 0}
              autoFocus={index === 0}
            />
          ))}
        </ul>
      )}

      <div className="agrm-foot">
        <div
          className="agrm-snooze"
          role="group"
          aria-label={t("agent.reminderSnooze")}
        >
          <span className="agrm-snooze-label" aria-hidden="true">
            {t("agent.reminderSnooze")}
          </span>
          {SNOOZE_MINUTES.map((minutes) => (
            <button
              type="button"
              key={minutes}
              className="agrm-chip"
              aria-label={t("agent.reminderSnoozeFor", {
                when:
                  minutes < 60
                    ? t("agent.reminderMinutes", { count: minutes })
                    : t("agent.reminderHours", { count: minutes / 60 }),
              })}
              onClick={() => onSnooze(reminder.id, minutes)}
            >
              {minutes < 60
                ? t("agent.reminderMinutesShort", { count: minutes })
                : t("agent.reminderHoursShort", { count: minutes / 60 })}
            </button>
          ))}
        </div>
        <button
          ref={doneRef}
          type="button"
          className="agrm-done"
          data-primary={hasLinks ? undefined : ""}
          onClick={() => onDone(reminder.id)}
        >
          <Check size={14} strokeWidth={2.2} />
          <span>{t("agent.reminderDone")}</span>
        </button>
      </div>
    </div>
  );
}
