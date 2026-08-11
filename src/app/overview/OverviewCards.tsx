/**
 * The four cards under the Overview hero.
 *
 * They replace the inert prototype tiles, and they are deliberately not four of
 * the same thing — each earns its slot a different way:
 *
 * In render order:
 *
 * | Card       | Kind          | Affordance                              |
 * |------------|---------------|-----------------------------------------|
 * | Shortcuts  | informational | READ-ONLY reference, not clickable      |
 * | Dictionary | interactive   | inline add field, never leaves the page |
 * | Agent      | promotional   | surfaces the summon key → Studio        |
 * | Extensions | navigational  | whole card is a button → store          |
 *
 * The Shortcuts card is a reference, not a control: it is a `<div>`, has no
 * hover lift and no pointer cursor, and carries no link, so nothing about it
 * invites a click. Rebinding lives in Settings.
 *
 * Every card tells the truth about state. A shortcut that is not currently
 * registered (AI keys with post-processing off, the summon key with Agent off)
 * renders as an explicit off-state with the fix one click away — never as a
 * keycap the user could press to no effect.
 */
import { useMemo, useState } from "react";
import { Blocks, BookOpen, ChevronRight, Keyboard, Plus } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { formatKeyPart } from "@/lib/utils/keyboard";
import { hashForRoute } from "../navigation";

const COPY = {
  extensions: {
    title: "Extensions",
    body: "Teach Grain new tricks.",
    // Two extensions that actually ship, so the chips are examples rather than
    // a promise. They double as the card's bottom row, which is what puts it on
    // the same baseline as the other three.
    examples: ["Voice Actions", "App Modes"],
  },
  dictionary: {
    title: "Dictionary",
    body: "Teach Grain a word it mishears.",
    placeholder: "Add a word",
    add: "Add word",
  },
  shortcuts: {
    title: "Shortcuts",
    aiOff: "AI off",
    none: "Not set",
  },
  agent: {
    title: "Agent",
    body: "Rewrite anything you have selected.",
    bodyOff: "Rewrite anything you have selected.",
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
 * A chord as one keycap per key, wrapping within the card rather than
 * truncating. A single wide cap of "Left Ctrl + Left Shift + Q" had to be
 * clipped to fit and spilled toward the card edge; separate caps that wrap read
 * cleanly at any width. Presentational only — never interactive.
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
 *
 * `combination` of `null` means the shortcut exists but holds no key right now;
 * the row says so rather than showing an empty cap.
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
      className="quick-card overview-card"
      type="button"
      onClick={() => go(hashForRoute({ page: "extensions", view: "store" }))}
    >
      <span className="action-icon" aria-hidden="true">
        <Blocks width={22} height={22} strokeWidth={1.7} />
      </span>
      <div className="action-copy">
        <strong>{COPY.extensions.title}</strong>
        <small>{COPY.extensions.body}</small>
        <div className="overview-chips">
          {COPY.extensions.examples.map((name) => (
            <span className="overview-chip" key={name}>
              {name}
            </span>
          ))}
        </div>
      </div>
      <ChevronRight
        className="overview-card-chev"
        size={15}
        aria-hidden="true"
      />
    </button>
  );
}

function DictionaryCard() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [word, setWord] = useState("");
  const words = useMemo(() => getSetting("custom_words") ?? [], [getSetting]);
  const busy = isUpdating("custom_words");

  // Same acceptance rule as the full Dictionary tool in Studio: one token, no
  // markup characters, 50 chars. Two front doors to one list must not disagree
  // on what the list can hold.
  const submit = () => {
    const candidate = word.trim().replace(/[<>"']/g, "");
    if (!candidate || candidate.includes(" ") || candidate.length > 50) return;
    if (words.includes(candidate)) {
      toast.error(`"${candidate}" is already in your dictionary.`);
      return;
    }
    void updateSetting("custom_words", [...words, candidate]);
    setWord("");
    toast.success(`Added "${candidate}" to your dictionary.`);
  };

  return (
    <div className="quick-card overview-card overview-card--static">
      <span className="action-icon" aria-hidden="true">
        <BookOpen width={22} height={22} strokeWidth={1.7} />
      </span>
      <div className="action-copy">
        <strong>{COPY.dictionary.title}</strong>
        <small>{COPY.dictionary.body}</small>
        <div className="overview-add-row">
          <input
            className="overview-add-input"
            value={word}
            spellCheck={false}
            maxLength={50}
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

  // Which capture key to surface. Under Single there is exactly one, so show
  // it. Under All there are three and the card only has room for one — Flow is
  // the mode this card's AI key actually pairs with, and the other two are one
  // click away in Settings.
  const modeSet = getSetting("capture_mode_set") ?? "single";
  const primary = getSetting("capture_primary_mode") ?? "transcribe";
  const captureId = modeSet === "single" ? primary : FLOW_BINDING_ID;

  const capture = bindings[captureId];
  const ai = bindings[AI_BINDING_ID];
  // The AI keys are released whenever post-processing is off — there is no
  // behaviour behind them to trigger. Reflect that instead of showing the key.
  const aiActive = (getSetting("post_process_enabled") ?? false) && Boolean(ai);

  // Both labels come from the binding itself, so renaming an action in one
  // place renames it here too — the card can never caption a key wrongly.
  const label = (id: string, fallback?: string) =>
    t(`settings.general.shortcut.bindings.${id}.name`, fallback ?? id);

  return (
    <div className="quick-card overview-card overview-card--static">
      <span className="action-icon" aria-hidden="true">
        <Keyboard width={22} height={22} strokeWidth={1.7} />
      </span>
      <div className="action-copy">
        <strong>{COPY.shortcuts.title}</strong>
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
      className="quick-card overview-card overview-card--agent"
      type="button"
      onClick={() => go(hashForRoute({ page: "tools", section: "agent" }))}
    >
      <span className="action-icon" aria-hidden="true">
        <AgentGlyph />
      </span>
      <div className="action-copy">
        <strong>{COPY.agent.title}</strong>
        <small>{enabled ? COPY.agent.body : COPY.agent.bodyOff}</small>
        {combination ? (
          <div className="overview-agent-press">
            <Keycap combination={combination} />
          </div>
        ) : (
          <span className="overview-card-cta">{COPY.agent.enable}</span>
        )}
      </div>
      <ChevronRight
        className="overview-card-chev"
        size={15}
        aria-hidden="true"
      />
    </button>
  );
}

/** The prototype's Agent face — kept so Agent stays recognisable across the app. */
function AgentGlyph() {
  return (
    <svg viewBox="0 0 24 24" className="action-svg action-icon-agent">
      <rect x="5.25" y="6" width="13.5" height="10.5" rx="4.25" />
      <circle cx="10" cy="11.25" r="1" />
      <circle cx="14" cy="11.25" r="1" />
      <path d="M10 14.1c.58.5 1.27.75 2 .75s1.42-.25 2-.75M12 6V3.9M8.75 18.2 7.5 20.1M15.25 18.2l1.25 1.9" />
    </svg>
  );
}

export function OverviewCards() {
  return (
    <div className="quick-grid overview-grid">
      <ShortcutsCard />
      <DictionaryCard />
      <AgentCard />
      <ExtensionsCard />
    </div>
  );
}
