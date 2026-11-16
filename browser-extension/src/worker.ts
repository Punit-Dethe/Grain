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
let groupId: number | null = null;

// The tab set survives a worker restart.
const restored = chrome.storage.session
  .get(["pilotTabs", "groupId"])
  .then((saved: any) => {
    for (const id of saved.pilotTabs ?? []) pilotTabs.add(id);
    groupId = saved.groupId ?? null;
  });

function persist() {
  return chrome.storage.session.set({ pilotTabs: [...pilotTabs], groupId });
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
chrome.alarms.create(RETRY_ALARM, { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener((alarm: any) => {
  if (alarm.name === RETRY_ALARM) connect();
});
connect();

chrome.tabs.onRemoved.addListener((tabId: number) => {
  if (pilotTabs.delete(tabId)) void persist();
});

function ownTab(value: unknown): number {
  const id = Number(value);
  if (!pilotTabs.has(id))
    throw new Error("Grain Pilot only works in tabs it opened.");
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

/** Run one page-script method in a Pilot tab, injecting the script first. */
async function inPage(
  tabId: number,
  method: string,
  args: unknown[],
): Promise<any> {
  const status = (await chrome.tabs.get(tabId)).status;
  if (status !== "complete") await waitForLoad(tabId);
  await chrome.scripting.executeScript({
    target: { tabId },
    files: ["page.js"],
  });
  const [frame] = await chrome.scripting.executeScript({
    target: { tabId },
    func: (method: string, args: unknown[]) =>
      (globalThis as any).__grainPilot[method](...args),
    args: [method, args],
  });
  if (frame?.error) throw new Error(String(frame.error.message ?? frame.error));
  return frame?.result;
}

/**
 * Run an action. An action that leaves the page (a link, a form submit)
 * removes the frame before the script can answer; that is a success.
 */
async function action(
  tabId: number,
  method: string,
  args: unknown[],
): Promise<any> {
  try {
    const result = await inPage(tabId, method, args);
    await afterAction(tabId);
    return result;
  } catch (error) {
    if (
      /Frame with ID \d+ was removed|frame was removed|navigat/i.test(
        String((error as any)?.message ?? error),
      )
    ) {
      await afterAction(tabId);
      return { ok: true, message: "Done; the page changed." };
    }
    throw error;
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
      const tab = await chrome.tabs.create({
        url,
        active: request.active !== false,
      });
      pilotTabs.add(tab.id);
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
        .map((tab: any) => ({ tab: tab.id, title: tab.title, url: tab.url }));
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
    case "scroll":
      return inPage(ownTab(request.tab), "scroll", [
        request.down !== false,
        Number(request.pages ?? 1),
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
