import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentAutocopy,
  AgentContextMode,
  AgentPanelPosition,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";

const AUTOCOPY_OPTIONS: { value: AgentAutocopy; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "first", label: "First reply only" },
  { value: "all", label: "All replies" },
];

const CONTEXT_OPTIONS: { value: AgentContextMode; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "unique", label: "Unique terms only" },
  { value: "full", label: "Full field text" },
  { value: "screen", label: "Whole window text" },
];

const LOOK_OPTIONS: { value: AgentPanelPosition; label: string }[] = [
  { value: "side", label: "Side card" },
  { value: "center", label: "Center panel (beta)" },
];

/** [GRAIN] Agent settings — the rows BELOW the feature's own switch, rendered
 * inside its panel (see [`FeaturePanel`]) as one ungrouped list.
 *
 * There are no sub-headings. Six controls split across "Reply surface",
 * "Replies" and "Input & context" spent three headings naming what the rows
 * already said, and made a short list look like a long one. They read in the
 * order you meet them instead: how the reply appears, how it comes back to you,
 * how you talk to it, and what it is allowed to read. All copy lives in the
 * per-row "i" hints. */
export const AgentSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const autocopy = getSetting("agent_autocopy") ?? "first";
  const quick = getSetting("agent_quick_enabled") ?? false;
  const contextMode = getSetting("agent_context_mode") ?? "off";
  const typeToExpand = getSetting("agent_input_type_to_expand") ?? true;
  const panelPosition = getSetting("agent_panel_position") ?? "side";

  // [GRAIN] SPEC §10.2: the centre layout is a surface-variant PACK. Its
  // dropdown option exists only while that extension is installed+enabled
  // (the built-in side card is the default occupant of the slot). Disabling
  // the pack elsewhere falls the position back to "side" backend-side.
  const [centerAvailable, setCenterAvailable] = useState(true);
  useEffect(() => {
    invoke<{ id: string; enabled: boolean }[]>("extensions_overview")
      .then((cards) =>
        setCenterAvailable(
          cards.some((c) => c.id === "grain.agent-center-layout" && c.enabled),
        ),
      )
      .catch(() => setCenterAvailable(true)); // never brick the dropdown
  }, []);
  const lookOptions = centerAvailable
    ? LOOK_OPTIONS
    : LOOK_OPTIONS.filter((o) => o.value !== "center");

  return (
    <>
      {/* 1. The key that summons it. It used to live with the capture keys,
          which is where you would look for it only if you already knew Agent
          existed — it is the first thing you want after switching Agent on. */}
      <ShortcutInput shortcutId="summon_agent" grouped descriptionMode="tooltip" />

      {/* 2. Where the reply appears. "Personalize" rather than "Position": one
          of the two options is a differently shaped surface, not the same card
          moved, so this is a choice about appearance. The setting KEY stays
          `agent_panel_position` — renaming it would migrate everyone's stored
          value to say the same thing. */}
      <SettingContainer
        title="Personalize"
        description="'Side card' is the original bottom-right reply card. 'Center panel' is the sleeker center-top surface that grows with your conversation up to a maximum height, then scrolls. The center panel is still in development."
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={lookOptions}
          selectedValue={panelPosition}
          disabled={isUpdating("agent_panel_position")}
          onSelect={(v) =>
            updateSetting("agent_panel_position", v as AgentPanelPosition)
          }
        />
      </SettingContainer>

      {/* 3. …or no reply surface at all. */}
      <ToggleSwitch
        label="Quick Agent"
        description="Skip the reply card entirely: the reply is auto-pasted straight at your cursor (replacing any still-selected text), then the pill briefly offers 'ask follow-up' in case you need to keep going. Same summon shortcut."
        descriptionMode="tooltip"
        grouped
        checked={quick}
        isUpdating={isUpdating("agent_quick_enabled")}
        onChange={(v) => updateSetting("agent_quick_enabled", v)}
      />

      {/* 4. How replies come back to you. */}
      <SettingContainer
        title="Auto-copy replies"
        description="Copy the Agent's replies to your clipboard as they arrive: only the first reply of a session, every reply (including retries and follow-ups), or never."
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={AUTOCOPY_OPTIONS}
          selectedValue={autocopy}
          disabled={isUpdating("agent_autocopy")}
          onSelect={(v) => updateSetting("agent_autocopy", v as AgentAutocopy)}
        />
      </SettingContainer>

      {/* 5. How you talk to it. */}
      <ToggleSwitch
        label="Type to expand"
        description="The summon card records by default. Start typing while it's listening to jump straight to the typing card; turn this off to keep it voice-first (press Tab or click to type)."
        descriptionMode="tooltip"
        grouped
        checked={typeToExpand}
        isUpdating={isUpdating("agent_input_type_to_expand")}
        onChange={(v) => updateSetting("agent_input_type_to_expand", v)}
      />
      {/* Renders its own row (name + description from the binding). While the
          Agent is open this shortcut OVERRIDES any other Grain shortcut on the
          same keys; outside the Agent it does nothing. */}
      <ShortcutInput
        shortcutId="agent_followup"
        grouped
        descriptionMode="tooltip"
      />

      {/* 6. What it is allowed to read. */}
      <SettingContainer
        title="Context"
        description="What the Agent reads at summon, from least to most. 'Unique terms' passes only high-signal names and identifiers, never raw text. 'Full field text' sends the field you're in (capped). 'Whole window text' also sends what surrounds it — the email thread you're replying to, the page you're on — read from the window's accessibility tree, never a screenshot, and only the window you're in. Selected text always stays the subject; context is reference only. Password fields are never read."
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={CONTEXT_OPTIONS}
          selectedValue={contextMode}
          disabled={isUpdating("agent_context_mode")}
          onSelect={(v) =>
            updateSetting("agent_context_mode", v as AgentContextMode)
          }
        />
      </SettingContainer>
    </>
  );
};
