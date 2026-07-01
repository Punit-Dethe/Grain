/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useEffect, useMemo } from "react";
import { useSettings } from "../../hooks/useSettings";
import {
  ink,
  fill,
  MechanicalToggle,
  Jack,
  KeyCaps,
  WellLabel,
  ConsoleSelect,
} from "./widgets";

const MONO = "var(--qp-font-mono)";
const Spacer: React.FC<{ h: number }> = ({ h }) => (
  <div style={{ height: h, flex: "none" }} />
);

/** One column of the side-by-side capture-mode trio: label on top, keycaps
 *  below (horizontally scrollable so a 4-key combo never breaks the column). */
const HotkeyChip: React.FC<{ label: string; binding: string }> = ({
  label,
  binding,
}) => (
  <div
    className="flex-1 flex flex-col items-center justify-center"
    style={{
      height: 52,
      minWidth: 0,
      gap: 5,
      borderRadius: 6,
      padding: "0 6px",
      backgroundColor: fill(0.03),
      border: `1px solid ${fill(0.06)}`,
    }}
  >
    <span
      style={{
        fontFamily: MONO,
        fontSize: 8,
        fontWeight: 700,
        letterSpacing: "0.5px",
        textTransform: "uppercase",
        color: ink(0.55),
      }}
    >
      {label}
    </span>
    {binding ? (
      <div
        className="max-w-full overflow-x-auto qp-scroll"
        style={{ scrollbarWidth: "none" }}
      >
        <KeyCaps binding={binding} />
      </div>
    ) : (
      <span style={{ fontFamily: MONO, fontSize: 9, color: ink(0.4) }}>
        unset
      </span>
    )}
  </div>
);

const ToggleBox: React.FC<{
  label: string;
  sub: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}> = ({ label, sub, checked, onChange }) => (
  <div
    className="flex-1 flex items-center justify-between"
    style={{
      height: 74,
      borderRadius: 8,
      padding: 12,
      backgroundColor: fill(0.04),
      border: `1px solid ${fill(0.05)}`,
    }}
  >
    <div className="flex flex-col" style={{ gap: 2 }}>
      <span style={{ fontSize: 11, fontWeight: 700, color: ink(0.85) }}>
        {label}
      </span>
      <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.45) }}>
        {sub}
      </span>
    </div>
    <MechanicalToggle checked={checked} onChange={onChange} />
  </div>
);

const BehaviourRow: React.FC<{
  label: string;
  sub: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}> = ({ label, sub, checked, onChange }) => (
  <div
    className="flex items-center justify-between"
    style={{
      height: 52,
      borderRadius: 8,
      paddingLeft: 12,
      paddingRight: 12,
      backgroundColor: fill(0.04),
      border: `1px solid ${fill(0.05)}`,
    }}
  >
    <div className="flex flex-col" style={{ gap: 2 }}>
      <span style={{ fontSize: 11, fontWeight: 700, color: ink(0.85) }}>
        {label}
      </span>
      <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.45) }}>
        {sub}
      </span>
    </div>
    <MechanicalToggle checked={checked} onChange={onChange} />
  </div>
);

/**
 * Module A — Configuration. Wired to the shared settings store (`useSettings`),
 * so every control here and its twin in the main Settings window read/write the
 * same state and stay in sync. Hotkeys are read-only (display the bindings).
 */
export const ModuleA: React.FC = () => {
  const { getSetting, updateSetting, audioDevices, refreshAudioDevices } =
    useSettings();

  // Ensure the device list is populated when the panel opens first.
  useEffect(() => {
    if (audioDevices.length === 0) void refreshAudioDevices();
  }, [audioDevices.length, refreshAudioDevices]);

  const bindings = getSetting("bindings") || {};
  const bindingFor = (id: string) => bindings[id]?.current_binding ?? "";

  const audioFeedback = getSetting("audio_feedback") ?? false;
  const audioConditioning = getSetting("audio_conditioning") ?? true;
  const autostart = getSetting("autostart_enabled") ?? false;
  const showTray = getSetting("show_tray_icon") ?? true;

  const selectedMic = getSetting("selected_microphone") || "Default";
  const micOptions = useMemo(() => {
    const names = audioDevices.map((d) => d.name);
    if (!names.includes(selectedMic)) names.unshift(selectedMic);
    return names.length ? names : ["Default"];
  }, [audioDevices, selectedMic]);

  return (
    <>
      <WellLabel letterSpacing={1.5} marginBottom={8}>
        SYSTEM HOTKEYS
      </WellLabel>
      {/* [GRAIN] The three capture engines, side-by-side: Batch, Rolling, Native
          ASR. Voice-to-AI stays a modifier of Batch/Rolling — it's listed in the
          main Settings hotkeys tab, not duplicated here. */}
      <div className="flex items-stretch" style={{ gap: 6 }}>
        <HotkeyChip label="Batch" binding={bindingFor("transcribe")} />
        <HotkeyChip label="Rolling" binding={bindingFor("transcribe_realtime")} />
        <HotkeyChip
          label="Native ASR"
          binding={bindingFor("transcribe_native_asr")}
        />
      </div>

      <Spacer h={20} />

      <WellLabel letterSpacing={1} marginBottom={6}>
        AUDIO SETTINGS
      </WellLabel>
      <ConsoleSelect
        value={selectedMic}
        options={micOptions}
        height={34}
        onChange={(v) => updateSetting("selected_microphone", v)}
      />
      <Spacer h={8} />
      <div className="flex" style={{ gap: 8 }}>
        <ToggleBox
          label="Play Sound"
          sub="Hotkey cues"
          checked={audioFeedback}
          onChange={(v) => updateSetting("audio_feedback", v)}
        />
        <ToggleBox
          label="Process Audio"
          sub="Clear enhancement"
          checked={audioConditioning}
          onChange={(v) => updateSetting("audio_conditioning", v)}
        />
      </div>

      <Spacer h={20} />

      <WellLabel letterSpacing={1} marginBottom={6}>
        SYSTEM BEHAVIOUR
      </WellLabel>
      <BehaviourRow
        label="Launch on Boot"
        sub="Autoload system daemon"
        checked={autostart}
        onChange={(v) => updateSetting("autostart_enabled", v)}
      />
      <Spacer h={8} />
      <BehaviourRow
        label="Minimize to System Tray"
        sub="Keep the tray icon when closed"
        checked={showTray}
        onChange={(v) => updateSetting("show_tray_icon", v)}
      />

      <Spacer h={10} />
      <div style={{ height: 1, backgroundColor: fill(0.1), flex: "none" }} />
      <Spacer h={8} />

      {/* Signal output jack */}
      <div
        className="flex items-center justify-between"
        style={{ height: 54, paddingBottom: 10 }}
      >
        <span
          style={{
            fontFamily: MONO,
            fontSize: 8,
            fontWeight: 700,
            letterSpacing: "1px",
            textTransform: "uppercase",
            color: ink(0.4),
          }}
        >
          Signal Output
        </span>
        <div
          className="flex items-center justify-between"
          style={{
            width: 104,
            height: 46,
            borderRadius: 8,
            paddingLeft: 12,
            paddingRight: 10,
            background:
              "linear-gradient(var(--qp-jack-top), var(--qp-jack-bottom))",
            border: `1px solid ${fill(0.1)}`,
          }}
        >
          <span
            style={{
              fontFamily: MONO,
              fontSize: 8,
              fontWeight: 700,
              letterSpacing: "1.2px",
              color: "#FF5D1E",
            }}
          >
            OUTPUT
          </span>
          <Jack size={34} jackId="moduleA.output" color="#FF5D1E" />
        </div>
      </div>
    </>
  );
};
