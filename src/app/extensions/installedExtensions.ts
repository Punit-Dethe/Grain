import type {
  ConnectionView,
  ExtensionCard,
  McpProviderStatus,
  StoreEntry,
} from "@/bindings";
import { sortExtensionCards } from "./extensionRuntime";

/** A shared presentation model; execution still uses each host's original owner. */
export function installedExtensionItems(
  cards: ExtensionCard[],
  connections: ConnectionView[],
  accounts: Record<string, McpProviderStatus>,
  listings: Record<string, StoreEntry>,
) {
  return sortExtensionCards([
    ...cards.map((card) => ({
      id: card.id,
      name: card.name,
      description: card.description,
      enabled: card.enabled,
      toggle_seq: card.toggle_seq,
      status: card.enabled ? "Enabled" : "Disabled",
      card,
      connection: null as ConnectionView | null,
    })),
    ...connections
      .filter((row) => row.source === "store")
      .map((connection) => {
        const account = accounts[connection.id];
        return {
          id: connection.extensionId ?? connection.id,
          name: connection.name,
          description:
            listings[connection.extensionId ?? ""]?.description ?? "",
          enabled: account?.enabled ?? false,
          toggle_seq: "18446744073709551615",
          status: !account
            ? "Status unavailable"
            : connection.authentication === "oauth" && !account.connected
              ? "Sign in required"
              : account.enabled
                ? "Enabled"
                : "Disabled",
          card: null as ExtensionCard | null,
          connection,
        };
      }),
  ]);
}
