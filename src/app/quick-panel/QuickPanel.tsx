// [GRAIN] Quick Panel — the ⌘/Ctrl-K command palette. Stage 1: fuzzy keyword
// search over settings + navigation (no embedding model). Opening it with an
// empty query shows a compact map (pages + settings sections); typing searches
// every reachable setting by title and alias.

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { hashForRoute } from "../navigation";
import { scoreItem } from "./fuzzy";
import { requestSettingFocus } from "./focus";
import {
  buildQuickItems,
  type QuickIconName,
  type QuickItem,
} from "./registry";

// Panel chrome copy lives in one object, mirroring the shell's PROTOTYPE_COPY:
// it keeps literals out of JSX (the i18next lint rule) and gives a future
// translation pass a single place to hook.
const QP_COPY = {
  esc: "Esc",
  empty: "No matches",
} as const;

interface QuickPanelProps {
  open: boolean;
  onClose: () => void;
}

function QpIcon({ name }: { name: QuickIconName }) {
  return (
    <svg className="icon" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

function rank(items: QuickItem[], query: string): QuickItem[] {
  const q = query.trim();
  if (q.length === 0) return items;

  const scored: { item: QuickItem; score: number }[] = [];
  for (const item of items) {
    const base = scoreItem(q, item.title, item.keywords);
    if (base === null) continue;
    // Nudge pages/sections above individual settings on near-ties.
    const bias = item.kind === "navigate" ? 6 : item.kind === "section" ? 3 : 0;
    scored.push({ item, score: base + bias });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map((entry) => entry.item);
}

export function QuickPanel({ open, onClose }: QuickPanelProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const items = useMemo(() => buildQuickItems(t, settings), [t, settings]);

  // Empty query = compact map (drop the ~40 individual settings); typing =
  // search everything.
  const results = useMemo(() => {
    const searching = query.trim().length > 0;
    const pool = searching
      ? items
      : items.filter((item) => item.kind !== "setting");
    return rank(pool, query);
  }, [items, query]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    const id = window.setTimeout(() => inputRef.current?.focus(), 30);
    return () => window.clearTimeout(id);
  }, [open]);

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-idx="${active}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [active, results]);

  if (!open) return null;

  const select = (item: QuickItem | undefined) => {
    if (!item) return;
    if (item.kind === "setting" && item.section) {
      requestSettingFocus(item.section, item.title);
    }
    window.location.hash = hashForRoute(item.route).slice(1);
    onClose();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((i) => (results.length ? (i + 1) % results.length : 0));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((i) =>
          results.length ? (i - 1 + results.length) % results.length : 0,
        );
        break;
      case "Enter":
        event.preventDefault();
        select(results[active]);
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
    }
  };

  const grouped = query.trim().length === 0;
  const groupLabel = (item: QuickItem): string =>
    item.kind === "navigate" ? "Navigate" : "Settings";

  let lastGroup = "";

  return (
    <div
      className="qp-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Quick panel"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="qp-panel"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="qp-search">
          <QpIcon name="search" />
          <input
            ref={inputRef}
            className="qp-input"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search settings or jump to a page"
            autoComplete="off"
            spellCheck={false}
            aria-label="Search Grain"
          />
          <kbd className="qp-kbd">{QP_COPY.esc}</kbd>
        </div>

        <div className="qp-list" ref={listRef} role="listbox">
          {results.length === 0 ? (
            <div className="qp-empty">{QP_COPY.empty}</div>
          ) : (
            results.map((item, idx) => {
              const label = grouped ? groupLabel(item) : "";
              const showLabel = grouped && label !== lastGroup;
              if (grouped) lastGroup = label;
              return (
                <div key={item.id}>
                  {showLabel && <div className="qp-group">{label}</div>}
                  <button
                    type="button"
                    role="option"
                    aria-selected={idx === active}
                    data-idx={idx}
                    className={`qp-item${idx === active ? " is-active" : ""}`}
                    onMouseMove={() => setActive(idx)}
                    onClick={() => select(item)}
                  >
                    <QpIcon name={item.icon} />
                    <span className="qp-item-title">{item.title}</span>
                    {item.kind !== "section" && (
                      <span className="qp-item-meta">{item.metaLabel}</span>
                    )}
                  </button>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
