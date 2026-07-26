import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronLeft } from "lucide-react";
import { useSettings } from "../../hooks/useSettings";
import { Switch } from "../ui/Switch";
import { FeaturePanel } from "../settings/experimentations/FeaturePanel";
import { ExtensionAnchor } from "../settings/experimentations/ExtensionSettings";
import { GrainSpaceSettings } from "../settings/grain-space/GrainSpaceSettings";
import { McpBridge } from "../settings/grain-space/McpBridge";
import { GrainSpaceOverlay } from "./GrainSpaceOverlay";

const TITLE = "Notes";

/**
 * [GRAIN] The Notes tab (NOTES-TAB-PLAN.md).
 *
 * Grain Note is a primary feature with a primary tab, not an extension and not a
 * separate window. The tab is ALWAYS present — that is the point of the pivot:
 * what made the feature feel heavy was never the notes, it was having to find
 * and install something before you could see one.
 *
 * `grain_space_enabled` still defaults off, and off still means off: no
 * directories, no index, no embedding model, no registered shortcuts. So when the
 * feature is off this tab is the on-ramp — one sentence and one switch — rather
 * than a hidden tab or a live workspace over a corpus that does not exist yet.
 *
 * Settings are a PAGE inside this tab, reached from the rail's bottom row, rather
 * than a pane beside the workspace: they are drawn with Grain's app tokens and the
 * workspace with `.gs-frame`'s own scoped ones, and nesting the two mixes two
 * visual languages inside one frame (and collides on `data-theme`). Same pattern
 * as an extension's own page.
 */
export function NotesTab() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = (getSetting("grain_space_enabled") as boolean) ?? false;
  const [settingsOpen, setSettingsOpen] = useState(false);

  if (!enabled) {
    return (
      <div className="w-full h-full flex items-center justify-center p-12">
        <div className="max-w-md text-center space-y-4">
          <h1 className="text-2xl font-semibold tracking-tight text-ink">
            {TITLE}
          </h1>
          <p className="text-sm text-ink-soft leading-relaxed">
            {t("grainSpaceOverlay.tabBlurb")}
          </p>
          <div className="flex justify-center pt-1">
            <Switch
              checked={false}
              isUpdating={isUpdating("grain_space_enabled")}
              onChange={() => updateSetting("grain_space_enabled", true)}
              ariaLabel={`Turn on ${TITLE}`}
            />
          </div>
          <p className="text-xs text-ink-faint">
            {t("grainSpaceOverlay.tabZeroCost")}
          </p>
        </div>
      </div>
    );
  }

  if (settingsOpen) {
    return (
      <div className="w-full h-full overflow-y-auto">
        <div className="max-w-4xl w-full mx-auto px-12 py-9 space-y-6">
          <button
            type="button"
            onClick={() => setSettingsOpen(false)}
            className="flex items-center gap-1 text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
          >
            <ChevronLeft width={13} height={13} />
            {TITLE}
          </button>
          {/* Exactly what the Extensions → Grain Space sub-tab used to render, in
              the same order. The master switch stays the first row: it is what
              turns the feature off, and it has to be reachable from the feature. */}
          <FeaturePanel
            settingKey="grain_space_enabled"
            title="Grain Space"
            info="A local notebook you dictate into and ask questions across. Notes are plain Markdown on your disk; search runs on this machine over full text, meaning and the things your notes mention."
          />
          <GrainSpaceSettings embedded />
          <McpBridge />
          {/* Extensions extend the notebook here — the MCP bridge was the first. */}
          <ExtensionAnchor anchor="grainspace.after" />
        </div>
      </div>
    );
  }

  return <GrainSpaceOverlay onOpenSettings={() => setSettingsOpen(true)} />;
}
