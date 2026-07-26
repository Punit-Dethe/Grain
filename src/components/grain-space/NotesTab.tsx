import { useSettings } from "../../hooks/useSettings";
import { Switch } from "../ui/Switch";
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
 */
export function NotesTab() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = (getSetting("grain_space_enabled") as boolean) ?? false;

  if (enabled) return <GrainSpaceOverlay />;

  return (
    <div className="w-full h-full flex items-center justify-center p-12">
      <div className="max-w-md text-center space-y-4">
        <h1 className="text-2xl font-semibold tracking-tight text-ink">
          {TITLE}
        </h1>
        <p className="text-sm text-ink-soft leading-relaxed">
          A notebook that lives on your disk as plain Markdown — dictate into it,
          write in it, or point it at an Obsidian vault you already have. Search
          runs on this machine.
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
          Nothing loads and nothing is written to disk until you turn it on.
        </p>
      </div>
    </div>
  );
}
