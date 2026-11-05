import { useEffect, useRef } from "react";
import { useModelStore } from "@/stores/modelStore";
import { ModelsSettings } from "@/handy-models/components/settings/models/ModelsSettings";
import { PreferredLanguages } from "@/handy-models/components/settings/PreferredLanguages";
import { watchModelJobs } from "@/handy-models/modelJobs";

/** Handy's catalogue, with Grain's existing settings and model store. */
export function ModelPicker() {
  const languages = useRef<HTMLDivElement>(null);
  useEffect(() => {
    void useModelStore.getState().initialize();
    return watchModelJobs();
  }, []);
  return (
    <div className="handy-model-catalogue space-y-4">
      <div ref={languages}>
        <PreferredLanguages />
      </div>
      <div className="flex h-[min(65vh,640px)] min-h-[320px]">
        <ModelsSettings
          onChangeLanguages={() => {
            languages.current?.scrollIntoView({ block: "nearest" });
            languages.current
              ?.querySelector<HTMLButtonElement>("button[aria-expanded]")
              ?.focus();
          }}
        />
      </div>
      <CatalogueError />
    </div>
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
