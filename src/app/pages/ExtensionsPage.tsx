/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useState } from "react";
import { McpConnections } from "../extensions/McpConnections";
import { BuiltInExtensions } from "../extensions/BuiltInExtensions";
import { ComposioExtensions } from "../extensions/ComposioExtensions";

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
        <McpConnections
          query={query}
          header={
            <header className="page-header">
              <div>
                <h1>Extensions</h1>
                <p className="page-subtitle">
                  Connect the apps and tools Grain can use.
                </p>
              </div>
            </header>
          }
        >
          {(customConnections) => (
            <>
              <BuiltInExtensions />
              <ComposioExtensions query={query} onQuery={setQuery} />
              {customConnections}
            </>
          )}
        </McpConnections>
      </div>
    </section>
  );
}
