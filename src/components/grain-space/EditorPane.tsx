import { lazy, Suspense, useRef } from "react";
import { useTranslation } from "react-i18next";
import { AlarmClock, ExternalLink, Lock, Pin, Trash2 } from "lucide-react";
import type { Note, ReminderState } from "@/bindings";
import { EditorToolbar } from "./EditorToolbar";
import type { EditorHandle } from "./MarkdownEditor";

/**
 * [GRAIN] The editor sheet. The markdown editor itself is code-split via
 * `React.lazy`: its chunk is never fetched (and its JS heap never exists)
 * until the first note actually opens — part of the workspace's low-idle-RAM
 * contract. Foreign vault notes render read-only (v1 rule: Grain never writes
 * a file the user might have open dirty in Obsidian).
 */
const MarkdownEditor = lazy(() => import("./MarkdownEditor"));

const fireFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});
const dateFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});

type Props = {
  note: Note;
  /** Editor-session key: changes only on a real note switch (NOT when a draft
   * adopts its minted id), so the document/caret survive the first save. */
  docKey: number;
  readonly: boolean;
  /** Active backend is the user's Obsidian vault (enables the deep link). */
  isObsidian: boolean;
  folder: string | null;
  onEdit: (note: Note) => void;
  onFlush: () => void;
  onTogglePin: () => void;
  onDelete: () => void;
  onArmReminder: () => void;
  onDismissReminder: () => void;
  onOpenExternal: () => void;
};

export function EditorPane({
  note,
  docKey,
  readonly,
  isObsidian,
  folder,
  onEdit,
  onFlush,
  onTogglePin,
  onDelete,
  onArmReminder,
  onDismissReminder,
  onOpenExternal,
}: Props) {
  const { t } = useTranslation();
  const editorRef = useRef<EditorHandle | null>(null);
  const reminder: ReminderState = note.reminder_state ?? {
    status: "none",
    fire_at: null,
  };

  return (
    <section className="gs-sheet">
      <div className="gs-meta">
        <span>{dateFormat.format(new Date(note.timestamp))}</span>
        {folder && <span className="gs-chip">{`#${folder}`}</span>}
        {readonly && (
          <span className="gs-chip gs-chip--quiet">
            <Lock width={9} height={9} />
            {t("grainSpaceOverlay.readonly")}
          </span>
        )}
      </div>
      <input
        className="gs-title"
        value={note.title}
        placeholder={t("grainSpaceOverlay.titlePlaceholder")}
        spellCheck={false}
        disabled={readonly}
        onChange={(e) => onEdit({ ...note, title: e.target.value })}
        onBlur={onFlush}
      />

      <Suspense
        fallback={
          <div className="gs-ed-loading">
            <textarea
              className="gs-bodytext"
              defaultValue={note.body}
              readOnly
              placeholder={t("grainSpaceOverlay.bodyPlaceholder")}
            />
          </div>
        }
      >
        <MarkdownEditor
          ref={editorRef}
          docKey={docKey}
          value={note.body}
          readOnly={readonly}
          placeholder={t("grainSpaceOverlay.bodyPlaceholder")}
          onChange={(body) => onEdit({ ...note, body })}
          onBlur={onFlush}
        />
      </Suspense>

      <div className="gs-actions">
        {!readonly && (
          <>
            <EditorToolbar editor={editorRef} />
            <span className="gs-fmt-divider" />
          </>
        )}
        <div className="gs-reminder">
          {reminder.status === "pending" && reminder.fire_at != null && (
            <>
              <AlarmClock width={13} height={13} />
              <span>{fireFormat.format(new Date(reminder.fire_at))}</span>
              <button type="button" className="gs-btn" onClick={onArmReminder}>
                {t("grainSpaceOverlay.armReminder")}
              </button>
            </>
          )}
          {(reminder.status === "armed" || reminder.status === "fired") && (
            <>
              <AlarmClock width={13} height={13} />
              {reminder.fire_at != null && (
                <span>{fireFormat.format(new Date(reminder.fire_at))}</span>
              )}
              <button
                type="button"
                className="gs-btn gs-btn--quiet"
                onClick={onDismissReminder}
              >
                {t("grainSpaceOverlay.dismiss")}
              </button>
            </>
          )}
        </div>
        {isObsidian && note.id && (
          <button
            type="button"
            className="gs-iconbtn"
            title="Open in Obsidian"
            onClick={onOpenExternal}
          >
            <ExternalLink width={14} height={14} />
          </button>
        )}
        {!readonly && (
          <>
            <button
              type="button"
              className={`gs-iconbtn${note.is_pinned ? " gs-iconbtn--active" : ""}`}
              title={note.is_pinned ? "Unpin" : "Pin"}
              onClick={onTogglePin}
              disabled={!note.id}
            >
              <Pin width={14} height={14} />
            </button>
            <button
              type="button"
              className="gs-iconbtn gs-iconbtn--danger"
              title="Delete note"
              onClick={onDelete}
            >
              <Trash2 width={14} height={14} />
            </button>
          </>
        )}
      </div>
    </section>
  );
}
