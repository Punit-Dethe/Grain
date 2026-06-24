import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  const baseClasses =
    "font-medium rounded-lg border focus:outline-none transition-[background-color,border-color,color,transform] duration-150 active:scale-[0.97] disabled:opacity-50 disabled:cursor-not-allowed disabled:active:scale-100 cursor-pointer";

  const variantClasses = {
    primary:
      "text-[var(--on-accent)] bg-accent border-accent hover:bg-accent/85 focus:ring-1 focus:ring-accent",
    "primary-soft":
      "text-accent bg-[var(--accent-tint)] border-transparent hover:bg-[var(--accent-tint)]/80 focus:ring-1 focus:ring-accent",
    secondary:
      "bg-paper-sunken border-line hover:bg-[var(--accent-tint)] hover:border-accent focus:outline-none",
    danger:
      "text-white bg-status-error border-status-error hover:opacity-90 focus:ring-1 focus:ring-status-error",
    "danger-ghost":
      "text-status-error border-transparent hover:text-status-error hover:bg-[var(--status-error-tint)] focus:bg-[var(--status-error-tint)]",
    ghost:
      "text-ink border-transparent hover:bg-paper-sunken hover:border-line focus:bg-paper-sunken",
  };

  const sizeClasses = {
    sm: "px-2 py-1 text-xs",
    md: "px-4 py-[5px] text-sm",
    lg: "px-4 py-2 text-base",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
