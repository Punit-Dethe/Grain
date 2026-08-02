import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => {
  const baseClasses =
    "px-2 py-1 text-sm font-medium bg-paper-sunken border border-line rounded-lg text-ink text-start transition-[background-color,border-color] duration-150 placeholder:text-ink-faint";

  const interactiveClasses = disabled
    ? "opacity-60 cursor-not-allowed bg-paper-sunken border-line"
    : "hover:bg-[var(--accent-tint)] hover:border-accent focus:outline-none focus:bg-[var(--accent-tint)] focus:border-accent";

  const variantClasses = {
    default: "px-3 py-2",
    compact: "px-2 py-1",
  } as const;

  return (
    <input
      className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
      disabled={disabled}
      {...props}
    />
  );
};
