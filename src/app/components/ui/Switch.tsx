import React from "react";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  ariaLabel?: string;
}

// [GRAIN] Bare mechanical toggle (no label/row) — the same hardware switch as
// ToggleSwitch, for inline use in custom headers. Charcoal track; the lever turns
// orange when ON.
export const Switch: React.FC<SwitchProps> = ({
  checked,
  onChange,
  disabled = false,
  isUpdating = false,
  ariaLabel,
}) => {
  const locked = disabled || isUpdating;
  return (
    <label
      className={`relative inline-flex items-center transition-transform duration-100 ${
        locked ? "cursor-not-allowed" : "cursor-pointer active:scale-90"
      }`}
    >
      <input
        type="checkbox"
        className="sr-only peer"
        checked={checked}
        disabled={locked}
        aria-label={ariaLabel}
        onChange={(e) => onChange(e.target.checked)}
      />
      <div className="relative w-8 h-[18px] rounded-full bg-ink transition-colors duration-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-[var(--focus)] peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-[14px] after:w-[14px] after:rounded-full after:bg-[linear-gradient(180deg,#eeeeee,#bbbbbb)] after:shadow-[0_1px_2px_rgba(0,0,0,0.3)] after:transition-all after:duration-200 peer-checked:after:translate-x-[14px] peer-checked:after:bg-[linear-gradient(180deg,#ff5d1e,#d94a12)] rtl:peer-checked:after:-translate-x-[14px]" />
    </label>
  );
};
