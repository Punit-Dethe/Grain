import {
  AudioLines,
  Bug,
  Info,
  Mic,
  MonitorCog,
  Sparkles,
  TextCursorInput,
  type LucideIcon,
} from "lucide-react";
import type { AppSettings } from "@/bindings";
import type { SettingsSectionId } from "../navigation";

export interface SettingsSection {
  id: SettingsSectionId;
  icon: LucideIcon;
  enabled: (settings: AppSettings | null) => boolean;
}

export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: "capture", icon: AudioLines, enabled: () => true },
  { id: "audio", icon: Mic, enabled: () => true },
  { id: "output", icon: TextCursorInput, enabled: () => true },
  { id: "application", icon: MonitorCog, enabled: () => true },
  {
    id: "post-processing",
    icon: Sparkles,
    enabled: () => true,
  },
  {
    id: "debug",
    icon: Bug,
    enabled: (settings) => settings?.debug_mode === true,
  },
  { id: "about", icon: Info, enabled: () => true },
] as const;

export function isSettingsSectionEnabled(
  section: SettingsSectionId,
  settings: AppSettings | null,
): boolean {
  return SETTINGS_SECTIONS.find(({ id }) => id === section)!.enabled(settings);
}
