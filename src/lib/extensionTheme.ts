export const PUBLIC_EXTENSION_TOKENS = [
  "paper",
  "paper-raised",
  "paper-sunken",
  "ink",
  "ink-soft",
  "ink-faint",
  "accent",
  "line",
] as const;

/** Empty custom properties suppress an extension author's fallback, so emit
 * only resolved values while preserving all eight public names when present. */
export function serializeExtensionPalette(
  readValue: (internalName: string) => string,
): string {
  return PUBLIC_EXTENSION_TOKENS.map((name) => [
    name,
    readValue(`--color-${name}`).trim(),
  ])
    .filter(([, value]) => value !== "")
    .map(([name, value]) => `--grain-${name}:${value}`)
    .join(";");
}
