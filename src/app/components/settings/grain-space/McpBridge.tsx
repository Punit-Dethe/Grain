import React, { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/**
 * [GRAIN] The Grain Space MCP bridge (docs/Grain Space 2.0/MCP-PLAN.md).
 *
 * Off by default and stated plainly: this hands another application read AND
 * write access to the user's notes, which is not something to arrive switched
 * on, and not something to describe in the abstract. Switching it on mints a
 * token; switching it off revokes it, so a client that was connected cannot
 * return.
 *
 * The config block is shown only while the bridge is on. A copyable snippet for
 * a thing that will refuse every call is an invitation to debug the wrong
 * problem.
 */
export const McpBridge: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("grain_space_mcp") ?? false;
  const [path, setPath] = useState("grain-mcp");
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    invoke<string>("grain_space_mcp_path")
      .then((p) => alive && p && setPath(p))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [enabled]);

  // JSON needs the path escaped; the shell line does not.
  const jsonPath = JSON.stringify(path);
  const config = `{
  "mcpServers": {
    "grain-space": { "command": ${jsonPath} }
  }
}`;
  const cli = `claude mcp add grain-space -- ${path}`;

  const copy = async (what: string, text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(what);
    window.setTimeout(() => setCopied((c) => (c === what ? null : c)), 1600);
  };

  return (
    <SettingsGroup>
      <ToggleSwitch
        label="Share this notebook over MCP"
        description="Lets an AI assistant — Claude Code, an IDE agent, a chat client — search, read and add to these notes. It reads the same notes you see; nothing is copied or uploaded."
        checked={enabled}
        isUpdating={isUpdating("grain_space_mcp")}
        onChange={(v) => updateSetting("grain_space_mcp", v)}
      />

      {enabled && (
        <div className="px-4 py-4 space-y-4 border-t border-line">
          <Snippet
            label="Claude Code"
            language="bash"
            text={cli}
            copied={copied === "cli"}
            onCopy={() => void copy("cli", cli)}
          />
          <Snippet
            label="Claude Desktop, or any client with a JSON config"
            language="json"
            text={config}
            copied={copied === "json"}
            onCopy={() => void copy("json", config)}
          />
          <p className="text-xs text-ink-faint leading-relaxed">
            The assistant starts its own copy of the bridge when it needs one and
            closes it afterwards, so nothing runs while nobody is asking. Turning
            this off cuts off every client immediately.
          </p>
        </div>
      )}
    </SettingsGroup>
  );
};

const Snippet: React.FC<{
  label: string;
  language: string;
  text: string;
  copied: boolean;
  onCopy: () => void;
}> = ({ label, language, text, copied, onCopy }) => (
  <div className="space-y-1.5">
    <div className="flex items-center justify-between gap-3">
      <span className="font-mono text-[0.68rem] font-semibold uppercase tracking-[0.1em] text-ink-soft">
        {label}
      </span>
      <button
        type="button"
        onClick={onCopy}
        className="inline-flex items-center gap-1 text-xs text-ink-soft hover:text-ink transition-colors cursor-pointer"
      >
        {copied ? (
          <>
            <Check width={12} height={12} /> Copied
          </>
        ) : (
          <>
            <Copy width={12} height={12} /> Copy
          </>
        )}
      </button>
    </div>
    {/* The path can be long; the block scrolls rather than the page. */}
    <pre
      data-language={language}
      className="overflow-x-auto rounded-lg border border-line bg-paper-sunken px-3 py-2.5 text-xs text-ink font-mono leading-relaxed"
    >
      {text}
    </pre>
  </div>
);
