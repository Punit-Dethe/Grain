import React, {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

export interface DropdownOption {
  value: string;
  label: string;
  disabled?: boolean;
  /** Optional heading this option sits under. Options carrying the same group
   * are listed together beneath one label, in the order they arrive; options
   * with no group are listed plainly. When NO option has a group the list is
   * rendered exactly as before, so existing dropdowns are untouched. */
  group?: string;
}

interface DropdownProps {
  options: DropdownOption[];
  className?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onRefresh?: () => void;
  ariaLabel?: string;
  /** Arrow-only trigger for a split control with a full-width options menu. */
  compact?: boolean;
  pagination?: { label: string; loading: boolean; onLoadMore: () => void };
}

/** Tallest the menu may get before it scrolls internally (px). */
const MAX_MENU_H = 280;
/** Shortest it may be squeezed to before it flips to the other side instead. */
const MIN_MENU_H = 120;
/** Gap between the trigger and the menu, and the margin kept off each edge. */
const GAP = 4;
const EDGE = 8;
const COMPACT_MENU_W = 264;

/** Where to hang the portalled menu.
 *
 * The nearest themed ancestor rather than `document.body`: the app's base rules
 * are scoped (`.grain-root button { font: inherit }`, `box-sizing`, the theme's
 * `--color-*` set), so a
 * menu parked on `<body>` renders in the UA's default font with a default
 * border, and in the wrong theme inside the overlay. Any themed root is high
 * enough in the tree to escape the card that was clipping the menu, and none of
 * them carry a transform — which is what would otherwise turn `position: fixed`
 * into "relative to that element" and invalidate the viewport coordinates. */
const portalHost = (anchor: React.RefObject<HTMLElement | null>): HTMLElement =>
  (anchor.current?.closest("[data-theme]") as HTMLElement | null) ??
  document.body;

/** Where the open menu sits, in viewport coordinates. */
type MenuBox = {
  left: number;
  width: number;
  maxHeight: number;
  /** Exactly one of these is set — `top` opens downward, `bottom` upward. */
  top?: number;
  bottom?: number;
};

export const Dropdown: React.FC<DropdownProps> = ({
  options,
  selectedValue,
  onSelect,
  className = "",
  placeholder = "Select an option...",
  disabled = false,
  onRefresh,
  ariaLabel,
  compact = false,
  pagination,
}) => {
  const { t } = useTranslation();
  const menuId = useId();
  const [isOpen, setIsOpen] = useState(false);
  const [box, setBox] = useState<MenuBox | null>(null);
  const anchorRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const focusOnOpen = useRef(false);

  /* [GRAIN] The menu is PORTALLED out (see `portalHost`) and positioned in
     viewport coordinates, rather than being `absolute` inside the trigger.

     An absolutely-positioned child is clipped by any ancestor that scrolls or
     hides its overflow, no matter how high its z-index — which is why the
     provider list used to be cut off by the settings card it sat in, taking the
     last entry (Gemini, appended by a backend migration) with it. Leaving the
     ancestor chain is the only fix that does not require every card in the app
     to know it might contain a dropdown.

     The cost of leaving is that the menu no longer follows the trigger by
     itself, so it is re-measured on scroll and resize below. */
  const place = useCallback(() => {
    const anchor = anchorRef.current;
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - GAP - EDGE;
    const above = rect.top - GAP - EDGE;
    // Flip up only when down genuinely cannot show a usable list AND up is
    // roomier — otherwise the menu jumps sides on tiny layout shifts.
    const flipUp = below < MIN_MENU_H && above > below;
    const room = flipUp ? above : below;
    const width = compact
      ? Math.min(COMPACT_MENU_W, window.innerWidth - 2 * EDGE)
      : rect.width;
    const left = compact ? rect.right - width : rect.left;
    setBox({
      left: Math.max(EDGE, Math.min(left, window.innerWidth - width - EDGE)),
      width,
      maxHeight: Math.max(MIN_MENU_H, Math.min(MAX_MENU_H, room)),
      ...(flipUp
        ? { bottom: window.innerHeight - rect.top + GAP }
        : { top: rect.bottom + GAP }),
    });
  }, [compact]);

  // Measure before paint so the menu never appears at a stale position.
  useLayoutEffect(() => {
    if (isOpen) place();
  }, [isOpen, place]);

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!isOpen || !menu || !focusOnOpen.current) return;
    focusOnOpen.current = false;
    const selected = menu.querySelector<HTMLButtonElement>(
      'button[aria-selected="true"]:enabled',
    );
    (
      selected ?? menu.querySelector<HTMLButtonElement>("button:enabled")
    )?.focus();
  }, [isOpen, box]);

  useEffect(() => {
    if (!isOpen) return;
    const onDocMouseDown = (event: MouseEvent) => {
      const target = event.target as Node;
      // The menu lives outside the trigger's subtree now, so "outside" has to
      // mean outside BOTH or every click on an option would close first.
      if (
        !anchorRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setIsOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (menuRef.current?.contains(document.activeElement)) {
        triggerRef.current?.focus();
      }
      setIsOpen(false);
    };
    // Capture phase: catches scrolling of any ancestor, not just the window.
    const onReflow = (event: Event) => {
      if (event.target !== menuRef.current) place();
    };
    document.addEventListener("mousedown", onDocMouseDown);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onReflow, true);
    window.addEventListener("resize", onReflow);
    return () => {
      document.removeEventListener("mousedown", onDocMouseDown);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onReflow, true);
      window.removeEventListener("resize", onReflow);
    };
  }, [isOpen, place]);

  // A dropdown that becomes disabled while open (a save in flight) must not
  // leave an orphaned menu floating over the page.
  useEffect(() => {
    if (disabled) setIsOpen(false);
  }, [disabled]);

  const selectedOption = options.find(
    (option) => option.value === selectedValue,
  );

  const handleSelect = (value: string, event: React.MouseEvent) => {
    onSelect(value);
    if (compact || event.detail === 0) triggerRef.current?.focus();
    setIsOpen(false);
  };

  const toggle = (fromKeyboard: boolean) => {
    if (disabled) return;
    focusOnOpen.current = !isOpen && (compact || fromKeyboard);
    if (!isOpen && onRefresh) onRefresh();
    setIsOpen(!isOpen);
  };

  const handleMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Tab") {
      triggerRef.current?.focus();
      setIsOpen(false);
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const buttons = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>("button:enabled"),
    );
    const index = buttons.findIndex((button) => button === event.target);
    const step = event.key === "ArrowDown" ? 1 : -1;
    buttons[index + step]?.focus();
  };

  const menu = box && (
    <div
      ref={menuRef}
      onKeyDown={handleMenuKeyDown}
      onScroll={
        pagination && !pagination.loading
          ? (event) => {
              const menu = event.currentTarget;
              if (menu.scrollHeight - menu.scrollTop - menu.clientHeight < 8) {
                pagination.onLoadMore();
              }
            }
          : undefined
      }
      className="fixed bg-paper-raised border border-line rounded-lg overflow-y-auto overscroll-contain"
      style={{
        left: box.left,
        width: box.width,
        maxHeight: box.maxHeight,
        top: box.top,
        bottom: box.bottom,
        boxShadow: "var(--shadow-float)",
        // Above every in-page surface. Set inline rather than as a utility so a
        // dropdown opened from inside a modal is never behind it.
        zIndex: 1000,
      }}
    >
      <div id={menuId} role="listbox" aria-label={ariaLabel}>
        {options.length === 0 ? (
          <div className="px-2 py-1 text-sm text-ink-soft">
            {t("common.noOptionsFound")}
          </div>
        ) : (
          options.map((option, i) => (
            <React.Fragment key={option.value}>
              {/* A heading whenever the group changes — so a list assembled
                from several sources says which is which, without every
                entry having to repeat it in its own label. */}
              {option.group && option.group !== options[i - 1]?.group && (
                <div className="px-2 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-ink-faint">
                  {option.group}
                </div>
              )}
              <button
                type="button"
                role="option"
                aria-selected={selectedValue === option.value}
                className={`w-full px-2 py-1 text-sm text-start transition-colors duration-150 ${
                  compact
                    ? "dropdown-compact-option"
                    : `hover:bg-[var(--accent-tint)] ${
                        selectedValue === option.value
                          ? "bg-[var(--accent-tint)] text-accent font-semibold"
                          : ""
                      }`
                } ${option.disabled ? "opacity-50 cursor-not-allowed" : ""}`}
                onClick={(event) => handleSelect(option.value, event)}
                disabled={option.disabled}
              >
                <span className="whitespace-normal break-words">
                  {option.label}
                </span>
              </button>
            </React.Fragment>
          ))
        )}
      </div>
      {pagination && (
        <button
          type="button"
          className="w-full px-2 py-1 text-sm text-start text-ink-soft hover:bg-[var(--accent-tint)]"
          aria-disabled={pagination.loading}
          onClick={() => {
            if (!pagination.loading) pagination.onLoadMore();
          }}
        >
          {pagination.loading
            ? t("settings.history.loading")
            : pagination.label}
        </button>
      )}
    </div>
  );

  return (
    <div className={`relative ${className}`} ref={anchorRef}>
      <button
        ref={triggerRef}
        type="button"
        className={
          compact
            ? "grid h-full w-7 place-items-center"
            : `px-2 py-[5px] text-sm font-medium bg-paper-sunken border border-line rounded-lg min-w-[200px] w-full text-start grid grid-cols-[1fr_auto] gap-2 items-center transition-all duration-150 ${
                disabled
                  ? "opacity-50 cursor-not-allowed"
                  : "hover:bg-[var(--accent-tint)] hover:border-accent cursor-pointer"
              }`
        }
        onClick={(event) => toggle(event.detail === 0)}
        onKeyDown={(event) => {
          if (
            isOpen ||
            (event.key !== "ArrowDown" && event.key !== "ArrowUp")
          ) {
            return;
          }
          event.preventDefault();
          toggle(true);
        }}
        disabled={disabled && !compact}
        aria-disabled={compact ? disabled : undefined}
        aria-label={ariaLabel}
        title={compact && !disabled ? selectedOption?.label : undefined}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={isOpen ? menuId : undefined}
      >
        {!compact && (
          <span className="truncate">
            {selectedOption?.label || placeholder}
          </span>
        )}
        <svg
          aria-hidden="true"
          className={`w-4 h-4 transition-transform duration-200 ${isOpen ? "transform rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {isOpen && !disabled && menu && createPortal(menu, portalHost(anchorRef))}
    </div>
  );
};
