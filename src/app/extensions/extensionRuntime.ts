import type { Result } from "@/bindings";

export type ToolSection = "dictionary" | "snippets" | "context" | "agent";

export function signInCancelled(reason: unknown): boolean {
  return (
    (reason instanceof Error ? reason.message : reason) === "Sign-in cancelled."
  );
}

export function unwrapResult<T, E>(result: Result<T, E>): T {
  if (result.status === "error") throw new Error(String(result.error));
  return result.data;
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
