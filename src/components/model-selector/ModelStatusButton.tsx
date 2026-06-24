import React from "react";

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

interface ModelStatusButtonProps {
  status: ModelStatus;
  displayText: string;
  isDropdownOpen: boolean;
  onClick: () => void;
  className?: string;
}

// Each state resolves to a warm, desaturated status token (see App.css) rather
// than a raw Tailwind hue, so the dot harmonizes with the beige paper. Only the
// transient states (loading/downloading/verifying/extracting) pulse; the rest
// are steady.
const STATUS_DOT: Record<ModelStatus, string> = {
  ready: "bg-status-ready",
  loading: "bg-status-load animate-pulse",
  downloading: "bg-accent animate-pulse",
  verifying: "bg-status-warn animate-pulse",
  extracting: "bg-status-warn animate-pulse",
  error: "bg-status-error",
  unloaded: "bg-status-idle",
  none: "bg-status-error",
};

const ModelStatusButton: React.FC<ModelStatusButtonProps> = ({
  status,
  displayText,
  isDropdownOpen,
  onClick,
  className = "",
}) => {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 hover:text-text/80 transition-colors ${className}`}
      title={`Model status: ${displayText}`}
    >
      <div className={`w-2 h-2 rounded-full ${STATUS_DOT[status] ?? "bg-status-idle"}`} />
      <span className="max-w-28 truncate">{displayText}</span>
      <svg
        className={`w-3 h-3 transition-transform ${isDropdownOpen ? "rotate-180" : ""}`}
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M19 9l-7 7-7-7"
        />
      </svg>
    </button>
  );
};

export default ModelStatusButton;
