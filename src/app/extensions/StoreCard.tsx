/**
 * [GRAIN] The store card, and the artwork it sits on.
 *
 * One component, two callers: the Extensions store grid and Studio's
 * contextual recommendations. Studio used to draw its own card — initials on a
 * coloured square, a different footer, no cover image — which meant the same
 * extension looked like two different products depending on where you met it,
 * and every change had to be made twice.
 */
import { useEffect, useState, type MouseEvent } from "react";
import { commands, type StoreEntry, type StoreMedia } from "@/bindings";
import { unwrapResult } from "./extensionRuntime";

export function MediaArtwork({
  media,
  icon,
  name,
  className,
  alt,
  placeholder = false,
}: {
  media?: StoreMedia;
  icon?: string | null;
  name: string;
  className: string;
  alt?: string;
  placeholder?: boolean;
}) {
  const mediaKey = media ? `${media.kind}:${media.sha256}` : "";
  const [resolved, setResolved] = useState<{
    key: string;
    url: string | null;
  } | null>(null);
  // Ignore an earlier image immediately, before the next effect fetch starts.
  const url = resolved?.key === mediaKey ? resolved.url : (icon ?? null);
  useEffect(() => {
    let alive = true;
    if (media) {
      void commands
        .storeMedia(media.sha256, media.kind)
        .then(unwrapResult)
        .then((value) => alive && setResolved({ key: mediaKey, url: value }))
        .catch(
          () => alive && setResolved({ key: mediaKey, url: icon ?? null }),
        );
    } else {
      setResolved(null);
    }
    return () => {
      alive = false;
    };
  }, [icon, media?.kind, media?.sha256, mediaKey]);

  return (
    <div
      className={className}
      data-extension-icon={!media && Boolean(icon) ? "true" : "false"}
      data-has-artwork={Boolean(url)}
    >
      {url ? (
        <img src={url} alt={alt ?? `${name} ${media ? "preview" : "icon"}`} />
      ) : placeholder ? (
        <span
          className="extension-icon-placeholder"
          role="img"
          aria-label={`${name} icon unavailable`}
        >
          {name.slice(0, 1).toLocaleUpperCase()}
        </span>
      ) : null}
    </div>
  );
}

export function StoreCard({
  entry,
  installedVersion,
  busy,
  canInstall,
  onInstall,
  onPreview,
}: {
  entry: StoreEntry;
  /** Installed version of this id, if any — drives Install / Update / Installed. */
  installedVersion?: string;
  busy: boolean;
  canInstall: boolean;
  onInstall: (entry: StoreEntry) => void;
  onPreview: (entry: StoreEntry) => void;
}) {
  const current = installedVersion === entry.version;

  return (
    <article
      className="extension-card polished-store-card"
      tabIndex={0}
      role="button"
      aria-label={`Preview ${entry.name}`}
      onClick={() => onPreview(entry)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onPreview(entry);
        }
      }}
    >
      <MediaArtwork
        media={entry.media[0]}
        name={entry.name}
        className="store-artwork"
      />
      <div className="store-extension-body">
        <div className="store-extension-head">
          <strong>
            <span className="store-extension-name">{entry.name}</span>
          </strong>
          <button
            className="button store-install"
            type="button"
            disabled={
              current || busy || entry.revocation === "revoked" || !canInstall
            }
            onClick={(event: MouseEvent) => {
              event.stopPropagation();
              onInstall(entry);
            }}
          >
            {busy
              ? "Installing…"
              : current
                ? "Installed"
                : installedVersion
                  ? "Update"
                  : "Install"}
          </button>
        </div>
        <p className="store-extension-blurb">{entry.description}</p>
      </div>
    </article>
  );
}
