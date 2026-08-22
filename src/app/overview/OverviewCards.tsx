import { useMemo, useState } from "react";
import { ArrowRight, Blocks, BookOpen, Keyboard, Plus } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { formatKeyPart } from "@/lib/utils/keyboard";
import { MAX_CUSTOM_WORD_LENGTH, normalizeCustomWord } from "@/lib/customWords";
import { hashForRoute } from "../navigation";

const COPY = {
  extensions: {
    title: "Extensions",
    body: "Supercharge your workflow with voice actions, modes, and tools.",
    explore: "Browse extensions",
  },
  dictionary: {
    title: "Dictionary",
    body: "Add unique words, names, or jargon Grain should always recognize.",
    placeholder: "Add custom word...",
    add: "Add word",
  },
  shortcuts: {
    title: "Shortcuts",
    body: "Your quick keys to start dictating.",
    aiOff: "AI off",
    none: "Not set",
  },
  agent: {
    title: "Agent",
    body: "Select any text on screen and summon AI to rewrite or summarize.",
    bodyOff: "Select any text on screen and summon AI to rewrite or summarize.",
    shortcutLabel: "Shortcut",
    enable: "Turn on in Studio",
  },
};

/** Where the AI key routes a transcript. Mirrors `CaptureModes`. */
const AI_BINDING_ID = "transcribe_send_to_ai";
/** Flow. In a three-key setup this is the capture the AI key pairs with. */
const FLOW_BINDING_ID = "transcribe_realtime";
const AGENT_BINDING_ID = "summon_agent";

function go(hash: string) {
  window.location.hash = hash.slice(1);
}

/**
 * A chord as one keycap per key.
 */
function Keycap({ combination }: { combination: string }) {
  const keys = combination
    .split("+")
    .map((part) => formatKeyPart(part))
    .filter(Boolean);
  return (
    <span className="overview-keys">
      {keys.map((key, index) => (
        <kbd className="overview-key" key={`${key}-${index}`}>
          {key}
        </kbd>
      ))}
    </span>
  );
}

/**
 * One `label — keycap` line in the Shortcuts card.
 */
function ShortcutRow({
  label,
  combination,
  offLabel,
}: {
  label: string;
  combination: string | null;
  offLabel?: string;
}) {
  return (
    <div className="overview-shortcut-row">
      <span className="overview-shortcut-label">{label}</span>
      {combination ? (
        <Keycap combination={combination} />
      ) : (
        <span className="overview-shortcut-off">
          {offLabel ?? COPY.shortcuts.none}
        </span>
      )}
    </div>
  );
}

function ExtensionsCard() {
  return (
    <button
      className="overview-card overview-card--interactive"
      type="button"
      onClick={() => go(hashForRoute({ page: "extensions", view: "store" }))}
    >
      <div className="overview-card-header">
        <Blocks className="overview-card-icon" size={18} strokeWidth={1.8} />
        <strong className="overview-card-title">{COPY.extensions.title}</strong>
      </div>
      <p className="overview-card-desc">{COPY.extensions.body}</p>
      <div className="overview-card-footer">
        <div className="overview-cta-row">
          <span className="overview-card-cta">{COPY.extensions.explore}</span>
          <ArrowRight size={13} className="overview-cta-arrow" aria-hidden="true" />
        </div>
      </div>
    </button>
  );
}

function DictionaryCard() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [word, setWord] = useState("");
  const words = useMemo(() => getSetting("custom_words") ?? [], [getSetting]);
  const busy = isUpdating("custom_words");

  const submit = () => {
    const candidate = normalizeCustomWord(word);
    if (!candidate || candidate.length > MAX_CUSTOM_WORD_LENGTH) return;
    if (words.includes(candidate)) {
      toast.error(`"${candidate}" is already in your dictionary.`);
      return;
    }
    void updateSetting("custom_words", [...words, candidate]);
    setWord("");
    toast.success(`Added "${candidate}" to your dictionary.`);
  };

  return (
    <div className="overview-card overview-card--static">
      <div className="overview-card-header">
        <BookOpen className="overview-card-icon" size={18} strokeWidth={1.8} />
        <strong className="overview-card-title">{COPY.dictionary.title}</strong>
      </div>
      <p className="overview-card-desc">{COPY.dictionary.body}</p>
      <div className="overview-card-footer">
        <div className="overview-add-row">
          <input
            className="overview-add-input"
            value={word}
            spellCheck={false}
            maxLength={MAX_CUSTOM_WORD_LENGTH}
            disabled={busy}
            placeholder={COPY.dictionary.placeholder}
            aria-label={COPY.dictionary.add}
            onChange={(event) => setWord(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                submit();
              }
            }}
          />
          <button
            className="overview-add-btn"
            type="button"
            title={COPY.dictionary.add}
            aria-label={COPY.dictionary.add}
            disabled={busy || word.trim().length === 0}
            onClick={submit}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
        </div>
      </div>
    </div>
  );
}

function ShortcutsCard() {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const bindings = getSetting("bindings") ?? {};

  const captureId = FLOW_BINDING_ID;
  const capture = bindings[captureId];
  const ai = bindings[AI_BINDING_ID];
  const aiActive = (getSetting("post_process_enabled") ?? false) && Boolean(ai);

  const label = (id: string, fallback?: string) =>
    t(`settings.general.shortcut.bindings.${id}.name`, fallback ?? id);

  return (
    <div className="overview-card overview-card--static">
      <div className="overview-card-header">
        <Keyboard className="overview-card-icon" size={18} strokeWidth={1.8} />
        <strong className="overview-card-title">{COPY.shortcuts.title}</strong>
      </div>
      <p className="overview-card-desc">{COPY.shortcuts.body}</p>
      <div className="overview-shortcut-list">
        <ShortcutRow
          label={label(captureId, capture?.name)}
          combination={capture?.current_binding || null}
        />
        <ShortcutRow
          label={label(AI_BINDING_ID, ai?.name)}
          combination={aiActive ? ai?.current_binding || null : null}
          offLabel={aiActive ? undefined : COPY.shortcuts.aiOff}
        />
      </div>
    </div>
  );
}

function AgentCard() {
  const { getSetting } = useSettings();
  const enabled = getSetting("agent_enabled") ?? false;
  const binding = (getSetting("bindings") ?? {})[AGENT_BINDING_ID];
  const combination = enabled ? binding?.current_binding || null : null;

  return (
    <button
      className="overview-card overview-card--interactive"
      type="button"
      onClick={() => go(hashForRoute({ page: "tools", section: "agent" }))}
    >
      <div className="overview-card-header">
        <AgentGlyph />
        <strong className="overview-card-title">{COPY.agent.title}</strong>
      </div>
      <p className="overview-card-desc">
        {enabled ? COPY.agent.body : COPY.agent.bodyOff}
      </p>
      <div className="overview-card-footer">
        {combination ? (
          <div className="overview-shortcut-row overview-agent-row">
            <span className="overview-shortcut-label">{COPY.agent.shortcutLabel}</span>
            <Keycap combination={combination} />
          </div>
        ) : (
          <div className="overview-cta-row">
            <span className="overview-card-cta">{COPY.agent.enable}</span>
            <ArrowRight size={13} className="overview-cta-arrow" aria-hidden="true" />
          </div>
        )}
      </div>
    </button>
  );
}

/** The prototype's Agent face — clean inline SVG glyph. */
function AgentGlyph() {
  return (
    <svg viewBox="0 0 24 24" className="overview-card-icon overview-agent-svg" aria-hidden="true">
      <rect x="5.25" y="6" width="13.5" height="10.5" rx="4.25" />
      <circle cx="10" cy="11.25" r="1" />
      <circle cx="14" cy="11.25" r="1" />
      <path d="M10 14.1c.58.5 1.27.75 2 .75s1.42-.25 2-.75M12 6V3.9M8.75 18.2 7.5 20.1M15.25 18.2l1.25 1.9" />
    </svg>
  );
}


export function OverviewCards() {
  return (
    <div className="overview-grid">
      <ShortcutsCard />
      <DictionaryCard />
      <AgentCard />
      <ExtensionsCard />
    </div>
  );
}
