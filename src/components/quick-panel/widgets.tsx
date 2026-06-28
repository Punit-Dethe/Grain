/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useEffect, useRef, useState } from "react";

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

/** One selectable entry in a ConsoleDropdown. `toggleable` rows render a
 *  right-edge ON/OFF control instead of a selection (used for rotation members);
 *  `dotColor` shows a small leading status dot (e.g. green = key present). */
export interface DropdownOption {
  value: string;
  label: string;
  /** Leading status dot colour. Omit for no dot. */
  dotColor?: string;
  /** When set, the row shows an ON/OFF control reflecting this state. */
  enabled?: boolean;
}

/** Custom console dropdown — the single source of truth for every combo box in
 *  the quick panel (mic, unload timeout, local model, cloud providers). Closed
 *  it reads like the old native select; open it floats a themed option list.
 *
 *  Two modes:
 *   - SELECT (default): clicking a row fires `onSelect(value)` and closes.
 *   - TOGGLE (`toggleable`): each row carries an ON/OFF control wired to
 *     `onToggle(value, next)`; rows don't close the panel. ON state paints the
 *     label orange and fills the row with darker beige (`--qp-ctrl-box-bg`).
 *     The closed-state shows `placeholder` (e.g. "Configure providers") rather
 *     than a single value. */
export const ConsoleDropdown: React.FC<{
  /** Closed-state label for SELECT mode (the current value's label). */
  value?: string;
  /** Closed-state label override (TOGGLE mode shows this, e.g. "Configure providers"). */
  placeholder?: string;
  options: DropdownOption[];
  height?: number;
  disabled?: boolean;
  /** Leading dot colour for the CLOSED state (SELECT mode, e.g. local model). */
  closedDotColor?: string;
  toggleable?: boolean;
  onSelect?: (value: string) => void;
  onToggle?: (value: string, next: boolean) => void;
  /** Shown in the panel when there are no options. */
  emptyLabel?: string;
}> = ({
  value,
  placeholder,
  options,
  height = 34,
  disabled = false,
  closedDotColor,
  toggleable = false,
  onSelect,
  onToggle,
  emptyLabel = "Nothing here yet",
}) => {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Close on outside click / Escape — no listeners linger while closed.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  // Closed-state label.
  const closedLabel = toggleable
    ? (placeholder ?? "Configure")
    : (options.find((o) => o.value === value)?.label ??
      value ??
      placeholder ??
      "");

  return (
    <div ref={rootRef} className="relative w-full" style={{ opacity: disabled ? 0.4 : 1 }}>
      {/* Trigger */}
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        className={`relative w-full flex items-center ${
          disabled ? "cursor-not-allowed" : "cursor-pointer"
        }`}
        style={{
          height,
          borderRadius: 6,
          padding: closedDotColor ? "0 28px 0 24px" : "0 28px 0 10px",
          backgroundColor: "var(--qp-input-bg)",
          border: `1px solid ${open ? "#ff5d1e" : fill(0.1)}`,
          fontSize: 11,
          fontWeight: 600,
          color: "var(--qp-input-text)",
          textAlign: "left",
          transition: "border-color 0.15s",
        }}
      >
        {closedDotColor && (
          <span
            className="absolute"
            style={{
              left: 10,
              top: "50%",
              transform: "translateY(-50%)",
              width: 6,
              height: 6,
              borderRadius: 3,
              backgroundColor: closedDotColor,
            }}
          />
        )}
        <span className="flex-1 truncate">{closedLabel}</span>
        <span
          className="absolute pointer-events-none flex items-center justify-center"
          style={{
            right: 8,
            top: "50%",
            transform: `translateY(-50%) rotate(${open ? 180 : 0}deg)`,
            fontSize: 9,
            lineHeight: 1,
            color: "var(--qp-input-text)",
            transition: "transform 0.15s",
          }}
        >
          ▾
        </span>
      </button>

      {/* Panel */}
      {open && !disabled && (
        <div
          className="absolute left-0 right-0 z-50 qp-scroll"
          style={{
            top: height + 4,
            maxHeight: 220,
            overflowY: "auto",
            borderRadius: 6,
            backgroundColor: "var(--qp-input-bg)",
            border: `1px solid ${fill(0.15)}`,
            boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
            padding: 4,
          }}
        >
          {options.length === 0 ? (
            <div
              className="flex items-center justify-center"
              style={{ height: 30, fontSize: 11, fontWeight: 600, color: ink(0.4) }}
            >
              {emptyLabel}
            </div>
          ) : (
            options.map((o) => {
              const on = o.enabled ?? false;
              const selected = !toggleable && o.value === value;
              return (
                <div
                  key={o.value}
                  onClick={() => {
                    if (toggleable) {
                      onToggle?.(o.value, !on);
                    } else {
                      onSelect?.(o.value);
                      setOpen(false);
                    }
                  }}
                  className="flex items-center cursor-pointer"
                  style={{
                    height: 30,
                    borderRadius: 5,
                    padding: "0 8px 0 10px",
                    gap: 8,
                    // ON rows (toggle mode) fill darker beige; selected rows in
                    // select mode get a subtle highlight.
                    backgroundColor:
                      toggleable && on
                        ? "var(--qp-ctrl-box-bg)"
                        : selected
                          ? fill(0.06)
                          : "transparent",
                    transition: "background-color 0.12s",
                  }}
                >
                  {o.dotColor && (
                    <span
                      className="shrink-0"
                      style={{
                        width: 6,
                        height: 6,
                        borderRadius: 3,
                        backgroundColor: o.dotColor,
                      }}
                    />
                  )}
                  <span
                    className="flex-1 truncate"
                    style={{
                      fontSize: 11,
                      fontWeight: 600,
                      // ON => orange text; otherwise normal ink (dimmed if a
                      // toggle row is OFF).
                      color:
                        toggleable && on
                          ? "#ff5d1e"
                          : toggleable
                            ? ink(0.55)
                            : "var(--qp-input-text)",
                    }}
                  >
                    {o.label}
                  </span>
                  {toggleable ? (
                    <span
                      className="shrink-0"
                      style={{
                        fontFamily: MONO,
                        fontSize: 8,
                        fontWeight: 700,
                        letterSpacing: "0.5px",
                        color: on ? "#ff5d1e" : ink(0.4),
                      }}
                    >
                      {on ? "ON" : "OFF"}
                    </span>
                  ) : (
                    selected && (
                      <span
                        className="shrink-0"
                        style={{ fontSize: 11, fontWeight: 700, color: "#ff5d1e" }}
                      >
                        ✓
                      </span>
                    )
                  )}
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
};

/** Back-compat wrapper: the old string-list `ConsoleSelect` API now renders the
 *  custom ConsoleDropdown so every existing call site gets the themed look. */
export const ConsoleSelect: React.FC<{
  value?: string;
  options: string[];
  height?: number;
  disabled?: boolean;
  onChange?: (v: string) => void;
}> = ({ value, options, height = 34, disabled = false, onChange }) => (
  <ConsoleDropdown
    value={value}
    options={options.map((o) => ({ value: o, label: o }))}
    height={height}
    disabled={disabled}
    onSelect={onChange}
  />
);
