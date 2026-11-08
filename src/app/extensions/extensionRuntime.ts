import type { ActionInfo, ExtensionCard, Result } from "@/bindings";

const NEVER_TOGGLED = "18446744073709551615";

const CAPABILITY_LABELS: Record<string, string> = {
  storage: "Store its own data on this device",
};

export type ToolSection = "dictionary" | "snippets" | "context" | "agent";

export function unwrapResult<T, E>(result: Result<T, E>): T {
  if (result.status === "error") throw new Error(String(result.error));
  return result.data;
}

export function capabilityLabel(capability: string): string {
  return capability.startsWith("net:")
    ? `Send data to ${capability.slice("net:".length)}`
    : (CAPABILITY_LABELS[capability] ?? capability);
}

export interface ApprovalRequest {
  approvalDigest: string;
  reviewedName: string;
  permissions: string[];
  actions: ActionInfo[];
  authentication: AuthenticationApproval | null;
}

export interface AuthenticationApproval {
  provider_name: string;
  scopes: string[];
  api_hosts: string[];
  authorization_host: string;
  token_host: string;
}

/**
 * The single structured error an enable can be refused with.
 *
 * Capabilities, actions and the account connection arrive together and are shown in one
 * sheet: two sheets in a row for one enable is how a user learns to click
 * Approve without reading, which defeats the point of asking.
 */
export function parseApprovalRequest(error: unknown): ApprovalRequest | null {
  try {
    const parsed = JSON.parse(String(error)) as {
      approvalDigest?: unknown;
      reviewedName?: unknown;
      needsPermissions?: unknown;
      needsActions?: unknown;
      needsAuthentication?: unknown;
    };
    if (
      typeof parsed.approvalDigest !== "string" ||
      !/^[a-f0-9]{64}$/.test(parsed.approvalDigest)
    )
      return null;
    if (typeof parsed.reviewedName !== "string" || !parsed.reviewedName.trim())
      return null;
    const permissions = Array.isArray(parsed.needsPermissions)
      ? (parsed.needsPermissions as string[])
      : [];
    const actions = Array.isArray(parsed.needsActions)
      ? (parsed.needsActions as ActionInfo[])
      : [];
    const rawAuthentication = parsed.needsAuthentication;
    const authentication =
      rawAuthentication &&
      typeof rawAuthentication === "object" &&
      "provider_name" in rawAuthentication &&
      typeof rawAuthentication.provider_name === "string" &&
      "authorization_host" in rawAuthentication &&
      typeof rawAuthentication.authorization_host === "string" &&
      "token_host" in rawAuthentication &&
      typeof rawAuthentication.token_host === "string" &&
      "scopes" in rawAuthentication &&
      Array.isArray(rawAuthentication.scopes) &&
      rawAuthentication.scopes.every((scope) => typeof scope === "string") &&
      "api_hosts" in rawAuthentication &&
      Array.isArray(rawAuthentication.api_hosts) &&
      rawAuthentication.api_hosts.every((host) => typeof host === "string")
        ? (rawAuthentication as AuthenticationApproval)
        : null;
    // A malformed account request must never be approved under a sheet that
    // only happens to show its other permissions.
    if (rawAuthentication != null && !authentication) return null;
    if (!permissions.length && !actions.length && !authentication) return null;
    return {
      approvalDigest: parsed.approvalDigest,
      reviewedName: parsed.reviewedName,
      permissions,
      actions,
      authentication,
    };
  } catch {
    return null;
  }
}

export function sortExtensionCards<
  T extends Pick<ExtensionCard, "enabled" | "toggle_seq" | "name">,
>(cards: T[]): T[] {
  const sequence = (card: T) =>
    card.toggle_seq === NEVER_TOGGLED
      ? Number.MAX_SAFE_INTEGER
      : Number(card.toggle_seq);
  return [...cards].sort((left, right) => {
    if (left.enabled !== right.enabled) return left.enabled ? -1 : 1;
    const order = sequence(left) - sequence(right);
    return order !== 0 ? order : left.name.localeCompare(right.name);
  });
}

export function filterExtensions<
  T extends { id: string; name: string; description: string },
>(entries: T[], query: string): T[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) =>
    `${entry.name} ${entry.id} ${entry.description}`
      .toLocaleLowerCase()
      .includes(needle),
  );
}
