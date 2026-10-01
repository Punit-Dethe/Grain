import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { access, readdir, readFile, writeFile } from "node:fs/promises";
import { resolve, join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
export const stampPath = join(here, ".build/host.json");
export const cliStampPath = join(here, ".build/cli.json");

export async function hashFile(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

export async function sourceFingerprint() {
  const paths = [];
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isSymbolicLink())
        throw new Error(
          `Source fingerprint refuses symlink: ${relative(repo, path)}`,
        );
      if (entry.isDirectory()) await visit(path);
      else paths.push(path);
    }
  }
  await visit(join(repo, "src/app"));
  await visit(join(repo, "src-tauri/src"));
  await visit(join(repo, "src-tauri/capabilities"));
  await visit(join(repo, "src-tauri/resources"));
  await visit(join(repo, "public"));
  for (const entry of await readdir(join(repo, "crates"), {
    withFileTypes: true,
  })) {
    if (!entry.isDirectory()) continue;
    await visit(join(repo, "crates", entry.name, "src"));
    paths.push(join(repo, "crates", entry.name, "Cargo.toml"));
  }
  paths.push(
    ...[
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
      "src-tauri/build.rs",
      "src-tauri/tauri.conf.json",
      "src-tauri/tauri.windows.conf.json",
      "Cargo.toml",
      "Cargo.lock",
      "package.json",
      "package-lock.json",
      "bun.lock",
      "tsconfig.json",
      "tsconfig.node.json",
      "vite.config.ts",
      "index.html",
      "extension-host.html",
      "extension-view.html",
    ].map((path) => join(repo, path)),
  );
  for (const input of [
    ".cargo/config.toml",
    ".cargo/config",
    ...paths
      .filter(
        (path) =>
          path.endsWith("Cargo.toml") &&
          path.includes(`${join(repo, "crates")}`),
      )
      .map((path) => relative(repo, join(dirname(path), "build.rs"))),
  ]) {
    const path = join(repo, input);
    try {
      await access(path);
      paths.push(path);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  const hash = createHash("sha256");
  for (const path of paths.sort()) {
    hash.update(relative(repo, path).replaceAll("\\", "/"));
    hash.update("\0");
    hash.update(await hashFile(path));
    hash.update("\0");
  }
  return hash.digest("hex");
}

// Test definitions can evolve without rebuilding the application. Record their
// independent identity so a report also identifies the oracle that ran it.
export async function runnerFingerprint() {
  const hash = createHash("sha256");
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      if (entry.name.startsWith(".")) continue;
      const path = join(directory, entry.name);
      if (entry.isSymbolicLink())
        throw new Error("Harness source cannot be a symlink");
      if (entry.isDirectory()) await visit(path);
      else if (/\.(mjs|js|ps1|json)$/.test(entry.name)) {
        hash.update(relative(here, path).replaceAll("\\", "/"));
        hash.update("\0");
        hash.update(await hashFile(path));
      }
    }
  }
  await visit(here);
  return hash.digest("hex");
}

export async function verifyBuild(binary) {
  const stamp = JSON.parse(await readFile(stampPath, "utf8"));
  if (stamp.schema !== 1 || stamp.applicationId !== "com.grain.agent-harness")
    throw new Error("Invalid harness build stamp");
  if ((await hashFile(binary)) !== stamp.binarySha256)
    throw new Error("Executable does not match its harness build stamp");
  if ((await sourceFingerprint()) !== stamp.sourceFingerprint)
    throw new Error(
      "Application sources changed since the harness build; rebuild before acceptance testing",
    );
  return stamp;
}

export async function verifyCli() {
  const stamp = JSON.parse(await readFile(cliStampPath, "utf8"));
  if (stamp.schema !== 1 || stamp.kind !== "grain-ext-cli")
    throw new Error("Invalid CLI build stamp");
  if ((await hashFile(stamp.binary)) !== stamp.binarySha256)
    throw new Error("CLI executable does not match its build stamp");
  if ((await sourceFingerprint()) !== stamp.sourceFingerprint)
    throw new Error(
      "Sources changed since the CLI build; rebuild before testing",
    );
  return stamp;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const cli = process.argv[2] === "--cli";
  const binaryArg = process.argv[cli ? 3 : 2];
  const expectedFingerprint = process.argv[cli ? 4 : 3];
  if (!binaryArg)
    throw new Error("stamp.mjs requires the built executable path");
  const binary = resolve(binaryArg);
  const fingerprint = await sourceFingerprint();
  if (!expectedFingerprint || fingerprint !== expectedFingerprint)
    throw new Error("Application inputs changed during the build; build again");
  const commit = (
    await promisify(execFile)("git", ["rev-parse", "HEAD"], { cwd: repo })
  ).stdout.trim();
  await writeFile(
    cli ? cliStampPath : stampPath,
    JSON.stringify(
      {
        schema: 1,
        ...(cli
          ? { kind: "grain-ext-cli" }
          : { applicationId: "com.grain.agent-harness" }),
        binary,
        binarySha256: await hashFile(binary),
        sourceFingerprint: fingerprint,
        commit,
        builtAt: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  console.log(`Build identity: ${cli ? cliStampPath : stampPath}`);
}
