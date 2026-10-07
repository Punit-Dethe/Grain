# Atomic app store cache and authenticated version identity

Date: 8 October 2026. Delivery stage: E4. This completes the local cache
persistence unit; public catalogue activation remains separate.

## Product change

The store now selects one `store/metadata.cache` file containing all six exact
document/signature byte sequences. Fixed component ordering, a small header and
bounded lengths replace independent disk pairs. Documents retain the existing
4 MiB bound and signatures the 64 KiB bound. Encoding streams directly to disk;
no JSON byte-array expansion, additional serialization-sized buffer, timer,
database, worker or service was added.

The cache reuses Grain's existing atomic file writer: create a private temporary
file in the same directory, write and sync its contents, then atomically replace
the selected file. Windows sharing locks use the existing bounded publication
retry; tool/network operations are not retried. Normal failure drops only the
owned staging file; the selected file is never removed/copied as a fallback.
The helper's implementation is unchanged apart from crate-private visibility.

Refresh verifies prior signed bytes before retaining them, enforces monotonic
versions and rejects different signed document bytes at an already accepted
version. Signature-comment changes do not redefine document identity. Pending
roots, the accepted catalogue and the selected highest authenticated revocation
policy are committed together. Only successful persistence can publish a fresh
installable view; root/catalogue/floor RAM state stays unchanged on failure.
Freshness is checked again after the synchronous write before installs are
enabled. Install/update preflight and final expiry guards from the preceding
block remain in place.

An authenticated current-key kill switch still takes effect in RAM even when
acquisition or persistence fails. Policy-only persistence retains the previous
trusted catalogue and its floor. Persistence failures are logged and do not
grant installation. If a stronger resident policy has no durable signed proof,
the store refuses acquisition instead of forging a serialized policy or
silently forgetting the stronger floor.

Startup authenticates one selected cache and recovers its version floor,
keeping the parsed catalogue unloaded while idle. An unreadable/untrusted cache
disables new acquisition; it is not silently interpreted as a new user.
Independently valid cached revocations and catalogue floors remain usable even
if another signature fails. An explicit reset/recovery explanation belongs to
E5; a corrupt cache is not automatically erased or overwritten.

The old independent cache-pair readers/writers and the test requiring that old
layout are retired. Old cache layouts are ignored, with no migration or legacy
upgrade support, consistent with the unreleased clean-break decision. Existing
signature fixtures remain only where they prove retirement or genuine signed
same-version replacement refusal.

## Scoped audit and evidence

Graph overview/search/impact preceded direct source inspection; change/review
queries were scoped to the four affected product files. Its keyword search
could not locate the cache helpers and derived coverage is incomplete. Source
review and actual tests, rather than graph test-gap counts, establish this
unit. Review covered bounded decoding, atomic replacement failures, exact
signed identity, publisher changes, policy preservation, ownership/lock order,
restart floors, expiry after persistence and guarded fixture trust.

Windows verification:

| Check | Result |
|---|---|
| Full locked core library, including five new cache test groups | 299/299 |
| Scoped strict core Clippy and rustfmt | Pass |
| Normal backend store tests | 11 Pass; one preexisting live-network test ignored |
| Existing real-app native store suite | 3/3 |
| Existing real-app MCP store suite | 3/3 |
| Stamped real-app build, TypeScript and Vite | Pass; 12 existing backend warnings |
| Scoped diff/format checks | Pass |

New core tests verify genuine signed seed round-trips and replacement, malformed
headers/lengths/UTF-8/truncation/trailing bytes, interrupted nonselected staging,
oversized input refusal, failed replacement preserving its target and owned
cleanup, plus monotonic/exact version identity. Backend tests authenticate a
saved selection, test partial/bad signatures, preserve floors across restart,
and exercise signed HTTP refresh with missing/tampered metadata, expiry,
rollback, genuine same-version root/catalogue replacement and cache failure.
The Windows persistence case holds the actual selected file without delete
sharing: atomic publication exhausts its bounded retry, leaves the file/floor
unchanged, cleans staging and keeps installation disabled.

The twelve-mode signed-refresh matrix maintains separate mode assertions;
suite-level test counts are not inflated into twelve independent test functions.
The two genuine signed replacement cases reuse public fixtures signed by the
app's genuine pinned/publisher keys; no private publisher key or trust override
is opened. The existing guarded host keeps its fixed fixture publisher and now
validates its whole selected catalogue/policy at startup. That test-only path
does not confer public root-rotation acceptance.

Final normal-backend evidence: `tests/agent-harness/.runs/logic-JVafEz/`.
Earlier affected successful repeats are `logic-Ft63oj/` and `logic-9llfJt/`;
they are supporting historical results, not substitutes for the final repeat.
Final real-app evidence: `run-mkp3Qy/` (native) and `run-u2uzfg/` (MCP), both
under the maintained `.runs/` directory. The same host SHA256 is
`bbdf6984e593101829d9d09f8b57de7f2e9f3db6d4a4a36ab7c3ff10d01facfb`.
Both reports record six separate Pass verdicts and cleanup Pass; independent
inspection finds no owned host or store listener across twelve host PID
references. The suites include restart, native consent/Agent greeting, offline
refusal, pending ownership changes, integrity/cancellation, MCP account
retirement and cached revocation. They use controlled providers/scripted models.

No Agent harness source, scenario, fixture, framework, visual replica or manual
batch was added. Inventory remains **108 / 93 / 81**. Existing ignored logs,
reports/screenshots/profiles are disposable evidence; retention is not file
deletion Pass. Product cache tests remain maintained under core/backend tests.
The existing read-only [Linux checkpoint 37674003172](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37674003172)
passes **5 cache / 11 trust / 77 maintainer / 4 publisher / 79 actual CLI checks**.
Downloaded logs/reports independently match Grain source
`f98d872a453f08135a64aa38af539de74dda56e5` and registry workflow source
`86747c6a9000c8de906d18fc8e74b34feac365a0`. All 67 retained and 12 shared CLI
verdicts match the preceding accepted checkpoint; reported keys created and
hosting changes remain zero/false. Linux evidence and comparison record are
`tests/agent-harness/.runs/e4-atomic-cache-linux-01/verification.json`.
Artifact ID `11506216288`; GitHub-reported archive SHA256
`c5f5afd8f0eca82407c017489020cc521a212bd75244c3d46abde00aed8e5b48`.
The Linux CLI SHA256 is
`8260cef56a0a707f86c2dc6613ddec74bec151bda202c80078787b4844c00475`.
The five cache test groups execute against the real Linux filesystem primitive;
they do not certify the Windows sharing-lock case on Linux or a Linux GUI host.
The branch checkpoint wrapper remains temporary until maintained product CI
owns this coverage; retain the core/backend product tests. Workflow syntax lint
passes with the previously checksum-verified actionlint 1.7.12; shellcheck and
pyflakes are disabled in this local invocation. Publisher/source-builder pins
remain unchanged; only this read-only checkpoint selects the new Grain source.

## Reproduction

```powershell
cargo test --locked -p grain-core --lib -- --test-threads=2
cargo clippy --locked -p grain-core --all-targets --no-deps -- -D warnings
node tests/agent-harness/production-tests.mjs --group store
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite store
node tests/agent-harness/run.mjs --suite store-mcp
```

## Limits and next handoff

This guarantees a single local selection through atomic replacement, not a
cryptographic network snapshot binding across the independently signed metadata
roles. It does not claim TUF compliance, OS/hardware power-loss certification or
directory-entry durability across every filesystem. The selected staging file
is synced; abrupt process death before replacement can leave an unselected
temporary file, which is never consumed as trusted metadata. Normal exception
and publication failure cleanup is verified; hard-kill orphan retirement and
platform durability remain explicit release checks.

Successful persistence retains accepted floors after restart. If policy
persistence itself fails, only the previously durable signed policy can survive
restart; the newer policy is enforced in that process and no fresh acquisition
is certified. Removing/tampering with the entire local profile is outside a
signed local cache's monotonicity guarantee.

Next E4: finalize the hosted metadata-generation contract and operator signing/
publication custody, then authorized signed initial activation and actual HTTP/
fresh-profile app coherence. These gates must not be credited from local
fixture runs. GitHub remains the chosen catalogue host; the experimental
website is parked. No production key, main merge, live publication or OAuth
adapter change occurred. Permanent OAuth client identity **56 stays Pending**
and live expiry remains Deferred.

E4 is current; E5 management UI, E6 Agent modules, E7 measured integration and
E8 release readiness follow. Foundation **52 Pass / 1 Deferred**, forward **71**
and the manual ledger stay unchanged. Unrelated bindings and registry working
changes are preserved. No new user test batch is needed.

## References

[Tempfile's atomic persistence documentation](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist)
and Rust's [file synchronization contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all)
support the existing write/sync/replace primitive and its durability limits.
[TUF's security model](https://theupdateframework.io/docs/security/) informs
rollback/freeze/mixed-metadata review, without adding a new TUF implementation.
