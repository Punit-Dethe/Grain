import React from "react";
import { AlertCircle, AlertTriangle, Info, CheckCircle } from "lucide-react";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  /** When true, removes rounded corners for use inside containers */
  contained?: boolean;
  children: React.ReactNode;
  className?: string;
}

// Warm, desaturated status tokens (see App.css) instead of raw red/yellow/blue/
// green — these tint the beige paper instead of fighting it.
const variantStyles: Record<
  AlertVariant,
  { container: string; icon: string; text: string }
> = {
  error: {
    container: "bg-[var(--status-error-tint)]",
    icon: "text-status-error",
    text: "text-status-error",
  },
  warning: {
    container: "bg-[var(--status-warn-tint)]",
    icon: "text-status-warn",
    text: "text-status-warn",
  },
  info: {
    container: "bg-[var(--status-info-tint)]",
    icon: "text-status-info",
    text: "text-status-info",
  },
  success: {
    container: "bg-[var(--status-ready-tint)]",
    icon: "text-status-ready",
    text: "text-status-ready",
  },
};

const variantIcons: Record<AlertVariant, React.ElementType> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert: React.FC<AlertProps> = ({
  variant = "error",
  contained = false,
  children,
  className = "",
}) => {
  const styles = variantStyles[variant];
  const Icon = variantIcons[variant];

  return (
    <div
      className={`flex items-start gap-3 p-4 ${styles.container} ${contained ? "" : "rounded-lg"} ${className}`}
    >
      <Icon className={`w-5 h-5 shrink-0 mt-0.5 ${styles.icon}`} />
      <p className={`text-sm ${styles.text}`}>{children}</p>
    </div>
  );
};
