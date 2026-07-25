import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, ChevronDown, Download, ExternalLink, ShieldCheck } from "lucide-react";
import {
  ExtensionSettings,
  ExtensionShortcuts,
  type SettingRow,
  type SettingsSection,
} from "./ExtensionSettings";
import { Markdown } from "./markdown";

export type DetailMedia = { sha256: string; kind: string };

/** [GRAIN] The single source of truth a detail view renders — populated the
 * SAME way whether it is opened from the store (to install) or from the
 * installed list (to manage). No duplicated header UI, one data shape. */
export type DetailMeta = {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: string;
  trust: string;
  repository: string | null;
  installs: number;
  /** README media hash (empty = none). */
  readme: string;
  /** Screenshots / GIFs; media[0] is the cover. */
  media: DetailMedia[];
};

const TIER_LABEL: Record<string, string> = {
  builtin: "built-in",
  pack: "pack",
  scripted: "scripted",
  native: "native",
};
const TRUST_BADGE: Record<string, { label: string; cls: string }> = {
  core: { label: "Core", cls: "bg-accent/15 text-accent" },
  verified: { label: "Verified", cls: "bg-emerald-500/15 text-emerald-600" },
  experimental: { label: "Experimental", cls: "bg-amber-500/15 text-amber-600" },
  community: { label: "Community", cls: "bg-line text-ink-soft" },
  dev: { label: "Community", cls: "bg-line text-ink-soft" },
};

const fmtInstalls = (n: number) =>
  n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n);

const repoLabelOf = (repo: string) =>
  repo
    .replace(/^https?:\/\/(www\.)?github\.com\//i, "")
    .replace(/\.git$/, "")
    .replace(/\/$/, "");

/** Lazily resolve a media blob to a data URL (dropped when the detail unmounts,
 * so nothing persists in RAM). */
function useMedia(m: DetailMedia | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!m) {
      setUrl(null);
      return;
    }
    let alive = true;
    invoke<string>("store_media", { sha256: m.sha256, kind: m.kind })
      .then((u) => alive && setUrl(u))
      .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [m?.sha256, m?.kind]);
  return url;
}

/** Lazily fetch the README markdown by hash. */
function useReadme(hash: string): string | null {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    if (!hash) {
      setText(null);
      return;
    }
    let alive = true;
    invoke<string>("store_readme", { sha256: hash })
      .then((t) => alive && setText(t))
      .catch(() => alive && setText(null));
    return () => {
      alive = false;
    };
  }, [hash]);
  return text;
}

export const Cover: React.FC<{
  media: DetailMedia | undefined;
  name: string;
  rounded?: string;
}> = ({ media, name, rounded = "rounded-xl" }) => {
  const url = useMedia(media);
  if (!media) return null;
  return (
    <div
      className={`w-full aspect-[16/9] ${rounded} overflow-hidden border border-line bg-paper-sunken`}
    >
      {url ? (
        <img src={url} alt={`${name} preview`} className="w-full h-full object-cover" />
      ) : (
        <div className="w-full h-full animate-pulse bg-paper-sunken" />
      )}
    </div>
  );
};

/** [GRAIN] The unified extension detail (SPEC §5.1). The header — cover image on
 * top, title, description, installs + GitHub — is identical whether you reach it
 * from the store or from your installed list. Below it:
 *   · an extension with IN-PLACE settings shows them (header compacts, its
 *     description collapses);
 *   · anything else (a store preview, or an extension whose settings are a
 *     follow-up anchored elsewhere) shows the full README.
 * So the same words and picture are authored once and read from one place. */
export const ExtensionDetail: React.FC<{
  meta: DetailMeta;
  onBack: () => void;
  /** Store mode: the install/update action. */
  install?: {
    label: string;
    disabled: boolean;
    onInstall: () => void;
  };
  /** Installed mode: enable toggle + this extension's own settings, if any. */
  installed?: {
    enabled: boolean;
    busy: boolean;
    onToggle: () => void;
    section?: SettingsSection;
    ownRows: SettingRow[];
    onChanged: () => void;
  };
}> = ({ meta, onBack, install, installed }) => {
  const hasInPlaceSettings = !!installed?.section && installed.ownRows.length > 0;
  const [descOpen, setDescOpen] = useState(!hasInPlaceSettings);
  const readme = useReadme(hasInPlaceSettings ? "" : meta.readme);
  const badge = TRUST_BADGE[meta.trust] ?? TRUST_BADGE.dev;
  const cover = meta.media[0];
  const gallery = meta.media.slice(1);

  return (
    <div className="space-y-5">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1 text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
      >
        <ChevronLeft width={13} height={13} />
        All extensions
      </button>

      {/* Cover image on top (unless we are compacting for in-place settings). */}
      {!hasInPlaceSettings && <Cover media={cover} name={meta.name} />}

      {/* Title row + primary action. */}
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0 space-y-1.5">
          <div className="flex items-center gap-2 flex-wrap">
            <h1 className="text-2xl font-semibold tracking-tight leading-none text-ink">
              {meta.name}
            </h1>
            <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${badge.cls}`}>
              {(meta.trust === "verified" || meta.trust === "core") && (
                <ShieldCheck width={9} height={9} className="inline mr-0.5 -mt-0.5" />
              )}
              {badge.label}
            </span>
            <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-paper-sunken text-ink-faint border border-line">
              {TIER_LABEL[meta.tier] ?? meta.tier} · v{meta.version}
            </span>
          </div>
          {/* installs + GitHub. */}
          <div className="flex items-center gap-3 text-xs text-ink-faint flex-wrap">
            {meta.installs > 0 && (
              <span className="inline-flex items-center gap-1">
                <Download width={12} height={12} />
                {fmtInstalls(meta.installs)} installs
              </span>
            )}
            {meta.repository && (
              <a
                href={meta.repository}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 hover:text-ink transition-colors"
              >
                <ExternalLink width={12} height={12} />
                {repoLabelOf(meta.repository)}
              </a>
            )}
          </div>
        </div>

        {install && (
          <button
            type="button"
            disabled={install.disabled}
            onClick={install.onInstall}
            className="shrink-0 px-3 py-1.5 rounded-lg text-sm font-medium bg-accent text-white hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer transition-opacity"
          >
            {install.label}
          </button>
        )}
        {installed && (
          <button
            type="button"
            role="switch"
            aria-checked={installed.enabled}
            aria-label={`Enable ${meta.name}`}
            disabled={installed.busy}
            onClick={installed.onToggle}
            className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer shrink-0 mt-1 ${
              installed.enabled ? "bg-accent" : "bg-paper-sunken border border-line"
            } ${installed.busy ? "opacity-50" : ""}`}
          >
            <span
              className={`absolute top-0.5 w-4 h-4 rounded-full bg-paper-raised shadow transition-all ${
                installed.enabled ? "left-[18px]" : "left-0.5"
              }`}
            />
          </button>
        )}
      </div>

      {/* Description — full by default; collapsible when it sits above settings. */}
      {meta.description &&
        (hasInPlaceSettings ? (
          <div>
            <button
              type="button"
              onClick={() => setDescOpen((o) => !o)}
              className="flex items-center gap-1 text-xs text-ink-faint hover:text-ink-soft transition-colors cursor-pointer"
            >
              <ChevronDown
                width={12}
                height={12}
                className={`transition-transform ${descOpen ? "" : "-rotate-90"}`}
              />
              About
            </button>
            {descOpen && (
              <p className="mt-1 text-sm text-ink-soft leading-relaxed max-w-2xl">
                {meta.description}
              </p>
            )}
          </div>
        ) : (
          <p className="text-sm text-ink-soft leading-relaxed max-w-2xl">
            {meta.description}
          </p>
        ))}

      {/* Body: in-place settings, else the README. */}
      {hasInPlaceSettings ? (
        <ExtensionSettings
          section={installed!.section!}
          rows={installed!.ownRows}
          onChanged={installed!.onChanged}
        />
      ) : (
        <>
          {readme && (
            <div className="rounded-xl border border-line bg-paper-raised px-5 py-4">
              <Markdown text={readme} />
            </div>
          )}
          {gallery.length > 0 && (
            <div className="grid grid-cols-2 gap-3">
              {gallery.map((m) => (
                <Cover key={m.sha256} media={m} name={meta.name} />
              ))}
            </div>
          )}
        </>
      )}

      {installed && <ExtensionShortcuts id={meta.id} />}
    </div>
  );
};
