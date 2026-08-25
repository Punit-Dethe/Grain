import { describe, expect, it } from "vitest";
import {
  EXTENSION_FRAME_CSP,
  EXTENSION_FRAME_POLICY_TAG,
} from "./extensionFramePolicy";

describe("extension frame policy", () => {
  it("denies ambient I/O while retaining the inline sandbox runtime", () => {
    expect(EXTENSION_FRAME_CSP).toContain("default-src 'none'");
    expect(EXTENSION_FRAME_CSP).toContain("connect-src 'none'");
    expect(EXTENSION_FRAME_CSP).toContain("frame-src 'none'");
    expect(EXTENSION_FRAME_CSP).toContain("form-action 'none'");
    expect(EXTENSION_FRAME_CSP).toContain("script-src 'unsafe-inline'");
    expect(EXTENSION_FRAME_CSP).not.toMatch(/https?:|wss?:/);
    expect(EXTENSION_FRAME_POLICY_TAG).toMatch(
      /^<meta http-equiv="Content-Security-Policy"/,
    );
  });
});
