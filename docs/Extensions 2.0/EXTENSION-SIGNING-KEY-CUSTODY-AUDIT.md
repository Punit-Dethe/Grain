# E4 explicit signing credentials and key custody tooling

Date: 8 October 2026. This block improves maintainer tools. It does not authorize
production key access, public catalogue activation or a release. The application,
SDK contract, author build contract, UI and Agent harness are unchanged.

## Change and scope

The previous signer always supplied an empty password. A shared private loader
now requires an explicit choice: a no-echo attached-terminal prompt, an
operator-owned password file, or a named disposable development-key mode.
There is no password-valued CLI option/environment setting or automatic prompt
on redirected stdin. The CLI rejects credential options for non-signing work.

Key generation uses the existing locked Minisign 0.7.9 implementation with a
nonempty password, or explicit development mode. No crypto was reimplemented
or upgraded. rpassword 7.5.4 was already in the lockfile and is now a direct
dependency for the explicit prompt. Verification/trust roles remain unchanged.

Bootstrap and renewal load once per operation after public trust, metadata,
snapshot/lifetime/size and output checks. Both new signatures are checked before
output creation. Reviewed native/MCP signing retains policy/source/receipt/
provenance/approval gates before credential access, then verifies the result.
The public reviewed route additionally rejects credentials inside its author
and serving inputs. Generic signing utilities share the loader but retain their
existing output behavior; this audit does not grant them publication authority
or claim that every raw utility has transactional catalogue output.

Credential reads reuse the existing bounded, nonlinked regular-file boundary:
8192 bytes for a key and 1024 bytes for a UTF-8 password file. One final LF/CRLF
is stripped; spaces remain meaningful. Empty/multiline/NUL passwords refuse.
Protected Unix files require owner-only permissions; generated keys are mode
600. Windows directory ACLs and filesystem custody must be secured by the
operator; ACL enforcement is not implemented here. Parent directory races and
a hostile operator filesystem are not certified by these path checks.

Generation refuses unsafe names and either existing final path before unlocking.
Complete public/secret files are staged and synced before no-clobber persistence.
Temporary files have scoped cleanup. The pair is not atomic: after the secret
commits, failure to commit its public file explicitly tells the operator to
retain the complete secret and inspect outputs. No overwrite or auto-regeneration.
This is not a guarantee of durability across every filesystem/power-loss case.

Passwords/unlocked key objects have operation-local scope, with no resident
signing service or cache. Minisign 0.7.9 does not certify secret zeroization;
scope termination must not be described as secure-memory erasure. No secret
enters the author builder, attester, conditional publisher or app.

## Verification and audit

Evidence lives under `tests/agent-harness/.runs/e4-key-custody-01/`; no new
Agent scenario or harness source is added. Tests use disposable keys only.

- Genuine protected-key signature verification and changed-byte refusal;
  wrong/missing/empty passwords, bounded reads and whitespace handling.
- Connected encrypted-key bootstrap/renewal, unchanged input, wrong-password
  refusal without output; existing native/MCP reviewed-signing regressions now
  use encrypted operator fixtures.
- No-clobber/unsafe-name refusal and credential input-tree separation. Unix
  permissions/symlink checks are additionally exercised by the Linux lane.
- `ci/check_key_cli.py` is a 12-case public-binary checkpoint, reusing the
  existing evidence format and disposable tempdir cleanup. It checks explicit
  modes, noninteractive prompting, flag conflicts, protected generation/sign,
  credential failures without signature output and unchanged key bytes.
  Terminal interaction/confirmation itself is not certified by that piped lane.
- The existing 13 public CLI and 17 publisher/HTTP cases remain connected;
  signature algorithms are tested in Rust, not reimplemented by Python.

Strict all-target maintainer Clippy, locked build and formatting pass. Clippy
caught explicit `drop` calls on the library's non-Drop secret object; lexical
scopes replace them, with no erasure claim. An initial CLI probe used the wrong
input flag; it failed, was corrected to actual `--input`, and rerun in fresh
evidence. Earlier development attempts remain distinct from final acceptance.

The first encrypted development suite passed 80 cases in 244.68 seconds. Cargo
now optimizes only the existing maintainer scrypt dependency in dev/test builds,
preserving all KDF work/memory factors. The root workspace excludes the app;
scrypt is absent from the author CLI dependency tree. No test is weakened or
skipped. Counts changed during development, so these timings are not an exact
same-matrix benchmark. This is a build-speed setting, not another test engine.

Final Windows: **82 maintainer / 17 publisher-transport / 13 initial CLI /
12 credential CLI** pass. The final maintainer suite has no failures/ignored
cases and took 150.82 seconds. Binary SHA256 is
`46e88a766f1e8febf0d156df8c1f096b506b47886e089d2952047c91481df648`;
both CLI reports name it. The scoped staged diff is clean; an unscoped check
finds preexisting generated-binding whitespace, which was preserved unchanged.

Source review covered command dispatch, per-operation unlock/scopes, signature
verification ordering, credential bounds/permission limits, key no-clobber
semantics and review gates. Graph MCP tools were unavailable; focused source
review was used rather than claiming graph coverage. No outstanding scoped
correctness finding remains. This is not an independent release audit.

Pushed Grain source is `84a579ff3b7f04ee8f3bd0dcb2019d977e37eb13`; the existing
publication/checkpoint workflows now pin that verifier. Registry workflow commit
is `2a074adbd1d062b23210f6a635a119352383228e`. Actionlint passes. The unchanged
author producer retains `ed55bf1df86dcd994fb43cecdb8f1abf033de8d7`; no author
contract change requires rebuilding it.

Verified [Linux checkpoint 37758436225](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37758436225)
passes **85 maintainer / 12 trust / 5 cache / 17 publisher-transport / 13 initial
CLI / 12 credential CLI**. Downloaded evidence includes the actual `git log -1`
source SHA, not just requested checkout arguments. All 25 public CLI labels,
exit codes and expected verdicts exactly match Windows. The Unix owner-only and
symlink regression passes. Both Linux CLI reports identify binary SHA256
`e462b7a620f66bc892c5c9c5c04fc7b8eb20047661fd57edf3a4b1be799a3051`.
Evidence summaries retain exact source/registry pins and verdicts. The Linux
maintainer suite took 15.56 seconds; OS/runtime conditions differ from Windows,
so that difference is not attributed solely to the scrypt profile setting.

## Remaining operational gates and direction

Actual root/publisher identity, encrypted storage, password/backup custody,
release authorization, GitHub protections, first bound public signed generation,
HTTP evidence and isolated real-app acceptance are still E4 gates. No production
key was read/generated/repinned, no publisher dispatched and no main merged.
The previous read-only inspection found no publish-environment protection rules
and main unprotected; this block does not change that configuration.

E5 management UI, E6 Agent modules, E7 measured integration and E8 release
readiness remain. Foundation 52 Pass / 1 Deferred token-expiry, forward 71,
OAuth identity 56 Pending and inventory 108 / 93 / 81 are unchanged. No new
manual test batch or website dependency is introduced. This completes the
credential-tooling gap, not the entire E4 operational acceptance block.

References: [official Minisign documentation](https://jedisct1.github.io/minisign/),
[library key-box API](https://docs.rs/minisign/latest/minisign/struct.SecretKeyBox.html)
and [Cargo profile overrides](https://doc.rust-lang.org/cargo/reference/profiles.html).
The exact locked 0.7.9 keypair/secret-key source was inspected locally; latest
API documentation was not treated as an instruction to upgrade the dependency.
