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
  name,
  className,
}: {
  media?: StoreMedia;
  name: string;
  className: string;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (media) {
      void commands
        .storeMedia(media.sha256, media.kind)
        .then(unwrapResult)
        .then((value) => alive && setUrl(value))
        .catch(() => alive && setUrl(null));
    }
    return () => {
      alive = false;
      setUrl(null);
    };
  }, [media?.kind, media?.sha256]);

  return (
    <div className={className}>
      {url && <img src={url} alt={`${name} preview`} />}
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
