import type { ReactNode } from "react";
import {
  ArrowUpRight,
  Bot,
  Boxes,
  Code2,
  Cpu,
  FileText,
  Sparkles,
  Trash2,
  WandSparkles,
} from "lucide-react";

const BROWSE_EXTENSIONS_LABEL = "Browse extensions for this feature";
const BROWSE_EXTENSIONS_ACTION = "Browse extensions";

function SurfaceIcon({ surface }: { surface: string }) {
  const Icon = surface.startsWith("snippets")
    ? Code2
    : surface.startsWith("context")
      ? Sparkles
      : surface.startsWith("agent")
        ? Bot
        : surface.startsWith("dictation")
          ? WandSparkles
          : surface.startsWith("grainspace")
            ? FileText
            : surface.startsWith("models")
              ? Cpu
              : Boxes;

  return <Icon size={20} strokeWidth={1.75} aria-hidden="true" />;
}

export function StudioExtensionCard({
  name,
  description,
  meta,
  badge,
  badgeTone = "verified",
  surface,
  primaryLabel,
  primaryDisabled = false,
  onPrimary,
  onRemove,
}: {
  name: string;
  description: string;
  meta: string;
  badge: string;
  badgeTone?: "verified" | "community" | "experimental" | "core" | "dev";
  surface: string;
  primaryLabel: string;
  primaryDisabled?: boolean;
  onPrimary: () => void;
  onRemove?: () => void;
}) {
  return (
    <article className="studio-extension-card">
      <div className="studio-extension-card-head">
        <span className="studio-extension-icon">
          <SurfaceIcon surface={surface} />
        </span>
        <div className="studio-extension-identity">
          <h3 title={name}>{name}</h3>
          <span>{meta}</span>
        </div>
      </div>
      <p>{description}</p>
      <div className="studio-extension-card-footer">
        <span className="studio-extension-badge" data-tone={badgeTone}>
          {badge}
        </span>
        <div className="studio-extension-actions">
          {onRemove && (
            <button
              className="studio-extension-remove"
              type="button"
              title={`Uninstall ${name}`}
              aria-label={`Uninstall ${name}`}
              disabled={primaryDisabled}
              onClick={onRemove}
            >
              <Trash2 size={15} aria-hidden="true" />
            </button>
          )}
          <button
            className="studio-extension-primary"
            type="button"
            disabled={primaryDisabled}
            onClick={onPrimary}
          >
            {primaryLabel}
          </button>
        </div>
      </div>
    </article>
  );
}

export function StudioExtensionMoreCard({
  title,
  description,
  surface,
  onClick,
  detail,
}: {
  title: string;
  description: string;
  surface: string;
  onClick: () => void;
  detail?: ReactNode;
}) {
  return (
    <button
      className="studio-extension-card studio-extension-more-card"
      type="button"
      onClick={onClick}
    >
      <span className="studio-extension-card-head">
        <span className="studio-extension-more-icon">
          <SurfaceIcon surface={surface} />
        </span>
        <span className="studio-extension-identity">
          <strong>{title}</strong>
          <span>{detail ?? "Extension Store"}</span>
        </span>
      </span>
      <span className="studio-extension-more-description">{description}</span>
      <span className="studio-extension-more-footer">
        {BROWSE_EXTENSIONS_ACTION}
        <ArrowUpRight size={15} aria-hidden="true" />
      </span>
      <span className="sr-only">{BROWSE_EXTENSIONS_LABEL}</span>
    </button>
  );
}
