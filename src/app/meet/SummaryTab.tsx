import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Copy, RefreshCw, Sparkles } from "lucide-react";
import type { MeetSummaryDoc, MeetingSummary } from "@/bindings";
import { Markdown } from "@/components/markdown/Markdown";
import "@/components/markdown/markdown.css";
import { hashForRoute } from "../navigation";
import { useMeetAiAvailable, type MeetController } from "./useMeetController";

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Shown by the Summary and Ask AI tabs while no AI provider is set up. */
export function AiRequired() {
  const { t } = useTranslation();
  return (
    <section className="meet-pane-state">
      <span className="meet-pane-icon" aria-hidden="true">
        <Sparkles size={22} strokeWidth={1.6} />
      </span>
      <h2>{t("ui2.meet.ai.requiredTitle")}</h2>
      <p>{t("ui2.meet.ai.requiredBody")}</p>
      <a
        className="button primary"
        href={hashForRoute({ page: "settings", section: "ai" })}
      >
        {t("ui2.meet.ai.openSettings")}
      </a>
    </section>
  );
}

/** The meeting's notes: what was said, decided and who said what. */
export function SummaryTab({
  meet,
  meeting,
}: {
  meet: MeetController;
  meeting: MeetingSummary;
}) {
  const { t, i18n } = useTranslation();
  const ai = useMeetAiAvailable();
  const [doc, setDoc] = useState<MeetSummaryDoc | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { loadSummary, generateSummary } = meet;
  // Writing started here, or by the automatic summary after transcription.
  const writing = generating || meet.status.summarizing === meeting.id;

  useEffect(() => {
    if (writing) return;
    let active = true;
    loadSummary(meeting.id)
      .then((next) => {
        if (!active) return;
        setDoc(next);
        setLoaded(true);
      })
      .catch((failure) => {
        if (!active) return;
        setError(errorMessage(failure));
        setLoaded(true);
      });
    return () => {
      active = false;
    };
  }, [loadSummary, meeting.id, meeting.transcript_revision, writing]);

  const written = useMemo(
    () =>
      doc
        ? new Intl.DateTimeFormat(i18n.language, {
            dateStyle: "medium",
            timeStyle: "short",
          }).format(doc.created_unix_ms)
        : "",
    [doc, i18n.language],
  );

  const write = async () => {
    setGenerating(true);
    setError(null);
    try {
      setDoc(await generateSummary(meeting.id));
      setLoaded(true);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setGenerating(false);
    }
  };

  const copy = async () => {
    if (!doc) return;
    try {
      await navigator.clipboard.writeText(doc.text);
      toast.success(t("ui2.meet.summary.copied"));
    } catch (failure) {
      toast.error(errorMessage(failure));
    }
  };

  if (writing) {
    return (
      <section className="meet-pane-state" role="status">
        <span className="meet-spinner" aria-hidden="true" />
        <h2>{t("ui2.meet.summary.writing")}</h2>
        <p>{t("ui2.meet.summary.writingBody")}</p>
      </section>
    );
  }

  if (doc) {
    const stale = doc.transcript_revision !== meeting.transcript_revision;
    return (
      <section className="meet-summary">
        <div className="meet-summary-bar">
          <p className="meet-summary-meta">
            {t("ui2.meet.summary.written", { time: written })}
            {stale && (
              <span className="meet-summary-stale">
                {t("ui2.meet.summary.stale")}
              </span>
            )}
          </p>
          <div className="meet-summary-actions">
            <button
              className="button"
              type="button"
              disabled={ai === false}
              onClick={write}
            >
              <RefreshCw className="icon sm" aria-hidden="true" />
              {t("ui2.meet.summary.rewrite")}
            </button>
            <button
              className="icon-button"
              type="button"
              aria-label={t("ui2.meet.summary.copy")}
              title={t("ui2.meet.summary.copy")}
              onClick={copy}
            >
              <Copy className="icon sm" aria-hidden="true" />
            </button>
          </div>
        </div>
        {error && (
          <p className="meet-inline-error" role="alert">
            {error}
          </p>
        )}
        <Markdown
          markdown={doc.text}
          className="gm-markdown meet-summary-body"
        />
      </section>
    );
  }

  if (!loaded || ai === null) return null;
  if (ai === false) return <AiRequired />;

  return (
    <section className="meet-pane-state">
      <span className="meet-pane-icon" aria-hidden="true">
        <Sparkles size={22} strokeWidth={1.6} />
      </span>
      <h2>{t("ui2.meet.summary.emptyTitle")}</h2>
      <p>{t("ui2.meet.summary.emptyBody")}</p>
      <button className="button primary" type="button" onClick={write}>
        {t("ui2.meet.summary.write")}
      </button>
      {error && (
        <p className="meet-inline-error" role="alert">
          {error}
        </p>
      )}
      <a
        className="meet-pane-link"
        href={hashForRoute({ page: "meet", settings: true })}
      >
        {t("ui2.meet.summary.automatic")}
      </a>
    </section>
  );
}
