import React, { useLayoutEffect, useRef } from "react";

interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: "default" | "compact";
  /** Grow with content from one line up to `maxRows`, then scroll. When set,
   * the fixed min-height of the variant is dropped so the field starts single-
   * line — removing the tall empty box that broke composer symmetry. */
  autoResize?: boolean;
  maxRows?: number;
}

export const Textarea: React.FC<TextareaProps> = ({
  className = "",
  variant = "default",
  autoResize = false,
  maxRows = 3,
  ...props
}) => {
  const ref = useRef<HTMLTextAreaElement>(null);

  // JS auto-resize (works across WebView2 versions): reset to auto, then clamp
  // to maxRows worth of line-height and toggle the scrollbar past that.
  useLayoutEffect(() => {
    if (!autoResize) return;
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    const cs = getComputedStyle(el);
    const line = parseFloat(cs.lineHeight) || 20;
    const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    const border =
      parseFloat(cs.borderTopWidth) + parseFloat(cs.borderBottomWidth);
    const max = line * maxRows + pad + border;
    el.style.height = `${Math.min(el.scrollHeight, max)}px`;
    el.style.overflowY = el.scrollHeight > max ? "auto" : "hidden";
  }, [props.value, autoResize, maxRows]);

  const baseClasses =
    "px-2 py-1 text-sm font-medium bg-paper-sunken border border-line rounded-lg text-ink text-start transition-[background-color,border-color] duration-150 placeholder:text-ink-faint hover:bg-[var(--accent-tint)] hover:border-accent focus:outline-none focus:bg-[var(--accent-tint)] focus:border-accent";

  const variantClasses = {
    default: autoResize ? "px-3 py-2" : "px-3 py-2 min-h-[100px]",
    compact: autoResize ? "px-2 py-1.5" : "px-2 py-1 min-h-[80px]",
  };

  const resizeClasses = autoResize
    ? "resize-none overflow-hidden leading-[1.5]"
    : "resize-y";

  return (
    <textarea
      ref={ref}
      rows={autoResize ? 1 : props.rows}
      className={`${baseClasses} ${variantClasses[variant]} ${resizeClasses} ${className}`}
      {...props}
    />
  );
};
