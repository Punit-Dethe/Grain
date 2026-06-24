import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  /** Optional two-digit section index (e.g. "01") shown in mono before the title. */
  index?: string;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  index,
  children,
}) => {
  return (
    <div className="space-y-2.5">
      {(title || description) && (
        <div className="space-y-1.5">
          {title && (
            <div className="flex items-center gap-2.5 px-1">
              {index && (
                // A printed reference marker: the numeral set solid in the
                // accent with a fine dot-matrix over it — a label stamped on
                // the well below, not just text.
                <span className="dither-fine font-mono text-[0.6rem] font-semibold text-[var(--on-accent)] bg-accent tabular-nums rounded-[3px] w-[1.35rem] h-[1.05rem] flex items-center justify-center leading-none">
                  {index}
                </span>
              )}
              <h2 className="font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
                {title}
              </h2>
              {/* The rule runs out to a small patch jack — a socket like the
                  Quick Panel console's, so each settings group reads as a
                  module on the same instrument. */}
              <div className="flex-1 flex items-center gap-2 translate-y-[-1px]">
                <span className="flex-1 border-t border-line" />
                <span className="grid place-items-center w-2.5 h-2.5 rounded-full border border-[var(--line-strong)] bg-paper shrink-0">
                  <span className="w-1 h-1 rounded-full bg-ink-faint/60" />
                </span>
              </div>
            </div>
          )}
          {description && (
            // Sits flush under the eyebrow title. When an index chip leads the
            // title, indent past it so the caption aligns under the words.
            <p
              className={`px-1 text-xs leading-snug text-ink-soft ${
                index ? "ps-[2.6rem]" : ""
              }`}
            >
              {description}
            </p>
          )}
        </div>
      )}
      {/* A recessed well of darker beige, debossed into the page — the holder
          for this group's rows (each row lights up under the cursor). */}
      <div className="surface-well overflow-visible">
        <div className="divide-y divide-line overflow-visible">{children}</div>
      </div>
    </div>
  );
};
