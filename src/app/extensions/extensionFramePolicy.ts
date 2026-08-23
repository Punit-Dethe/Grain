/**
 * Custom extension markup runs in an opaque, script-capable sandbox. Its CSP
 * must still deny ambient network access: governed I/O goes through Grain's
 * host API, where the extension identity and declared grants are enforced.
 */
export const EXTENSION_FRAME_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  "img-src data: blob:",
  "media-src data: blob:",
  "font-src data:",
  "connect-src 'none'",
  "frame-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

export const EXTENSION_FRAME_POLICY_TAG = `<meta http-equiv="Content-Security-Policy" content="${EXTENSION_FRAME_CSP}">`;
