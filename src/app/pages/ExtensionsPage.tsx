/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Integrations surface. */
import { useState } from "react";
import { McpConnections } from "../extensions/McpConnections";
import { McpPresets } from "../extensions/McpPresets";

export function ExtensionsPage() {
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        <header className="page-header">
          <div>
            <h1>Integrations</h1>
            <p className="page-subtitle">
              Connect the apps and tools Grain can use.
            </p>
          </div>
        </header>
        <McpConnections
          query={query}
          search={
            <label className="search-field">
              <svg className="icon sm" aria-hidden="true">
                <use href="#i-search" />
              </svg>
              <input
                aria-label="Search integrations"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search integrations"
              />
            </label>
          }
        >
          {(customConnections) => (
            <McpPresets query={query} customConnections={customConnections} />
          )}
        </McpConnections>
      </div>
    </section>
  );
}
