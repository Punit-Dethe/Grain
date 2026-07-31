import { NotesTab } from "@/components/grain-space/NotesTab";

export function NotesPage() {
  return (
    <section
      className="page active notes-workspace-page"
      data-page-panel="notes"
    >
      <NotesTab variant="next" />
    </section>
  );
}
