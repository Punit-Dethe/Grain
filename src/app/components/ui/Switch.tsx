import React from "react";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  ariaLabel?: string;
}

// [GRAIN] Bare toggle (no label/row) — the same switch as ToggleSwitch, for
// inline use in custom headers. Same geometry and colors: --toggle-off track
// OFF, accent track ON, white thumb that slides 16px.
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
      <div className="relative w-9 h-5 rounded-full bg-[var(--toggle-off)] transition-colors duration-200 peer-checked:bg-[var(--color-accent)] peer-focus-visible:outline-none peer-focus-visible:ring-2 peer-focus-visible:ring-[var(--accent-focus)] peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:shadow-[0_1px_2px_rgba(0,0,0,0.28)] after:transition-transform after:duration-200 peer-checked:after:translate-x-4 rtl:peer-checked:after:-translate-x-4" />
    </label>
  );
};
