import { mkdir, realpath, writeFile } from "node:fs/promises";
import { resolve, relative, isAbsolute } from "node:path";
import { performance } from "node:perf_hooks";

export class Blocked extends Error {}

export async function waitFor(
  description,
  operation,
  { timeoutMs = 20000, intervalMs = 100, signal } = {},
) {
  const started = performance.now();
  let lastError;
  while (performance.now() - started < timeoutMs) {
    signal?.throwIfAborted();
    try {
      const result = await operation();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(
    `${description} timed out after ${Math.round(performance.now() - started)}ms${lastError ? `: ${lastError.message}` : ""}`,
  );
}

export function assertWithin(root, candidate) {
  const rel = relative(resolve(root), resolve(candidate));
  if (!rel || rel.startsWith("..") || isAbsolute(rel))
    throw new Error("Path must be a child of the harness run root");
  return resolve(candidate);
}

export async function writeReport(root, report) {
  const evidence = assertWithin(root, resolve(root, "evidence"));
  await mkdir(evidence, { recursive: true });
  assertWithin(await realpath(root), await realpath(evidence));
  await writeFile(
    resolve(evidence, "report.json"),
    JSON.stringify(report, null, 2),
  );
  const summary = [
    `# Grain Agent acceptance run`,
    "",
    `Run: ${report.runId}`,
    `Evidence: ${report.evidenceClass}`,
    `Commit: ${report.commit}`,
    `Cleanup: ${report.cleanup.status}`,
    "",
    "| Scenario | Result | Detail |",
    "|---|---|---|",
  ];
  for (const result of report.results)
    summary.push(
      `| ${result.id} | ${result.status} | ${(result.error ?? result.description ?? "").replaceAll("|", "\\|").replaceAll("\n", " ")} |`,
    );
  summary.push(
    "",
    "Numbered manual check statuses are unchanged. This run certifies only the paths described in each scenario.",
  );
  await writeFile(resolve(evidence, "report.md"), summary.join("\n") + "\n");
}
