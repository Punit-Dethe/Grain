/* eslint-disable i18next/no-literal-string -- UI 2.0 prototype copy is a frozen visual contract until the cutover translation pass. */
import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  AppWindow,
  ArrowLeft,
  Bot,
  BriefcaseBusiness,
  Code2,
  Globe2,
  Mail,
  MessageCircle,
  Plus,
  Search,
  Shapes,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  commands,
  type ContextProfileInfo,
  type CustomContextProfile,
  type InstalledApp,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

type ContextProfileId =
  | "email"
  | "work"
  | "casual"
  | "technical"
  | "ai_chat"
  | "other";

const INSTRUCTION_EDITOR_MAX_HEIGHT = 184;

type ContextProfile = {
  id: ContextProfileId;
  label: string;
  /** Names the profile in the tab row, and stands in for a site's favicon until
   *  (or unless) it resolves. */
  tabIcon: LucideIcon;
  summary: string;
  detail: string;
  available: boolean;
  /** Lives under the Other tab rather than getting a tab of its own. Keeps the
   *  tab row to the four surfaces most people dictate into, while still being a
   *  real built-in profile — and a worked example of what a profile in Other
   *  looks like, next to the ones the user makes. */
  inOtherTab?: boolean;
};

const CONTEXT_PROFILES: readonly ContextProfile[] = [
  {
    id: "email",
    label: "Email",
    tabIcon: Mail,
    summary: "This profile applies in email applications",
    detail: "Grain keeps dictated email polished without inventing structure.",
    available: true,
  },
  {
    id: "work",
    label: "Work",
    tabIcon: BriefcaseBusiness,
    summary: "This profile applies in work applications",
    detail: "Grain stays concise across team chat, tickets, and tasks.",
    available: true,
  },
  {
    id: "casual",
    label: "Casual",
    tabIcon: MessageCircle,
    summary: "This profile applies in casual applications",
    detail: "Grain protects the phrasing and personality of everyday messages.",
    available: true,
  },
  {
    id: "technical",
    label: "Technical",
    tabIcon: Code2,
    summary: "This profile applies in technical applications",
    detail: "Grain preserves exact syntax in editors, terminals, and AI tools.",
    available: true,
  },
  {
    id: "ai_chat",
    label: "AI chat",
    tabIcon: Bot,
    summary: "This profile applies in AI assistants",
    detail:
      "Grain writes your prompt into the box instead of answering it, and leaves your specifics alone.",
    available: true,
    inOtherTab: true,
  },
  {
    id: "other",
    label: "Other",
    tabIcon: Shapes,
    summary: "",
    detail: "",
    available: true,
  },
];

/** The tab row: everything that is not parked under Other. */
const TAB_PROFILES = CONTEXT_PROFILES.filter((profile) => !profile.inOtherTab);
/** Built-in profiles shown as cards inside the Other tab. */
const OTHER_TAB_PROFILES = CONTEXT_PROFILES.filter(
  (profile) => profile.inOtherTab,
);

/** The backend's target shape, widened only where the editor needs it. `value`
 *  is normalised on save (exe stem / bare host), so what is typed here is a
 *  convenience — pasting a full path or URL is expected to work. */
type CustomProfileTarget = {
  kind: "application" | "website";
  value: string;
};

type CustomProfileDialogState =
  | { mode: "create" }
  | { mode: "edit"; profile: CustomContextProfile };

/** Most icons a stack shows. Past three the overlap stops reading as separate
 *  icons and starts reading as a smudge. */
const ICON_STACK_MAX = 3;

/** Most rows the app picker draws before it asks you to narrow the search.
 *
 *  Not a display limit for its own sake — every visible row resolves a real icon
 *  through the Shell, and a few hundred at once on opening a picker is work
 *  nobody asked for. Typing two letters cuts any machine's list below this. */
const APP_PICKER_LIMIT = 50;

/** The installed-application catalogue, fetched at most once per app run.
 *
 *  Module-level and promise-valued so that the picker and every profile card
 *  asking for an app icon share ONE enumeration — it walks a shell namespace, so
 *  a second one would be squarely the wrong kind of cheap. Nothing calls this
 *  unless an application actually needs to be drawn or chosen, so a user with no
 *  application targets never pays for it at all. */
let installedAppsPromise: Promise<InstalledApp[]> | null = null;

/** `refresh` re-reads rather than reusing the cached walk.
 *
 *  The picker passes it on open, because that is the one moment freshness is
 *  worth anything: without it an app installed since Grain started would be
 *  missing from the list, with no hint as to why. Everywhere else — resolving an
 *  icon for a target already on a profile — the cached answer is the right one,
 *  and re-walking a shell namespace to draw a 18px tile would not be. */
function loadInstalledApps(refresh = false): Promise<InstalledApp[]> {
  if (refresh || !installedAppsPromise) {
    installedAppsPromise = commands.installedApps().catch(() => []);
  }
  return installedAppsPromise;
}

/** Resolved app icons by stored target. `null` means "asked, none to be had". */
const appIconCache = new Map<string, string | null>();

/** An installed application's real icon, falling back to a glyph.
 *
 *  Two hops rather than one, because a target is a MATCH key and not something
 *  the Shell can draw: `code` identifies VS Code to the detector but names
 *  nothing to the shell, whose id for it is `Microsoft.VisualStudioCode`. The
 *  catalogue is what maps between them, which is also why a hand-typed
 *  application quietly keeps the glyph — there is nothing to look it up in. */
function AppIcon({
  target,
  fallback: Fallback = AppWindow,
}: {
  target: string;
  fallback?: LucideIcon;
}) {
  const [src, setSrc] = useState<string | null>(
    () => appIconCache.get(target) ?? null,
  );

  useEffect(() => {
    if (appIconCache.has(target)) {
      setSrc(appIconCache.get(target) ?? null);
      return;
    }
    let live = true;
    void (async () => {
      const apps = await loadInstalledApps();
      const match = apps.find((app) => app.target === target);
      const data = match ? await commands.appIcon(match.icon_id) : null;
      appIconCache.set(target, data);
      if (live) setSrc(data);
    })();
    return () => {
      live = false;
    };
  }, [target]);

  return src ? (
    <img src={src} alt="" width={18} height={18} />
  ) : (
    <Fallback size={15} strokeWidth={1.8} />
  );
}

/** Resolved favicons by host, shared by every stack on the page.
 *
 *  Module-level rather than component state because the same host appears in
 *  several places (a profile card and its edit dialog), and a per-component
 *  cache would fetch each one once per place it is drawn. `null` is a real
 *  entry meaning "asked, and there is no icon" — without it an unsupported host
 *  is re-requested on every render. */
const siteIconCache = new Map<string, string | null>();

/** A site's real favicon, falling back to a glyph until (or unless) it loads.
 *
 *  Fetched through the backend, which shares its cache with the pill — so the
 *  first time a site's icon appears anywhere in Grain it is warm everywhere. */
function SiteIcon({
  host,
  fallback: Fallback,
}: {
  host: string;
  fallback: LucideIcon;
}) {
  const [src, setSrc] = useState<string | null>(
    () => siteIconCache.get(host) ?? null,
  );

  useEffect(() => {
    if (siteIconCache.has(host)) {
      setSrc(siteIconCache.get(host) ?? null);
      return;
    }
    let live = true;
    void commands.siteIcon(host).then((data) => {
      siteIconCache.set(host, data);
      // The card can be unmounted while a cold favicon is still being
      // fetched — the cache above still keeps the result.
      if (live) setSrc(data);
    });
    return () => {
      live = false;
    };
  }, [host]);

  return src ? (
    <img src={src} alt="" width={18} height={18} />
  ) : (
    <Fallback size={15} strokeWidth={1.8} />
  );
}

/** Overlapping stack of the real icons for what a profile covers — a website's
 *  favicon or an application's own icon, whichever each target is.
 *
 *  `className` chooses the arrangement, because the two places this appears want
 *  opposite ones: the wide panel header has room to fan them out sideways, while
 *  a grid card has height to spare and no width, so its stack runs downward. The
 *  tiles themselves are identical, which is the point of one component. */
function ProfileIconStack({
  targets,
  fallback = Globe2,
  className = "context-custom-profile-icons",
}: {
  targets: ReadonlyArray<CustomProfileTarget>;
  fallback?: LucideIcon;
  className?: string;
}) {
  const shown = targets.slice(0, ICON_STACK_MAX);
  return (
    <div className={className} aria-hidden="true">
      {shown.length === 0 ? (
        <span>
          <AppWindow size={15} strokeWidth={1.8} />
        </span>
      ) : (
        shown.map((target) => (
          <span key={`${target.kind}:${target.value}`} title={target.value}>
            {target.kind === "website" ? (
              <SiteIcon host={target.value} fallback={fallback} />
            ) : (
              <AppIcon target={target.value} />
            )}
          </span>
        ))
      )}
    </div>
  );
}

/** A built-in profile's representative sites as icon-stack targets.
 *
 *  The hosts come from Rust, taken from the same table that routes a site to
 *  this profile — so the faces on the card are, by construction, sites the
 *  profile actually covers rather than a second list that can drift. */
function sampleTargets(info?: ContextProfileInfo): CustomProfileTarget[] {
  return (info?.sample_sites ?? []).map((host) => ({
    kind: "website",
    value: host,
  }));
}

/** Search the installed applications and pick one.
 *
 *  Chosen over asking someone to type an executable name, which is what this
 *  replaced: the name of the binary is rarely the name of the app (Grain's own
 *  is `handy.exe`), so typing it was a guess that failed silently — the profile
 *  saved, looked right, and never fired.
 *
 *  Each row shows the executable underneath the name, because the Start menu is
 *  not a list of distinct applications: a shortcut that opens a file in a
 *  generic host resolves to that host, so an entry called "Icecast Config" is
 *  really Notepad. Showing what it resolves to makes that visible rather than
 *  misleading. */
function AppPicker({
  chosen,
  onPick,
  onCancel,
}: {
  /** Targets already on the profile, so they can be marked rather than offered
   *  again as though picking one would do something. */
  chosen: ReadonlySet<string>;
  onPick: (value: string) => void;
  onCancel: () => void;
}) {
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let live = true;
    void loadInstalledApps(true).then((rows) => {
      if (live) setApps(rows);
    });
    return () => {
      live = false;
    };
  }, []);

  const needle = query.trim().toLowerCase();
  const matches = (apps ?? [])
    .filter(
      (app) =>
        !needle ||
        app.name.toLowerCase().includes(needle) ||
        app.target.toLowerCase().includes(needle),
    )
    .slice(0, APP_PICKER_LIMIT);

  return (
    <div className="context-app-picker">
      <div className="context-app-picker-search">
        <Search size={14} aria-hidden="true" />
        <input
          autoFocus
          className="dictionary-dialog-input"
          value={query}
          aria-label="Search applications"
          placeholder="Search applications"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onCancel();
            }
          }}
        />
        <button
          className="context-profile-target-cancel"
          type="button"
          aria-label="Cancel adding application"
          onClick={onCancel}
        >
          <X size={14} aria-hidden="true" />
        </button>
      </div>

      <div className="context-app-picker-list" role="listbox" tabIndex={-1}>
        {apps === null && (
          <p className="context-app-picker-empty">Reading your applications…</p>
        )}
        {apps !== null && matches.length === 0 && (
          <p className="context-app-picker-empty">
            {/* An app installed without a Start menu entry is invisible to
                Windows' own app list too, so typing the name stays reachable
                rather than being a dead end. */}
            No match. Applications without a Start menu entry can be added by
            executable name.
          </p>
        )}
        {matches.map((app) => {
          const already = chosen.has(app.target.toLocaleLowerCase());
          return (
            <button
              key={app.target}
              className="context-app-picker-row"
              type="button"
              role="option"
              aria-selected={already}
              disabled={already}
              onClick={() => onPick(app.target)}
            >
              <span className="context-app-picker-icon" aria-hidden="true">
                <AppIcon target={app.target} />
              </span>
              <span className="context-app-picker-name">
                <strong>{app.name}</strong>
                {/* A packaged app has no executable to show — its identity IS
                    the name, so there is nothing to disambiguate. */}
                {!app.target.includes("!") && <span>{app.target}.exe</span>}
              </span>
              {already && (
                <span className="context-app-picker-added">Added</span>
              )}
            </button>
          );
        })}
        {needle && (
          <button
            className="context-app-picker-row context-app-picker-manual"
            type="button"
            onClick={() => onPick(query.trim())}
          >
            <span className="context-app-picker-icon" aria-hidden="true">
              <Plus size={14} />
            </span>
            <span className="context-app-picker-name">
              <strong>Use “{query.trim()}”</strong>
              <span>Add by name, for an app that is not listed</span>
            </span>
          </button>
        )}
      </div>
    </div>
  );
}

function CustomProfileDialog({
  state,
  onClose,
  onSave,
  onDelete,
}: {
  state: CustomProfileDialogState;
  onClose: () => void;
  onSave: (profile: CustomContextProfile) => void;
  onDelete: (id: string) => void;
}) {
  const editing = state.mode === "edit";
  const [title, setTitle] = useState(editing ? state.profile.title : "");
  const [instruction, setInstruction] = useState(
    editing ? state.profile.instruction : "",
  );
  const [targets, setTargets] = useState<CustomProfileTarget[]>(
    editing
      ? (state.profile.targets ?? []).map((target) => ({
          ...target,
          kind: target.kind as CustomProfileTarget["kind"],
        }))
      : [],
  );
  const [targetKind, setTargetKind] = useState<
    CustomProfileTarget["kind"] | null
  >(null);
  const [targetValue, setTargetValue] = useState("");
  const dialogRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const returnFocusTo = document.activeElement as HTMLElement | null;
    dialogRef.current
      ?.querySelector<HTMLElement>("[data-dialog-initial-focus]")
      ?.focus();
    return () => returnFocusTo?.focus();
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      if (event.key !== "Tab") return;
      const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), textarea:not(:disabled)",
      );
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const addTarget = (
    kind: CustomProfileTarget["kind"] | null = targetKind,
    raw = targetValue,
  ) => {
    const value = raw.trim();
    if (!value || !kind) return;
    const duplicate = targets.some(
      (target) =>
        target.kind === kind &&
        target.value.toLocaleLowerCase() === value.toLocaleLowerCase(),
    );
    if (!duplicate) {
      setTargets((current) => [...current, { kind, value }]);
    }
    setTargetValue("");
    setTargetKind(null);
  };

  /** Application targets already on the profile, lowercased for comparison —
   *  the picker marks these rather than offering them a second time. */
  const chosenApps = new Set(
    targets
      .filter((target) => target.kind === "application")
      .map((target) => target.value.toLocaleLowerCase()),
  );

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!title.trim() || !instruction.trim() || targets.length === 0) return;
    onSave({
      id: editing ? state.profile.id : crypto.randomUUID(),
      title: title.trim(),
      instruction: instruction.trim(),
      targets,
    });
    onClose();
  };

  return (
    <div
      className="dictionary-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        ref={dialogRef}
        className="dictionary-dialog context-profile-editor-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="context-profile-editor-title"
        aria-describedby="context-profile-editor-description"
      >
        <button
          className="dictionary-dialog-close"
          type="button"
          aria-label="Close"
          onClick={onClose}
        >
          <X size={16} aria-hidden="true" />
        </button>
        <header className="dictionary-dialog-header">
          <h2 id="context-profile-editor-title">
            {editing ? "Edit profile" : "Create profile"}
          </h2>
          <p id="context-profile-editor-description">
            Choose where this profile applies and how Grain should shape your
            dictation.
          </p>
        </header>

        <form
          className="dictionary-dialog-form context-profile-editor-form"
          onSubmit={submit}
        >
          <div className="context-profile-editor-fields">
            <label className="context-profile-editor-field">
              <span>Title</span>
              <input
                data-dialog-initial-focus
                className="dictionary-dialog-input"
                value={title}
                maxLength={48}
                placeholder="e.g. Customer support"
                onChange={(event) => setTitle(event.target.value)}
              />
            </label>

            <label className="context-profile-editor-field">
              <span>Instruction</span>
              <textarea
                className="dictionary-dialog-input dictionary-dialog-textarea context-profile-editor-instruction"
                value={instruction}
                maxLength={1200}
                placeholder="Describe how Grain should write in this profile"
                onChange={(event) => setInstruction(event.target.value)}
              />
            </label>

            <div
              className="context-profile-editor-field"
              role="group"
              aria-labelledby="context-profile-targets-label"
            >
              <span id="context-profile-targets-label">
                Applications and websites
              </span>
              {targets.length > 0 && (
                <div className="context-profile-target-list">
                  {targets.map((target) => {
                    return (
                      <span
                        key={`${target.kind}:${target.value}`}
                        className="context-profile-target"
                      >
                        <span
                          className="context-profile-target-icon"
                          aria-hidden="true"
                        >
                          {/* Both resolve against the same cache the pill uses,
                              so a target added here shows the very icon the pill
                              will show while dictating into it — which is also
                              the confirmation that Grain understood it. */}
                          {target.kind === "website" ? (
                            <SiteIcon host={target.value} fallback={Globe2} />
                          ) : (
                            <AppIcon target={target.value} />
                          )}
                        </span>
                        <span>{target.value}</span>
                        <button
                          type="button"
                          aria-label={`Remove ${target.value}`}
                          onClick={() =>
                            setTargets((current) =>
                              current.filter(
                                (item) =>
                                  !(
                                    item.kind === target.kind &&
                                    item.value === target.value
                                  ),
                              ),
                            )
                          }
                        >
                          <X size={12} aria-hidden="true" />
                        </button>
                      </span>
                    );
                  })}
                </div>
              )}

              {targetKind === "application" ? (
                <AppPicker
                  chosen={chosenApps}
                  onPick={(value) => addTarget("application", value)}
                  onCancel={() => setTargetKind(null)}
                />
              ) : targetKind === "website" ? (
                <div className="context-profile-target-entry">
                  <Globe2 size={15} aria-hidden="true" />
                  <input
                    autoFocus
                    className="dictionary-dialog-input"
                    value={targetValue}
                    aria-label="Website address"
                    placeholder="example.com"
                    onChange={(event) => setTargetValue(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        addTarget();
                      }
                    }}
                  />
                  <button
                    className="context-profile-target-add"
                    type="button"
                    disabled={!targetValue.trim()}
                    onClick={() => addTarget()}
                  >
                    Add
                  </button>
                  <button
                    className="context-profile-target-cancel"
                    type="button"
                    aria-label="Cancel adding target"
                    onClick={() => {
                      setTargetKind(null);
                      setTargetValue("");
                    }}
                  >
                    <X size={14} aria-hidden="true" />
                  </button>
                </div>
              ) : (
                <div className="context-profile-target-actions">
                  <button
                    type="button"
                    onClick={() => setTargetKind("application")}
                  >
                    <AppWindow size={14} aria-hidden="true" />
                    Add application
                  </button>
                  <button
                    type="button"
                    onClick={() => setTargetKind("website")}
                  >
                    <Globe2 size={14} aria-hidden="true" />
                    Add website
                  </button>
                </div>
              )}
            </div>
          </div>

          <div className="dictionary-dialog-actions">
            {editing && (
              <button
                className="context-profile-delete"
                type="button"
                onClick={() => {
                  onDelete(state.profile.id);
                  onClose();
                }}
              >
                Delete
              </button>
            )}
            <button
              className="dictionary-save-button"
              type="submit"
              disabled={
                !title.trim() || !instruction.trim() || targets.length === 0
              }
            >
              {editing ? "Save changes" : "Create profile"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

/** [GRAIN] Context awareness settings rendered below the page-level feature
 * switch. The public profiles are a frontend-only presentation layer for now.
 *
 * The automatic SOFT tone/vocabulary layer is the feature itself and has no
 * setting; what is configurable is how much it may read. HARD per-app formatting
 * is not here either — it is what the App Modes extension does, in its own
 * storage and its own transform hook, anchored directly below. */
export const ContextAwareSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const nearbyTerms = getSetting("context_nearby_terms") ?? false;
  const caretText = getSetting("context_caret_text") ?? false;
  const [activeProfileId, setActiveProfileId] =
    useState<ContextProfileId>("email");
  const [mode, setMode] = useState<"read" | "write">("read");
  /** Backend truth, keyed by profile id. Rust owns this text — the shipped
   *  default AND the user's edit — so that what this editor shows is provably
   *  the string the model receives. Undefined until the first load resolves. */
  const [profileInfo, setProfileInfo] = useState<
    Partial<Record<ContextProfileId, ContextProfileInfo>>
  >({});
  /** What is in the textarea right now. Separate from `profileInfo` because a
   *  half-typed instruction must not be written to settings on every keystroke;
   *  it is persisted on blur and when leaving the profile. */
  const [drafts, setDrafts] = useState<
    Partial<Record<ContextProfileId, string>>
  >({});

  const loadProfiles = useCallback(async () => {
    const rows = await commands.contextProfiles();
    const byId = Object.fromEntries(
      rows.map((row) => [row.id, row]),
    ) as Partial<Record<ContextProfileId, ContextProfileInfo>>;
    setProfileInfo(byId);
    setDrafts(
      Object.fromEntries(
        rows.map((row) => [row.id, row.instruction]),
      ) as Partial<Record<ContextProfileId, string>>,
    );
  }, []);

  const [customProfiles, setCustomProfiles] = useState<CustomContextProfile[]>(
    [],
  );

  const loadCustomProfiles = useCallback(async () => {
    setCustomProfiles(await commands.contextCustomProfiles());
  }, []);

  useEffect(() => {
    void loadProfiles();
    void loadCustomProfiles();
  }, [loadCustomProfiles, loadProfiles]);
  const [customProfileDialog, setCustomProfileDialog] =
    useState<CustomProfileDialogState | null>(null);
  const activeProfile =
    CONTEXT_PROFILES.find((profile) => profile.id === activeProfileId) ??
    CONTEXT_PROFILES[0];
  const instructionEditorRef = useRef<HTMLTextAreaElement>(null);
  const fitInstructionEditor = useCallback(
    (editor: HTMLTextAreaElement | null) => {
      if (!editor) return;
      editor.style.height = "auto";
      const overflowing = editor.scrollHeight > INSTRUCTION_EDITOR_MAX_HEIGHT;
      editor.style.height = `${Math.min(
        editor.scrollHeight,
        INSTRUCTION_EDITOR_MAX_HEIGHT,
      )}px`;
      editor.style.overflowY = overflowing ? "auto" : "hidden";
    },
    [],
  );

  useLayoutEffect(() => {
    if (mode === "write") fitInstructionEditor(instructionEditorRef.current);
  }, [activeProfileId, fitInstructionEditor, drafts, mode]);

  /** Write one profile's instruction back, then re-read. The re-read is what
   *  keeps `edited` honest: the backend clears the override when the text
   *  matches the shipped default, so it — not this component — decides whether
   *  a profile counts as customised. */
  const persist = useCallback(
    async (id: ContextProfileId, text: string) => {
      if (id === "other") return; // not a real profile; nothing to store
      if (text === profileInfo[id]?.instruction) return; // unchanged
      const result = await commands.setContextProfileInstruction(id, text);
      if (result.status === "error") {
        // Put the stored text back rather than leaving the editor showing an
        // edit that was never saved.
        setDrafts((current) => ({
          ...current,
          [id]: profileInfo[id]?.instruction ?? "",
        }));
        return;
      }
      await loadProfiles();
    },
    [loadProfiles, profileInfo],
  );

  const activeInfo =
    activeProfileId === "other" ? undefined : profileInfo[activeProfileId];
  const activeDraft = drafts[activeProfileId] ?? "";

  /** Write the whole set back, then re-read. The backend normalises targets —
   *  a pasted path becomes an exe stem, a pasted URL becomes a bare host — so
   *  the list must come back FROM it rather than be assumed, or the editor
   *  would show `https://figma.com/files` for a target stored as `figma.com`. */
  const saveCustomProfiles = useCallback(
    async (next: CustomContextProfile[]) => {
      const result = await commands.updateContextCustomProfiles(next);
      if (result.status === "error") return;
      await loadCustomProfiles();
    },
    [loadCustomProfiles],
  );

  const saveCustomProfile = (profile: CustomContextProfile) => {
    const exists = customProfiles.some((item) => item.id === profile.id);
    void saveCustomProfiles(
      exists
        ? customProfiles.map((item) =>
            item.id === profile.id ? profile : item,
          )
        : [...customProfiles, profile],
    );
  };

  const deleteCustomProfile = (id: string) => {
    void saveCustomProfiles(customProfiles.filter((item) => item.id !== id));
  };

  return (
    <>
      <section
        className="context-profile-settings"
        aria-label="Context profiles"
      >
        <div
          className="context-profile-tabs"
          role="group"
          aria-label="Context profiles"
        >
          {TAB_PROFILES.map((profile) => {
            // Selecting AI chat keeps the Other tab lit, because that is where
            // you found it and where Back returns you to.
            const active =
              profile.id === activeProfileId ||
              (profile.id === "other" && activeProfileId === "ai_chat");
            const TabIcon = profile.tabIcon;
            return (
              <button
                key={profile.id}
                id={`context-profile-tab-${profile.id}`}
                className={active ? "active" : undefined}
                type="button"
                aria-pressed={active}
                aria-disabled={!profile.available || undefined}
                disabled={!profile.available}
                title={
                  profile.available
                    ? undefined
                    : "Custom profiles will be available later"
                }
                onClick={() => {
                  // Commit the profile being left before the draft it belongs
                  // to stops being the visible one.
                  if (mode === "write" && profile.id !== activeProfileId) {
                    void persist(activeProfileId, activeDraft);
                  }
                  setActiveProfileId(profile.id);
                  setMode("read");
                }}
              >
                <TabIcon size={14} strokeWidth={1.8} aria-hidden="true" />
                <span>{profile.label}</span>
              </button>
            );
          })}
        </div>

        {activeProfile.available && activeProfile.id !== "other" && (
          <div
            id={`context-profile-panel-${activeProfile.id}`}
            className="context-profile-panel"
            role="region"
            aria-labelledby={`context-profile-tab-${activeProfile.id}`}
          >
            {/* Outside the card, not inside it: this leaves the card, so sitting
                within its border read as part of the profile — and, with the
                card's own padding not applying to it, sat flush in the corner. */}
            {activeProfile.inOtherTab && (
              <button
                type="button"
                className="context-profile-back"
                aria-label="Back to Other"
                onClick={() => {
                  void persist(activeProfile.id, activeDraft);
                  setActiveProfileId("other");
                  setMode("read");
                }}
              >
                <ArrowLeft size={14} strokeWidth={2} aria-hidden="true" />
                <span>Back</span>
              </button>
            )}
            <div className="context-profile-card">
              <div className="context-profile-card-header">
                <ProfileIconStack
                  className="context-profile-apps"
                  targets={sampleTargets(profileInfo[activeProfile.id])}
                  fallback={activeProfile.tabIcon}
                />
                <div className="context-profile-copy">
                  <strong>{activeProfile.summary}</strong>
                  <span>{activeProfile.detail}</span>
                </div>
                <div
                  className="context-profile-mode"
                  role="group"
                  aria-label="Instruction mode"
                >
                  <button
                    type="button"
                    className={mode === "read" ? "active" : undefined}
                    aria-pressed={mode === "read"}
                    onClick={() => {
                      // Leaving the editor is a commit point: the textarea's blur
                      // does not fire when the toggle steals focus first.
                      void persist(activeProfile.id, activeDraft);
                      setMode("read");
                    }}
                  >
                    Read
                  </button>
                  <button
                    type="button"
                    className={mode === "write" ? "active" : undefined}
                    aria-pressed={mode === "write"}
                    onClick={() => setMode("write")}
                  >
                    Write
                  </button>
                </div>
              </div>

              <div className="context-profile-prompt">
                <span
                  id={`context-profile-instruction-label-${activeProfile.id}`}
                  className="context-profile-prompt-label"
                >
                  Instruction
                  {activeInfo?.edited && (
                    <button
                      type="button"
                      className="context-profile-reset"
                      onClick={() => {
                        const fallback = activeInfo.default_instruction;
                        setDrafts((current) => ({
                          ...current,
                          [activeProfile.id]: fallback,
                        }));
                        void persist(activeProfile.id, fallback);
                      }}
                    >
                      Reset to default
                    </button>
                  )}
                </span>
                {mode === "read" ? (
                  <p
                    id={`context-profile-prompt-${activeProfile.id}`}
                    aria-labelledby={`context-profile-instruction-label-${activeProfile.id}`}
                  >
                    {activeDraft}
                  </p>
                ) : (
                  <textarea
                    id={`context-profile-prompt-${activeProfile.id}`}
                    ref={instructionEditorRef}
                    autoFocus
                    value={activeDraft}
                    aria-labelledby={`context-profile-tab-${activeProfile.id} context-profile-instruction-label-${activeProfile.id}`}
                    onChange={(event) => {
                      const next = event.target.value;
                      setDrafts((current) => ({
                        ...current,
                        [activeProfile.id]: next,
                      }));
                    }}
                    onBlur={() => void persist(activeProfile.id, activeDraft)}
                  />
                )}
              </div>
            </div>
          </div>
        )}

        {activeProfile.id === "other" && (
          <div
            className="context-custom-profile-grid"
            role="region"
            aria-labelledby="context-profile-tab-other"
          >
            {OTHER_TAB_PROFILES.map((profile) => {
              const CardIcon = profile.tabIcon;
              return (
                <button
                  key={profile.id}
                  className="context-custom-profile-card"
                  type="button"
                  aria-label={`Edit the ${profile.label} profile`}
                  onClick={() => setActiveProfileId(profile.id)}
                >
                  <ProfileIconStack
                    targets={sampleTargets(profileInfo[profile.id])}
                    fallback={CardIcon}
                  />
                  <span className="context-custom-profile-create-copy">
                    <strong>{profile.label}</strong>
                    <span className="context-custom-profile-instruction">
                      {profileInfo[profile.id]?.instruction ?? profile.detail}
                    </span>
                  </span>
                </button>
              );
            })}

            <button
              className="context-custom-profile-card context-custom-profile-create"
              type="button"
              onClick={() => setCustomProfileDialog({ mode: "create" })}
            >
              <span className="context-custom-profile-create-icon">
                <Plus size={18} aria-hidden="true" />
              </span>
              <span className="context-custom-profile-create-copy">
                <strong>Create profile</strong>
                <span>
                  Add a title, instruction, and the applications or websites it
                  applies to.
                </span>
              </span>
            </button>

            {customProfiles.map((profile) => (
              <button
                key={profile.id}
                className="context-custom-profile-card"
                type="button"
                aria-label={`Edit ${profile.title} profile`}
                onClick={() =>
                  setCustomProfileDialog({
                    mode: "edit",
                    profile: {
                      ...profile,
                      targets: (profile.targets ?? []).map((target) => ({
                        ...target,
                      })),
                    },
                  })
                }
              >
                <ProfileIconStack
                  targets={
                    (profile.targets ??
                      []) as ReadonlyArray<CustomProfileTarget>
                  }
                />
                <span className="context-custom-profile-create-copy">
                  <strong>{profile.title}</strong>
                  <span className="context-custom-profile-instruction">
                    {profile.instruction}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </section>

      <div className="context-profile-options" aria-label="Context sources">
        <ToggleSwitch
          label="Fit text to the cursor"
          description="Read only the nearest useful sentence fragment around your cursor so mid-sentence dictation flows naturally. Sends at most 200 characters before and 80 after; when both sides are empty, no cursor hint is sent. Never stored, password fields skipped."
          descriptionMode="tooltip"
          grouped
          checked={caretText}
          isUpdating={isUpdating("context_caret_text")}
          onChange={(v) => updateSetting("context_caret_text", v)}
        />
        {/* These remain separate because nearby-term hints promise to send only
            unique tokens, while cursor fitting sends a short raw excerpt. */}
        <ToggleSwitch
          label="Nearby-term hints (silent)"
          description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and use them to improve accuracy — both as a spelling hint for the AI and to bias the recognizer itself. Never sends raw text, never stored, password fields skipped."
          descriptionMode="tooltip"
          grouped
          checked={nearbyTerms}
          isUpdating={isUpdating("context_nearby_terms")}
          onChange={(v) => updateSetting("context_nearby_terms", v)}
        />
      </div>

      {customProfileDialog && (
        <CustomProfileDialog
          key={
            customProfileDialog.mode === "edit"
              ? customProfileDialog.profile.id
              : "create-profile"
          }
          state={customProfileDialog}
          onClose={() => setCustomProfileDialog(null)}
          onSave={saveCustomProfile}
          onDelete={deleteCustomProfile}
        />
      )}
    </>
  );
};
