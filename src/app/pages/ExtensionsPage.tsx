/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useState } from "react";
import { McpConnections } from "../extensions/McpConnections";
import { McpPresets } from "../extensions/McpPresets";
import { hashForRoute, type ExtensionViewId } from "../navigation";

export function ExtensionsPage({ view }: { view: ExtensionViewId }) {
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });
  const browse = view === "browse";

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        <div className="extensions-browse-view">
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
          {!browse && <McpConnections query={query} />}
        </div>
      </div>
    </section>
  );
}
