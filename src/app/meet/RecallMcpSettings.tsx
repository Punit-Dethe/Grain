import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type McpClient,
  type McpClientStatus,
  type RecallMcpStatus,
} from "@/bindings";
import { Button } from "../components/ui/Button";
import { SettingContainer } from "../components/ui/SettingContainer";
import { SettingsGroup } from "../components/ui/SettingsGroup";
import { ToggleSwitch } from "../components/ui/ToggleSwitch";

type Busy = "switch" | McpClient | null;

/** Recall's "Use your meetings in AI apps" switch and one Connect row per AI
 * app. The MCP server runs only inside an AI app that starts it; this page
 * just edits the gate and the apps' configs, and holds nothing once closed. */
export function RecallMcpSettings() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<RecallMcpStatus | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refresh = useCallback(async () => {
    const result = await commands.recallMcpStatus();
    if (result.status === "ok") setStatus(result.data);
    else setError(result.error);
  }, []);

  // Re-check when the window regains focus: the user may have installed an
  // AI app or edited its config in the meantime. Listener lives with the page.
  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const run = async (
    what: Busy,
    action: () => Promise<
      | { status: "ok"; data: RecallMcpStatus }
      | { status: "error"; error: string }
    >,
  ) => {
    setBusy(what);
    setError(null);
    try {
      const result = await action();
      if (result.status === "ok") {
        setStatus(result.data);
      } else {
        setError(result.error);
        await refresh();
      }
    } finally {
      setBusy(null);
    }
  };

  const copySetup = async () => {
    if (!status) return;
    try {
      await navigator.clipboard.writeText(status.setup_json);
      setCopied(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const enabled = status?.enabled ?? false;
  const missingServer = status !== null && status.server_path === null;

  return (
    <div className="meet-settings-group">
      <SettingsGroup
        title={t("ui2.meet.settings.mcp.title")}
        info={t("ui2.meet.settings.mcp.info")}
      >
        <ToggleSwitch
          label={t("ui2.meet.settings.mcp.enable")}
          description={t("ui2.meet.settings.mcp.enableDescription")}
          descriptionMode="inline"
          grouped
          checked={enabled}
          disabled={status === null}
          isUpdating={busy === "switch"}
          onChange={(value) =>
            void run("switch", () => commands.recallMcpSetEnabled(value))
          }
        />
        {enabled &&
          status?.clients.map((client) => (
            <ClientRow
              key={client.client}
              client={client}
              busy={busy === client.client}
              disabled={busy !== null || missingServer}
              onConnect={() =>
                void run(client.client, () =>
                  commands.recallMcpConnect(client.client),
                )
              }
              onDisconnect={() =>
                void run(client.client, () =>
                  commands.recallMcpDisconnect(client.client),
                )
              }
            />
          ))}
        {enabled && (
          <SettingContainer
            title={t("ui2.meet.settings.mcp.other")}
            description={t("ui2.meet.settings.mcp.otherDescription")}
            descriptionMode="inline"
            grouped
          >
            <Button
              variant="secondary"
              size="sm"
              disabled={missingServer}
              onClick={() => void copySetup()}
            >
              {copied
                ? t("ui2.meet.settings.mcp.copied")
                : t("ui2.meet.settings.mcp.copySetup")}
            </Button>
          </SettingContainer>
        )}
      </SettingsGroup>

      {enabled && !missingServer && (
        <p className="meet-settings-note">
          {t("ui2.meet.settings.mcp.restartNote")}
        </p>
      )}
      {enabled && missingServer && (
        <div className="meet-callout is-error" role="alert">
          <p>{t("ui2.meet.settings.mcp.missingServer")}</p>
        </div>
      )}
      {error && (
        <div className="meet-callout is-error" role="alert">
          <p className="meet-settings-error">{error}</p>
        </div>
      )}
    </div>
  );
}

function ClientRow({
  client,
  busy,
  disabled,
  onConnect,
  onDisconnect,
}: {
  client: McpClientStatus;
  busy: boolean;
  disabled: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
}) {
  const { t } = useTranslation();
  const state = client.state;
  return (
    <SettingContainer
      title={client.name}
      description={t(`ui2.meet.settings.mcp.state.${state}`)}
      descriptionMode="inline"
      grouped
      disabled={state === "not_installed"}
    >
      <div className="meet-mcp-actions">
        {state === "needs_update" && (
          <Button
            variant="primary-soft"
            size="sm"
            disabled={disabled}
            onClick={onConnect}
          >
            {t("ui2.meet.settings.mcp.update")}
          </Button>
        )}
        {(state === "connected" || state === "needs_update") && (
          <Button
            variant="ghost"
            size="sm"
            disabled={disabled}
            onClick={onDisconnect}
          >
            {busy
              ? t("ui2.meet.settings.mcp.working")
              : t("ui2.meet.settings.mcp.disconnect")}
          </Button>
        )}
        {state === "not_connected" && (
          <Button
            variant="secondary"
            size="sm"
            disabled={disabled}
            onClick={onConnect}
          >
            {busy
              ? t("ui2.meet.settings.mcp.working")
              : t("ui2.meet.settings.mcp.connect")}
          </Button>
        )}
      </div>
    </SettingContainer>
  );
}
