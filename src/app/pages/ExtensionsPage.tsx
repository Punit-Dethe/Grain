/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useState } from "react";
import { McpConnections } from "../extensions/McpConnections";
import { BuiltInExtensions } from "../extensions/BuiltInExtensions";
import {
  ComposioBrowse,
  ComposioConnected,
  ComposioError,
  RemoveComposio,
  useComposio,
} from "../extensions/Composio";

export function ExtensionsPage() {
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });
  const composio = useComposio();

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        <McpConnections
          query={query}
          header={<h1 className="ext-title">Extensions</h1>}
          actions={<RemoveComposio composio={composio} />}
          toolbar={
            <label className="search-field ext-search">
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
          }
        >
          {(customConnections) => (
            <>
              <ComposioError composio={composio} />
              <section className="ext-section" aria-labelledby="ext-connected">
                <h2 className="ext-label" id="ext-connected">
                  Connected
                </h2>
                <div className="ext-list">
                  <BuiltInExtensions query={query} />
                  <ComposioConnected composio={composio} query={query} />
                </div>
              </section>
              {customConnections}
              <ComposioBrowse composio={composio} query={query} />
            </>
          )}
        </McpConnections>
      </div>
    </section>
  );
}
