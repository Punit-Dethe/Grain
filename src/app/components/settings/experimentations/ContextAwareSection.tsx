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
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

type ContextProfileId = "email" | "work" | "casual" | "technical" | "other";

const INSTRUCTION_EDITOR_MAX_HEIGHT = 184;

type ContextProfile = {
  id: ContextProfileId;
  label: string;
  tabIcon: LucideIcon;
  summary: string;
  detail: string;
  applications: ReadonlyArray<{ name: string; icon: LucideIcon }>;
  instruction: string;
  available: boolean;
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
    instruction:
      "Polish for professional email. Keep the user's meaning and structure; add no subject, greeting, sign-off, or email layout unless dictated.",
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
    instruction:
      "Keep work messages professional, concise, and conversational. Preserve names and technical terms; add no greeting, pleasantries, or formal paragraph structure unless dictated.",
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
    instruction:
      "Keep the user's own voice, slang, and phrasing. Use light cleanup only; add no hashtags, emoji, or formality unless dictated.",
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
    instruction:
      "Treat this as technical writing, code, a command, or an AI instruction. Preserve identifiers, flags, paths, casing, and exact intent; for commands, add no sentence casing or trailing period.",
    available: true,
  },
  {
    id: "other",
    label: "Other",
    tabIcon: Shapes,
    summary: "",
    detail: "",
    applications: [],
    instruction: "",
    available: true,
  },
];

type CustomProfileTarget = {
  id: string;
  kind: "application" | "website";
  label: string;
};

type CustomContextProfile = {
  id: string;
  title: string;
  instruction: string;
  targets: CustomProfileTarget[];
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
          <span key={target.id} title={target.label}>
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
}: {
  state: CustomProfileDialogState;
  onClose: () => void;
  onSave: (profile: CustomContextProfile) => void;
}) {
  const editing = state.mode === "edit";
  const [title, setTitle] = useState(editing ? state.profile.title : "");
  const [instruction, setInstruction] = useState(
    editing ? state.profile.instruction : "",
  );
  const [targets, setTargets] = useState<CustomProfileTarget[]>(
    editing ? state.profile.targets.map((target) => ({ ...target })) : [],
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
    const label = targetValue.trim();
    if (!label || !targetKind) return;
    const duplicate = targets.some(
      (target) =>
        target.kind === targetKind &&
        target.label.toLocaleLowerCase() === label.toLocaleLowerCase(),
    );
    if (!duplicate) {
      setTargets((current) => [
        ...current,
        { id: crypto.randomUUID(), kind: targetKind, label },
      ]);
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
                      <span key={target.id} className="context-profile-target">
                        <Icon size={13} aria-hidden="true" />
                        <span>{target.label}</span>
                        <button
                          type="button"
                          aria-label={`Remove ${target.label}`}
                          onClick={() =>
                            setTargets((current) =>
                              current.filter((item) => item.id !== target.id),
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
  const [instructions, setInstructions] = useState<
    Record<ContextProfileId, string>
  >(
    () =>
      Object.fromEntries(
        CONTEXT_PROFILES.map((profile) => [profile.id, profile.instruction]),
      ) as Record<ContextProfileId, string>,
  );
  const [customProfiles, setCustomProfiles] = useState<CustomContextProfile[]>(
    [],
  );
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
  }, [activeProfileId, fitInstructionEditor, instructions, mode]);

  const saveCustomProfile = (profile: CustomContextProfile) => {
    setCustomProfiles((current) => {
      const existing = current.findIndex((item) => item.id === profile.id);
      if (existing < 0) return [...current, profile];
      return current.map((item) => (item.id === profile.id ? profile : item));
    });
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
          {CONTEXT_PROFILES.map((profile) => {
            const active = profile.id === activeProfileId;
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
                  onClick={() => setMode("read")}
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
              </span>
              {mode === "read" ? (
                <p
                  id={`context-profile-prompt-${activeProfile.id}`}
                  aria-labelledby={`context-profile-instruction-label-${activeProfile.id}`}
                >
                  {instructions[activeProfile.id]}
                </p>
              ) : (
                <textarea
                  id={`context-profile-prompt-${activeProfile.id}`}
                  ref={instructionEditorRef}
                  autoFocus
                  value={instructions[activeProfile.id]}
                  aria-labelledby={`context-profile-tab-${activeProfile.id} context-profile-instruction-label-${activeProfile.id}`}
                  onChange={(event) =>
                    setInstructions((current) => ({
                      ...current,
                      [activeProfile.id]: event.target.value,
                    }))
                  }
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
                      targets: profile.targets.map((target) => ({ ...target })),
                    },
                  })
                }
              >
                <CustomProfileTargetStack targets={profile.targets} />
                <strong>{profile.title}</strong>
                <span className="context-custom-profile-instruction">
                  {profile.instruction}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>

      <div className="context-profile-options" aria-label="Context sources">
        <ToggleSwitch
          label="Nearby-term hints (silent)"
          description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and use them to improve accuracy — both as a spelling hint for the AI and to bias the recognizer itself. Never sends raw text, never stored, password fields skipped."
          descriptionMode="tooltip"
          grouped
          checked={nearbyTerms}
          isUpdating={isUpdating("context_nearby_terms")}
          onChange={(v) => updateSetting("context_nearby_terms", v)}
        />
        {/* Deliberately a second switch rather than part of the one above: that
            one promises to send only unique tokens and never raw text, and this
            one sends a short raw excerpt of what surrounds the cursor. Folding
            them together would quietly break the narrower promise. */}
        <ToggleSwitch
          label="Fit text to the cursor"
          description="Read a short span of text either side of your cursor so dictation inserted mid-sentence flows: correct spacing, no stray capital, no repeated words. Sends that excerpt (up to ~320 characters each side) to your AI provider along with the transcript. Never stored, password fields skipped."
          descriptionMode="tooltip"
          grouped
          checked={caretText}
          isUpdating={isUpdating("context_caret_text")}
          onChange={(v) => updateSetting("context_caret_text", v)}
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
        />
      )}
    </>
  );
};
