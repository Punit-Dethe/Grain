import { NotesTab } from "@/components/grain-space/NotesTab";

export function NotesPage() {
  return (
    <section className="page notes-workspace-page" data-page-panel="notes">
      <NotesTab variant="next" />
    </section>
  );
}
