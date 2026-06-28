/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useState } from "react";

/* Theme helper shorthands — mirror ConsoleTheme.qml's ink()/inkOnOuter()/fill().
 * Alpha-bearing colours read the per-mode RGB triples set in quickPanel.css. */
export const ink = (a: number) => `rgb(var(--qp-ink-rgb) / ${a})`;
export const inkOnOuter = (a: number) => `rgb(var(--qp-ink-on-outer-rgb) / ${a})`;
export const fill = (a: number) => `rgb(var(--qp-line-rgb) / ${a})`;

const MONO = "var(--qp-font-mono)";

/** Mechanical slide toggle — 32×18 track, 14px lever, orange when on. */
export const MechanicalToggle: React.FC<{
  checked: boolean;
  onChange?: (next: boolean) => void;
}> = ({ checked, onChange }) => (
  <button
    type="button"
    onClick={() => onChange?.(!checked)}
    className="relative shrink-0 cursor-pointer"
    style={{
      width: 32,
      height: 18,
      borderRadius: 99,
      backgroundColor: "var(--qp-toggle-track)",
      boxShadow: "inset 0 1px 3px rgba(0,0,0,0.8)",
    }}
  >
    <span
      className="absolute"
      style={{
        width: 14,
        height: 14,
        top: 2,
        left: 2,
        borderRadius: "50%",
        background: checked ? "#ff5d1e" : "linear-gradient(#eee,#bbb)",
        boxShadow: "0 1px 3px rgba(0,0,0,0.4)",
        transform: checked ? "translateX(14px)" : "translateX(0)",
        transition: "transform 0.2s cubic-bezier(0.16,1,0.3,1), background 0.2s",
      }}
    />
  </button>
);

/** Eurorack patch jack — metallic socket with a recessed hole. When `activeSink`
 *  it shows a pulsing terminal ring in its colour (the copy-target marker). */
export const Jack: React.FC<{
  size?: number;
  jackId?: string;
  color?: string;
  activeSink?: boolean;
}> = ({ size = 28, jackId, color = "#FF5D1E", activeSink = false }) => {
  const hole = Math.round(size * 0.5);
  const off = Math.round(size * 0.25);
  const ring = size + 10;
  return (
    <div
      data-jack-id={jackId}
      data-jack-color={color}
      className="relative shrink-0 cursor-pointer qp-jack"
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: "radial-gradient(circle at 35% 35%, #555, #1c1a17 75%)",
        boxShadow:
          "0 1px 2px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.15)",
      }}
    >
      <span
        className="absolute"
        style={{
          width: hole,
          height: hole,
          top: off,
          left: off,
          borderRadius: "50%",
          background: "#0d0c0b",
          boxShadow: "inset 0 2px 5px rgba(0,0,0,0.9)",
        }}
      />
      {activeSink && (
        <span
          className="absolute qp-pulse-ring pointer-events-none"
          style={{
            width: ring,
            height: ring,
            top: -5,
            left: -5,
            borderRadius: "50%",
            border: `2px solid ${color}`,
          }}
        />
      )}
    </div>
  );
};

/** A two-segment pill selector (LCL/CLD, XIX/LLM). value 0 = left, 1 = right.
 *  `leftLocked` dims + disables the left segment (e.g. local locked out while
 *  cloud smart-rotation is on). */
export const SegToggle: React.FC<{
  left: string;
  right: string;
  value: 0 | 1;
  activeColor: string;
  leftLocked?: boolean;
  onChange?: (v: 0 | 1) => void;
}> = ({ left, right, value, activeColor, leftLocked = false, onChange }) => {
  const seg = (label: string, idx: 0 | 1) => {
    const locked = idx === 0 && leftLocked;
    return (
      <button
        type="button"
        onClick={() => {
          if (locked) return;
          onChange?.(idx);
        }}
        title={locked ? "Local is disabled while smart rotation is on" : undefined}
        className={`flex-1 h-full flex items-center justify-center ${
          locked ? "cursor-not-allowed" : "cursor-pointer"
        }`}
        style={{
          borderRadius: 6,
          backgroundColor: value === idx ? activeColor : "transparent",
          transition: "background-color 0.15s",
          fontFamily: MONO,
          fontSize: 10,
          fontWeight: 700,
          opacity: locked ? 0.4 : 1,
          color: value === idx ? "#ffffff" : ink(0.5),
        }}
      >
        {label}
      </button>
    );
  };
  return (
    <div
      className="flex items-center"
      style={{
        width: 80,
        height: 28,
        borderRadius: 8,
        padding: 2,
        gap: 2,
        backgroundColor: fill(0.1),
        border: `1px solid ${fill(0.05)}`,
      }}
    >
      {seg(left, 0)}
      {seg(right, 1)}
    </div>
  );
};

/** The 104×46 metallic jack housing with a label and a jack. */
export const JackHousing: React.FC<{
  label: string;
  color: string;
  jackSide: "left" | "right";
  jackId?: string;
  activeSink?: boolean;
}> = ({ label, color, jackSide, jackId, activeSink }) => {
  const jack = (
    <Jack size={34} color={color} jackId={jackId} activeSink={activeSink} />
  );
  const text = (
    <span
      className="flex-1"
      style={{
        fontFamily: MONO,
        fontSize: 8,
        fontWeight: 700,
        letterSpacing: "1.2px",
        color,
        textAlign: jackSide === "left" ? "left" : "left",
      }}
    >
      {label}
    </span>
  );
  return (
    <div
      className="flex items-center"
      style={{
        width: 104,
        height: 46,
        borderRadius: 8,
        paddingLeft: jackSide === "left" ? 10 : 12,
        paddingRight: jackSide === "left" ? 12 : 10,
        gap: 8,
        background: "linear-gradient(var(--qp-jack-top), var(--qp-jack-bottom))",
        border: `1px solid ${fill(0.1)}`,
      }}
    >
      {jackSide === "left" ? (
        <>
          {jack}
          {text}
        </>
      ) : (
        <>
          {text}
          {jack}
        </>
      )}
    </div>
  );
};

export interface HistoryEntry {
  id?: number;
  time: string;
  text: string;
}

/** A single history row — click to copy the FULL text; preview is one line with
 *  ellipsis, with all whitespace collapsed so a structured/email result can't
 *  break the row height. Shows a transient ✓ on copy. */
const HistoryRow: React.FC<{ entry: HistoryEntry }> = ({ entry }) => {
  const [copied, setCopied] = useState(false);
  const [hover, setHover] = useState(false);
  const preview = entry.text.replace(/\s+/g, " ").trim();

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(entry.text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1100);
    } catch {
      /* ignore */
    }
  };

  return (
    <div
      onClick={copy}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className="flex items-center cursor-pointer"
      style={{
        height: 26,
        borderRadius: 6,
        padding: "0 8px",
        gap: 8,
        backgroundColor: hover ? fill(0.05) : "transparent",
        transition: "background-color 0.12s",
      }}
    >
      <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.4), flex: "none" }}>
        {entry.time}
      </span>
      <span
        className="flex-1 truncate"
        style={{ fontFamily: MONO, fontSize: 9, color: ink(0.85) }}
      >
        {preview}
      </span>
      {copied ? (
        <span style={{ fontSize: 11, fontWeight: 700, color: "#2E9440", flex: "none" }}>
          ✓
        </span>
      ) : (
        hover && (
          <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.4), flex: "none" }}>
            copy
          </span>
        )
      )}
    </div>
  );
};

/** Transcribed / Processed history well with empty state + bottom fade. */
export const HistoryBox: React.FC<{
  label: string;
  entries: HistoryEntry[];
}> = ({ label, entries }) => (
  <div className="flex flex-col flex-1 min-h-0" style={{ gap: 2 }}>
    <div
      style={{
        fontFamily: MONO,
        fontSize: 8,
        fontWeight: 700,
        letterSpacing: "1.5px",
        color: ink(0.45),
        marginLeft: 2,
      }}
    >
      {label}
    </div>
    <div
      className="relative flex-1 min-h-0 overflow-hidden"
      style={{
        borderRadius: 8,
        backgroundColor: fill(0.04),
        border: `1px solid ${fill(0.05)}`,
      }}
    >
      {entries.length === 0 ? (
        <div
          className="absolute inset-0 flex items-center justify-center"
          style={{ fontFamily: MONO, fontSize: 10, color: ink(0.4) }}
        >
          No history yet
        </div>
      ) : (
        <div className="absolute inset-0 overflow-y-auto qp-scroll" style={{ padding: 4 }}>
          {entries.map((e, i) => (
            <HistoryRow key={e.id ?? i} entry={e} />
          ))}
        </div>
      )}
      <div
        className="absolute bottom-0 left-0 right-0 pointer-events-none"
        style={{
          height: 24,
          background: "linear-gradient(transparent, var(--qp-history-fade))",
        }}
      />
    </div>
  </div>
);

/** A row of keycap chips parsed from a binding string e.g. "Ctrl+Shift+Space". */
export const KeyCaps: React.FC<{ binding: string }> = ({ binding }) => {
  const keys = binding.split("+").map((k) => k.trim().toLowerCase());
  return (
    <div className="flex items-center" style={{ gap: 3 }}>
      {keys.map((k, i) => (
        <span
          key={`${k}-${i}`}
          className="relative inline-flex items-center justify-center"
          style={{
            height: 26,
            padding: "0 7px",
            borderRadius: 4,
            backgroundColor: ink(0.07),
            border: `1px solid ${ink(0.18)}`,
            fontFamily: MONO,
            fontSize: 9,
            fontWeight: 700,
            color: ink(0.72),
          }}
        >
          {k}
          <span
            className="absolute left-0 right-0 bottom-0"
            style={{ height: 2, borderRadius: 4, backgroundColor: fill(0.1) }}
          />
        </span>
      ))}
    </div>
  );
};

/** Section eyebrow label inside a well, e.g. "SYSTEM HOTKEYS". */
export const WellLabel: React.FC<{
  children: React.ReactNode;
  letterSpacing?: number;
  marginBottom?: number;
}> = ({ children, letterSpacing = 1.5, marginBottom = 8 }) => (
  <div
    style={{
      fontFamily: MONO,
      fontSize: 8,
      fontWeight: 700,
      letterSpacing,
      textTransform: "uppercase",
      color: ink(0.4),
      marginBottom,
    }}
  >
    {children}
  </div>
);

/** A labelled row card (label + sublabel on the left, control on the right). */
export const SettingRow: React.FC<{
  label: string;
  sub?: string;
  height?: number;
  children?: React.ReactNode;
}> = ({ label, sub, height = 52, children }) => (
  <div
    className="flex items-center justify-between"
    style={{
      height,
      borderRadius: 8,
      padding: "0 12px",
      backgroundColor: fill(0.04),
      border: `1px solid ${fill(0.05)}`,
    }}
  >
    <div className="flex flex-col" style={{ gap: 2 }}>
      <span style={{ fontSize: 11, fontWeight: 700, color: ink(0.85) }}>
        {label}
      </span>
      {sub && (
        <span
          style={{ fontFamily: MONO, fontSize: 8, color: ink(0.45) }}
        >
          {sub}
        </span>
      )}
    </div>
    {children}
  </div>
);

/** Native-styled select matching the console combo boxes. When `disabled` it is
 *  dimmed and non-interactive (used to gate the cloud picker while cloud is off). */
export const ConsoleSelect: React.FC<{
  value?: string;
  options: string[];
  height?: number;
  disabled?: boolean;
  onChange?: (v: string) => void;
}> = ({ value, options, height = 34, disabled = false, onChange }) => (
  <div
    className="relative w-full"
    style={{
      height,
      borderRadius: 6,
      backgroundColor: "var(--qp-input-bg)",
      border: `1px solid ${fill(0.1)}`,
      opacity: disabled ? 0.4 : 1,
      cursor: disabled ? "not-allowed" : "default",
    }}
  >
    <select
      value={value}
      disabled={disabled}
      onChange={(e) => onChange?.(e.target.value)}
      className={`w-full h-full bg-transparent outline-none appearance-none ${
        disabled ? "cursor-not-allowed" : "cursor-pointer"
      }`}
      style={{
        padding: "0 30px 0 10px",
        fontSize: 11,
        fontWeight: 600,
        color: "var(--qp-input-text)",
      }}
    >
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
    <span
      className="absolute pointer-events-none flex items-center justify-center"
      style={{
        right: 8,
        top: "50%",
        transform: "translateY(-50%)",
        fontSize: 9,
        lineHeight: 1,
        color: "var(--qp-input-text)",
      }}
    >
      ▾
    </span>
  </div>
);

/** Sleek inline on/off pill for a single rotation member — sits on a thin row
 *  without the bulk of a full slide toggle. Filled orange + "ON" when active,
 *  hollow + "OFF" when inactive. */
export const PillToggle: React.FC<{
  checked: boolean;
  disabled?: boolean;
  onChange?: (next: boolean) => void;
}> = ({ checked, disabled = false, onChange }) => (
  <button
    type="button"
    disabled={disabled}
    onClick={() => onChange?.(!checked)}
    className={`shrink-0 flex items-center justify-center ${
      disabled ? "cursor-not-allowed" : "cursor-pointer"
    }`}
    style={{
      width: 38,
      height: 18,
      borderRadius: 99,
      fontFamily: "var(--qp-font-mono)",
      fontSize: 8,
      fontWeight: 700,
      letterSpacing: "0.5px",
      opacity: disabled ? 0.4 : 1,
      color: checked ? "#ffffff" : ink(0.45),
      backgroundColor: checked ? "#ff5d1e" : "transparent",
      border: `1px solid ${checked ? "#ff5d1e" : fill(0.18)}`,
      transition: "background-color 0.15s, color 0.15s, border-color 0.15s",
    }}
  >
    {checked ? "ON" : "OFF"}
  </button>
);
