import { describe, expect, it } from "vitest";

const reviewError = (body: string): string =>
  JSON.stringify({
    ...JSON.parse(body),
    approvalDigest: "a".repeat(64),
    reviewedName: "Test tools",
  });
import { filterExtensions, parseApprovalRequest } from "./extensionRuntime";

describe("extension collection helpers", () => {
  it("searches names, ids, and descriptions directly", () => {
    const entries = [
      {
        id: "voice.actions",
        name: "Voice Actions",
        description: "Voice controls",
      },
      { id: "app.modes", name: "App Modes", description: "App profiles" },
    ];
    expect(filterExtensions(entries, "voice")).toEqual([entries[0]]);
    expect(filterExtensions(entries, "app.modes")).toEqual([entries[1]]);
  });

  it("parses permission holds", () => {
    expect(
      parseApprovalRequest(
        reviewError('{"needsPermissions":["storage","open:url"]}'),
      ),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
      permissions: ["storage", "open:url"],
      actions: [],
      authentication: null,
    });
  });

  it("parses an approval that is only about what an extension can do", () => {
    // The third thing one enable can be refused for. Keyed on actions alone
    // because an extension can declare one while asking for no capability the
    // sheet would otherwise mention.
    const parsed = parseApprovalRequest(
      reviewError(
        '{"needsPermissions":[],"needsActions":[{"id":"next","title":"Skip to the next track","confirms":false}]}',
      ),
    );
    expect(parsed?.actions).toHaveLength(1);
    expect(parsed?.actions[0]?.title).toBe("Skip to the next track");
  });

  it("opens approval for authentication alone", () => {
    expect(parseApprovalRequest('{"needsPermissions":[]}')).toBeNull();
    expect(parseApprovalRequest("not json")).toBeNull();
    expect(
      parseApprovalRequest(
        reviewError(
          '{"needsAuthentication":{"provider_name":"GitHub","scopes":["read:user"],"api_hosts":["api.github.com"],"authorization_host":"github.com","token_host":"github.com"}}',
        ),
      ),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
      permissions: [],
      actions: [],
      authentication: {
        provider_name: "GitHub",
        scopes: ["read:user"],
        api_hosts: ["api.github.com"],
        authorization_host: "github.com",
        token_host: "github.com",
      },
    });
    expect(
      parseApprovalRequest(
        reviewError(
          '{"needsPermissions":["storage"],"needsAuthentication":{"provider_name":"GitHub","scopes":[42]}}',
        ),
      ),
    ).toBeNull();
  });

  it("refuses permission sheets without a valid package review identity", () => {
    for (const approvalDigest of [
      undefined,
      null,
      12,
      "",
      "a".repeat(63),
      "g".repeat(64),
    ]) {
      expect(
        parseApprovalRequest(
          JSON.stringify({ needsPermissions: ["storage"], approvalDigest }),
        ),
      ).toBeNull();
    }
    expect(
      parseApprovalRequest(reviewError('{"needsPermissions":["storage"]}'))
        ?.approvalDigest,
    ).toBe("a".repeat(64));
  });
});
