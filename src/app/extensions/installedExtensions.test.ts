import { describe, expect, it } from "vitest";
import type {
  ConnectionView,
  ExtensionCard,
  McpProviderStatus,
  StoreEntry,
} from "@/bindings";
import { filterExtensions } from "./extensionRuntime";
import { installedExtensionItems } from "./installedExtensions";

const native = {
  id: "native.tools",
  name: "Tools",
  description: "Local tools",
  enabled: true,
  toggle_seq: "1",
} as ExtensionCard;
const connection: ConnectionView = {
  id: "store:github",
  source: "store",
  extensionId: "com.grain.github",
  version: "0.1.0",
  revision: "revision-one",
  name: "GitHub",
  url: "https://example.com/mcp",
  authentication: "oauth",
  state: "inactive",
};
const account = (
  overrides: Partial<McpProviderStatus> = {},
): McpProviderStatus => ({
  id: connection.id,
  name: "GitHub",
  description: "",
  endpoint: connection.url,
  setup_url: "",
  requires_client_credentials: false,
  client_id_configured: false,
  connected: true,
  enabled: true,
  state: "stored",
  ...overrides,
});
const listings = {
  "com.grain.github": { description: "Repositories and issues" } as StoreEntry,
};

describe("unified installed extensions", () => {
  it("combines native and store-installed cards while preserving their exact execution owners", () => {
    const items = installedExtensionItems(
      [native],
      [connection],
      { [connection.id]: account() },
      listings,
    );
    expect(items.map((item) => item.name)).toEqual(["Tools", "GitHub"]);
    expect(items[0].card).toBe(native);
    expect(items[1].connection).toBe(connection);
    expect(items[1].card).toBeNull();
    expect(items[1].status).toBe("Enabled");
    expect(filterExtensions(items, "issues").map((item) => item.id)).toEqual([
      "com.grain.github",
    ]);
  });
  it("keeps manually configured servers out of the installed store collection", () => {
    expect(
      installedExtensionItems(
        [],
        [{ ...connection, source: "configured" }],
        {},
        {},
      ),
    ).toEqual([]);
  });
  it("shows required sign-in without treating a disabled account as connected", () => {
    const [item] = installedExtensionItems(
      [],
      [connection],
      { [connection.id]: account({ connected: false, enabled: false }) },
      listings,
    );
    expect(item.status).toBe("Sign in required");
    expect(item.enabled).toBe(false);
  });
  it("retains a manageable card when catalogue or account status is unavailable", () => {
    const [item] = installedExtensionItems([], [connection], {}, {});
    expect(item.name).toBe("GitHub");
    expect(item.connection?.revision).toBe("revision-one");
    expect(item.status).toBe("Status unavailable");
    expect(item.enabled).toBe(false);
  });
  it("does not request sign-in for an unauthenticated server and reflects disablement", () => {
    const [item] = installedExtensionItems(
      [],
      [{ ...connection, authentication: "none" }],
      { [connection.id]: account({ connected: false, enabled: false }) },
      {},
    );
    expect(item.status).toBe("Disabled");
  });
});
