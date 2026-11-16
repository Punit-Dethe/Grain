// Grain Pilot page script. Injected on demand into tabs Grain opened, never
// declared for all sites. Uses @page-agent/page-controller (MIT) to read the
// page as a list of interactive elements, and its action helpers to click,
// type, pick and scroll.
//
// Grain's own additions:
// - stable control numbers: an element keeps its number across reads, and
//   numbers never repeat within a tab (the worker passes each new document a
//   starting number), so an old number can never hit a different control;
// - field values in reads (never for secret fields);
// - typing goes into the field that has focus after the click (sites such as
//   Wikipedia swap the field on click) and the value is checked, so a failed
//   type is never reported as a success;
// - password, card and one-time-code fields are refused here as well as in
//   Grain.
import {
  PageController,
  clickElement,
  inputTextElement,
  scrollHorizontally,
  scrollVertically,
  selectOptionElement,
} from "@page-agent/page-controller";

type Result = { ok: boolean; message: string };

const SECRET_AUTOCOMPLETE =
  /^(cc-|current-password|new-password|one-time-code)/i;
const SECRET_REFUSAL =
  "That is a password, card or code field. Hand over with show.";

function isSecret(el: Element | null): boolean {
  if (!(el instanceof HTMLInputElement)) return false;
  if (el.type === "password") return true;
  const auto = (el.getAttribute("autocomplete") || "").trim();
  return auto.split(/\s+/).some((token) => SECRET_AUTOCOMPLETE.test(token));
}

function isTextField(
  el: Element | null,
): el is HTMLInputElement | HTMLTextAreaElement {
  if (el instanceof HTMLTextAreaElement) return true;
  if (!(el instanceof HTMLInputElement)) return false;
  return ![
    "button",
    "submit",
    "reset",
    "checkbox",
    "radio",
    "file",
    "image",
    "range",
    "color",
    "hidden",
  ].includes(el.type);
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Wait until the DOM stops changing for `quietMs`, at most `maxMs`. */
function settle(quietMs = 300, maxMs = 3000): Promise<void> {
  return new Promise((resolve) => {
    let timer = setTimeout(done, quietMs);
    const cap = setTimeout(done, maxMs);
    const observer = new MutationObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(done, quietMs);
    });
    observer.observe(document, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
    function done() {
      observer.disconnect();
      clearTimeout(timer);
      clearTimeout(cap);
      resolve();
    }
  });
}

type SelectorMap = Map<number, { ref?: Element }>;

const KEYS: Record<string, { key: string; code: string; keyCode: number }> = {
  return: { key: "Enter", code: "Enter", keyCode: 13 },
  enter: { key: "Enter", code: "Enter", keyCode: 13 },
  escape: { key: "Escape", code: "Escape", keyCode: 27 },
  tab: { key: "Tab", code: "Tab", keyCode: 9 },
  up: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  down: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  left: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  right: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  space: { key: " ", code: "Space", keyCode: 32 },
  backspace: { key: "Backspace", code: "Backspace", keyCode: 8 },
  pageup: { key: "PageUp", code: "PageUp", keyCode: 33 },
  pagedown: { key: "PageDown", code: "PageDown", keyCode: 34 },
  home: { key: "Home", code: "Home", keyCode: 36 },
  end: { key: "End", code: "End", keyCode: 35 },
};

function pressOn(target: Element, name: string): boolean {
  const spec = KEYS[name.toLowerCase()];
  if (!spec) return false;
  const init = {
    ...spec,
    which: spec.keyCode,
    bubbles: true,
    cancelable: true,
  };
  const proceed = target.dispatchEvent(new KeyboardEvent("keydown", init));
  if (spec.key === "Enter")
    target.dispatchEvent(new KeyboardEvent("keypress", init));
  target.dispatchEvent(new KeyboardEvent("keyup", init));
  if (
    proceed &&
    spec.key === "Enter" &&
    target instanceof HTMLInputElement &&
    target.form
  ) {
    target.form.requestSubmit();
  }
  return true;
}

class GrainPilot {
  private full = new PageController({ viewportExpansion: -1 });
  private visible = new PageController({ viewportExpansion: 0 });
  private numberOf = new WeakMap<Element, number>();
  private elements = new Map<number, WeakRef<Element>>();
  next = 1;

  /** First call in a document: number from where the tab's last page ended. */
  init(base: number) {
    if (this.next < base) this.next = base;
  }

  private number(el: Element): number {
    let n = this.numberOf.get(el);
    if (n === undefined) {
      n = this.next++;
      this.numberOf.set(el, n);
      this.elements.set(n, new WeakRef(el));
    }
    return n;
  }

  private element(n: number): Element | null {
    const el = this.elements.get(n)?.deref() ?? null;
    return el && el.isConnected ? el : null;
  }

  async read(view: "full" | "visible") {
    await settle(200, 1500);
    const pc = view === "visible" ? this.visible : this.full;
    const state = await pc.getBrowserState();
    const map = (pc as unknown as { selectorMap: SelectorMap }).selectorMap;
    const content = state.content
      .split("\n")
      .map((line) => {
        const match = line.match(/^(\s*)\*?\[(\d+)\]<(\w+)/);
        if (!match) return line;
        const el = map.get(Number(match[2]))?.ref ?? null;
        if (!el) return line;
        const head = `${match[1]}[${this.number(el)}]<${match[3]}`;
        let rest = line.slice(match[0].length);
        if (isSecret(el)) rest = ` secret${rest}`;
        else {
          let value: string | null = null;
          if (isTextField(el) && el.value) value = el.value;
          else if (el instanceof HTMLSelectElement)
            value = el.selectedOptions[0]?.text ?? null;
          if (value !== null) {
            const shown = value.length > 80 ? `${value.slice(0, 80)}…` : value;
            rest = ` value=${JSON.stringify(shown)}${rest}`;
          }
        }
        return head + rest;
      })
      .join("\n");
    return {
      url: state.url,
      title: state.title,
      header: state.header,
      content,
      footer: state.footer,
    };
  }

  private missing(n: number): Result {
    return {
      ok: false,
      message: `[${n}] is no longer on the page. Read it again.`,
    };
  }

  async click(n: number): Promise<Result> {
    const el = this.element(n);
    if (!el) return this.missing(n);
    if (isSecret(el)) return { ok: false, message: SECRET_REFUSAL };
    await clickElement(el as HTMLElement);
    await settle();
    return { ok: true, message: `Clicked [${n}].` };
  }

  async type(n: number, text: string, submit: boolean): Promise<Result> {
    const original = this.element(n);
    if (!original) return this.missing(n);
    if (isSecret(original)) return { ok: false, message: SECRET_REFUSAL };
    if (original instanceof HTMLElement && original.isContentEditable) {
      await inputTextElement(original, text);
      if (submit) pressOn(original, "enter");
      await settle();
      return { ok: true, message: `Typed into [${n}].` };
    }
    await clickElement(original as HTMLElement);
    await sleep(150);
    // The site may have replaced the field on click; type where focus went.
    const focused = document.activeElement;
    const target = isTextField(focused)
      ? focused
      : isTextField(original) && original.isConnected
        ? original
        : null;
    if (!target) return { ok: false, message: `[${n}] is not a text field.` };
    if (isSecret(target)) return { ok: false, message: SECRET_REFUSAL };
    const proto =
      target instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(target, text);
    target.dispatchEvent(
      new InputEvent("input", {
        bubbles: true,
        inputType: "insertText",
        data: text,
      }),
    );
    target.dispatchEvent(new Event("change", { bubbles: true }));
    await sleep(50);
    if (target.value !== text) {
      return {
        ok: false,
        message: `The field did not take the text (it holds ${JSON.stringify(target.value)}).`,
      };
    }
    if (submit) pressOn(target, "enter");
    await settle();
    return {
      ok: true,
      message: `Typed into [${n}]${submit ? " and submitted" : ""}.`,
    };
  }

  async select(n: number, option: string): Promise<Result> {
    const el = this.element(n);
    if (!el) return this.missing(n);
    if (!(el instanceof HTMLSelectElement)) {
      return {
        ok: false,
        message: `[${n}] is not a native drop-down. Click it, read, then click the option.`,
      };
    }
    try {
      await selectOptionElement(el, option);
    } catch (error) {
      return { ok: false, message: String((error as Error)?.message ?? error) };
    }
    await settle();
    return { ok: true, message: `Picked ${JSON.stringify(option)} in [${n}].` };
  }

  async press(key: string): Promise<Result> {
    const target = document.activeElement ?? document.body;
    if (isSecret(target)) return { ok: false, message: SECRET_REFUSAL };
    if (!pressOn(target, key))
      return { ok: false, message: `Grain Pilot cannot press ${key}.` };
    await settle();
    return { ok: true, message: `Pressed ${key}.` };
  }

  async scroll(direction: string, pixels: number): Promise<Result> {
    const amount =
      direction === "up" || direction === "left" ? -pixels : pixels;
    const message =
      direction === "left" || direction === "right"
        ? await scrollHorizontally(amount)
        : await scrollVertically(amount);
    await settle(200, 1500);
    return { ok: true, message };
  }
}

const G = globalThis as unknown as { __grainPilot?: GrainPilot };
if (!G.__grainPilot) G.__grainPilot = new GrainPilot();
