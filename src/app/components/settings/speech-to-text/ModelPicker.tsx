import { useEffect, useId, useState } from "react";
import { ChevronDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useModelStore } from "@/stores/modelStore";
import { ModelsSettings } from "@/handy-models/components/settings/models/ModelsSettings";
import { useModelEntries } from "@/handy-models/components/settings/models/modelEntries";
import { toTableModel } from "@/handy-models/components/shared/modelTable/tableModel";
import {
  Codes,
  Values,
} from "@/handy-models/components/shared/modelTable/columns";
import { watchModelJobs } from "@/handy-models/modelJobs";

/** Handy's catalogue, with Grain's existing settings and model store. */
export function ModelPicker() {
  const { t } = useTranslation();
  const { models, currentModel, loading } = useModelStore();
  const { facts, lists } = useModelEntries();
  const [expanded, setExpanded] = useState<boolean | null>(null);
  const catalogueId = useId();
  const selectedInfo = models.find((model) => model.id === currentModel);
  const selected = selectedInfo ? toTableModel(selectedInfo, t) : null;
  // One language keeps the closed row compact; the catalogue shows every
  // preferred language in its full set of accuracy columns.
  const summaryGrading = { ...facts, langs: facts.langs.slice(0, 1) };
  const open = expanded ?? (!loading && !selected);

  useEffect(() => {
    void useModelStore.getState().initialize();
    return watchModelJobs();
  }, []);

  useEffect(() => {
    if (currentModel) setExpanded(false);
  }, [currentModel]);

  return (
    <section className="handy-model-catalogue space-y-3">
      <h2 className="text-xl font-semibold">{t("settings.models.title")}</h2>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={open ? catalogueId : undefined}
        onClick={() => setExpanded(!open)}
        className="grid w-full grid-cols-[minmax(0,1fr)_auto_16px] items-center gap-4 rounded-xl border border-line bg-paper-raised px-4 py-3 text-start transition-colors hover:bg-wash focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
      >
        <span className="min-w-0">
          <span
            className="block truncate text-sm font-semibold"
            title={selected?.name}
          >
            {selected?.name ?? t("settings.models.choose", "Choose a model")}
          </span>
          <span className="mt-1 block text-xs text-ink-3">
            {t(
              open
                ? "settings.models.hideCatalogue"
                : "settings.models.browseCatalogue",
            )}
          </span>
        </span>
        {selected ? (
          <span className="grid gap-2">
            <Codes langs={summaryGrading.langs} />
            <Values
              grading={summaryGrading}
              model={selected.model}
              streaming={selected.streaming}
              sizeMb={selected.sizeMb}
            />
          </span>
        ) : (
          <span />
        )}
        <ChevronDown
          size={16}
          aria-hidden="true"
          className={open ? "rotate-180" : ""}
        />
      </button>
      {open && (
        <div
          id={catalogueId}
          className="flex h-[min(65vh,640px)] min-h-[320px]"
        >
          <ModelsSettings facts={facts} lists={lists} />
        </div>
      )}
      <CatalogueError />
    </section>
  );
}

function CatalogueError() {
  const error = useModelStore((state) => state.error);
  return error ? (
    <p role="alert" className="text-status-error">
      {error}
    </p>
  ) : null;
}
