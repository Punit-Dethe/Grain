import { ToolCanvas } from "./ToolsPage";

export function AgentPage() {
  return (
    <section
      className="page active tools-workspace-page"
      data-page-panel="agent"
    >
      <div className="page-wrap tools-page-wrap">
        <div className="tools-shell agent-page-shell">
          <ToolCanvas section="agent" />
        </div>
      </div>
    </section>
  );
}
