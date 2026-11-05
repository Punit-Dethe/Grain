import React, {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Languages, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLanguagePicker } from "@/handy-models/components/shared/LanguagePicker";
import { suggestedLanguages } from "@/handy-models/lib/utils/languageSuggestions";
import { isWeakLanguage } from "@/handy-models/lib/models/catalog";
import { useSettings } from "@/hooks/useSettings";
import { isComposingKeyEvent } from "@/handy-models/lib/utils/keyboard";
import { cjkSubsetFont } from "@/handy-models/lib/utils/cjkFont";
import { languageList } from "@/handy-models/lib/constants/languages";

type Picker = ReturnType<typeof useLanguagePicker>;
const SearchIcon = () => <Search size={14} aria-hidden="true" />;
const CheckIcon = () => <Check size={16} aria-hidden="true" />;

/** The menu under +: a search over every language, the picked ones first and ticked. */
const LanguageMenu: React.FC<{
  picker: Picker;
  /** The picked languages when the menu opened: they lead, and stay put as they're toggled. */
  pinned: string[];
  /** Highlight the first addable even with nothing typed (it's the suggestion). */
  highlightFirst: boolean;
}> = ({ picker, pinned, highlightFirst }) => {
  const { t } = useTranslation();
  const { inputRef, query, languages, matches } = picker;
  // Picked ones in the chips' order, then the rest in search order.
  const rows = [
    ...pinned.flatMap((code) => matches.filter((l) => l.code === code)),
    ...matches.filter((l) => !pinned.includes(l.code)),
  ];
  const firstAddable = rows.find((l) => !languages.includes(l.code))?.code;

  return (
    <div
      // Keep focus in the search: WebKit doesn't focus a pressed button, so
      // the blur would close the menu before the click lands.
      onMouseDown={(e) => {
        if (e.target !== inputRef.current) e.preventDefault();
      }}
      className="flex min-h-0 flex-1 flex-col"
    >
      <label className="flex shrink-0 items-center gap-2 border-b border-line px-3 text-ink-3">
        <SearchIcon />
        <input
          ref={inputRef}
          type="text"
          autoComplete="off"
          spellCheck={false}
          aria-label={t("languagePicker.search")}
          placeholder={t("languagePicker.search")}
          value={query}
          onChange={(e) => picker.setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (isComposingKeyEvent(e.nativeEvent)) return;
            // Escape with a query clears it; without one, the menu closes.
            if (picker.onKeyDown(e)) e.stopPropagation();
          }}
          className="h-9 w-full min-w-0 bg-transparent text-sm text-text outline-none placeholder:text-ink-3"
        />
      </label>
      <div className="min-h-0 overflow-y-auto p-1">
        {rows.map((l) => {
          const name = picker.nameOf(l.code);
          const picked = languages.includes(l.code);
          const locked = picked && !picker.removable;
          const highlighted =
            (!!query || highlightFirst) && l.code === firstAddable;
          return (
            <button
              key={l.code}
              type="button"
              aria-pressed={picked}
              disabled={locked}
              onClick={() => picker.toggle(l.code)}
              // Picked rows are only ticked: unlike Dropdown's one choice,
              // several fills would stack into stripes.
              className={`flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm transition-colors disabled:cursor-default ${
                highlighted
                  ? "bg-wash"
                  : "hover:bg-wash disabled:hover:bg-transparent"
              }`}
            >
              <span
                lang={l.code}
                className={`shrink-0 ${picked ? "font-semibold" : ""} ${cjkSubsetFont(l.native) ?? ""}`}
              >
                {l.native}
              </span>
              {l.native !== name && (
                <span className="truncate text-xs text-ink-3">{name}</span>
              )}
              <span
                className={`ms-auto flex w-4 shrink-0 ${locked ? "text-ink-3" : "text-ink-2"}`}
              >
                {picked && <CheckIcon />}
              </span>
            </button>
          );
        })}
        {!rows.length && (
          <div className="px-2 py-3 text-center text-xs text-ink-3">
            {t("settings.general.preferredLanguages.noMatch", {
              query: query.trim(),
            })}
          </div>
        )}
      </div>
    </div>
  );
};

/** Catalogue language preferences, beside the capability filters. */
export const PreferredLanguages: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const stored = getSetting("spoken_languages");
  const languages = stored?.length ? stored : suggestedLanguages();
  const picker = useLanguagePicker({
    languages,
    onChange: (next) => updateSetting("spoken_languages", next),
    keepOne: true,
  });
  const { setQuery, inputRef } = picker;
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState<string[]>([]);
  const [box, setBox] = useState<CSSProperties | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const weak = languages.filter(isWeakLanguage);
  const close = () => {
    setOpen(false);
    setQuery("");
  };

  // The same viewport placement as Grain's dropdowns. A portal avoids clipping
  // the search menu inside the model table's scrolling/rounded container.
  const place = useCallback(() => {
    const anchor = buttonRef.current;
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    const width = Math.min(280, window.innerWidth - 16);
    const below = window.innerHeight - rect.bottom - 12;
    const above = rect.top - 12;
    const up = below < 160 && above > below;
    setBox({
      width,
      left: Math.max(
        8,
        Math.min(rect.right - width, window.innerWidth - width - 8),
      ),
      maxHeight: Math.max(0, Math.min(320, up ? above : below)),
      ...(up
        ? { bottom: window.innerHeight - rect.top + 4 }
        : { top: rect.bottom + 4 }),
    });
  }, []);
  useLayoutEffect(() => {
    if (open) place();
  }, [open, place]);
  useEffect(() => {
    if (!open) return;
    inputRef.current?.focus({ preventScroll: true });
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !rootRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open, place, inputRef, setQuery]);

  return (
    <div
      ref={rootRef}
      onBlur={(event) => {
        const target = event.relatedTarget as Node | null;
        if (
          !rootRef.current?.contains(target) &&
          !menuRef.current?.contains(target)
        )
          close();
      }}
    >
      <button
        ref={buttonRef}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={`${t("settings.general.preferredLanguages.title")}: ${languageList(languages, i18n.language)}`}
        title={languageList(languages, i18n.language)}
        onClick={() => {
          if (open) close();
          else {
            setPinned(languages);
            setOpen(true);
          }
        }}
        className={`flex h-[26px] shrink-0 cursor-pointer items-center gap-[6px] whitespace-nowrap rounded-[7px] px-[8px] text-small font-medium transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary ${open ? "bg-line text-text" : "text-ink-3 hover:bg-wash hover:text-text"}`}
      >
        <Languages size={14} aria-hidden="true" />
        {t("modelTable.languages", "Languages")}
        <span className="text-xs tabular-nums">{languages.length}</span>
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {open &&
        createPortal(
          <div
            id={menuId}
            ref={menuRef}
            role="dialog"
            aria-label={t("settings.general.preferredLanguages.title")}
            data-model-toolbar-popup=""
            style={box ?? { visibility: "hidden" }}
            className="fixed z-[100] flex flex-col overflow-hidden rounded-lg border border-line bg-background text-start text-text"
            onKeyDown={(event) => {
              if (isComposingKeyEvent(event.nativeEvent)) return;
              if (event.key === "Escape" && !event.defaultPrevented) {
                event.preventDefault();
                event.stopPropagation();
                close();
                buttonRef.current?.focus({ preventScroll: true });
              }
            }}
          >
            <LanguageMenu
              picker={picker}
              pinned={pinned}
              highlightFirst={false}
            />
            {weak.length > 0 && (
              <p className="shrink-0 border-t border-line px-3 py-2 text-xs text-ink-3">
                {t("settings.general.preferredLanguages.weak", {
                  languages: languageList(weak, i18n.language),
                })}
              </p>
            )}
          </div>,
          rootRef.current?.closest<HTMLElement>("[data-theme]") ??
            document.body,
        )}
    </div>
  );
};
