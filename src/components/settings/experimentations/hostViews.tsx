import React from "react";
import { GrainSpaceSettings } from "../grain-space/GrainSpaceSettings";

/**
 * [GRAIN] Host-implemented panel views (`SettingKind::Panel` with
 * `uiSource: "grain://<view-id>"`).
 *
 * A normal custom card is author HTML in an opaque-origin sandboxed iframe. A
 * host view is the opposite: Grain's OWN React, rendered with Grain's own
 * privileges. That is only sound because `grain-sdk` restricts the `grain://`
 * scheme to the **builtin** tier at import, which is itself restricted to
 * reserved `grain.` ids — so a community pack can never name one of these.
 *
 * It exists for exactly one reason: a feature whose implementation is compiled
 * into Grain (Grain Space owns an ONNX embedding engine, sqlite-vec, a native
 * overlay window and global shortcuts) has settings that cannot be expressed as
 * declarative rows and cannot be re-plumbed through `extension_host_call` —
 * a native folder picker, a Hugging Face download with live progress, an index
 * rebuild. Those keep their real UI instead of being rewritten as a worse one.
 *
 * Adding an entry is a deliberate act: the view id is part of a published
 * manifest, so renaming one breaks an installed extension.
 */
const HOST_VIEWS: Record<string, React.ComponentType> = {
  // Grain Space's whole settings surface, minus the page header the extension
  // detail view already draws.
  "grain-space/settings": () => <GrainSpaceSettings embedded />,
};

/** The `grain://` view id a panel names, or null for ordinary embedded markup. */
export function hostViewId(uiSource: string): string | null {
  const trimmed = uiSource.trim();
  if (!trimmed.startsWith("grain://")) return null;
  const id = trimmed.slice("grain://".length);
  return id.length > 0 ? id : null;
}

/**
 * Render a host view. An id this build doesn't know renders a plain notice
 * rather than a blank space — a pack from a newer Grain must degrade visibly,
 * never silently (the same rule `SettingKind::Unsupported` follows).
 */
export const HostView: React.FC<{ viewId: string }> = ({ viewId }) => {
  const Component = HOST_VIEWS[viewId];
  if (!Component) {
    return (
      <div className="px-3 py-2 rounded-lg bg-paper-sunken text-xs text-ink-soft">
        This setting needs a newer version of Grain.
      </div>
    );
  }
  return <Component />;
};
