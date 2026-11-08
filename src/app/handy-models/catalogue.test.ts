import { describe, expect, it } from "vitest";
import type { ModelInfo } from "@/bindings";
import { catalog, modelMatchesLanguage } from "@handy-computer/models";
import { CURATED_ASR, groupModels, picksFor } from "./lib/models/catalog";
import { toTableModel } from "./components/shared/modelTable/tableModel";
import { downloadStateOf } from "./lib/models/downloadState";
import type { TFunction } from "i18next";

const model = (streaming: boolean): ModelInfo => ({
  id: "local/test.gguf",
  name: "Test",
  description: "",
  filename: "test.gguf",
  source: "Local",
  size_mb: 1,
  is_downloaded: false,
  is_downloading: false,
  partial_size: 0,
  accuracy_score: 0,
  speed_score: 0,
  supports_translation: false,
  is_recommended: false,
  supported_languages: ["en"],
  supports_language_selection: false,
  is_custom: true,
  supports_streaming: streaming,
  supports_language_detection: false,
});
const t = (() => "") as unknown as TFunction;

describe("Handy catalogue integration", () => {
  it("uses the package ASR catalogue, retaining batch and streaming entries", () => {
    expect(CURATED_ASR).toEqual(
      catalog.curatedModels.filter((m) => m.role === "asr"),
    );
    for (const streaming of [false, true]) {
      const entry = toTableModel(model(streaming), t);
      expect(entry.id).toBe("local/test.gguf");
      expect(entry.streaming).toBe(streaming);
      expect(entry.kind).toBe("custom");
    }
  });
  it("keeps every curated model in exactly one language group", () => {
    for (const langs of [["en"], ["en", "hi"], ["zh"]]) {
      const groups = groupModels(
        langs,
        "cpu",
        CURATED_ASR,
        picksFor(langs, () => true),
      );
      const rows = [...groups.picks, ...groups.others, ...groups.partial];
      expect(new Set(rows.map((r) => r.model.id)).size).toBe(
        CURATED_ASR.length,
      );
      expect(rows.length).toBe(CURATED_ASR.length);
      for (const row of groups.others) {
        expect(
          langs.every((lang) => modelMatchesLanguage(row.model, lang)),
        ).toBe(true);
      }
    }
  });
  it("does not show a selected missing or unverified download as ready", () => {
    const m = model(true);
    const state = {
      models: [m],
      downloadingModels: {},
      verifyingModels: {},
      downloadProgress: {},
    };
    expect(downloadStateOf(state, m.id).state).toBe("none");
    m.is_downloaded = true;
    expect(downloadStateOf(state, m.id).state).toBe("ready");
    expect(
      downloadStateOf({ ...state, verifyingModels: { [m.id]: true } }, m.id)
        .state,
    ).toBe("checking");
  });
});
