import React from "react";
import { SettingContainer } from "./SettingContainer";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  tooltipPosition?: "top" | "bottom";
}

export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({
  checked,
  onChange,
  disabled = false,
  isUpdating = false,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  tooltipPosition = "top",
}) => {
  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition={tooltipPosition}
    >
      <label
        className={`inline-flex items-center transition-transform duration-100 active:scale-90 ${disabled || isUpdating ? "cursor-not-allowed active:scale-100" : "cursor-pointer"}`}
      >
        <input
          type="checkbox"
          value=""
          className="sr-only peer"
          checked={checked}
          disabled={disabled || isUpdating}
          onChange={(e) => onChange(e.target.checked)}
        />
        {/* [GRAIN] Single source of truth for the settings toggle. Geometry is a
            matched set: 36×20 track, 16px white thumb inset 2px, travels 16px
            (translate-x-4) → even 2px margins, slides cleanly, cannot overflow.
            OFF track = --toggle-off (theme-aware); ON track = accent. To resize,
            change track/thumb/travel together: travel = trackW − thumb − 2·inset. */}
        <div className="relative w-9 h-5 rounded-full bg-[var(--toggle-off)] transition-colors duration-200 peer-checked:bg-[var(--color-accent)] peer-focus-visible:outline-none peer-focus-visible:ring-2 peer-focus-visible:ring-[var(--accent-focus)] peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:shadow-[0_1px_2px_rgba(0,0,0,0.28)] after:transition-transform after:duration-200 peer-checked:after:translate-x-4 rtl:peer-checked:after:-translate-x-4"></div>
      </label>
      {isUpdating && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
