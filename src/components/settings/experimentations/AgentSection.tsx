import React from "react";
import type { AgentAutocopy, AgentContextMode } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
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
];

/** [GRAIN] Agent settings, consolidated into two groups so single controls no
 * longer each get their own heading: Replies (auto-copy, the follow-up
 * shortcut, Quick Agent) and Input & context (type-to-expand + what the Agent
 * reads from the focused field at summon). All copy lives in per-row "i" hints. */
export const AgentSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const autocopy = getSetting("agent_autocopy") ?? "first";
  const quick = getSetting("agent_quick_enabled") ?? false;
  const contextMode = getSetting("agent_context_mode") ?? "off";
  const typeToExpand = getSetting("agent_input_type_to_expand") ?? true;

  return (
    <div className="space-y-6">
      <SettingsGroup
        title="Replies"
        info="How the Agent hands its replies back to you. Confirm (Enter on the reply card) always pastes the shown reply into the app you summoned the Agent from."
      >
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
            onSelect={(v) =>
              updateSetting("agent_autocopy", v as AgentAutocopy)
            }
          />
        </SettingContainer>
        {/* Renders its own row (name + description from the binding). While the
            Agent is open this shortcut OVERRIDES any other Grain shortcut on
            the same keys; outside the Agent it does nothing. */}
        <ShortcutInput
          shortcutId="agent_followup"
          grouped
          descriptionMode="tooltip"
        />
        <ToggleSwitch
          label="Quick Agent"
          description="Skip the reply card entirely: the reply is auto-pasted straight at your cursor (replacing any still-selected text), then the pill briefly offers 'ask follow-up' in case you need to keep going. Same summon shortcut."
          descriptionMode="tooltip"
          grouped
          checked={quick}
          isUpdating={isUpdating("agent_quick_enabled")}
          onChange={(v) => updateSetting("agent_quick_enabled", v)}
        />
      </SettingsGroup>

      <SettingsGroup
        title="Input & context"
        info="How the native summon card behaves, and what the Agent may read from the field you summoned it from."
      >
        <ToggleSwitch
          label="Type to expand"
          description="The summon card records by default. Start typing while it's listening to jump straight to the typing card; turn this off to keep it voice-first (press Tab or click to type)."
          descriptionMode="tooltip"
          grouped
          checked={typeToExpand}
          isUpdating={isUpdating("agent_input_type_to_expand")}
          onChange={(v) => updateSetting("agent_input_type_to_expand", v)}
        />
        <SettingContainer
          title="Field context"
          description="What the Agent reads from the focused field at summon. 'Unique terms' passes only high-signal names and identifiers (never raw text); 'Full field text' sends the field content (capped) so the Agent understands the surrounding document. Selected text always stays the subject — the field content is reference only. Password fields are never read."
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
      </SettingsGroup>
    </div>
  );
};
