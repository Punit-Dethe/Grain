/* eslint-disable i18next/no-literal-string -- UI 2.0 prototype copy is a frozen visual contract until the cutover translation pass. */
import { useEffect, useState } from "react";
import { Layers } from "lucide-react";

import { commands, type ExtensionCard } from "@/bindings";
import {
  promptLayerScope,
  promptStackRows,
  type PromptStackRow,
} from "../../../extensions/promptStack";

const SOURCE_LABEL: Record<PromptStackRow["source"], string> = {
  you: "You",
  extension: "Extension",
  grain: "Grain",
};

/**
 * What can shape a dictation, in authority order.
 *
 * This exists because one of these rungs is invisible: an extension's prompt
 * layer changes what the model does to the user's own words, needs no
 * capability, and after the approval sheet is dismissed there was nowhere to
 * read it again. The rest of the ladder is here because a list with one entry
 * explains nothing — the point is the ORDER, and that the user's own words sit
 * above anything a third party contributes.
 */
export function PromptStackPanel({
  contextAwarenessEnabled,
  customProfileCount,
  basePromptName,
}: {
  contextAwarenessEnabled: boolean;
  customProfileCount: number;
  basePromptName: string | null;
}) {
  const [cards, setCards] = useState<ExtensionCard[]>([]);

  useEffect(() => {
    let alive = true;
    void commands
      .extensionsOverview()
      .then((result) => {
        if (alive && result.status === "ok") setCards(result.data);
      })
      // An unreadable registry must not take the Context Aware tab down; the
      // ladder simply shows no extension rows.
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const rows = promptStackRows({
    contextAwarenessEnabled,
    customProfileCount,
    basePromptName,
    cards,
  });

  return (
    <section className="prompt-stack" aria-label="What shapes your dictation">
      <header className="prompt-stack-header">
        <Layers size={14} strokeWidth={1.8} aria-hidden="true" />
        <div>
          <h3>What shapes your dictation</h3>
          <p>
            Highest authority first. Nothing an extension adds can outrank what
            you typed or said.
          </p>
        </div>
      </header>
      <ol className="prompt-stack-rows">
        {rows.map((row) => (
          <li
            key={row.key}
            className={row.active ? undefined : "prompt-stack-inactive"}
          >
            <div className="prompt-stack-row-head">
              <span className={`prompt-stack-source is-${row.source}`}>
                {SOURCE_LABEL[row.source]}
              </span>
              <span className="prompt-stack-title">{row.title}</span>
            </div>
            <p className="prompt-stack-detail">{row.detail}</p>
            {row.layers.length > 0 && (
              <ul className="prompt-stack-layers">
                {row.layers.map((layer) => (
                  <li key={layer.id}>
                    <span className="prompt-stack-scope">
                      {promptLayerScope(layer)}
                    </span>
                    {/* Verbatim, always. This is the text the model receives;
                        a summary here would describe something the user never
                        agreed to. */}
                    <q>{layer.text}</q>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
