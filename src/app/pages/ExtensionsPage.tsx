import { useState } from "react";
import { McpConnections } from "../extensions/McpConnections";
import { McpPresets } from "../extensions/McpPresets";
import { hashForRoute, type ExtensionViewId } from "../navigation";

export function ExtensionsPage({ view }: { view: ExtensionViewId }) {
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });
  const [customQuery, setCustomQuery] = useState("");
  const browse = view === "browse";
  const configure = view === "configure";
  const searchLabel = configure ? "Search MCP connections" : "Search extensions";

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        <div className="extensions-browse-view">
          <div className="page-header">
            <div className="extensions-page-heading">
              <h1>{configure ? "Configure MCP" : "Extensions"}</h1>
              <p className="page-subtitle">
                {configure
                  ? "Add and manage your own remote MCP servers."
                  : "Browse supported apps and manage your connections."}
              </p>
            </div>
            <div className="header-actions">
              <a
                className="button"
                href={hashForRoute({
                  page: "extensions",
                  view: configure ? "installed" : "configure",
                })}
              >
                {configure ? "Back to extensions" : "Configure MCP"}
              </a>
            </div>
          </div>
          <div className="extension-toolbar">
            {!configure && (
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
                    {tab === "installed"
                      ? "My connections"
                      : "Browse extensions"}
                  </button>
                ))}
              </div>
            )}
            <label className="search-field">
              <svg className="icon sm" aria-hidden="true">
                <use href="#i-search" />
              </svg>
              <input
                aria-label={searchLabel}
                value={configure ? customQuery : query}
                onChange={(event) =>
                  (configure ? setCustomQuery : setQuery)(event.target.value)
                }
                placeholder={searchLabel}
              />
            </label>
          </div>
          {configure ? (
            <McpConnections query={customQuery} />
          ) : (
            <McpPresets query={query} browse={browse} />
          )}
        </div>
      </div>
    </section>
  );
}
