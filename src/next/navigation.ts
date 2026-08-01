export const SETTINGS_SECTION_IDS = [
  "general",
  "advanced",
  "speech-to-text",
  "post-processing",
  "debug",
  // About sits last: it is the reference shelf of the settings pane, not a
  // workspace of its own.
  "about",
] as const;

export type SettingsSectionId = (typeof SETTINGS_SECTION_IDS)[number];

export const TOOL_SECTION_IDS = [
  "dictionary",
  "snippets",
  "context",
  "agent",
] as const;

export type ToolSectionId = (typeof TOOL_SECTION_IDS)[number];

export const EXTENSION_VIEW_IDS = ["installed", "store"] as const;

export type ExtensionViewId = (typeof EXTENSION_VIEW_IDS)[number];

export type AppRoute =
  | { page: "overview" }
  | { page: "notes" }
  | { page: "history" }
  | { page: "settings"; section: SettingsSectionId }
  | { page: "tools"; section: ToolSectionId }
  | { page: "extensions"; view: ExtensionViewId }
  | { page: "extension-settings"; extensionId: string };

const settingsSections = new Set<string>(SETTINGS_SECTION_IDS);
const toolSections = new Set<string>(TOOL_SECTION_IDS);
const extensionViews = new Set<string>(EXTENSION_VIEW_IDS);

function decodeRouteSegment(segment: string): string | null {
  try {
    const decoded = decodeURIComponent(segment).trim();
    return decoded.length > 0 ? decoded : null;
  } catch {
    return null;
  }
}

export function routeFromHash(hash: string): AppRoute {
  const path = hash.replace(/^#/, "").split(/[?#]/, 1)[0].replace(/\/+$/, "");

  if (path === "/history") return { page: "history" };
  if (path === "/notes") return { page: "notes" };
  // Legacy destination from the brief period About was its own tab.
  if (path === "/about") return { page: "settings", section: "about" };
  if (path === "/tools") return { page: "tools", section: "dictionary" };
  if (path === "/extensions") {
    return { page: "extensions", view: "installed" };
  }

  const toolsMatch = path.match(/^\/tools\/([^/]+)$/);
  if (toolsMatch && toolSections.has(toolsMatch[1])) {
    return { page: "tools", section: toolsMatch[1] as ToolSectionId };
  }

  const extensionsMatch = path.match(/^\/extensions\/([^/]+)$/);
  if (extensionsMatch && extensionViews.has(extensionsMatch[1])) {
    return {
      page: "extensions",
      view: extensionsMatch[1] as ExtensionViewId,
    };
  }

  const extensionSettingsMatch = path.match(/^\/extension-settings\/([^/]+)$/);
  if (extensionSettingsMatch) {
    const extensionId = decodeRouteSegment(extensionSettingsMatch[1]);
    if (extensionId) return { page: "extension-settings", extensionId };
  }

  const settingsMatch = path.match(/^\/settings\/([^/]+)$/);
  if (settingsMatch && settingsSections.has(settingsMatch[1])) {
    return {
      page: "settings",
      section: settingsMatch[1] as SettingsSectionId,
    };
  }

  return { page: "overview" };
}

export function hashForRoute(route: AppRoute): string {
  if (route.page === "settings") return `#/settings/${route.section}`;
  if (route.page === "tools") return `#/tools/${route.section}`;
  if (route.page === "extensions") return `#/extensions/${route.view}`;
  if (route.page === "extension-settings") {
    return `#/extension-settings/${encodeURIComponent(route.extensionId)}`;
  }
  return `#/${route.page}`;
}

export function routeUsesCompactGlobalRail(route: AppRoute): boolean {
  return (
    route.page === "notes" ||
    route.page === "settings" ||
    route.page === "tools" ||
    route.page === "extension-settings"
  );
}
