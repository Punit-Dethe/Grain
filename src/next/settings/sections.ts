import {
  AudioLines,
  Bug,
  SlidersHorizontal,
  Sparkles,
  Wrench,
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
  { id: "general", icon: SlidersHorizontal, enabled: () => true },
  { id: "advanced", icon: Wrench, enabled: () => true },
  { id: "speech-to-text", icon: AudioLines, enabled: () => true },
  {
    id: "post-processing",
    icon: Sparkles,
    enabled: (settings) => settings?.post_process_enabled === true,
  },
  {
    id: "debug",
    icon: Bug,
    enabled: (settings) => settings?.debug_mode === true,
  },
] as const;

export function isSettingsSectionEnabled(
  section: SettingsSectionId,
  settings: AppSettings | null,
): boolean {
  return SETTINGS_SECTIONS.find(({ id }) => id === section)!.enabled(settings);
}
