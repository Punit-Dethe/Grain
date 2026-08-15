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
  AtSign,
  Bot,
  BriefcaseBusiness,
  Code2,
  Globe2,
  ListChecks,
  Mail,
  MessageCircle,
  MessagesSquare,
  Plus,
  Send,
  Shapes,
  Smartphone,
  SquareTerminal,
  Users,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  commands,
  type ContextProfileInfo,
  type CustomContextProfile,
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
  tabIcon: LucideIcon;
  summary: string;
  detail: string;
  applications: ReadonlyArray<{ name: string; icon: LucideIcon }>;
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
    applications: [
      { name: "Email", icon: Mail },
      { name: "Webmail", icon: AtSign },
      { name: "Mail composer", icon: Send },
    ],
    available: true,
  },
  {
    id: "work",
    label: "Work",
    tabIcon: BriefcaseBusiness,
    summary: "This profile applies in work applications",
    detail: "Grain stays concise across team chat, tickets, and tasks.",
    applications: [
      { name: "Work chat", icon: MessagesSquare },
      { name: "Project workspace", icon: BriefcaseBusiness },
      { name: "Issue tracker", icon: ListChecks },
    ],
    available: true,
  },
  {
    id: "casual",
    label: "Casual",
    tabIcon: MessageCircle,
    summary: "This profile applies in casual applications",
    detail: "Grain protects the phrasing and personality of everyday messages.",
    applications: [
      { name: "Messages", icon: MessageCircle },
      { name: "Mobile messenger", icon: Smartphone },
      { name: "Social conversation", icon: Users },
    ],
    available: true,
  },
  {
    id: "technical",
    label: "Technical",
    tabIcon: Code2,
    summary: "This profile applies in technical applications",
    detail: "Grain preserves exact syntax in editors, terminals, and AI tools.",
    applications: [
      { name: "Code editor", icon: Code2 },
      { name: "Terminal", icon: SquareTerminal },
      { name: "AI assistant", icon: Bot },
    ],
    available: true,
  },
  {
    id: "ai_chat",
    label: "AI chat",
    tabIcon: Bot,
    summary: "This profile applies in AI assistants",
    detail:
      "Grain writes your prompt into the box instead of answering it, and leaves your specifics alone.",
    applications: [
      { name: "AI assistant", icon: Bot },
      { name: "Chat website", icon: Globe2 },
      { name: "Prompt box", icon: MessageCircle },
    ],
    available: true,
    inOtherTab: true,
  },
  {
    id: "other",
    label: "Other",
    tabIcon: Shapes,
    summary: "",
    detail: "",
    applications: [],
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

function CustomProfileTargetStack({
  targets,
}: {
  targets: CustomProfileTarget[];
}) {
  return (
    <div className="context-custom-profile-icons" aria-hidden="true">
      {targets.slice(0, 3).map((target) => {
        const Icon = target.kind === "website" ? Globe2 : AppWindow;
        return (
          <span key={`${target.kind}:${target.value}`} title={target.value}>
            <Icon size={15} strokeWidth={1.8} />
          </span>
        );
      })}
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

  const addTarget = () => {
    const value = targetValue.trim();
    if (!value || !targetKind) return;
    const duplicate = targets.some(
      (target) =>
        target.kind === targetKind &&
        target.value.toLocaleLowerCase() === value.toLocaleLowerCase(),
    );
    if (!duplicate) {
      setTargets((current) => [...current, { kind: targetKind, value }]);
    }
    setTargetValue("");
    setTargetKind(null);
  };

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
                    const Icon = target.kind === "website" ? Globe2 : AppWindow;
                    return (
                      <span
                        key={`${target.kind}:${target.value}`}
                        className="context-profile-target"
                      >
                        <Icon size={13} aria-hidden="true" />
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

              {targetKind ? (
                <div className="context-profile-target-entry">
                  {targetKind === "website" ? (
                    <Globe2 size={15} aria-hidden="true" />
                  ) : (
                    <AppWindow size={15} aria-hidden="true" />
                  )}
                  <input
                    autoFocus
                    className="dictionary-dialog-input"
                    value={targetValue}
                    aria-label={
                      targetKind === "website"
                        ? "Website address"
                        : "Application name"
                    }
                    placeholder={
                      targetKind === "website"
                        ? "example.com"
                        : "Application name"
                    }
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
                    onClick={addTarget}
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
            className="context-profile-card"
            role="region"
            aria-labelledby={`context-profile-tab-${activeProfile.id}`}
          >
            <div className="context-profile-card-header">
              <div className="context-profile-apps" aria-hidden="true">
                {activeProfile.applications.map(({ name, icon: Icon }) => (
                  <span key={name} title={name}>
                    <Icon size={15} strokeWidth={1.8} />
                  </span>
                ))}
              </div>
              <div className="context-profile-copy">
                <strong>{activeProfile.summary}</strong>
                <span>{activeProfile.detail}</span>
                {activeProfile.inOtherTab && (
                  <button
                    type="button"
                    className="context-profile-back"
                    onClick={() => {
                      void persist(activeProfile.id, activeDraft);
                      setActiveProfileId("other");
                      setMode("read");
                    }}
                  >
                    Back to Other
                  </button>
                )}
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
                  <div
                    className="context-custom-profile-icons"
                    aria-hidden="true"
                  >
                    <span>
                      <CardIcon size={15} strokeWidth={1.8} />
                    </span>
                  </div>
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
                <CustomProfileTargetStack
                  targets={(profile.targets ?? []) as CustomProfileTarget[]}
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
          description="Read a short span of text either side of your cursor so dictation inserted mid-sentence flows: correct spacing, no stray capital, no repeated words. Sends that excerpt (up to ~320 characters each side) to your AI provider along with the transcript. Never stored, password fields skipped."
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
