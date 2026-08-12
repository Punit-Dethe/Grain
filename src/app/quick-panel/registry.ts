// [GRAIN] Quick Panel registry — the single source of truth for what the panel
// can find. Settings live as ~50 individual components with no central index, so
// this file *is* that index: each entry pairs an i18n title key with a section
// (for navigation + gating) and a hand-curated alias list. The aliases are the
// "semantic-lite" layer — "mic" finds Microphone, "dark mode" finds Appearance —
// which is how real command palettes (VS Code, Linear, Raycast) feel smart
// without an embedding model.

import type { TFunction } from "i18next";
import type { AppSettings } from "@/bindings";
import type { AppRoute, SettingsSectionId } from "../navigation";
import { SETTINGS_SECTIONS } from "../settings/sections";

export type QuickIconName =
  | "home"
  | "note"
  | "clock"
  | "zap"
  | "sliders"
  | "box"
  | "command"
  | "search"
  | "panel"
  | "folder"
  | "star";

export type QuickKind = "navigate" | "section" | "setting";

export interface QuickItem {
  id: string;
  kind: QuickKind;
  /** Already-translated, ready to render. */
  title: string;
  icon: QuickIconName;
  /** Muted right-aligned label (page name / owning settings section). */
  metaLabel: string;
  route: AppRoute;
  /** Present for settings so the panel can scroll+pulse the row after landing. */
  section?: SettingsSectionId;
  /** Lower-cased haystack of synonyms searched alongside the title. */
  keywords: string[];
}

// -- top-level navigation -------------------------------------------------------

interface NavDef {
  id: string;
  title: string;
  icon: QuickIconName;
  route: AppRoute;
  keywords: string[];
}

const NAV_DEFS: NavDef[] = [
  {
    id: "overview",
    title: "Overview",
    icon: "home",
    route: { page: "overview" },
    keywords: ["dashboard", "home", "start", "recent"],
  },
  {
    id: "notes",
    title: "Notes",
    icon: "note",
    route: { page: "notes" },
    keywords: ["grain space", "documents", "writing", "vault"],
  },
  {
    id: "history",
    title: "History",
    icon: "clock",
    route: { page: "history" },
    keywords: ["transcripts", "past recordings", "log"],
  },
  {
    id: "studio",
    title: "Studio",
    icon: "zap",
    route: { page: "tools", section: "dictionary" },
    keywords: ["tools", "dictionary", "snippets", "agent", "context awareness"],
  },
  {
    id: "settings",
    title: "Settings",
    icon: "sliders",
    route: { page: "settings", section: "capture" },
    keywords: ["preferences", "options", "config", "configuration"],
  },
  {
    id: "extensions",
    title: "Extensions",
    icon: "box",
    route: { page: "extensions", view: "installed" },
    keywords: ["plugins", "add ons", "store", "marketplace"],
  },
];

// -- individual settings --------------------------------------------------------

type Gate = (settings: AppSettings | null) => boolean;

const experimentalOn: Gate = (s) => s?.experimental_enabled === true;
const postProcessOn: Gate = (s) => s?.post_process_enabled === true;
const debugOn: Gate = (s) => s?.debug_mode === true;

interface SettingDef {
  id: string;
  titleKey: string;
  section: SettingsSectionId;
  keywords: string[];
  /** Hidden until reachable (matches the pane's own conditional rendering). */
  enabled?: Gate;
}

const SETTING_DEFS: SettingDef[] = [
  // capture ---------------------------------------------------------------
  {
    id: "capture-shortcut",
    titleKey: "ui2.capture.set.title",
    section: "capture",
    keywords: [
      "shortcut",
      "hotkey",
      "keybinding",
      "record key",
      "start recording",
    ],
  },
  {
    id: "push-to-talk",
    titleKey: "settings.general.pushToTalk.label",
    section: "capture",
    keywords: ["ptt", "hold to talk", "walkie talkie"],
  },
  {
    id: "ai-always",
    titleKey: "ui2.capture.ai.always.title",
    section: "capture",
    keywords: ["ai key", "post processing shortcut", "always process"],
  },
  {
    id: "ai-end",
    titleKey: "ui2.capture.ai.end.title",
    section: "capture",
    keywords: ["ai after recording", "refine on end"],
  },
  {
    id: "ai-start-mode",
    titleKey: "ui2.capture.ai.startMode.title",
    section: "capture",
    keywords: ["ai start mode"],
  },
  {
    id: "language",
    titleKey: "settings.general.language.title",
    section: "capture",
    keywords: ["spoken language", "dictation language", "locale"],
  },
  {
    id: "translate-english",
    titleKey: "settings.advanced.translateToEnglish.label",
    section: "capture",
    keywords: ["translate", "english"],
  },
  // audio -----------------------------------------------------------------
  {
    id: "microphone",
    titleKey: "settings.sound.microphone.title",
    section: "audio",
    keywords: ["mic", "input device", "recording device"],
  },
  {
    id: "voice-processing",
    titleKey: "settings.debug.voiceProcessing.label",
    section: "audio",
    keywords: [
      "noise",
      "agc",
      "high pass",
      "audio conditioning",
      "clean audio",
    ],
  },
  {
    id: "mute-while-recording",
    titleKey: "settings.debug.muteWhileRecording.label",
    section: "audio",
    keywords: ["mute speakers", "silence output"],
  },
  {
    id: "audio-feedback",
    titleKey: "settings.sound.audioFeedback.label",
    section: "audio",
    keywords: ["sounds", "beep", "chime", "start sound"],
  },
  {
    id: "output-device",
    titleKey: "settings.sound.outputDevice.title",
    section: "audio",
    keywords: ["speaker", "playback device"],
  },
  {
    id: "volume",
    titleKey: "settings.sound.volume.title",
    section: "audio",
    keywords: ["loudness", "feedback volume"],
  },
  // output ----------------------------------------------------------------
  {
    id: "paste-method",
    titleKey: "settings.advanced.pasteMethod.title",
    section: "output",
    keywords: ["paste", "insert text", "clipboard paste", "typing"],
  },
  {
    id: "typing-tool",
    titleKey: "settings.advanced.typingTool.title",
    section: "output",
    keywords: ["keyboard simulation", "enigo", "xdotool"],
  },
  {
    id: "clipboard-handling",
    titleKey: "settings.advanced.clipboardHandling.title",
    section: "output",
    keywords: ["clipboard", "copy"],
  },
  {
    id: "auto-submit",
    titleKey: "settings.advanced.autoSubmit.title",
    section: "output",
    keywords: ["press enter", "send", "submit"],
  },
  {
    id: "append-space",
    titleKey: "settings.debug.appendTrailingSpace.label",
    section: "output",
    keywords: ["trailing space", "add space"],
  },
  {
    id: "scrap-that",
    titleKey: "ui2.settings.scrapThat.title",
    section: "output",
    keywords: ["undo", "voice reset", "cancel dictation"],
  },
  // application -----------------------------------------------------------
  {
    id: "appearance",
    titleKey: "ui2.appearance.title",
    section: "application",
    keywords: ["theme", "dark mode", "light mode", "colour", "color"],
  },
  {
    id: "default-panel",
    titleKey: "settings.advanced.defaultPanel.title",
    section: "application",
    keywords: ["startup page", "default view", "landing"],
  },
  {
    id: "overlay",
    titleKey: "settings.advanced.overlay.title",
    section: "application",
    keywords: ["show overlay", "pill", "recording indicator", "hud"],
  },
  {
    id: "autostart",
    titleKey: "settings.advanced.autostart.label",
    section: "application",
    keywords: ["launch on login", "start with system", "boot"],
  },
  {
    id: "start-hidden",
    titleKey: "settings.advanced.startHidden.label",
    section: "application",
    keywords: ["minimized", "tray start", "background"],
  },
  {
    id: "tray-icon",
    titleKey: "settings.advanced.showTrayIcon.label",
    section: "application",
    keywords: ["system tray", "menu bar icon"],
  },
  {
    id: "update-checks",
    titleKey: "settings.debug.updateChecks.label",
    section: "application",
    keywords: ["auto update", "check for updates"],
  },
  {
    id: "history-limit",
    titleKey: "settings.debug.historyLimit.title",
    section: "application",
    keywords: ["history", "retention count", "saved transcriptions"],
  },
  {
    id: "recording-retention",
    titleKey: "settings.debug.recordingRetention.title",
    section: "application",
    keywords: ["delete recordings", "audio retention", "privacy"],
  },
  {
    id: "experimental",
    titleKey: "settings.advanced.experimentalToggle.label",
    section: "application",
    keywords: ["beta", "experiments", "advanced"],
  },
  {
    id: "keyboard-implementation",
    titleKey: "settings.debug.keyboardImplementation.title",
    section: "application",
    keywords: ["keyboard backend", "enigo"],
    enabled: experimentalOn,
  },
  {
    id: "acceleration",
    titleKey: "settings.advanced.acceleration.whisper.title",
    section: "application",
    keywords: ["gpu", "cuda", "vulkan", "hardware acceleration"],
    enabled: experimentalOn,
  },
  {
    id: "lazy-stream-close",
    titleKey: "settings.advanced.lazyStreamClose.label",
    section: "application",
    keywords: ["stream close", "latency"],
    enabled: experimentalOn,
  },
  // speech-to-text --------------------------------------------------------
  {
    id: "stt",
    titleKey: "settings.speechToText.title",
    section: "speech-to-text",
    keywords: [
      "model",
      "asr",
      "whisper",
      "parakeet",
      "transcription engine",
      "cloud provider",
    ],
  },
  {
    id: "rolling-live-preview",
    titleKey: "settings.speechToText.rollingLivePreview.label",
    section: "speech-to-text",
    keywords: ["live preview", "streaming preview", "rolling"],
  },
  // post-processing (gated) ----------------------------------------------
  {
    id: "post-processing-prompts",
    titleKey: "settings.postProcessing.prompts.title",
    section: "post-processing",
    keywords: ["ai prompt", "refine", "cleanup prompt", "llm"],
    enabled: postProcessOn,
  },
  // debug (gated) ---------------------------------------------------------
  {
    id: "log-level",
    titleKey: "settings.debug.logLevel.title",
    section: "debug",
    keywords: ["logging", "verbosity"],
    enabled: debugOn,
  },
  {
    id: "sound-theme",
    titleKey: "settings.debug.soundTheme.label",
    section: "debug",
    keywords: ["sound pack", "feedback sounds"],
    enabled: debugOn,
  },
  {
    id: "word-correction",
    titleKey: "settings.debug.wordCorrectionThreshold.title",
    section: "debug",
    keywords: ["correction threshold", "dictionary strength"],
    enabled: debugOn,
  },
  {
    id: "recording-buffer",
    titleKey: "settings.debug.recordingBuffer.title",
    section: "debug",
    keywords: ["buffer", "pre roll"],
    enabled: debugOn,
  },
  {
    id: "always-on-mic",
    titleKey: "settings.debug.alwaysOnMicrophone.label",
    section: "debug",
    keywords: ["keep mic open", "warm microphone"],
    enabled: debugOn,
  },
  {
    id: "live-logs",
    titleKey: "settings.debug.liveLogs.title",
    section: "debug",
    keywords: ["live logs", "console", "diagnostics"],
    enabled: debugOn,
  },
  // about -----------------------------------------------------------------
  {
    id: "about-version",
    titleKey: "settings.about.version.title",
    section: "about",
    keywords: ["version", "build number"],
  },
  {
    id: "app-data-directory",
    titleKey: "settings.about.appDataDirectory.title",
    section: "about",
    keywords: ["data folder", "storage location", "where is my data"],
  },
  {
    id: "source-code",
    titleKey: "settings.about.sourceCode.title",
    section: "about",
    keywords: ["github", "repository", "open source"],
  },
];

/**
 * Build the full, current set of searchable items. Rebuilt whenever settings
 * change so gated sections/rows appear exactly when they are reachable.
 */
export function buildQuickItems(
  t: TFunction,
  settings: AppSettings | null,
): QuickItem[] {
  const items: QuickItem[] = [];

  for (const nav of NAV_DEFS) {
    items.push({
      id: `navigate:${nav.id}`,
      kind: "navigate",
      title: nav.title,
      icon: nav.icon,
      metaLabel: "Page",
      route: nav.route,
      keywords: nav.keywords,
    });
  }

  for (const section of SETTINGS_SECTIONS) {
    if (!section.enabled(settings)) continue;
    items.push({
      id: `section:${section.id}`,
      kind: "section",
      title: t(`ui2.settings.sections.${section.id}.label`),
      icon: "sliders",
      metaLabel: "Settings",
      route: { page: "settings", section: section.id },
      section: section.id,
      keywords: [t(`ui2.settings.sections.${section.id}.description`)],
    });
  }

  for (const def of SETTING_DEFS) {
    if (def.enabled && !def.enabled(settings)) continue;
    items.push({
      id: `setting:${def.id}`,
      kind: "setting",
      title: t(def.titleKey),
      icon: "sliders",
      metaLabel: t(`ui2.settings.sections.${def.section}.label`),
      route: { page: "settings", section: def.section },
      section: def.section,
      keywords: def.keywords,
    });
  }

  return items;
}
