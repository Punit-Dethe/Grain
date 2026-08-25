import type { ExtensionChoiceCandidate } from "@/bindings";

function searchable(candidate: ExtensionChoiceCandidate): string {
  return `${candidate.name}\n${candidate.purpose}\n${candidate.extensionId}`.toLocaleLowerCase();
}

export function filterChoiceCandidates(
  candidates: ExtensionChoiceCandidate[],
  query: string,
): ExtensionChoiceCandidate[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/u).filter(Boolean);
  if (terms.length === 0) return candidates;
  return candidates.filter((candidate) => {
    const haystack = searchable(candidate);
    return terms.every((term) => haystack.includes(term));
  });
}

export function resolveChoiceSelection(
  candidates: ExtensionChoiceCandidate[],
  query: string,
  currentId: string | null,
): string | null {
  if (
    currentId &&
    candidates.some((candidate) => candidate.extensionId === currentId)
  ) {
    return currentId;
  }
  if (query.trim()) return candidates[0]?.extensionId ?? null;
  return (
    candidates.find((candidate) => candidate.signal !== "none")?.extensionId ??
    null
  );
}
