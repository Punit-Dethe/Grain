import React, {
  useCallback,
  useEffect,
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
}

/** Tallest the menu may get before it scrolls internally (px). */
const MAX_MENU_H = 280;
/** Shortest it may be squeezed to before it flips to the other side instead. */
const MIN_MENU_H = 120;
/** Gap between the trigger and the menu, and the margin kept off each edge. */
const GAP = 4;
const EDGE = 8;

/** Where to hang the portalled menu.
 *
 * The nearest themed ancestor rather than `document.body`: the app's base rules
 * are scoped (`.grain-root button { font: inherit }`, `box-sizing`, the theme's
 * `--color-*` set, and the Grain Space overlay's own `data-theme` frame), so a
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
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [box, setBox] = useState<MenuBox | null>(null);
  const anchorRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

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
    setBox({
      left: Math.max(
        EDGE,
        Math.min(rect.left, window.innerWidth - rect.width - EDGE),
      ),
      width: rect.width,
      maxHeight: Math.max(MIN_MENU_H, Math.min(MAX_MENU_H, room)),
      ...(flipUp
        ? { bottom: window.innerHeight - rect.top + GAP }
        : { top: rect.bottom + GAP }),
    });
  }, []);

  // Measure before paint so the menu never appears at a stale position.
  useLayoutEffect(() => {
    if (isOpen) place();
  }, [isOpen, place]);

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
      if (event.key === "Escape") setIsOpen(false);
    };
    // Capture phase: catches scrolling of any ancestor, not just the window.
    const onReflow = () => place();
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

  const handleSelect = (value: string) => {
    onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (disabled) return;
    if (!isOpen && onRefresh) onRefresh();
    setIsOpen(!isOpen);
  };

  const menu = box && (
    <div
      ref={menuRef}
      className="fixed bg-paper-raised border border-line rounded-lg overflow-y-auto"
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
      role="listbox"
    >
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
              className={`w-full px-2 py-1 text-sm text-start hover:bg-[var(--accent-tint)] transition-colors duration-150 ${
                selectedValue === option.value
                  ? "bg-[var(--accent-tint)] text-accent font-semibold"
                  : ""
              } ${option.disabled ? "opacity-50 cursor-not-allowed" : ""}`}
              onClick={() => handleSelect(option.value)}
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
  );

  return (
    <div className={`relative ${className}`} ref={anchorRef}>
      <button
        type="button"
        className={`px-2 py-[5px] text-sm font-medium bg-paper-sunken border border-line rounded-lg min-w-[200px] w-full text-start grid grid-cols-[1fr_auto] gap-2 items-center transition-all duration-150 ${
          disabled
            ? "opacity-50 cursor-not-allowed"
            : "hover:bg-[var(--accent-tint)] hover:border-accent cursor-pointer"
        }`}
        onClick={handleToggle}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
      >
        <span className="truncate">{selectedOption?.label || placeholder}</span>
        <svg
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
