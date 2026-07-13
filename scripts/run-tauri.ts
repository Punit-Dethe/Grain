#!/usr/bin/env bun
/**
 * [GRAIN] Tauri launcher — Windows MAX_PATH / junction workaround.
 *
 * `transcribe-cpp-sys` (bumped to 0.1.3, upstream PR #1664) builds its native
 * CMake/MSBuild tree through an NTFS junction at `%LOCALAPPDATA%\tcs\<hash>` ->
 * OUT_DIR, as a MAX_PATH mitigation for deep checkouts (its `build.rs`
 * `windows_short_out_dir`). When a short `CARGO_TARGET_DIR` is ALREADY in effect
 * (this repo's machine-local `.cargo/config.toml` -> `C:/gt`), that junction is
 * redundant — and on some Windows machines MSBuild's FileTracker cannot write
 * its `.tlog` files THROUGH the junction (`error MSB3491`), which kills the
 * whole build. `GGML_CPU_ALL_VARIANTS` makes it worse: one MSBuild project per
 * CPU tier, each writing `.tlog`s through the junction.
 *
 * `build.rs` only takes the junction when `LOCALAPPDATA` or `TEMP` is set. So
 * when a short target dir is configured, we unset both for the child build
 * (keeping a valid `TMP` for other tools) — `build.rs` then builds directly in
 * the short OUT_DIR, no junction, `.tlog`s write fine. Tauri's runtime path
 * APIs use the Windows known-folder API, not `LOCALAPPDATA`, so the app's data
 * dir is unaffected while it runs under `tauri dev`.
 *
 * Guarded: only fires on Windows AND only when a short target dir is set, so
 * machines that genuinely rely on the junction (deep checkout, no short target
 * dir) are untouched. A plain passthrough everywhere else.
 */
import { readFileSync } from "node:fs";
import { run } from "@tauri-apps/cli";

/** The effective cargo target dir: `CARGO_TARGET_DIR`, else the `target-dir`
 * from the nearest `.cargo/config.toml` (src-tauri wins — that's the dir cargo
 * runs from under `tauri`). `null` when none is configured. */
function configuredTargetDir(): string | null {
  if (process.env.CARGO_TARGET_DIR) return process.env.CARGO_TARGET_DIR;
  for (const cfg of ["src-tauri/.cargo/config.toml", ".cargo/config.toml"]) {
    try {
      const m = readFileSync(cfg, "utf8").match(
        /^\s*target-dir\s*=\s*"([^"]+)"/m,
      );
      if (m) return m[1];
    } catch {
      /* no config at this path */
    }
  }
  return null;
}

if (process.platform === "win32") {
  const target = configuredTargetDir()?.replace(/[/\\]+$/, "");
  // "Short" ⇒ the build root won't blow MAX_PATH on its own, so the junction is
  // pure downside (e.g. `C:/gt`, `C:\t`). Only then do we disable it.
  if (target && target.length <= 12) {
    delete process.env.LOCALAPPDATA;
    delete process.env.TEMP;
    // build.rs ignores TMP; keep one valid so other build tools still get scratch.
    if (!process.env.TMP) process.env.TMP = "C:\\Windows\\Temp";
    console.log(
      `[run-tauri] short target dir (${target}) detected — disabling the ` +
        `transcribe-cpp-sys build junction (unset LOCALAPPDATA/TEMP for the build).`,
    );
  }
}

run(process.argv.slice(2), "tauri").catch((e) => {
  console.error(e);
  process.exit(1);
});
