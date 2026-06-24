/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useEffect, useState } from "react";
import { commands, type ModelUnloadTimeout } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { useSttPool } from "../settings/speech-to-text/useSttPool";
import {
  ink,
  fill,
  MechanicalToggle,
  SegToggle,
  ConsoleSelect,
  JackHousing,
  HistoryBox,
} from "./widgets";
import { DotMatrix } from "./DotMatrix";
import { useTranscriptionHistory } from "./useHistory";

const MONO = "var(--qp-font-mono)";
const Spacer: React.FC<{ h: number }> = ({ h }) => (
  <div style={{ height: h, flex: "none" }} />
);

// NOTE: the runtime serde values are "min2"/"hour1" etc. (no underscore before
// the digit) — the generated `ModelUnloadTimeout` binding spells them "min_2",
// so we keep raw strings and cast, exactly like the main ModelUnloadTimeout
// setting component does.
const UNLOAD: { label: string; value: string }[] = [
  { label: "Instant", value: "immediately" },
  { label: "2 min", value: "min2" },
  { label: "5 min", value: "min5" },
  { label: "10 min", value: "min10" },
  { label: "15 min", value: "min15" },
  { label: "1 hr", value: "hour1" },
  { label: "Never", value: "never" },
];

/** Local model picker — green status dot + native select over the model store. */
const LocalModelSelect: React.FC<{
  models: { id: string; name: string; is_downloaded: boolean }[];
  currentId: string;
  onSelect: (id: string) => void;
}> = ({ models, currentId, onSelect }) => {
  const downloaded = models.filter((m) => m.is_downloaded);
  const list = downloaded.length ? downloaded : models;
  return (
    <div
      className="relative w-full flex items-center"
      style={{
        height: 34,
        borderRadius: 6,
        backgroundColor: "var(--qp-input-bg)",
        border: `1px solid ${fill(0.1)}`,
      }}
    >
      <span
        className="absolute"
        style={{
          left: 10,
          width: 6,
          height: 6,
          borderRadius: 3,
          backgroundColor: currentId ? "#10B981" : ink(0.18),
        }}
      />
      <select
        value={currentId}
        onChange={(e) => onSelect(e.target.value)}
        className="w-full h-full bg-transparent outline-none cursor-pointer appearance-none"
        style={{
          padding: "0 28px 0 24px",
          fontSize: 11,
          fontWeight: 600,
          color: "var(--qp-input-text)",
        }}
      >
        {list.length === 0 && <option value="">No model installed</option>}
        {list.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </select>
      <span
        className="absolute pointer-events-none"
        style={{ right: 10, fontSize: 10, color: "var(--qp-input-text)" }}
      >
        ▾
      </span>
    </div>
  );
};

const MiniBox: React.FC<{
  label: string;
  sub: string;
  children: React.ReactNode;
}> = ({ label, sub, children }) => (
  <div
    className="flex-1 flex items-center justify-between"
    style={{
      height: 52,
      borderRadius: 8,
      padding: 10,
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
    {children}
  </div>
);

export const ModuleB: React.FC = () => {
  const pool = useSttPool();
  const { getSetting, updateSetting } = useSettings();
  const { models, currentModel, selectModel } = useModelStore();
  const history = useTranscriptionHistory();

  const smartRotation = pool.smartRotation;
  // [GRAIN] LCL/CLD is a LOCAL view choice, independent of smart rotation —
  // switching to CLD must NOT enable rotation. Rotation is the SAME backend
  // setting as the Transcription tab (via useSttPool → stt_smart_rotation), so
  // toggling it here syncs there. When rotation IS on, cloud is active: force the
  // cloud view and lock LCL (handled by `leftLocked`).
  const [route, setRoute] = useState<0 | 1>(smartRotation ? 1 : 0);
  useEffect(() => {
    if (smartRotation) setRoute(1);
  }, [smartRotation]);

  // Launch-on-start has no Handy equivalent yet — placeholder, not persisted.
  const [launch, setLaunch] = useState(false);

  const unloadEnum = (getSetting("model_unload_timeout") as string) ?? "never";
  const unloadLabel =
    UNLOAD.find((u) => u.value === unloadEnum)?.label ?? "Never";
  const setUnload = (label: string) => {
    const v = (UNLOAD.find((u) => u.label === label)?.value ??
      "never") as ModelUnloadTimeout;
    void commands.setModelUnloadTimeout(v);
    void updateSetting("model_unload_timeout", v);
  };

  const cloudNames = pool.cloudProviders.map((p) => p.name);

  return (
    <>
      {/* Aura Core Monitor */}
      <div
        style={{
          fontFamily: MONO,
          fontSize: 8,
          fontWeight: 700,
          letterSpacing: "1.2px",
          color: ink(0.45),
          marginBottom: 4,
        }}
      >
        Aura Core Monitor
      </div>
      <div
        style={{
          height: 140,
          borderRadius: 8,
          backgroundColor: "#120500",
          border: "1px solid rgba(255,93,30,0.25)",
          overflow: "hidden",
          flex: "none",
        }}
      >
        <DotMatrix />
      </div>

      <Spacer h={8} />

      {/* Model Route */}
      <div className="flex items-center justify-between" style={{ height: 28 }}>
        <span
          style={{
            fontFamily: MONO,
            fontSize: 8,
            fontWeight: 700,
            letterSpacing: "1.5px",
            color: ink(0.45),
          }}
        >
          Model Route
        </span>
        <SegToggle
          left="LCL"
          right="CLD"
          value={route}
          activeColor="#FF5D1E"
          leftLocked={smartRotation}
          onChange={(v) => setRoute(v as 0 | 1)}
        />
      </div>
      <Spacer h={3} />

      {route === 0 ? (
        <>
          <LocalModelSelect
            models={models}
            currentId={currentModel}
            onSelect={(id) => void selectModel(id)}
          />
          <Spacer h={5} />
          <div className="flex" style={{ gap: 8 }}>
            <MiniBox label="Launch" sub="Load on start">
              <MechanicalToggle checked={launch} onChange={setLaunch} />
            </MiniBox>
            <MiniBox label="Unload" sub="Auto-idle">
              <div style={{ width: 68 }}>
                <ConsoleSelect
                  value={unloadLabel}
                  options={UNLOAD.map((u) => u.label)}
                  height={26}
                  onChange={setUnload}
                />
              </div>
            </MiniBox>
          </div>
        </>
      ) : (
        <>
          <ConsoleSelect
            value={cloudNames[0] ?? "No providers yet"}
            options={cloudNames.length ? cloudNames : ["No providers yet"]}
            height={34}
          />
          <Spacer h={5} />
          <div className="flex" style={{ gap: 8 }}>
            <MiniBox label="Real-time" sub="coming soon">
              <MechanicalToggle checked={false} />
            </MiniBox>
            <MiniBox label="Smart Rotate" sub="Provider fallback">
              <MechanicalToggle
                checked={smartRotation}
                onChange={(v) => pool.setSmartRotation(v)}
              />
            </MiniBox>
          </div>
        </>
      )}

      {/* Transcription history (live) */}
      <div style={{ height: 14, flex: "none" }} />
      <HistoryBox label="TRANSCRIBED" entries={history} />
      <Spacer h={8} />

      <div style={{ height: 1, backgroundColor: fill(0.1), flex: "none" }} />
      <Spacer h={8} />

      {/* IN / OUT jacks */}
      <div
        className="flex items-center"
        style={{ height: 54, paddingBottom: 10, gap: 4 }}
      >
        <JackHousing
          label="INPUT"
          color="#FF5D1E"
          jackSide="left"
          jackId="moduleB.input"
        />
        <div className="flex-1" />
        <JackHousing
          label="OUTPUT"
          color="#10B981"
          jackSide="right"
          jackId="moduleB.output"
          activeSink
        />
      </div>
    </>
  );
};
