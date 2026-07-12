import React from "react";
import { ArrowRight } from "lucide-react";

/** [GRAIN] Shared building blocks for the Extensions composers, so Snippets,
 * Actions, and Modes read as one instrument. All are text-content-free (they
 * only render children/icons) to keep copy in the calling component. */

/** Mono micro-cap label that sits above a field — self-documents the input so
 * placeholders can stay short and headings stay clean. */
export const FieldLabel: React.FC<{
  children: React.ReactNode;
  htmlFor?: string;
  className?: string;
}> = ({ children, htmlFor, className }) => (
  <label
    htmlFor={htmlFor}
    className={`block font-mono text-[0.58rem] font-semibold uppercase tracking-[0.15em] text-ink-faint ${
      className ?? ""
    }`}
  >
    {children}
  </label>
);

/** A saved trigger phrase rendered as a compact keycap-style chip. */
export const TriggerChip: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <span className="inline-flex items-center max-w-full truncate font-mono text-xs font-medium text-ink bg-paper-sunken border border-line rounded-md px-2 py-[3px] leading-none">
    {children}
  </span>
);

/** The trigger → result glyph used in list rows and the composer. */
export const MapArrow: React.FC<{ className?: string }> = ({ className }) => (
  <ArrowRight
    width={14}
    height={14}
    className={`shrink-0 text-ink-faint ${className ?? ""}`}
  />
);
