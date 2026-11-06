# GitHub sign-in and catalogue reliability fix

6 November 2026. Base: main `830a65d45`, application 0.0.11. Work is isolated on
`codex/fix-github-extension-auth`; the original checkout and generated-binding edit
are preserved. This fixes three distinct causes rather than treating browser
consent as proof that an extension works.

## Confirmed causes

1. **Other-account installation:** the owner's verified GitHub App Advanced page
   shows **Make public**. The app remains private and cannot be installed by other
   accounts. The owner must change this setting; each tester then separately
   installs the app on their selected repositories and authorizes access.
2. **Normal beta authentication:** the shared build workflow passed both registered
   GitHub desktop-client inputs only for the dedicated alpha. Main already enables
   signed-store MCPs in ordinary releases (`42d537dfd`), but those releases omitted
   the registration. Normal distribution callers now explicitly opt in, validate
   both inputs before building and compile the same dedicated desktop registration.
   PR builds remain opted out. No keys/tokens were displayed or new registration
   created. Alpha packaging/profile isolation and custom MCP gates remain.
3. **Signed-in account with no tools:** the live GitHub all-toolset response exceeds
   the old SSE-only 524,288-byte bound. The instrumented actual host observed
   `received=523561 chunk=1370 limit=524288`, followed by transport closure. The
   accepted 18 October alpha also reproduces catalogue failure with its account
   saved/enabled and Developer Mode off. This failure precedes model selection.

## Runtime change

Use the existing **2 MiB catalogue budget** for SSE as well as JSON. Keep the
128-tool, 32-page and 8 MiB operation-wire limits, destination/auth checks,
deadlines, cancellation and owned-service cleanup. No retained SDK client,
background health task, provider fork or arbitrary limit removal is introduced.
Failure logs contain static SDK categories, byte counts, HTTP status and JSON
error coordinates; they never include raw provider errors, bodies or credentials.

After the change, the real alpha-profile probe discovers **118 tools**, with
**531,322 bytes** of serialized supported definitions, including
`get_file_contents` and `issue_write`. A real Agent request then reads only the
disposable repository README. The returned text exactly matches a separate
GitHub API read (SHA-256
`011a8f20dd1311a121bc7e2c4b2ccd41477bded66e89b2eb4c918854317d4748`), and
the production trace records `call_tool` success. Initial three-second preparation
can still time out; ordinary recovery successfully loads the catalogue. No write
was requested or replayed.

## Checks and evidence

- 87 affected backend MCP tests pass, including new JSON/SSE parity above 512 KiB
  and existing oversized, cancellation, session-cleanup and no-replay tests.
- Five runner/build-policy tests pass, including distribution-client inclusion,
  PR exclusion and secret-safe failure classification.
- Production frontend build, Rust build/format, JavaScript/YAML formatting and
  diff whitespace checks pass. Clippy completes with existing warnings; it is not
  a warning-free repository baseline.
- Maintained installed-alpha runner: `.runs/github-alpha-xz1DY0` records saved,
  enabled, Developer Mode off and failed `mcp_test_provider`; owned cleanup Pass.
  The deliberately older 14 October reference cannot pass current preflight;
  `.runs/github-alpha-s9mPNv` retains that failure without acceptance credit.
- Debug headless evidence is in
  `tests/agent-harness/.runs/github-fix-probe-20261106/`: final Agent report/trace,
  independent read verification and bounded original failure observations. These
  account-bound files remain ignored. Ordinary model/account settings and the
  running normal app were not changed; host-owned SDK token refresh is retained.

The existing Build Test workflow adds a **windows-only** normal-beta selector so
the corrected 0.0.11 installer can be built without a seven-platform batch, a
GitHub Release or an updater publication. Build artifacts and final non-owner
consent/read results remain separate acceptance from the backend proof above.
An old installed executable cannot acquire these fixes without an updated build.

## Remaining acceptance

Verify public installation availability after the owner applies the setting,
then use a fresh non-owner account with the corrected normal beta to install,
sign in, read an explicitly granted repository and read again after restart.
Do not request developer credentials from ordinary users or infer non-owner
acceptance from the owner's successful read. The historical per-call-confirmation
and `load_extension` runner workflow stays excluded; retire its stale assertions
separately rather than restoring obsolete product behavior.
