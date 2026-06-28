/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useEffect, useState } from "react";
import { commands, type ModelUnloadTimeout } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { useSttPoolStore } from "@/stores/sttPoolStore";
import {
  ink,
  fill,
  MechanicalToggle,
  SegToggle,
  ConsoleSelect,
  ConsoleDropdown,
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

/** Local model picker — green status dot when a model is active, rendered with
 *  the shared custom ConsoleDropdown so it matches every other combo box. */
const LocalModelSelect: React.FC<{
  models: { id: string; name: string; is_downloaded: boolean }[];
  currentId: string;
  onSelect: (id: string) => void;
}> = ({ models, currentId, onSelect }) => {
  const downloaded = models.filter((m) => m.is_downloaded);
  const list = downloaded.length ? downloaded : models;
  return (
    <ConsoleDropdown
      value={currentId}
      options={list.map((m) => ({ value: m.id, label: m.name }))}
      closedDotColor={currentId ? "#10B981" : ink(0.18)}
      emptyLabel="No model installed"
      onSelect={onSelect}
    />
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
  // [GRAIN] Use the singleton store — shared with the settings panel so
  // provider changes there are reflected here immediately.
  const pool = useSttPoolStore();
  const { getSetting, updateSetting } = useSettings();
  const { models, currentModel, selectModel } = useModelStore();
  const history = useTranscriptionHistory();

  // Ensure the pool is loaded when this module mounts (no-op if already loaded).
  useEffect(() => {
    if (pool.loading && pool.view === null) {
      void pool.reload();
    }
  }, []);

  const smartRotation = pool.smartRotation;
  // [GRAIN] LCL/CLD is a LOCAL view choice, independent of smart rotation —
  // switching to CLD must NOT enable rotation. Rotation is the SAME backend
  // setting as the Transcription tab (via sttPoolStore → stt_smart_rotation),
  // so toggling it here syncs there. When rotation IS on, cloud is active:
  // force the cloud view and lock LCL (handled by `leftLocked`).
  const [route, setRoute] = useState<0 | 1>(smartRotation ? 1 : 0);
  useEffect(() => {
    if (smartRotation) setRoute(1);
  }, [smartRotation]);


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
  const hasCloud = pool.cloudProviders.length > 0;

  const enabledCloudCount = pool.cloudProviders.filter((p) => p.enabled ?? true).length;
  const totalCloudCount = pool.cloudProviders.length;
  let dynamicPlaceholder = "Configure providers";
  if (enabledCloudCount === 1) {
    dynamicPlaceholder =
      pool.cloudProviders.find((p) => p.enabled ?? true)?.name ||
      "Configure providers";
  } else if (enabledCloudCount === 0 && totalCloudCount > 0) {
    dynamicPlaceholder = "Turn on a provider";
  } else if (totalCloudCount > 0) {
    dynamicPlaceholder = `${enabledCloudCount} / ${totalCloudCount} active`;
  }

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
            <MiniBox label="Unload model" sub="Idle timeout">
              <div style={{ width: 140 }}>
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
          {/* One custom dropdown for both states. Cloud OFF: gated (gray, not
              openable) so the local/cloud mental model is clear. Cloud ON: the
              closed label reads "Configure providers" and the open panel lists
              every cloud provider with a right-edge ON/OFF control (ON = orange
              text + darker beige row). */}
          {smartRotation ? (
            <ConsoleDropdown
              toggleable
              placeholder={dynamicPlaceholder}
              options={pool.cloudProviders.map((p) => ({
                value: p.id,
                label: p.name,
                enabled: p.enabled ?? true,
              }))}
              emptyLabel="No providers yet"
              onToggle={(id, next) => {
                const provider = pool.cloudProviders.find((p) => p.id === id);
                if (provider) void pool.setProviderEnabled(provider, next);
              }}
            />
          ) : (
            <ConsoleDropdown
              value={hasCloud ? cloudNames[0] : undefined}
              placeholder={hasCloud ? undefined : "No providers yet"}
              options={pool.cloudProviders.map((p) => ({
                value: p.name,
                label: p.name,
              }))}
              disabled
            />
          )}
          <Spacer h={5} />
          <div className="flex" style={{ gap: 8 }}>
            <MiniBox label="Cloud Providers" sub="Enable remote">
              <MechanicalToggle
                checked={smartRotation}
                onChange={(v) => void pool.setSmartRotation(v)}
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
