/* eslint-disable i18next/no-literal-string -- fixed console design typography. */
import React, { useState } from "react";
import { useSettings } from "../../hooks/useSettings";
import { usePpPool } from "../settings/post-processing/pool/usePpPool";
import {
  ink,
  fill,
  MechanicalToggle,
  SegToggle,
  ConsoleSelect,
  JackHousing,
  HistoryBox,
} from "./widgets";
import { useProcessingHistory } from "./useHistory";

const MONO = "var(--qp-font-mono)";
const Spacer: React.FC<{ h: number }> = ({ h }) => (
  <div style={{ height: h, flex: "none" }} />
);

/** Dictionary chip with a hover × to remove. */
const WordChip: React.FC<{ word: string; onRemove: () => void }> = ({
  word,
  onRemove,
}) => {
  const [hover, setHover] = useState(false);
  return (
    <span
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className="inline-flex items-center"
      style={{
        height: 28,
        padding: "0 9px",
        gap: 4,
        borderRadius: 5,
        backgroundColor: hover ? fill(0.08) : fill(0.05),
        border: `1px solid ${fill(0.08)}`,
        fontFamily: MONO,
        fontSize: 9,
        fontWeight: 700,
        color: ink(0.75),
        whiteSpace: "nowrap",
        transition: "background-color 0.1s",
      }}
    >
      {word}
      {hover && (
        <button
          type="button"
          onClick={onRemove}
          className="cursor-pointer"
          style={{ fontSize: 11, color: ink(0.5), lineHeight: 1 }}
        >
          ×
        </button>
      )}
    </span>
  );
};

export const ModuleC: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();
  const pool = usePpPool();
  const history = useProcessingHistory();
  const [llmMode, setLlmMode] = useState<0 | 1>(1); // visual placeholder (XIX/LLM)
  const [word, setWord] = useState("");

  // Prompts
  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = getSetting("post_process_selected_prompt_id") || "";
  const selectedPromptName =
    prompts.find((p) => p.id === selectedPromptId)?.name ||
    prompts[0]?.name ||
    "General";
  const onPromptChange = (name: string) => {
    const p = prompts.find((x) => x.name === name);
    if (p) void updateSetting("post_process_selected_prompt_id", p.id);
  };

  // Dictionary (custom_words) — shared with the main Settings; add/remove sync.
  const words = getSetting("custom_words") || [];
  const addWord = () => {
    const w = word.trim().replace(/[<>"'&]/g, "");
    if (!w || w.includes(" ") || w.length > 50 || words.includes(w)) return;
    void updateSetting("custom_words", [...words, w]);
    setWord("");
  };
  const removeWord = (w: string) =>
    void updateSetting(
      "custom_words",
      words.filter((x) => x !== w),
    );

  // LLM providers (configured = has a key)
  const configured = pool.providers.filter((p) =>
    pool.providersWithKeys.has(p.id),
  );
  const activeLabel =
    configured.find((p) => p.id === pool.selectedProviderId)?.label ||
    configured[0]?.label ||
    "No providers configured";
  const onProviderChange = (label: string) => {
    const p = configured.find((x) => x.label === label);
    if (p) void pool.setActiveProvider(p.id);
  };

  return (
    <>
      {/* Directive Prompt */}
      <div
        style={{
          fontFamily: MONO,
          fontSize: 8,
          fontWeight: 700,
          color: ink(0.45),
          marginBottom: 4,
        }}
      >
        Directive Prompt
      </div>
      <ConsoleSelect
        value={selectedPromptName}
        options={prompts.length ? prompts.map((p) => p.name) : ["General"]}
        height={34}
        onChange={onPromptChange}
      />

      <Spacer h={8} />

      {/* Vocabulary dictionary */}
      <div
        className="flex flex-col"
        style={{
          height: 98,
          borderRadius: 8,
          padding: 10,
          gap: 6,
          backgroundColor: fill(0.04),
          border: `1px solid ${fill(0.04)}`,
          flex: "none",
        }}
      >
        <div className="flex items-center" style={{ gap: 8 }}>
          <input
            value={word}
            onChange={(e) => setWord(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addWord();
              }
            }}
            placeholder="Add to dictionary"
            className="flex-1 outline-none"
            style={{
              height: 28,
              borderRadius: 5,
              padding: "0 8px",
              backgroundColor: fill(0.05),
              border: `1px solid ${fill(0.14)}`,
              fontFamily: MONO,
              fontSize: 9,
              fontWeight: 700,
              color: "var(--qp-input-text)",
            }}
          />
          <button
            type="button"
            onClick={addWord}
            className="flex items-center justify-center cursor-pointer"
            style={{
              width: 28,
              height: 28,
              borderRadius: 5,
              backgroundColor: fill(0.05),
              border: `1px solid ${fill(0.14)}`,
              fontSize: 16,
              fontWeight: 700,
              color: ink(0.6),
            }}
          >
            +
          </button>
        </div>
        <div
          className="flex items-center overflow-x-auto qp-scroll"
          style={{ gap: 5, flex: 1 }}
        >
          {words.length === 0 ? (
            <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.35) }}>
              no words added yet
            </span>
          ) : (
            [...words]
              .reverse()
              .map((w) => (
                <WordChip key={w} word={w} onRemove={() => removeWord(w)} />
              ))
          )}
        </div>
      </div>

      <Spacer h={8} />

      {/* Processor LLM */}
      <div className="flex items-center justify-between" style={{ height: 28 }}>
        <span
          style={{
            fontFamily: MONO,
            fontSize: 8,
            fontWeight: 700,
            letterSpacing: "1.5px",
            color: ink(0.45),
          }}
        >
          Processor LLM
        </span>
        <SegToggle
          left="XIX"
          right="LLM"
          value={llmMode}
          activeColor="#8B5CF6"
          onChange={setLlmMode}
        />
      </div>
      <Spacer h={3} />
      <div style={{ opacity: configured.length ? 1 : 0.45 }}>
        <ConsoleSelect
          value={activeLabel}
          options={
            configured.length
              ? configured.map((p) => p.label)
              : ["No providers configured"]
          }
          height={34}
          onChange={onProviderChange}
        />
      </div>
      <Spacer h={5} />
      <div
        className="flex items-center justify-between"
        style={{
          height: 52,
          borderRadius: 8,
          padding: "0 12px",
          backgroundColor: fill(0.04),
          border: `1px solid ${fill(0.05)}`,
        }}
      >
        <div className="flex flex-col" style={{ gap: 2 }}>
          <span style={{ fontSize: 11, fontWeight: 700, color: ink(0.85) }}>
            Smart Rotate
          </span>
          <span style={{ fontFamily: MONO, fontSize: 8, color: ink(0.45) }}>
            Round-robin routing
          </span>
        </div>
        <MechanicalToggle
          checked={pool.smartRotation}
          onChange={(v) => pool.setSmartRotation(v)}
        />
      </div>

      {/* Processing history — only entries the AI has processed (post_processed_text). */}
      <div style={{ height: 14, flex: "none" }} />
      <HistoryBox label="PROCESSED" entries={history} />
      <Spacer h={8} />

      <div style={{ height: 1, backgroundColor: fill(0.1), flex: "none" }} />
      <Spacer h={8} />

      {/* IN / OUT jacks */}
      <div
        className="flex items-center"
        style={{ height: 54, paddingBottom: 10, gap: 4 }}
      >
        <JackHousing
          label="INPUT"
          color="#10B981"
          jackSide="left"
          jackId="moduleC.input"
        />
        <div className="flex-1" />
        <JackHousing
          label="OUTPUT"
          color="#8B5CF6"
          jackSide="right"
          jackId="moduleC.output"
          activeSink
        />
      </div>
    </>
  );
};
