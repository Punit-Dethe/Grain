import React from "react";
import { useTranslation } from "react-i18next";
import { Cog, FlaskConical, History, Info, Sparkles, AudioLines } from "lucide-react";
import HandyHand from "./icons/HandyHand";
import { useSettings } from "../hooks/useSettings";
import { useTheme } from "../contexts/ThemeContext";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  PostProcessingSettings,
  SpeechToTextSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

// Brand chrome (proper-noun console labels — kept as constants so the i18n lint
// treats them as identifiers, not translatable copy).
const BRAND_BADGE = "GRAIN // ADVANCED";
const CONSOLE_SUB = "SETTINGS CONSOLE";
const NAV_LABEL = "NAVIGATION";
// Stylized console-chrome label (brackets), not translatable UI copy.
const QUICK_PANEL_LABEL = "[ Quick Panel ]";

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: HandyHand,
    component: GeneralSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  speechToText: {
    labelKey: "sidebar.speechToText",
    icon: AudioLines,
    component: SpeechToTextSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
  onOpenQuickPanel: () => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
  onOpenQuickPanel,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { isSettingsDark } = useTheme();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    // [GRAIN] Sidebar matched to the Advanced Calibration console: a recessed
    // #DDD5C8 rail with a charcoal brand badge, a NAVIGATION label, and numbered
    // tabs that light up in the accent (tint + hairline + pip) when active.
    <div className="flex flex-col w-60 h-full border-e border-line bg-paper-sunken">
      {/* Brand mark — flips in dark mode so it stays legible against the dark sidebar. */}
      <div className="px-5 pt-7">
        <div
          className="h-[30px] rounded-md flex items-center justify-center"
          style={{ backgroundColor: isSettingsDark ? "var(--color-accent)" : "var(--color-ink)" }}
        >
          <span
            className="text-[0.7rem] tracking-[0.18em]"
            style={{
              fontFamily: "Syne, Inter, system-ui, sans-serif",
              color: isSettingsDark ? "#fff" : "var(--on-accent)",
            }}
          >
            {BRAND_BADGE}
          </span>
        </div>
        <div className="mt-1.5 text-center font-mono text-[0.56rem] tracking-[0.2em] text-ink-faint">
          {CONSOLE_SUB}
        </div>
      </div>

      <div className="px-[1.35rem] pt-7 pb-2.5">
        <span className="font-mono text-[0.5rem] tracking-[0.2em] text-ink-faint uppercase">
          {NAV_LABEL}
        </span>
      </div>

      <nav className="flex flex-col w-full gap-1 px-3.5">
        {availableSections.map((section, i) => {
          const isActive = activeSection === section.id;
          const index = String(i + 1).padStart(2, "0");

          return (
            <button
              key={section.id}
              type="button"
              onClick={() => onSectionChange(section.id)}
              className={`flex items-center gap-2.5 h-11 px-3 rounded-[10px] text-start border transition-colors duration-200 ${
                isActive
                  ? "bg-[var(--accent-tint)] border-accent/40"
                  : "border-transparent hover:bg-[rgba(20,19,18,0.03)]"
              }`}
            >
              <span
                className={`font-mono text-[0.62rem] tabular-nums transition-colors ${
                  isActive ? "text-accent" : "text-ink-faint"
                }`}
              >
                {index}
              </span>
              <span
                className={`flex-1 font-mono text-[0.7rem] tracking-[0.08em] uppercase truncate transition-colors ${
                  isActive ? "text-accent" : "text-ink-soft"
                }`}
              >
                {t(section.labelKey)}
              </span>
              {/* Active pip — the row's "selected" tell. */}
              <span
                className={`w-[5px] h-[5px] rounded-full bg-accent transition-opacity duration-200 ${
                  isActive ? "opacity-100" : "opacity-0"
                }`}
              />
            </button>
          );
        })}
      </nav>

      {/* Quick Panel button — near-bottom, slightly above the very edge. */}
      <div className="mt-auto px-3.5 pb-8">
        <button
          type="button"
          onClick={onOpenQuickPanel}
          className="w-full h-11 rounded-[10px] flex items-center justify-center font-mono text-[0.82rem] font-semibold tracking-[0.14em] uppercase text-accent border border-transparent transition-colors hover:bg-[var(--accent-tint)] hover:border-accent/40"
        >
          {QUICK_PANEL_LABEL}
        </button>
      </div>
    </div>
  );
};
