/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ScaledStage } from "./ScaledStage";
import { ModuleFrame } from "./ModuleFrame";
import { ModuleA } from "./ModuleA";
import { ModuleB } from "./ModuleB";
import { ModuleC } from "./ModuleC";
import { PatchCables } from "./PatchCables";
import { useTheme } from "../../contexts/ThemeContext";
import { useSystemStatus, type StatusTone } from "./useSystemStatus";
import "./quickPanel.css";

const DESIGN_W = 1280;
const DESIGN_H = 760;

interface QuickPanelProps {
  /** Switch to the full Advanced (settings) view. */
  onOpenAdvanced: () => void;
}

const appWindow = getCurrentWindow();

/** Frameless window controls (minimize / maximize / close) using Tauri APIs. */
const WindowControls: React.FC = () => {
  const btn =
    "w-[18px] h-[18px] flex items-center justify-center cursor-pointer";
  return (
    <div
      className="flex items-center gap-[16px] rounded-[6px] px-[12px] h-[24px]"
      style={{ backgroundColor: "var(--qp-ctrl-box-bg)" }}
    >
      <button
        type="button"
        className={btn}
        onClick={() => appWindow.minimize()}
        title="Minimize"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
          <path
            d="M20 12H4"
            stroke="rgb(var(--qp-ink-rgb) / 0.7)"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      </button>
      <button
        type="button"
        className={btn}
        onClick={() => appWindow.toggleMaximize()}
        title="Maximize"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none">
          <path
            d="M4 4h16v16H4z"
            stroke="rgb(var(--qp-ink-rgb) / 0.7)"
            strokeWidth="2"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      <button
        type="button"
        className={btn}
        onClick={() => appWindow.close()}
        title="Close"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
          <path
            d="M6 18L18 6M6 6l12 12"
            stroke="rgb(var(--qp-ink-rgb) / 0.7)"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      </button>
    </div>
  );
};

const ThemeToggle: React.FC<{
  isDark: boolean;
  onToggle: () => void;
}> = ({ isDark, onToggle }) => (
  <button
    type="button"
    onClick={onToggle}
    className="relative w-[90px] h-[26px] rounded-[8px] cursor-pointer shrink-0"
    style={{ backgroundColor: isDark ? "#272422" : "#DDD5C8" }}
  >
    <span
      className="absolute top-[3px] h-[20px] w-[38px] rounded-[8px] transition-all duration-300"
      style={{
        left: isDark ? 49 : 3,
        backgroundColor: isDark ? "#ECE5DA" : "#111010",
      }}
    />
    <span
      className="absolute left-[10px] top-1/2 -translate-y-1/2"
      style={{
        fontFamily: "var(--qp-font-mono)",
        fontSize: 8,
        fontWeight: 700,
        color: isDark ? "rgba(236,229,218,0.32)" : "#DDD5C8",
      }}
    >
      LIGHT
    </span>
    <span
      className="absolute right-[10px] top-1/2 -translate-y-1/2"
      style={{
        fontFamily: "var(--qp-font-mono)",
        fontSize: 8,
        fontWeight: 700,
        color: isDark ? "#1a1816" : "rgba(20,18,16,0.38)",
      }}
    >
      DARK
    </span>
  </button>
);

/** Maps a status tone to its accent colour. Orange / green / red only — the
 *  panel's existing accent vocabulary (no new hues). */
const toneColor = (tone: StatusTone): string => {
  switch (tone) {
    case "error":
      return "#E5484D";
    case "ok":
      return "var(--qp-green)";
    case "busy":
    case "active":
      return "var(--qp-orange)";
    case "idle":
    default:
      return "rgb(var(--qp-ink-rgb) / 0.45)";
  }
};

/** Renders the persistent `<transcription route> // <processing route>` line:
 *  the two route values are emphasised and the `//` separator is dimmed. No
 *  segment labels. Falls back to a single span for any string without a `//`. */
const RouteLine: React.FC<{ text: string }> = ({ text }) => {
  const dim = "rgb(var(--qp-ink-rgb) / 0.38)";
  const bright = "rgb(var(--qp-ink-rgb) / 0.72)";
  const sep = text.indexOf(" // ");
  if (sep === -1) {
    return <span style={{ color: bright }}>{text}</span>;
  }
  const stt = text.slice(0, sep);
  const pp = text.slice(sep + 4);
  return (
    <>
      <span style={{ color: bright }}>{stt}</span>
      <span style={{ color: dim }}> // </span>
      <span style={{ color: bright }}>{pp}</span>
    </>
  );
};

/** Bottom-right live status: an animated indicator dot + the route line.
 *  Transient events surface as a single label; the steady state is the
 *  two-segment route summary. All state is derived from signals the webview
 *  already receives. */
const StatusIndicator: React.FC = () => {
  const status = useSystemStatus();
  const color = toneColor(status.tone);
  return (
    <div className="flex items-center" style={{ gap: 8, minWidth: 0 }}>
      <span
        className={`qp-status-dot qp-status-dot--${status.tone}`}
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          backgroundColor: color,
          color, // currentColor drives the glow in the keyframes
          flex: "none",
        }}
      />
      <span
        key={status.label}
        className="qp-status-label truncate"
        style={{
          fontFamily: "var(--qp-font-mono)",
          fontSize: 10,
          fontWeight: 700,
          letterSpacing: "1.5px",
          color: status.transient ? color : "rgb(var(--qp-ink-rgb) / 0.6)",
        }}
      >
        {status.transient ? (
          status.label
        ) : (
          <RouteLine text={status.label} />
        )}
      </span>
    </div>
  );
};

export const QuickPanel: React.FC<QuickPanelProps> = ({ onOpenAdvanced }) => {
  // [GRAIN] Use independent quick panel theme state
  const { isQuickPanelDark, toggleQuickPanel } = useTheme();

  return (
    <div
      className="qp-root w-full h-full"
      data-qp-theme={isQuickPanelDark ? "dark" : "light"}
      // The console floats on a near-black chassis surround; the card sits on
      // top with its 36px rounded corners visible. The window is opaque +
      // OS-rounded (DWM), so this fills the frame outside the rounded card.
      style={{ backgroundColor: "#0c0b0a" }}
    >
      <ScaledStage designWidth={DESIGN_W} designHeight={DESIGN_H}>
        <div
          className="relative w-full h-full overflow-hidden flex flex-col"
          style={{
            backgroundColor: "var(--qp-window-bg)",
            paddingLeft: 29,
            paddingRight: 29,
            paddingTop: 16,
            paddingBottom: 20,
          }}
        >
          {/* Patch-cable overlay (drawn over the rack, clicks pass through). */}
          <PatchCables />
          {/* ── HEADER ── */}
          <div
            className="h-[32px] flex items-center justify-between shrink-0"
            style={{ borderBottom: "1px solid rgb(var(--qp-line-rgb) / 0.08)" }}
            data-tauri-drag-region
            onMouseDown={(e) => {
              if ((e.target as HTMLElement).closest("[data-no-drag]")) return;
              appWindow.startDragging();
            }}
          >
            <div className="flex items-center gap-[14px]" data-no-drag>
              <div
                className="flex items-center justify-center rounded-[5px]"
                style={{
                  width: 210,
                  height: 28,
                  backgroundColor: "var(--qp-brand-badge-bg)",
                }}
              >
                <span
                  style={{
                    fontFamily: "var(--qp-font-display)",
                    fontSize: 12,
                    fontWeight: 700,
                    letterSpacing: "1.2px",
                    color: "var(--qp-brand-badge-text)",
                  }}
                >
                  GRAIN // QUICK PANEL
                </span>
              </div>
              <button
                type="button"
                onClick={onOpenAdvanced}
                className="cursor-pointer hover:opacity-70 transition-opacity"
                style={{
                  fontFamily: "var(--qp-font-mono)",
                  fontSize: 11,
                  fontWeight: 700,
                  letterSpacing: "1.5px",
                  color: "var(--qp-orange)",
                }}
              >
                [ Advanced Calibration ]
              </button>
            </div>

            <div className="flex items-center gap-[12px]" data-no-drag>
              <ThemeToggle isDark={isQuickPanelDark} onToggle={toggleQuickPanel} />
              <WindowControls />
            </div>
          </div>

          {/* ── MODULE RACK ── */}
          <div className="flex-1 min-h-0 flex gap-[19px] pt-[12px] pb-[12px]">
            <ModuleFrame
              eyebrow="MODULE A"
              title="Configuration"
              footerLeft="HARDWARE: ARMED"
              footerRight="TYPE: CONTROL"
            >
              <ModuleA />
            </ModuleFrame>
            <ModuleFrame
              eyebrow="MODULE B"
              title="Transcription"
              footerLeft="MODEL: PARAKEET-TDT-0.6B-V3"
              footerRight="TYPE: AUDIO-STT"
            >
              <ModuleB />
            </ModuleFrame>
            <ModuleFrame
              eyebrow="MODULE C"
              title="Processing"
              footerLeft="CACHE: ACTIVE_RAM"
              footerRight="TYPE: CHAT-LLM"
            >
              <ModuleC />
            </ModuleFrame>
          </div>

          {/* ── BOTTOM STATUS BAR ── */}
          <div className="h-[16px] flex items-center justify-between shrink-0">
            <button
              type="button"
              className="cursor-pointer hover:opacity-100 transition-opacity"
              style={{
                fontFamily: "var(--qp-font-mono)",
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: "1.5px",
                color: "rgb(var(--qp-ink-rgb) / 0.6)",
              }}
            >
              [ View Telemetry Logs ]
            </button>
            <StatusIndicator />
          </div>
        </div>
      </ScaledStage>
    </div>
  );
};
