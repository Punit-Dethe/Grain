// Stable IDs are report contracts. Numbered checks below are supporting coverage,
// not permission to change the manual acceptance ledger automatically.
export const AUTOMATIC_MCP_WORKFLOWS = [
  "agent.workflow-mcp",
  "agent.workflow-stop-mcp",
  "agent.workflow-failure-mcp",
  "agent.workflow-unknown-mcp",
];

export const scenarios = [
  {
    id: "agent.model-reply",
    suite: "smoke",
    checks: [],
    description:
      "Real Agent shows a scripted model reply without connected tools",
  },
  {
    id: "agent.close-model",
    suite: "smoke",
    checks: [],
    description:
      "Closing the real Agent cancels its held model request and a fresh request succeeds",
  },
  ...[
    [
      "agent.workflow-stop-mcp",
      "Close a held model before and after an actual MCP write; exact wire counts prevent continuation or replay",
    ],
    [
      "agent.workflow-failure-mcp",
      "Actual MCP write receipt survives model outage; verification never dispatches",
    ],
    [
      "agent.workflow-unknown-mcp",
      "Peer writes once and drops its reply; uncertainty blocks retry and an independent read verifies effects",
    ],
  ].map(([id, description]) => ({
    id,
    description,
    suite: "agent-interruption",
    checks: [48],
  })),
  {
    id: "agent.workflow-mcp",
    suite: "agent-workflow",
    checks: [47],
    description:
      "Automatic MCP read, write and independent verification with exact wire receipts",
  },
];

scenarios.push({
  id: "mcp.legacy-http-probe",
  suite: "mcp",
  checks: [8, 17],
  description:
    "Generic HTTP 400 discovery rejection, one fresh legacy handshake per operation, actual approved JSON/SSE nested result and session cleanup",
});

scenarios.push(
  ...["disable", "disconnect"].map((operation) => ({
    id: `mcp.linear-cancel-${operation}`,
    suite: "mcp-linear-preflight",
    checks: [9],
    description: `Opt-in actual Linear SDK read-only consent, ${operation} before sign-in, callback/grant cleanup and fresh retry after restart; browser consent excluded`,
  })),
);

// Explicit Linear browser/grant control cases are also excluded from all.
scenarios.push({
  id: "mcp.hf-nested-read",
  suite: "mcp-hf-live",
  checks: [17],
  description:
    "Opt-in anonymous Hugging Face hf_fs nested public document read, actual selective schema/approval/result and repeat after real restart",
});
scenarios.push(
  {
    id: "mcp.linear-contracts",
    suite: "mcp-linear-contracts",
    checks: [12, 17],
    description:
      "One explicit human sign-in inspects actual supported structural input contracts and repeats discovery after restart; no tools execute and no numbered test completes from metadata alone",
  },
  {
    id: "mcp.linear-consent-guards",
    suite: "mcp-linear-guards",
    checks: [9],
    description:
      "Actual SDK readiness and guarded browser/grant controls refuse missing consent, retire without a browser, and remain disconnected after restart",
  },
  {
    id: "mcp.linear-browser-consent",
    suite: "mcp-linear-sign-in",
    checks: [9, 12, 17],
    description:
      "Explicit human browser cancellation/consent, actual read-only scoped grant/lifetime metadata, discovery and restart; no tool execution or refresh certification",
  },
);

// Official runner owns the server prerequisite. Normal --suite all excludes
// these entries; conformance.mjs invokes each finite case with its owned binding.
scenarios.push(
  {
    id: "mcp.conformance-initialize",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official legacy initialization through Grain production discovery, no actions",
  },
  {
    id: "mcp.conformance-tools-legacy",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official legacy numeric tool through real Agent discovery, selected schema and approval",
  },
  {
    id: "mcp.conformance-tools-modern",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official modern numeric tool through real Agent discovery, selected schema and approval",
  },
);

export function selectScenarios(suite) {
  if (suite === "mcp-agent-workflows")
    return scenarios.filter((scenario) =>
      AUTOMATIC_MCP_WORKFLOWS.includes(scenario.id),
    );
  if (suite === "mcp-connection-checkpoint")
    return [
      "mcp-configured",
      "mcp-configured-runtime",
      "mcp-configured-auth",
      "mcp-foundation",
      "smoke",
    ].flatMap(selectScenarios);
  if (suite === "mcp-configured")
    return scenarios.filter((scenario) => scenario.suite === "mcp-configured");
  if (suite === "mcp-configured-auth")
    return scenarios.filter((x) => x.suite === "mcp-configured-auth");
  if (suite === "mcp-configured-runtime")
    return scenarios.filter(
      (scenario) => scenario.suite === "mcp-configured-runtime",
    );
  if (suite === "mcp-refresh")
    return scenarios.filter((scenario) =>
      ["mcp.auth-refresh-recovery", "mcp.auth-refresh-refusal"].includes(
        scenario.id,
      ),
    );
  if (suite === "mcp-independence")
    return scenarios.filter((scenario) =>
      [
        "mcp.auth-provider-independence",
        "mcp.auth-fixed-port-conflict",
        "mcp.auth-issuer-binding",
      ].includes(scenario.id),
    );
  if (suite === "mcp-foundation")
    return scenarios.filter((scenario) =>
      ["mcp", "mcp-auth"].includes(scenario.suite),
    );
  if (
    ![
      "smoke",

      "agent-workflow",

      "agent-interruption",

      "mcp",
      "mcp-live",
      "mcp-hf-live",
      "mcp-linear-preflight",
      "mcp-linear-guards",
      "mcp-linear-sign-in",
      "mcp-linear-contracts",
      "mcp-auth",

      "all",
    ].includes(suite)
  )
    throw new Error(`Unknown suite: ${suite}`);
  return scenarios.filter(
    (scenario) =>
      (suite === "all" &&
        ![
          "mcp-conformance",
          "mcp-live",
          "mcp-hf-live",
          "mcp-linear-preflight",
          "mcp-linear-guards",
          "mcp-linear-sign-in",
          "mcp-linear-contracts",
        ].includes(scenario.suite)) ||
      scenario.suite === suite,
  );
}

scenarios.push(
  {
    id: "mcp.auth-fixture",
    suite: "mcp-auth",
    checks: [12, 13, 17, 31],
    description:
      "Actual SDK discovery/DCR/PKCE/vault, approved A/B nested reads, old account refusal and restart; controlled provider prerequisite",
  },
  {
    id: "mcp.auth-denied-cancelled",
    suite: "mcp-auth",
    checks: [9],
    description:
      "Denied/cancelled SDK login, actual reusable callback listener, zero token exchange and fresh authenticated reads",
  },
  {
    id: "mcp.auth-late-callback",
    suite: "mcp-auth",
    checks: [10],
    description:
      "Cancel-sign-in, disable and Developer Mode off refuse issued old callbacks before/after restart; fresh B grant/read recovers",
  },
  {
    id: "mcp.auth-close-cancellation",
    suite: "mcp-auth",
    checks: [31],
    description:
      "Authenticated JSON/SSE modern/legacy held/pending close, bounded cleanup, late reply refusal, preserved real grant and fresh/restart A reads",
  },
  {
    id: "mcp.live-read-disable",
    suite: "mcp-live",
    checks: [8],
    description:
      "Live public DeepWiki discovery, approved fixed-repository read, disabled stale/fresh refusal and restart recovery",
  },
  {
    id: "mcp.auth-shutdown",
    suite: "mcp-auth",
    checks: [13, 15],
    description:
      "Authenticated provider-disable/Developer Mode shutdown across JSON/SSE modern/legacy, bounded unknown result, stale approval refusal and fresh/restart recovery without reauthorization",
  },
  {
    id: "mcp.auth-client-configuration",
    suite: "mcp-auth",
    checks: [11, 13],
    description:
      "Actual preregistered public/confidential SDK login, scoped secret persistence/removal, ID/secret rotation invalidates grants/approvals, unsupported empty secret refuses and fresh/restart reads recover",
  },
  {
    id: "mcp.live-response-recovery",
    suite: "mcp-live",
    checks: [20],
    description:
      "Live public normal/large documentation reads through production bounds, actual Agent approval and fresh recovery",
  },
  {
    id: "mcp.auth-provider-independence",
    suite: "mcp-auth",
    checks: [14],
    description:
      "Two independent owned DCR issuers/overlapping login listeners, Cancel versus completed account, approved A/B reads and isolated disable/disconnect/restarts",
  },
  {
    id: "mcp.auth-fixed-port-conflict",
    suite: "mcp-auth",
    checks: [14],
    description:
      "Two preregistered issuers: exact simultaneous fixed-port refusal, zero second exchange/grant, retained first flow and serial A/B recovery/restart",
  },
);

scenarios.push(
  {
    id: "mcp.auth-refresh-recovery",
    suite: "mcp-auth",
    checks: [12],
    description:
      "Actual SDK expiry after restart, two account-preserving resource-bound rotating refresh exchanges, persisted refreshed grant and approved nested reads; controlled issuer only",
  },
  {
    id: "mcp.auth-refresh-refusal",
    suite: "mcp-auth",
    checks: [12],
    description:
      "Real expiry without refresh and invalid_grant refusal dispatch no tool or new login; explicit reconnect and restart recover the actual account",
  },
);

scenarios.push(
  {
    id: "mcp.transport-contract",
    suite: "mcp",
    checks: [8, 17],
    description:
      "Real MCP JSON/SSE stateless discovery and legacy handshake, nested wire/results, approval, restart and stale disable refusal (controlled peer only)",
  },
  {
    id: "mcp.mixed-catalog",
    suite: "mcp",
    checks: [23],
    description:
      "Real MCP mixed pages, unsupported selection/empty catalog, repeated cursor refusal, changed approval and fresh recovery",
  },
  {
    id: "mcp.response-preview",
    suite: "mcp",
    checks: [20],
    description:
      "Real JSON/SSE large UTF-8 and structured previews retain explicit byte bounds, omission notices and fresh-call recovery",
  },
  {
    id: "mcp.transport-bounds",
    suite: "mcp",
    checks: [20],
    description:
      "Separate modern/legacy declared/chunked JSON, SSE data/comments, error-body overflow and dropped replies classify unknown, never replay and recover",
  },
  {
    id: "mcp.close-cancellation",
    suite: "mcp",
    checks: [31],
    description:
      "Modern/legacy JSON/SSE approved held calls and pending approvals close through real Agent UI; late replies cannot contaminate fresh sessions",
  },
  {
    id: "mcp.catalog-budgets",
    suite: "mcp",
    checks: [20, 23],
    description:
      "Management Test and actual Agent reject tool/duplicate/cursor/page/metadata/cumulative-wire limits independently, expose no actions and recover",
  },
  {
    id: "mcp.http-deadline",
    suite: "mcp",
    checks: [20, 31],
    description:
      "Real 45-second JSON/SSE HTTP deadlines in modern/legacy approved reads remain unknown without replay, release replies and recover",
  },
  {
    id: "mcp.discovery-deadline",
    suite: "mcp",
    checks: [20, 31],
    description:
      "Actual 90-second discovery deadline bounds successive sub-45s pages across modern/legacy, dispatches nothing, retires timers/sessions and recovers",
  },
);

scenarios.push({
  id: "mcp.auth-issuer-binding",
  suite: "mcp-auth",
  checks: [54],
  description:
    "Preregistered issuer rotation refuses old secrets before consent/exchange, persists through disconnect/restart, and explicit re-registration and failed-discovery recovery are verified",
});

scenarios.push({
  id: "mcp.auth-client-metadata",
  suite: "mcp-auth",
  checks: [56],
  description:
    "Owned HTTPS client document drives SDK CIMD priority/no-DCR, DCR fallback, confidential priority, malformed-document refusal, fixed-port/cancel recovery and restart reads; permanent public hosting remains gated",
});

scenarios.push({
  id: "mcp.auth-temporary-recovery",
  suite: "mcp-auth",
  checks: [55],
  description:
    "Real expired SDK refresh under HTTP 503, malformed and dropped replies in modern/legacy transport preserves grants; restart/manual recovery and metadata failure require no new login",
});

scenarios.push({
  id: "mcp.destination-boundary",
  suite: "mcp-auth",
  checks: [61],
  description:
    "Real host refuses out-of-scope OAuth destinations and oversized metadata before credential publication/consent/exchange; fresh and restarted authenticated reads recover",
});

for (const [id, description] of [
  [
    "ownership",
    "Real inactive custom imports, host identity isolation, stale/concurrent edits, account rotation, removal and restart persistence",
  ],
  [
    "access",
    "Strict credential-free JSON and actual Developer Mode/main-window restrictions preserve saved definitions",
  ],
  [
    "storage",
    "Externally changed and corrupt scoped registry bytes cannot be overwritten; repair/restart, 32-record limit and explicit cleanup",
  ],
])
  scenarios.push({
    id: `mcp.configured-${id}`,
    suite: "mcp-configured",
    checks: [62],
    description,
  });

for (const [id, description] of [
  [
    "execution",
    "Direct configured anonymous server enablement, actual modern/legacy JSON/SSE Agent reads, restart, destination/account isolation and OAuth activation refusal",
  ],
  [
    "approval-ownership",
    "A selected configured tool refuses edits, disable and removal before dispatch; unchanged saves, peer removal and developer-mode changes preserve its owner",
  ],
  [
    "active-cancellation",
    "Four real held configured calls cancel truthfully, dispose sessions, discard late replies and recover; unchanged active metadata saves do not cancel",
  ],
])
  scenarios.push({
    id: `mcp.configured-${id}`,
    suite: "mcp-configured-runtime",
    checks: [63],
    description,
  });

for (const [id, description] of [
  [
    "ownership",
    "Actual configured SDK DCR, two same-endpoint accounts, approved account reads/restart, stale commands/approvals and exact endpoint/auth/removal grant retirement",
  ],
  [
    "cancel-reconnect",
    "Cancelling a replacement sign-in preserves the existing grant, enablement and tool discovery",
  ],
  [
    "cancellation",
    "Five actual pending configured OAuth flows cannot survive disable/label/removal/destination/disconnect; old callbacks refuse without exchange and fresh sign-in recovers",
  ],
  [
    "refresh-cancellation",
    "Actual configured issuer expiry/SDK refresh persists through restart; modern JSON and legacy SSE held calls disconnect/remove truthfully without replay and recover",
  ],
])
  scenarios.push({
    id: `mcp.configured-auth-${id}`,
    suite: "mcp-configured-auth",
    checks: [64],
    description,
  });

for (const [id, description] of [
  [
    "rotation",
    "Configured preregistered public/confidential clients, same-ID secret rotation, stale approvals, restart, logout retention and explicit reset to SDK DCR",
  ],
  [
    "ownership",
    "Same-endpoint configured client isolation, discovery failure preservation, pending-consent save/reset cancellation and revision/metadata retirement",
  ],
])
  scenarios.push({
    id: `mcp.configured-auth-client-${id}`,
    suite: "mcp-configured-auth",
    checks: [65],
    description,
  });
