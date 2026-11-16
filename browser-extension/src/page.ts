// Grain Pilot page script. Injected on demand into tabs Grain opened, never
// declared for all sites. Wraps @page-agent/page-controller (MIT), which reads
// the page as a numbered list of interactive elements and acts on them.
//
// Grain's own additions:
// - field values in reads (never for password fields), so the model can see
//   what a box holds;
// - typing goes into the field that has focus after the click (sites such as
//   Wikipedia swap the field on click) and the value is checked, so a failed
//   type is never reported as a success;
// - password, card and one-time-code fields are refused here as well as in
//   Grain.
import { PageController } from "@page-agent/page-controller";

type Result = { ok: boolean; message: string };

const G = globalThis as unknown as { __grainPilot?: unknown };

const SECRET_AUTOCOMPLETE =
  /^(cc-|current-password|new-password|one-time-code)/i;

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

class GrainPilot {
  private full = new PageController({ viewportExpansion: -1 });
  private visible = new PageController({ viewportExpansion: 0 });
  /** The controller whose numbers the model saw last. */
  private last: PageController = this.full;

  private element(index: number): Element | null {
    const map = (
      this.last as unknown as { selectorMap: Map<number, { ref?: Element }> }
    ).selectorMap;
    return map.get(index)?.ref ?? null;
  }

  async read(view: "full" | "visible") {
    await settle(200, 1500);
    const pc = view === "visible" ? this.visible : this.full;
    this.last = pc;
    const state = await pc.getBrowserState();
    const map = (
      pc as unknown as { selectorMap: Map<number, { ref?: Element }> }
    ).selectorMap;
    const content = state.content
      .split("\n")
      .map((line) => {
        const match = line.match(/^(\s*\*?\[(\d+)\]<\w+)/);
        if (!match) return line;
        const el = map.get(Number(match[2]))?.ref ?? null;
        if (isSecret(el)) return line.replace(match[1], `${match[1]} secret`);
        let value: string | null = null;
        if (isTextField(el) && el.value) value = el.value;
        else if (el instanceof HTMLSelectElement)
          value = el.selectedOptions[0]?.text ?? null;
        if (value === null) return line;
        const shown = value.length > 80 ? `${value.slice(0, 80)}…` : value;
        return line.replace(
          match[1],
          `${match[1]} value=${JSON.stringify(shown)}`,
        );
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

  async click(index: number): Promise<Result> {
    const el = this.element(index);
    if (isSecret(el))
      return {
        ok: false,
        message: "That is a password, card or code field. Hand over with show.",
      };
    const result = await this.last.clickElement(index);
    await settle();
    return { ok: result.success, message: result.message };
  }

  async type(index: number, text: string, submit: boolean): Promise<Result> {
    const original = this.element(index);
    if (!original)
      return {
        ok: false,
        message: `There is no [${index}]. Read the page again.`,
      };
    if (isSecret(original))
      return {
        ok: false,
        message: "That is a password, card or code field. Hand over with show.",
      };
    if (original instanceof HTMLElement && original.isContentEditable) {
      const result = await this.last.inputText(index, text);
      await settle();
      return { ok: result.success, message: result.message };
    }
    await this.last.clickElement(index);
    await sleep(150);
    // The site may have replaced the field on click; type where focus went.
    const focused = document.activeElement;
    const target = isTextField(focused)
      ? focused
      : isTextField(original) && original.isConnected
        ? original
        : null;
    if (!target)
      return { ok: false, message: `[${index}] is not a text field.` };
    if (isSecret(target))
      return {
        ok: false,
        message: "That is a password, card or code field. Hand over with show.",
      };
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
    if (submit) {
      const keys = {
        key: "Enter",
        code: "Enter",
        keyCode: 13,
        which: 13,
        bubbles: true,
        cancelable: true,
      };
      const proceed = target.dispatchEvent(new KeyboardEvent("keydown", keys));
      target.dispatchEvent(new KeyboardEvent("keypress", keys));
      target.dispatchEvent(new KeyboardEvent("keyup", keys));
      if (proceed && target.form) target.form.requestSubmit();
    }
    await settle();
    return {
      ok: true,
      message: `Typed into [${index}]${submit ? " and submitted" : ""}.`,
    };
  }

  async select(index: number, option: string): Promise<Result> {
    const result = await this.last.selectOption(index, option);
    await settle();
    return { ok: result.success, message: result.message };
  }

  async scroll(down: boolean, pages: number): Promise<Result> {
    const result = await this.last.scroll({ down, numPages: pages });
    await settle(200, 1500);
    return { ok: result.success, message: result.message };
  }
}

if (!G.__grainPilot) G.__grainPilot = new GrainPilot();
