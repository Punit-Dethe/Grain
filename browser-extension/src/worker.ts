// Grain Pilot service worker. Connects to Grain through the native messaging
// host, opens tabs on Grain's request and runs the page script in them.
//
// Rules:
// - Only tabs this extension opened (`pilotTabs`) are ever read or acted on.
//   A request naming any other tab is refused. The user's own tabs are never
//   touched.
// - The page script is injected on demand, only into Pilot's tabs.
// - No LLM, no decisions: Grain sends one operation, the worker runs it and
//   replies.
declare const chrome: any;

const HOST = "com.grain.pilot";
const RETRY_ALARM = "grain-reconnect";
const LOAD_TIMEOUT_MS = 20_000;

type Request = { id: number; op: string; [key: string]: unknown };

let port: any = null;
const pilotTabs = new Set<number>();
/** Per tab: the next control number, so numbers never repeat in a tab. */
let nextNumber: Record<string, number> = {};
let groupId: number | null = null;

// Pilot's tabs survive a worker restart.
const restored = chrome.storage.session
  .get(["pilotTabs", "groupId", "nextNumber"])
  .then((saved: any) => {
    for (const id of saved.pilotTabs ?? []) pilotTabs.add(id);
    groupId = saved.groupId ?? null;
    nextNumber = saved.nextNumber ?? {};
  });

function persist() {
  return chrome.storage.session.set({
    pilotTabs: [...pilotTabs],
    groupId,
    nextNumber,
  });
}

function browserName(): string {
  const brands: { brand: string }[] =
    (navigator as any).userAgentData?.brands ?? [];
  const known = brands
    .map((b) => b.brand)
    .find((b) => /Edge|Chrome|Brave|Opera|Vivaldi|Chromium/.test(b));
  return known ?? "Chromium";
}

function connect() {
  if (port) return;
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    port = null;
    return;
  }
  port.onMessage.addListener((message: Request) => {
    handle(message).then(
      (result) => port?.postMessage({ id: message.id, ok: true, result }),
      (error) =>
        port?.postMessage({
          id: message.id,
          ok: false,
          error: String(error?.message ?? error),
        }),
    );
  });
  port.onDisconnect.addListener(() => {
    // Grain is not running, or it stopped. Try again on the next alarm.
    void chrome.runtime.lastError;
    port = null;
  });
  port.postMessage({
    type: "hello",
    browser: browserName(),
    version: chrome.runtime.getManifest().version,
  });
}

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
// 30 s is the shortest alarm Chromium allows (older versions round up to 1
// minute). Each retry while Grain is closed starts and ends the small host.
chrome.alarms.create(RETRY_ALARM, { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm: any) => {
  if (alarm.name === RETRY_ALARM) connect();
});
connect();

chrome.tabs.onRemoved.addListener((tabId: number) => {
  if (pilotTabs.delete(tabId)) {
    delete nextNumber[tabId];
    void persist();
  }
});

function ownTab(value: unknown): number {
  const id = Number(value);
  if (!pilotTabs.has(id))
    throw new Error(
      "That tab is closed, or Grain Pilot did not open it. Call tabs to list Pilot's tabs.",
    );
  return id;
}

function waitForLoad(tabId: number): Promise<void> {
  return new Promise((resolve) => {
    const finish = () => {
      clearTimeout(timer);
      chrome.tabs.onUpdated.removeListener(listener);
      resolve();
    };
    const listener = (id: number, change: any) => {
      if (id === tabId && change.status === "complete") finish();
    };
    const timer = setTimeout(finish, LOAD_TIMEOUT_MS);
    chrome.tabs.onUpdated.addListener(listener);
    chrome.tabs.get(tabId).then((tab: any) => {
      if (tab.status === "complete") finish();
    }, finish);
  });
}

const ASLEEP =
  "The browser put this tab to sleep. Call show to bring it forward, then read it again.";
/** Longer than any page-script wait, shorter than Grain's own timeout. */
const PAGE_TIMEOUT_MS = 25_000;

/** `work`, or an error if the page has not answered within `ms`. */
function answerWithin<T>(ms: number, work: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () =>
        reject(
          new Error(
            "The page did not answer; it may be asleep or stuck. Call show to bring it forward, then read it again.",
          ),
        ),
      ms,
    );
  });
  return Promise.race([work, late]).finally(() => clearTimeout(timer));
}

/** Run one page-script method in a Pilot tab, injecting the script first. */
async function inPage(
  tabId: number,
  method: string,
  args: unknown[],
): Promise<any> {
  const tab = await chrome.tabs.get(tabId);
  if (tab.discarded) {
    // The browser unloaded it to save memory; the page has to load again.
    await chrome.tabs.reload(tabId);
    await waitForLoad(tabId);
  } else if (tab.frozen) {
    throw new Error(ASLEEP);
  } else if (tab.status !== "complete") {
    await waitForLoad(tabId);
  }
  await answerWithin(
    PAGE_TIMEOUT_MS,
    chrome.scripting.executeScript({ target: { tabId }, files: ["page.js"] }),
  );
  const [frame] = await answerWithin(PAGE_TIMEOUT_MS, chrome.scripting.executeScript({
    target: { tabId },
    func: async (method: string, args: unknown[], base: number) => {
      const pilot = (globalThis as any).__grainPilot;
      pilot.init(base);
      const result = await pilot[method](...args);
      return { result, next: pilot.next };
    },
    args: [method, args, nextNumber[tabId] ?? 1],
  }));
  if (frame?.error) throw new Error(String(frame.error.message ?? frame.error));
  const next = frame?.result?.next;
  if (typeof next === "number" && next > (nextNumber[tabId] ?? 1)) {
    nextNumber[tabId] = next;
    await persist();
  }
  return frame?.result?.result;
}

/**
 * Run an action. An action that leaves the page (a link, a form submit)
 * ends the script before it can answer; that is a success, flagged `left`.
 * Browsers may keep the old page frozen for Back instead of removing it, so
 * its answer never comes: when the tab starts loading and the new document
 * has no Pilot script, the old page is gone. A single-page app that only
 * changes its address keeps the script, and the action's own answer wins.
 */
async function action(
  tabId: number,
  method: string,
  args: unknown[],
): Promise<any> {
  const LEFT = { ok: true, message: "Done; the page changed.", left: true };
  let stop = () => {};
  const left = new Promise<typeof LEFT>((resolve) => {
    const listener = async (id: number, change: any) => {
      if (id !== tabId || change.status !== "loading") return;
      await waitForLoad(tabId);
      const [probe] = await chrome.scripting
        .executeScript({
          target: { tabId },
          func: () => Boolean((globalThis as any).__grainPilot),
        })
        .catch(() => [] as any[]);
      if (probe?.result !== true) resolve(LEFT);
    };
    chrome.tabs.onUpdated.addListener(listener);
    stop = () => chrome.tabs.onUpdated.removeListener(listener);
  });
  try {
    const result = await Promise.race([inPage(tabId, method, args), left]);
    await afterAction(tabId);
    return result;
  } catch (error) {
    if (
      /Frame with ID \d+ was removed|frame was removed|navigat/i.test(
        String((error as any)?.message ?? error),
      )
    ) {
      await afterAction(tabId);
      return LEFT;
    }
    throw error;
  } finally {
    stop();
  }
}

/** After an action: if the page started loading, wait for it. */
async function afterAction(tabId: number) {
  await new Promise((resolve) => setTimeout(resolve, 300));
  const tab = await chrome.tabs.get(tabId).catch(() => null);
  if (tab?.status === "loading") await waitForLoad(tabId);
}

async function handle(request: Request): Promise<unknown> {
  await restored;
  switch (request.op) {
    case "ping":
      return { browser: browserName() };
    case "open": {
      const url = String(request.url);
      if (!/^https?:\/\//i.test(url))
        throw new Error("Grain Pilot only opens http and https links.");
      // Background by default: the user keeps working in their own tab.
      // With no browser window open, a window is made without focus.
      const active = request.active === true;
      const tab = await chrome.tabs
        .create({ url, active })
        .catch(async () => {
          const made = await chrome.windows.create({ url, focused: active });
          return made.tabs[0];
        });
      pilotTabs.add(tab.id);
      // A busy background tab may otherwise be put to sleep mid-task.
      await chrome.tabs.update(tab.id, { autoDiscardable: false }).catch(() => {});
      try {
        if (groupId !== null)
          await chrome.tabs.group({ groupId, tabIds: [tab.id] });
        else throw new Error("no group");
      } catch {
        groupId = await chrome.tabs.group({
          tabIds: [tab.id],
          createProperties: { windowId: tab.windowId },
        });
        await chrome.tabGroups.update(groupId, {
          title: "Grain",
          color: "blue",
        });
      }
      await persist();
      await waitForLoad(tab.id);
      return { tab: tab.id };
    }
    case "tabs": {
      const tabs = await Promise.all(
        [...pilotTabs].map((id) => chrome.tabs.get(id).catch(() => null)),
      );
      return tabs
        .filter(Boolean)
        .map((tab: any) => ({
          tab: tab.id,
          title: tab.title,
          url: tab.url,
          asleep: Boolean(tab.frozen || tab.discarded),
        }));
    }
    case "read":
      return inPage(ownTab(request.tab), "read", [
        request.view === "visible" ? "visible" : "full",
      ]);
    case "click":
      return action(ownTab(request.tab), "click", [Number(request.index)]);
    case "type":
      return action(ownTab(request.tab), "type", [
        Number(request.index),
        String(request.text),
        !!request.submit,
      ]);
    case "select":
      return action(ownTab(request.tab), "select", [
        Number(request.index),
        String(request.option),
      ]);
    case "press":
      return action(ownTab(request.tab), "press", [String(request.key)]);
    case "scroll":
      return action(ownTab(request.tab), "scroll", [
        String(request.direction ?? "down"),
        Number(request.pixels ?? 500),
      ]);
    case "show": {
      const tab = await chrome.tabs.update(ownTab(request.tab), {
        active: true,
      });
      await chrome.windows.update(tab.windowId, { focused: true });
      return { shown: tab.id };
    }
    case "close":
      await chrome.tabs.remove(ownTab(request.tab));
      return { closed: true };
    default:
      throw new Error(`Grain Pilot has no operation ${request.op}`);
  }
}
