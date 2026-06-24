import React, { useRef, useState } from "react";
import { Tooltip } from "./Tooltip";

interface InfoHintProps {
  text: string;
  position?: "top" | "bottom";
}

// [GRAIN] A standalone info "i" that reveals a styled tooltip on hover — the same
// affordance SettingContainer uses, extracted for use in custom headers.
export const InfoHint: React.FC<InfoHintProps> = ({ text, position = "top" }) => {
  const [show, setShow] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  return (
    <div
      ref={ref}
      className="relative inline-flex"
      onMouseEnter={() => setShow(true)}
      onMouseLeave={() => setShow(false)}
      onClick={() => setShow((s) => !s)}
    >
      <svg
        className="w-4 h-4 text-ink-faint cursor-help hover:text-accent transition-colors duration-200 select-none"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
        aria-label="More information"
        role="button"
        tabIndex={0}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
      {show && (
        <Tooltip targetRef={ref} position={position}>
          <p className="text-sm text-center leading-relaxed">{text}</p>
        </Tooltip>
      )}
    </div>
  );
};
