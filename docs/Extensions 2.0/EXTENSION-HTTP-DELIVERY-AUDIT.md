# E4 public HTTP delivery and operator readiness audit

Date: 8 October 2026. This is a maintained publishing-tool block, not a public
catalogue activation or release approval. The Agent harness is unchanged.

## Implemented boundary

`crates/grain-registry-tools/ci/check_hosted_github.py` is a read-only operator
command. It captures exact committed publication bytes through the existing
Rust CLI, then verifies the complete signed hosting bundle with the app's real
trust anchor, exact receipt and current freshness requirements. No author code,
production signing key, custom trust root or new crypto implementation is used.

The checked signed primary base URL must equal the expected repository's raw
`main/v1/` route, so a successful check cannot certify a different app endpoint.
Every captured file is compared at the full commit URL and mutable `main` URL:
six signed metadata files, native/MCP addressed blobs, DESCRIPTION/media and
retained publication proof. Each response must be complete, status 200,
identity-encoded and have the expected exact length and SHA256. Redirects,
missing/partial/stale/mixed/truncated/oversized/changed files refuse. Local paths
are allowlisted and linked inputs refused; the existing Rust capture has already
bounded file count, artifact formats and total bytes.

The scan uses a fixed HTTPS host with standard certificate/hostname verification,
no authentication/cookies/proxy environment and 64KiB reads. Connections and
responses close on success and error; temporary committed capture is scoped to
the command and removed. There is no background service, monitor, retry loop,
database, app worker or idle resource. The 1GiB budget is for the verified expected
bundle; commit/live scans read it twice, plus six final live metadata reads.
Socket inactivity is at most ten seconds. Thirty-second file and ten-minute scan
elapsed checks surround blocking body operations; these do not establish a hard
deadline against trickled headers/chunk framing. The existing publishing job has
a twenty-minute deadline. This fixed GitHub operator check is not a general MCP
or arbitrary-host network client.

Before/after remote `main` must name the independently expected commit; the final
six live metadata responses are rechecked and signed freshness is reverified.
Successful JSON evidence records expected commit/receipt, timestamps, primary
URL, file hashes/lengths and request count. It certifies this observed HTTP view,
not all CDN locations, atomic snapshots, mirrors, app installation or approval.

`publish_github.py --publish --check-http` uses the same function after the
single conditional push is confirmed. An HTTP failure explicitly reports **Git
publication confirmed / HTTP acceptance failed** and directs the operator to
rerun only the read-only command. Unknown Git outcomes never reach this check;
neither kind of failure retries a push. Fixed repository validation and remote
head parsing are reused by both commands. No signing/key environment is added.

## Scoped verification and audit

Windows evidence: `tests/agent-harness/.runs/e4-http-delivery-01/`.
After the scoped lock fix, all **77 Windows maintainer tests**, locked build,
strict all-target maintainer Clippy and workspace rustfmt check also pass.
The rebuilt verifier SHA256 is
`46fa43ddb4c96c6ad5da3f70ef96e257af3defa9523a73c6ad9046ae3ca470bc`;
all **13 final public CLI checks** pass again on that executable. The earlier
transport/CLI evidence retains its original verifier digest below.

- **17/17 Python product tests** pass: the four existing publisher tests, one
  post-push outcome/temporary-helper cleanup test and twelve delivery tests.
  Transport cases use a real owned loopback HTTP server; only the TLS connection
  is replaced. No public CLI host/trust override is available. Signature/expiry
  failures are controlled subprocess-boundary tests; genuine signature testing
  remains in the Rust verifier suite rather than duplicated in Python.
- Positive scan covers immutable/live routes, final metadata recheck and native,
  MCP, media and history paths. Failure cases cover stale/mixed main, late HTTP
  drift, remote drift before/after, wrong signed primary host, expiry after scan,
  bad signature before networking, redirect/partial/missing/encoded response,
  short/extra/wrong-hash bodies, malformed/duplicate/conflicting length framing,
  chunked/connection-close success, transport timeout, elapsed deadline and paths.
- The original publisher regression still exercises real disposable Git success
  and both competing-writer windows. The new orchestration test deliberately
  controls the final activation boundary; it is not a live publication claim.
- **13/13 actual public CLI boundaries** pass against the unchanged compiled
  Rust verifier (`f17dd6209b3e27e84062b82034a33f652b4246e646d9242c66f5539a53eba65c`).
  Bytecode compilation, focused Ruff F checks and scoped diff checks pass.
- An actual anonymous HTTPS request through the production transport matches
  exact committed `v1/roots.json` bytes at registry commit
  `af6e24425d0eba1f667913f8a5403e9a6fb7ce76`: **228 bytes**, SHA256
  `5b1f7cf03f47eb0a63945279933715802b1104d89c7df7e991a416ee608563d0`.
  This proves the TLS transport/path/byte comparison, not a complete new catalogue.
- The actual read-only public CLI refuses that existing incomplete experimental
  publication before claiming HTTP acceptance. No seed exemption is introduced.
- Graph exploration/review was followed by scoped file review because new Python
  nodes were not indexed and the graph listed stale `head`/unrelated module
  impacts. Checked callers are the manual publisher, standalone read-only CLI,
  focused tests and pinned read-only checkpoint. No app SDK, backend, frontend,
  normal profile or upstream Handy file changed. Existing desktop checkpoints
  remain prior evidence; rerunning unrelated Agent workflows is unnecessary.

Initial Linux run [37685248003](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37685248003)
failed two existing Rust fixture operations with an occupied promotion lock
before the Python tests ran. This failure is retained, not counted as acceptance.

Scoped investigation identified a close-only lock owner: Unix flock is attached
to an open file description, shared by dup/fork. A parallel Git process can retain
a copy briefly until exec even with CLOEXEC; dropping only the owner's descriptor
can leave the next operation falsely excluded. `StoreLock` now explicitly unlocks
on owner-scope exit without removing/replacing the lock inode, adding a worker,
retrying a promotion or weakening exclusivity. The original acquisition error also
retains its OS diagnostic. Close remains a fallback if explicit unlock fails.

A Unix regression deterministically reproduces the old retained-descriptor lock
with `File::try_clone`, then proves owner-scope release while a copy still exists,
active-writer exclusion and that dropping the old copy cannot unlock the new
owner. Existing signed publication/contention tests remain. This explains a
possible fork/exec window matching the observed CI failure; the failed run did
not directly trace the forked descriptor. Final [Linux checkpoint 37685777643](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37685777643)
passes **79 maintainer / 12 core trust / 5 cache / 17 Python / 13 public CLI**
checks. The downloaded actual checkout log names Grain
`0105abd7124d9eeb3c14ea8e14edde73c808ba01`; workflow head is registry
`e33adb8060ee2141367c9fb1fad8583b299ba9e6`. All seventeen Python test names/results
and thirteen CLI label/exit/expectation tuples equal Windows. The deterministic
Unix retained-description regression passes. Evidence is under the Windows
folder's `linux/` child, with an independently checked `verification.json`.
Linux verifier SHA256:
`376c72e5409abda44c83e4d1f33498ce57db35305135794c3c429c90fd1b0d38`.
Artifact ID **11510664523**, service-reported archive digest
`sha256:f46a7daee212436282934bb9ec14f3538f8f2f8468a2eacd692a8946cfe0b491`.
The archive digest is GitHub's reported value, not a locally recomputed archive
hash. Both workflow pins now use the accepted tool source; the unchanged isolated
source builder keeps its independently verified producer pin. Actionlint passes
for both changed workflow files; external shellcheck/pyflakes are not installed
in this local lint invocation. No public publisher or signing workflow ran.

## Actual operator state and remaining gates

Read-only GitHub REST inspection on 8 October, recorded in the same evidence
folder, found:

- `GET /repos/Punit-Dethe/Grain-Extention/environments/publish`: environment
  exists, `protection_rules: []`, `deployment_branch_policy: null`.
- `GET /repos/Punit-Dethe/Grain-Extention/branches/main/protection`: HTTP 404,
  **Branch not protected**. The effective `rules/branches/main` endpoint also returned an empty list.
  The operator must review applicable rules and workflow dispatch rights.

No administrative setting was changed. Named YAML environment is not proof of
enforced approval. The maintainer README now gives the concrete activation order:
review actual controls, separate offline root/publisher custody, produce/verify
the signed clean candidate, authorized single push, HTTP check, isolated actual
app acceptance and assigned renewal/incident ownership. Tools accept an explicit
unencrypted minisign key; encrypted storage/access/backup are operator custody,
not silently supplied by a development file or CI pass.

Remaining E4 gates are actual operator custody/authorization, genuinely signed
bound public generation, HTTP evidence on that publication and real-app acceptance
of reviewed native/MCP entries. No live signer or publisher was run, no main
merge occurred and no complete public catalogue is marked Pass. The workflow
draft connects the check; a read-only rerun remains separate from publication.

E5 management UI, E6 Agent modules, E7 measured integration and E8 release
readiness follow. Foundation **52 Pass / 1 Deferred**, forward **71**, permanent
OAuth identity **56 Pending** and Agent inventory **108 / 93 / 81** remain
unchanged. No new manual test batch; the experimental website stays parked.

## References and cleanup scope

GitHub's [repository contents documentation](https://docs.github.com/en/rest/repos/contents)
defines raw content delivery. Python's [HTTP client](https://docs.python.org/3/library/http.client.html)
and [URL opener documentation](https://docs.python.org/3/library/urllib.request.html)
describe TLS/timeouts and default proxy/redirect handling; the checker uses the
fixed direct HTTPS client to avoid inheriting those opener handlers. The
[TUF specification](https://theupdateframework.github.io/specification/v1.0.36/)
supports bounded downloaded-byte verification and metadata hash binding. This
checker reuses Grain's signed bundle; it does not claim TUF compliance or replace
the app's verifier.

The checker is maintained operator tooling, not temporary Agent harness code.
Its focused standard-library tests are product transport tests, not another
application. The branch-only `serving-tree-checkpoint` wrapper remains existing
cleanup debt: retire the wrapper when normal trusted product CI owns these same
checks. Do not remove the checker or its security tests as harness cleanup.

The scoped lock fix follows Rust's [File locking/unlock documentation](https://doc.rust-lang.org/std/fs/struct.File.html)
and the Linux [flock semantics](https://man7.org/linux/man-pages/man2/flock.2.html).
It changes only the maintainer store lock lifetime, not app locking, installer
execution, provider lifecycle or a new concurrency subsystem.
