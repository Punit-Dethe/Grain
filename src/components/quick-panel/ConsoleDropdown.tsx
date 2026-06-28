/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
/**
 * [GRAIN] Custom dropdown for the Quick Panel.
 *
 * Design goals:
 * - Beige/cream background matching --qp-input-bg, never the OS native widget.
 * - Thin rows (28 px) so the list doesn't feel cramped.
 * - Text truncation with ellipsis at a configurable max-width.
 * - Optional per-item rotation indicator: a small amber ◉ dot on the right
 *   edge when smart rotation is active for that item — no full toggle widget.
 * - Closes on outside click or Escape.
 */
import React, { useEffect, useRef, useState } from "react";
import { ink, fill } from "./widgets";

const MONO = "var(--qp-font-mono)";

export interface DropdownItem {
  /** Display label shown in the trigger and list. */
  label: string;
  /** Unique key (may differ from label). */
  key: string;
  /** When true, show the amber rotation dot on the right. */
  inRotation?: boolean;
}

interface ConsoleDropdownProps {
  /** Currently selected item key. */
  value: string;
  items: DropdownItem[];
  height?: number;
  /** Max px the label text may occupy before truncating. Default 160. */
  labelMaxWidth?: number;
  /** Whether smart rotation is globally on (controls dot visibility). */
  smartRotation?: boolean;
  /** Accent colour for the rotation dot. Default amber #F59E0B. */
  rotationColor?: string;
  onChange?: (key: string) => void;
  disabled?: boolean;
}

/** Amber rotation dot — shown on the right of a row when the provider is
 *  enrolled in smart rotation. Deliberately subtle: 6 px filled circle. */
const RotationDot: React.FC<{ color: string }> = ({ color }) => (
  <span
    title="In smart rotation"
    style={{
      width: 6,
      height: 6,
      borderRadius: "50%",
      backgroundColor: color,
      flexShrink: 0,
      display: "inline-block",
      boxShadow: `0 0 4px ${color}88`,
    }}
  />
);

export const ConsoleDropdown: React.FC<ConsoleDropdownProps> = ({
  value,
  items,
  height = 34,
  labelMaxWidth = 160,
  smartRotation = false,
  rotationColor = "#F59E0B",
  onChange,
  disabled = false,
}) => {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const selected = items.find((i) => i.key === value) ?? items[0];

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  const handleSelect = (key: string) => {
    setOpen(false);
    if (key !== value) onChange?.(key);
  };

  return (
    <div
      ref={rootRef}
      className="relative w-full"
      style={{ userSelect: "none" }}
    >
      {/* Trigger */}
      <button
        type="button"
        disabled={disabled}
        onClick={() => !disabled && setOpen((o) => !o)}
        className="w-full flex items-center"
        style={{
          height,
          borderRadius: 6,
          backgroundColor: "var(--qp-input-bg)",
          border: `1px solid ${open ? fill(0.22) : fill(0.1)}`,
          padding: "0 10px",
          gap: 6,
          cursor: disabled ? "not-allowed" : "pointer",
          opacity: disabled ? 0.45 : 1,
          transition: "border-color 0.12s",
        }}
      >
        {/* Selected label — truncated */}
        <span
          style={{
            flex: 1,
            fontSize: 11,
            fontWeight: 600,
            fontFamily: MONO,
            color: "var(--qp-input-text)",
            textAlign: "left",
            overflow: "hidden",
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
            maxWidth: labelMaxWidth,
          }}
        >
          {selected?.label ?? "—"}
        </span>

        {/* Rotation dot for the selected item */}
        {smartRotation && selected?.inRotation && (
          <RotationDot color={rotationColor} />
        )}

        {/* Chevron */}
        <span
          style={{
            fontSize: 9,
            color: ink(0.45),
            flexShrink: 0,
            transform: open ? "rotate(180deg)" : "rotate(0deg)",
            transition: "transform 0.15s",
            lineHeight: 1,
          }}
        >
          ▾
        </span>
      </button>

      {/* Dropdown list */}
      {open && (
        <div
          className="absolute left-0 right-0 z-50 overflow-hidden"
          style={{
            top: height + 3,
            borderRadius: 7,
            backgroundColor: "var(--qp-input-bg)",
            border: `1px solid ${fill(0.14)}`,
            boxShadow: "0 8px 24px rgba(0,0,0,0.18), 0 2px 6px rgba(0,0,0,0.12)",
          }}
        >
          {items.map((item, idx) => {
            const isSelected = item.key === value;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => handleSelect(item.key)}
                className="w-full flex items-center"
                style={{
                  height: 28,
                  padding: "0 10px",
                  gap: 6,
                  cursor: "pointer",
                  backgroundColor: isSelected ? fill(0.07) : "transparent",
                  borderBottom:
                    idx < items.length - 1
                      ? `1px solid ${fill(0.05)}`
                      : "none",
                  transition: "background-color 0.08s",
                }}
                onMouseEnter={(e) => {
                  if (!isSelected)
                    (e.currentTarget as HTMLElement).style.backgroundColor =
                      fill(0.04);
                }}
                onMouseLeave={(e) => {
                  if (!isSelected)
                    (e.currentTarget as HTMLElement).style.backgroundColor =
                      "transparent";
                }}
              >
                {/* Active tick */}
                <span
                  style={{
                    width: 8,
                    flexShrink: 0,
                    fontSize: 9,
                    color: isSelected ? ink(0.7) : "transparent",
                    fontFamily: MONO,
                  }}
                >
                  ✓
                </span>

                {/* Label — truncated */}
                <span
                  style={{
                    flex: 1,
                    fontSize: 11,
                    fontWeight: isSelected ? 700 : 600,
                    fontFamily: MONO,
                    color: isSelected ? ink(0.9) : ink(0.7),
                    textAlign: "left",
                    overflow: "hidden",
                    whiteSpace: "nowrap",
                    textOverflow: "ellipsis",
                    maxWidth: labelMaxWidth,
                  }}
                >
                  {item.label}
                </span>

                {/* Rotation dot — only when smart rotation is on */}
                {smartRotation && item.inRotation && (
                  <RotationDot color={rotationColor} />
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
};
