/**
 * [GRAIN] Capture modes — the one place shortcut bloat is decided.
 *
 * Grain ships three capture modes and an AI key. Registering all four globally
 * asks the user to memorise four chords before they have said a word, and most
 * people live in exactly one mode. This surface makes that a choice:
 *
 *   1. One mode or all three.
 *   2. If one — which, with enough about each to choose on purpose.
 *   3. What the AI key does, as toggles rather than yet more shortcuts.
 *
 * The mode names and descriptions are read from `settings.bindings`, not
 * hardcoded here: the backend already returns a complete, self-describing map
 * (PLAN.md §3.4), and a hardcoded list is how the old UI grew a dead row.
 */
import React from "react";
import { useTranslation } from "react-i18next";
import type { AppSettings, CaptureModeSet } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { ShortcutInput } from "../ShortcutInput";
import { PostProcessingToggle } from "../PostProcessingToggle";

/** Mirrors `CAPTURE_MODE_IDS` in grain-core. Order = least to most machinery. */
const CAPTURE_MODE_IDS = [
  "transcribe",
  "transcribe_realtime",
  "transcribe_native_asr",
] as const;

type CaptureModeId = (typeof CAPTURE_MODE_IDS)[number];

interface CaptureMode {
  id: CaptureModeId;
  name: string;
  description: string;
}

function captureModes(settings: AppSettings | null): CaptureMode[] {
  return CAPTURE_MODE_IDS.map((id) => {
    const binding = settings?.bindings?.[id];
    return {
      id,
      name: binding?.name ?? id,
      description: binding?.description ?? "",
    };
  });
}

export const CaptureModes: React.FC = () => {
  const { t } = useTranslation();
  const { settings, getSetting, updateSetting, isUpdating } = useSettings();

  const modeSet: CaptureModeSet = getSetting("capture_mode_set") ?? "single";
  const isSingle = modeSet === "single";
  const primary = getSetting("capture_primary_mode") ?? "transcribe";
  const pushToTalk = getSetting("push_to_talk") ?? false;
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const alwaysAi = getSetting("capture_always_ai") ?? false;
  const endWithAi = getSetting("capture_end_with_ai") ?? true;
  const aiStartMode = getSetting("capture_ai_start_mode") ?? "transcribe";

  const modes = captureModes(settings);

  return (
    <>
      <SettingsGroup title={t("ui2.capture.group")}>
        <SettingContainer
          title={t("ui2.capture.set.title")}
          description={t("ui2.capture.set.description")}
          descriptionMode="tooltip"
          grouped
        >
          <div
            className="view-switch"
            role="radiogroup"
            aria-label={t("ui2.capture.set.title")}
          >
            {(["single", "all"] as const).map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={modeSet === option}
                className={modeSet === option ? "active" : ""}
                disabled={isUpdating("capture_mode_set")}
                onClick={() => updateSetting("capture_mode_set", option)}
              >
                {t(`ui2.capture.set.${option}`)}
              </button>
            ))}
          </div>
        </SettingContainer>

        {/* One mode: pick it from cards rather than a bare dropdown. The whole
            point of collapsing to one shortcut is that the user chooses it
            deliberately, and they cannot do that from three names alone. */}
        {isSingle ? (
          <div className="capture-mode-picker" role="radiogroup">
            {modes.map((mode) => {
              const selected = mode.id === primary;
              return (
                <button
                  key={mode.id}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  className={`capture-mode-option${selected ? " selected" : ""}`}
                  disabled={isUpdating("capture_primary_mode")}
                  onClick={() => {
                    void updateSetting("capture_primary_mode", mode.id);
                  }}
                >
                  <span className="capture-mode-mark" aria-hidden="true" />
                  <span className="capture-mode-copy">
                    <strong>{mode.name}</strong>
                    <small>{mode.description}</small>
                  </span>
                </button>
              );
            })}
          </div>
        ) : null}

        {/* The live shortcut(s). Under "one" this is the single key the whole
            product now hangs off, so it sits directly beneath its own mode. */}
        {isSingle ? (
          <ShortcutInput shortcutId={primary} grouped />
        ) : (
          CAPTURE_MODE_IDS.map((id) => (
            <ShortcutInput key={id} shortcutId={id} grouped />
          ))
        )}
      </SettingsGroup>

      {/* AI is deliberately its own group: it is a property of what happens
          *after* speech, not a fourth way to start speaking. Its master switch
          leads the group rather than owning a section elsewhere, so the rows it
          governs sit directly under the thing that turns them on — and turning
          it off leaves one row rather than an empty heading. */}
      <SettingsGroup title={t("ui2.capture.ai.group")}>
        <PostProcessingToggle descriptionMode="tooltip" grouped />

        {postProcessEnabled && (
          <ToggleSwitch
            label={t("ui2.capture.ai.always.title")}
            description={t("ui2.capture.ai.always.description")}
            descriptionMode="tooltip"
            grouped
            checked={alwaysAi}
            isUpdating={isUpdating("capture_always_ai")}
            onChange={(value) => updateSetting("capture_always_ai", value)}
          />
        )}

        {/* With every capture already going to AI there is nothing for the AI
            key to add mid-capture, so we do not offer a switch that changes
            nothing. */}
        {postProcessEnabled && !alwaysAi && (
          <ShortcutInput shortcutId="transcribe_send_to_ai" grouped />
        )}

        {/* Push-to-talk ends a capture by releasing the key, so there is no
            moment at which a second key could end it with AI. */}
        {postProcessEnabled && !alwaysAi && !pushToTalk && (
          <ToggleSwitch
            label={t("ui2.capture.ai.end.title")}
            description={t("ui2.capture.ai.end.description")}
            descriptionMode="tooltip"
            grouped
            checked={endWithAi}
            isUpdating={isUpdating("capture_end_with_ai")}
            onChange={(value) => updateSetting("capture_end_with_ai", value)}
          />
        )}

        {/* Only meaningful with all three modes live: under "one" the AI key
            necessarily starts the one mode that has a shortcut. */}
        {postProcessEnabled && !alwaysAi && !isSingle && (
          <SettingContainer
            title={t("ui2.capture.ai.startMode.title")}
            description={t("ui2.capture.ai.startMode.description")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={modes.map((mode) => ({
                value: mode.id,
                label: mode.name,
              }))}
              selectedValue={aiStartMode}
              onSelect={(value) => updateSetting("capture_ai_start_mode", value)}
            />
          </SettingContainer>
        )}

        {/* Prompt cycling only exists once there are prompts to cycle. */}
        {postProcessEnabled && (
          <>
            <ShortcutInput shortcutId="prompt_prev" grouped />
            <ShortcutInput shortcutId="prompt_next" grouped />
          </>
        )}
      </SettingsGroup>
    </>
  );
};
