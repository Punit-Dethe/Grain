/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ShieldCheck, Trash2 } from "lucide-react";
import { commands, type ExtensionCard } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import { useSettings } from "@/hooks/useSettings";
import { McpConnections } from "../extensions/McpConnections";
import { McpPresets } from "../extensions/McpPresets";
import { ExtensionApprovalDetails } from "../extensions/ExtensionApprovalDetails";
import { hashForRoute, type ExtensionViewId } from "../navigation";
import {
  capabilityLabel,
  filterExtensions,
  parseApprovalRequest,
  sortExtensionCards,
  unwrapResult,
  type ApprovalRequest,
} from "../extensions/extensionRuntime";

function useInstalledExtensions() {
  const { refreshSettings } = useSettings();
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [pending, setPending] = useState<
    ({ card: ExtensionCard } & ApprovalRequest) | null
  >(null);

  const refresh = useCallback(async () => {
    const overview = unwrapResult(await commands.extensionsOverview());
    setCards(sortExtensionCards(overview));
    setError(null);
    await refreshSettings();
  }, [refreshSettings]);

  useEffect(() => {
    let alive = true;
    void refresh()
      .catch((reason) => alive && setError(String(reason)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [refresh]);

  const toggle = useCallback(
    async (card: ExtensionCard) => {
      setBusy(card.id);
      setError(null);
      try {
        unwrapResult(
          await commands.extensionSetEnabled(card.id, !card.enabled),
        );
        await refresh();
      } catch (reason) {
        const message =
          reason instanceof Error ? reason.message : String(reason);
        const approval = parseApprovalRequest(message);
        if (approval)
          setPending({
            card: { ...card, name: approval.reviewedName },
            ...approval,
          });
        else setError(message);
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const uninstall = useCallback(
    async (card: ExtensionCard) => {
      if (
        !window.confirm(
          `Uninstall "${card.name}"?\n\nIts saved data is kept for a future reinstall.`,
        )
      ) {
        return false;
      }
      setBusy(card.id);
      setError(null);
      try {
        unwrapResult(await commands.extensionUninstall(card.id, false));
        await refresh();
        return true;
      } catch (reason) {
        setError(String(reason));
        return false;
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const approve = useCallback(async () => {
    if (!pending) return;
    const { card, permissions, approvalDigest } = pending;
    setPending(null);
    setBusy(card.id);
    try {
      unwrapResult(
        await commands.extensionGrant(card.id, permissions, approvalDigest),
      );
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(null);
    }
  }, [pending, refresh]);

  const permissionDialog = pending ? (
    <div
      className="extension-confirm-backdrop"
      role="presentation"
      onClick={() => setPending(null)}
    >
      <div
        className="extension-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="permission-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="extension-confirm-title">
          <ShieldCheck size={17} />
          <h2 id="permission-title">Allow “{pending.card.name}”?</h2>
        </div>
        <ExtensionApprovalDetails card={pending.card} approval={pending} />
        <div className="extension-confirm-actions">
          <button
            className="button"
            type="button"
            onClick={() => setPending(null)}
          >
            Cancel
          </button>
          <button
            className="button primary"
            type="button"
            onClick={() => void approve()}
          >
            Allow and enable
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return {
    cards,
    loading,
    error,
    busy,
    refresh,
    toggle,
    uninstall,
    permissionDialog,
  };
}

export function ExtensionsPage({ view }: { view: ExtensionViewId }) {
  const controller = useInstalledExtensions();
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });
  const [detailId, setDetailId] = useState<string | null>(null);
  const origin = useRef<HTMLButtonElement | null>(null);
  const back = useRef<HTMLButtonElement>(null);
  const detail = controller.cards.find((card) => card.id === detailId);
  const browse = view === "browse";

  const close = useCallback(() => {
    setDetailId(null);
    requestAnimationFrame(() => origin.current?.focus());
  }, []);

  useEffect(() => {
    setDetailId(null);
  }, [view]);

  useEffect(() => {
    if (!detailId) return;
    back.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close, detailId]);

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        {controller.error && (
          <div className="extension-inline-error" role="alert">
            {controller.error}
          </div>
        )}
        {detail && (
          <article
            className="extension-detail"
            aria-labelledby="extension-detail-title"
          >
            <div className="extension-detail-toolbar">
              <button
                ref={back}
                className="extension-detail-back-button"
                type="button"
                onClick={close}
              >
                <ChevronLeft size={15} aria-hidden="true" /> Back to extensions
              </button>
            </div>
            <div className="extension-detail-content">
              <header className="extension-detail-intro">
                <div className="extension-detail-heading">
                  <h1 id="extension-detail-title">{detail.name}</h1>
                  <p>{detail.description}</p>
                </div>
                {detail.trust !== "dev" && (
                  <button
                    className="button danger"
                    type="button"
                    disabled={controller.busy !== null}
                    onClick={() =>
                      void controller.uninstall(detail).then((removed) => {
                        if (removed) close();
                      })
                    }
                  >
                    <Trash2 size={14} aria-hidden="true" /> Uninstall
                  </button>
                )}
              </header>
              <section
                className="mcp-connections"
                aria-label="Extension settings"
              >
                <div className="extension-setting-row">
                  <div>
                    <h3>Enabled</h3>
                    <p>Make this extension’s tools available to the Agent.</p>
                  </div>
                  <Switch
                    checked={detail.enabled}
                    disabled={controller.busy !== null}
                    ariaLabel={`${detail.enabled ? "Disable" : "Enable"} ${detail.name}`}
                    onChange={() => void controller.toggle(detail)}
                  />
                </div>
                <h3>Declared permissions</h3>
                {detail.capabilities.length ? (
                  <ul>
                    {detail.capabilities.map((capability) => (
                      <li key={capability}>{capabilityLabel(capability)}</li>
                    ))}
                  </ul>
                ) : (
                  <p>This extension declares no runtime permissions.</p>
                )}
              </section>
            </div>
          </article>
        )}
        <div className="extensions-browse-view" hidden={Boolean(detail)}>
          <div className="page-header">
            <div className="extensions-page-heading">
              <h1>Extensions</h1>
              <p className="page-subtitle">
                Connect a supported app or add your own remote MCP server.
              </p>
            </div>
          </div>
          <div className="extension-toolbar">
            <div className="segmented" aria-label="Extension collection">
              {(["installed", "browse"] as const).map((tab) => (
                <button
                  key={tab}
                  className={view === tab ? "active" : ""}
                  aria-pressed={view === tab}
                  type="button"
                  onClick={() => {
                    window.location.hash = hashForRoute({
                      page: "extensions",
                      view: tab,
                    });
                  }}
                >
                  {tab === "installed" ? "My connections" : "Browse extensions"}
                </button>
              ))}
            </div>
            <label className="search-field">
              <svg className="icon sm" aria-hidden="true">
                <use href="#i-search" />
              </svg>
              <input
                aria-label="Search extensions"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search extensions"
              />
            </label>
          </div>
          <McpPresets query={query} browse={browse} />
          {!browse && (
            <>
              <McpConnections query={query} />
              {controller.loading && (
                <div role="status">Loading installed extensions…</div>
              )}
              <div className="installed-extension-list">
                {filterExtensions(controller.cards, query).map((card) => (
                  <article
                    key={card.id}
                    className="extension-card installed-extension-card"
                  >
                    <div className="installed-extension-copy">
                      <strong>{card.name}</strong>
                      <p>{card.description}</p>
                    </div>
                    <div className="installed-extension-actions">
                      <button
                        className="button"
                        type="button"
                        onClick={(event) => {
                          origin.current = event.currentTarget;
                          setDetailId(card.id);
                        }}
                      >
                        Settings
                      </button>
                      <Switch
                        checked={card.enabled}
                        disabled={controller.busy !== null}
                        ariaLabel={`${card.enabled ? "Disable" : "Enable"} ${card.name}`}
                        onChange={() => void controller.toggle(card)}
                      />
                    </div>
                  </article>
                ))}
              </div>
            </>
          )}
        </div>
      </div>
      {controller.permissionDialog}
    </section>
  );
}
