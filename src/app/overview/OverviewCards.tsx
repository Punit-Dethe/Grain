import { ArrowRight, BookOpen, FileText, Keyboard } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { formatKeyPart } from "@/lib/utils/keyboard";
import { getFlowAvailability } from "@/lib/flowAvailability";
import { useModelStore } from "@/stores/modelStore";
import { hashForRoute } from "../navigation";

const COPY = {
  snippets: {
    title: "Snippets",
    body: "Expand a short spoken phrase into text you use often.",
    explore: "Manage snippets",
  },
  dictionary: {
    title: "Dictionary",
    body: "Teach Grain the names and terms you use.",
    explore: "Go to dictionary",
  },
  shortcuts: {
    title: "Shortcuts",
    body: "Your quick keys to start dictating.",
    aiOff: "AI off",
    flowOff: "Unavailable",
    none: "Not set",
  },
  agent: {
    title: "Agent",
    body: "Rewrite or summarize selected text right where you work.",
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

/** A shortcut chord without keycap boxes. */
function Keycap({ combination }: { combination: string }) {
  const keys = combination
    .split("+")
    .map((part) => formatKeyPart(part))
    .filter(Boolean);
  return (
    <kbd className="overview-keys">
      {keys.map((key, index) => (
        <span className="overview-key-part" key={`${key}-${index}`}>
          {index > 0 && <span className="overview-key-separator">+</span>}
          {key}
        </span>
      ))}
    </kbd>
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

function SnippetsCard() {
  return (
    <button
      className="overview-card overview-card--interactive"
      type="button"
      onClick={() => go(hashForRoute({ page: "tools", section: "snippets" }))}
    >
      <div className="overview-card-header">
        <FileText className="overview-card-icon" size={18} strokeWidth={1.8} />
        <strong className="overview-card-title">{COPY.snippets.title}</strong>
      </div>
      <p className="overview-card-desc">{COPY.snippets.body}</p>
      <div className="overview-card-footer">
        <div className="overview-cta-row">
          <span className="overview-card-cta">{COPY.snippets.explore}</span>
          <ArrowRight
            size={13}
            className="overview-cta-arrow"
            aria-hidden="true"
          />
        </div>
      </div>
    </button>
  );
}

function DictionaryCard() {
  return (
    <button
      className="overview-card overview-card--interactive"
      type="button"
      onClick={() => go(hashForRoute({ page: "tools", section: "dictionary" }))}
    >
      <div className="overview-card-header">
        <BookOpen className="overview-card-icon" size={18} strokeWidth={1.8} />
        <strong className="overview-card-title">{COPY.dictionary.title}</strong>
      </div>
      <p className="overview-card-desc">{COPY.dictionary.body}</p>
      <div className="overview-card-footer">
        <div className="overview-cta-row">
          <span className="overview-card-cta">{COPY.dictionary.explore}</span>
          <ArrowRight
            size={13}
            className="overview-cta-arrow"
            aria-hidden="true"
          />
        </div>
      </div>
    </button>
  );
}

function ShortcutsCard() {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const bindings = getSetting("bindings") ?? {};
  const { models, currentModel } = useModelStore();

  const captureId = FLOW_BINDING_ID;
  const capture = bindings[captureId];
  const ai = bindings[AI_BINDING_ID];
  const aiActive = (getSetting("post_process_enabled") ?? false) && Boolean(ai);
  const flowAvailable = getFlowAvailability(
    models,
    currentModel,
    getSetting("translate_to_english") ?? false,
  ).available;

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
          combination={flowAvailable ? capture?.current_binding || null : null}
          offLabel={flowAvailable ? undefined : COPY.shortcuts.flowOff}
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
      <p className="overview-card-desc">{COPY.agent.body}</p>
      <div className="overview-card-footer">
        {combination ? (
          <div className="overview-shortcut-row overview-agent-row">
            <span className="overview-shortcut-label">
              {COPY.agent.shortcutLabel}
            </span>
            <Keycap combination={combination} />
          </div>
        ) : (
          <div className="overview-cta-row">
            <span className="overview-card-cta">{COPY.agent.enable}</span>
            <ArrowRight
              size={13}
              className="overview-cta-arrow"
              aria-hidden="true"
            />
          </div>
        )}
      </div>
    </button>
  );
}

/** The prototype's Agent face — clean inline SVG glyph. */
function AgentGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="overview-card-icon overview-agent-svg"
      aria-hidden="true"
    >
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
      <AgentCard />
      <DictionaryCard />
      <SnippetsCard />
    </div>
  );
}
