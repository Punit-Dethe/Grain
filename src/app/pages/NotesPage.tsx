import { NotesTab } from "@/components/grain-space/NotesTab";

export function NotesPage() {
  const openSettings =
    new URLSearchParams(window.location.hash.split("?", 2)[1] ?? "").get(
      "settings",
    ) === "1";
  return (
    <section
      className="page active notes-workspace-page"
      data-page-panel="notes"
    >
      <NotesTab variant="next" initialSettingsOpen={openSettings} />
    </section>
  );
}
