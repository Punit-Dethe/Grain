export const SETTINGS_SECTION_IDS = [
  "general",
  "advanced",
  "speech-to-text",
  "post-processing",
  "debug",
] as const;

export type SettingsSectionId = (typeof SETTINGS_SECTION_IDS)[number];

export type AppRoute =
  | { page: "overview" }
  | { page: "notes" }
  | { page: "history" }
  | { page: "settings"; section: SettingsSectionId };

const settingsSections = new Set<string>(SETTINGS_SECTION_IDS);

export function routeFromHash(hash: string): AppRoute {
  const path = hash.replace(/^#/, "").split(/[?#]/, 1)[0].replace(/\/+$/, "");

  if (path === "/history") return { page: "history" };
  if (path === "/notes") return { page: "notes" };

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
  return `#/${route.page}`;
}
