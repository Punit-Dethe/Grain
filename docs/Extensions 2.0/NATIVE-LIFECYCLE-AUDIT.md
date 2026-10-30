# Native Agent close/reload audit

**Date:** 1 October 2026. **Branch:** `extensions/tool-only-retirement`.

This is the focused Block 2B audit, following the user's close, repeated replacement and idle checks and the first real-application harness sprint. Its scope is native registry publication, transient shortcut ownership, Agent cancellation, developer reload and replacement-worker cleanup. It does not certify all Agent features, MCP transports, authentication, installation/migration or release resource/security gates.

## Findings and repairs

### A. Windows registry publication failed during repeated replacement

The existing stamped application failed `native.ten-replacements` in clean profile `run-L8HNtp`: developer registration reported `persist .../extensions.json: Access is denied. (os error 5)`. Cleanup passed. Earlier failures remain in the local evidence directory; this result was not retried into a Pass.

A regression test opens the saved destination with real Windows read/write sharing but without delete sharing. The original atomic writer then fails with the same OS error when that handle is held briefly. This establishes a reproducible locking mechanism; it does **not** identify which process held the file during the application failure. Antivirus involvement remains an inference, not an observed cause.

The writer now retains its already serialized and synced, uniquely owned staging file and retries **only** its atomic publication on Windows errors 5, 32 or 33. Five backoffs of 10, 20, 40, 80 and 100 ms cap added sleep at 250 ms. Successful saves have no added sleep. Other platforms retain one atomic persist attempt. There is no delete/copy fallback, registry-operation retry, authorization bypass or tool-call replay.

A real 80 ms Windows lock must recover with one writer invocation and complete new bytes. A persistent lock must still fail, preserve old destination bytes and unowned recovery files, remove owned staging, and permit a fresh save after release. Existing rollback/revision tests remain applicable. The sleep ceiling is not a hard deadline for blocking filesystem calls, and persistent access/ACL problems are not repaired by retrying.

### B. Delayed shortcut cleanup could release a newer registration

The previous 150 ms cleanup checked window/input presence and then unregistered Enter/Escape without synchronizing those operations against new registration. A newer Agent could register Escape after that check or before its window was constructed, leaving the replacement panel without its shortcut. Earlier failed Escape runs are retained; this source race is not asserted to explain every historical input failure.

A small mutex and generation now serialize transient registration and eligible delayed cleanup. Each new registration invalidates older cleanup. The last ownership check and OS unregistration happen under the same guard, so a new registration either wins before cleanup or runs after cleanup finishes. Follow-up binding suppression/registration uses that guard too. Scheduling cleanup reads the generation atomically, so the shortcut manager's event thread cannot block behind a registration awaiting that same manager.

Cleanup also recognizes an instruction whose panel creation is queued. Cancellation removes a queued first instruction as well as the running/held action, preventing an abandoned instruction from retaining ownership indefinitely. Existing window/input, ordinary recording and follow-up-offer guards remain.

Two deterministic ownership tests cover stale cleanup and cleanup already in progress, including a nonblocking generation observation. The maintained real-app `agent.reopen-escape` scenario performs ten immediate close/reopen pairs, sends actual Windows Escape to the owned foreground panel, checks that discarded approvals never dispatch, and finishes with a successful fresh greeting. No artificial handoff delay or scenario retry is added. `--fault missing-escape` withholds the event to verify that a live panel fails the oracle rather than timing out as a Pass.

## Review and verification record

Reviewed the current worker/token/generation checks, pending-reply drainage, idle reaper admission recheck, production developer socket and host confirmation path. Changed-source and stale-approval behavior continue through production commands and actual workers. The harness preserves isolated profile/vault identities, real WebViews, finite observations, source/runner hashes and owned cleanup. Its application controls were not expanded.

Core verification: 260 unit tests and four integration tests passed; Clippy with warnings denied passed. The maintained production-logic batch passed 47 registry + 18 Agent + 48 native-host + 46 MCP + 23 auth tests (182, including the registry subset of the 260 core tests). The Agent group was rerun after queued-instruction cancellation changed and all 18 passed. The nine harness self-tests, frontend type check, embedded harness frontend/backend build, scoped formatting and whitespace checks passed. Ordinary backend test compilation and runtime application evidence remain separate.

Current real-app `run-f3Vfd2`: all thirteen scenarios and cleanup Pass. Worker idle retirement was observed after 144,957 ms, and the near-boundary slow call survived on the replacement worker; the full idle case took 272,217 ms. Executable SHA-256: `36a639d84334e57a2372466bfaf91ef9bf5ac60b093a666638167a2441b72cf8`. Reports identify the dirty working tree based on `e42a3a4c`, source fingerprint, runner fingerprint and actual WebView version. These results do not reuse the first sprint's different executable identity.

Negative oracle `run-w2P04k`: withholding Escape produced the expected window-destruction failure after about eight seconds, process exit 1 and cleanup Pass. Its host still held the unapproved confirmation at failure; no tool had dispatched. This is a passing harness failure-detection check, not a passing product scenario. The earlier `run-L8HNtp` product failure is retained separately.

**Acceptance state:** Block 2B is accepted on 1 October 2026. The focused native close/reload source audit and current thirteen-case runtime batch are complete. All twelve short cases also passed from clean profile `run-zqk29A`, with cleanup Pass and the same executable/source/runner identities. The user subsequently reports the remaining ordinary dictation/recording-pill observations and Agent greeting worked. Check 37 combines those human observations with recorded automated reload/disabled/in-flight coverage; it was not marked Pass from automation alone. The report is aggregate; the user's running build identity and ordinary-profile reload timing were not separately supplied. This closes 2B, not the whole native block or the release.

## Human portion of check 37 — completed

**User-reported Pass, 1 October:** the user clarified that the first two observations meant ordinary dictation, which they tested, and explicitly confirmed the third Agent observation worked. The completed procedure below remains as a reproduction reference; no repeat is requested now.

Use the updated ordinary app with the existing harmless Lifecycle Smoke extension. After its developer watcher has rebuilt/reloaded it, perform these three observations:

Start the updated ordinary app from `C:\Projects\Grain\grain` with `npm run tauri -- dev`. In a separate PowerShell, start the already prepared fixture watcher:

```powershell
$env:PATH = 'C:\Projects\Grain\grain\node_modules\.bin;' + $env:PATH
Set-Location 'C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a'
C:\t\debug\grain-ext.exe dev
```

With that watcher running, use another PowerShell for the single harmless edit:

```powershell
Add-Content -LiteralPath 'C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a\src\main.ts' -Value '// shared listener acceptance'
```

Wait for the watcher to report rebuild/reload. If this old temporary fixture no longer exists, prepare a new harmless fixture rather than editing a real extension; report the missing prerequisite. The paths were checked during this audit. Then perform the batch:

1. Dictate a short sentence into a disposable text field. The normal recording pill should appear, recording should stop normally and the expected text should arrive once.
2. Start another recording and use the pill's Cancel control. It should stop without pasted text, a stuck recording state or a disconnected pill. Start and stop one more recording to confirm recovery.
3. Summon Agent and request/approve `Say lifecycle hello`. Expect one `Lifecycle hello completed.` result. Close and reopen Agent once and confirm it remains usable.

The harness automated repeated authenticated developer reload, in-flight replacement and disabled-state preservation. The user's completed human observations cover the real microphone/native pill and ordinary profile shared listener. No hosted provider connection is needed for this block. Continue **Block 2C native deadlines/results/input**, then **2D consent/installation/persistence**, auditing each before the next. Controlled MCP connection/tool tests follow in Block 3; live-account authentication acceptance follows in Block 4.

Stop the fixture watcher with Ctrl+C in its own terminal after testing. Restore its harmless source with:

```powershell
Copy-Item -LiteralPath 'C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-main-baseline-20261001.ts' -Destination 'C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a\src\main.ts'
```

This restores the fixture source; a later watcher start rebuilds it. The previously compiled comment has no handler effect.

The Quick Agent warm-hidden follow-up and offer-expiry paths are outside this focused suite. Their OS shortcut-manager call sites require review and real shortcut coverage in the deeper Agent block; the current harness disables that mode. Physical Enter delivery, other platform adapters, actual RAM/handle trends and complete cancellation of externally performed effects also remain separate gates.

## Research informing the repairs

- [Microsoft file movement/replacement](https://learn.microsoft.com/en-us/windows/win32/fileio/moving-and-replacing-files) and [CreateFile sharing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): replacement depends on the sharing permissions of open handles.
- [Rust Windows OpenOptions](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode): removing delete sharing prevents rename until the handle closes; this is the regression fixture's real OS mechanism.
- [tempfile persist contract](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist) and [Windows implementation](https://github.com/Stebalien/tempfile/blob/master/src/file/imp/windows.rs): atomic replacement returns the same owned temporary file on failure, enabling safe publication retries without rewriting bytes.
- [VS Code filesystem implementation](https://github.com/microsoft/vscode/blob/main/src/vs/base/node/pfs.ts): bounded Windows rename retries for access/permission/busy failures provide an established application precedent. Grain chooses a much shorter policy appropriate to its synchronous registry path and preserves its atomic replacement invariant.
- [Tokio watch implementation](https://github.com/tokio-rs/tokio/blob/master/tokio/src/sync/watch.rs): synchronized version checks illustrate why ownership checks must remain coupled to mutation. Grain adds only a small inline guard to its existing shortcut path, without adopting another background service.

These references inform the mechanism and design; they are not evidence that another project's behavior certifies Grain. Application evidence and deterministic tests are recorded separately.
