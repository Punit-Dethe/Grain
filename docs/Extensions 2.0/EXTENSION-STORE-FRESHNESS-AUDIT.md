# App catalogue freshness and deferred root adoption

Date: 8 October 2026 (local time). Delivery stage: E4. This block changes
the real store client; it does not activate a public catalogue.

## Implemented

- Corrected the fallback host to the GitHub raw URL already present in the
  genuine signed seed: `Punit-Dethe/Grain-Extention/main/v1/`.
- Added small shared freshness checks for authenticated roots and revocations,
  using the existing supported spec and clock-skew rule. Optional root expiry
  remains supported. Unknown specs and malformed timestamps cannot authorize
  acquisition.
- A refresh now requires an authenticated, fresh revocation response as well as
  its catalogue. Missing or bad revocation data no longer enables installation.
  A higher resident revocation version is retained rather than rolled back;
  acquisition also checks that retained policy is fresh.
- Verified candidate roots stay local until the catalogue and revocations have
  passed verification and freshness checks under current operation ownership.
  Failed catalogue/revocation downloads or signatures leave active roots and
  their disk pair unchanged. Publisher changes cannot borrow policy verified
  only under the predecessor's key.
- Native installation and MCP acquisition/update check root and revocation
  freshness before downloading and again before committing downloaded bytes.
  Metadata expiring during a valid download therefore refuses the mutation.
- Already-authenticated revocations continue to block existing extensions when
  stale or offline. Freshness restricts new acquisition; it does not switch off
  the cached kill switch. Current-key signed negative policy can still advance
  while catalogue acquisition stays offline.

No transport, OAuth adapter, Agent executor, UI design, public SDK or publisher
workflow was changed. No legacy compatibility layer, timer or background service
was introduced.

## Focused review and verification

Graph overview/impact and scoped change/review queries preceded direct source
review. Its derived coverage is incomplete: graph warnings are not actual test
failures or proof of missing runtime coverage. Manual diff review covered
operation ownership, read/write lock ordering, expiry at commit, version floors,
publisher changes, offline fallback and revocation enforcement.

Windows checks passed:

| Boundary | Result |
|---|---|
| Full locked core library | 294/294 |
| Shared trust subset, included above | 11/11 |
| Unchanged maintainer suite after shared trust change | 76/76 |
| Normal backend store component tests | 11 Pass; 1 existing live-network test ignored |
| Existing real-app native store suite | 3/3 |
| Existing real-app MCP store suite | 3/3 |
| Existing runner self-tests | 78/78 |
| Scoped strict Clippy, formatting and diff checks | Pass |
| Stamped real-app build, TypeScript and Vite | Pass; 12 existing backend warnings |

The native and MCP component matrices separately cover roots/revocations
expiring during downloads. The signed refresh matrix additionally covers missing
and tampered catalogue/revocation responses, expiry during refresh and retained
version floors, including the unchanged root/cache assertions after failures.
The existing real-app suites cover ten close/reopen cycles, verified native
install/consent/Agent greeting across restart, offline browsing/refusal, pending
owner mutations, integrity/cancellation, MCP acquisition/update/removal, account
retirement and cached revocation. These are controlled-provider/scripted-model
checks, not live public hosting or live account expiry certification.

Reproduce using existing maintained commands:

```powershell
cargo test --locked -p grain-core --lib -- --test-threads=2
cargo test --locked -p grain-registry-tools -- --test-threads=2
node tests/agent-harness/production-tests.mjs --group store
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite store
node tests/agent-harness/run.mjs --suite store-mcp
node --test tests/agent-harness/runner.test.mjs
```

Evidence remains in existing ignored directories:
`tests/agent-harness/.runs/logic-8B7WSG/`,
`run-xdYnHT/` (native), and `run-HHVnfk/` (MCP).
The real host SHA256 is
`18994e38f18b2a71db0e34117d0e0007779082908dbdf84ce801b7b63c8b6054`.
Both accepted desktop reports record the same binary/source fingerprint and
separate verdicts. All reports record cleanup Pass; an independent process and
listener check finds no owned host or store listener across thirteen host PID
references, including the failed attempt. A PID briefly reused by Windows
RuntimeBroker was not mistaken for an owned host or terminated.

## Failed attempts and bounded fixture maintenance

A direct backend test invocation compiled but could not load its Windows DLLs
(`0xC0000139`); no test ran. The existing production-test runner supplies the
proper DLL runner and passes. This is not an app crash or accepted direct run.

The first desktop attempt, `run-f1KdxL/`, failed while waiting for its native
listing; the next two cases were Not run. Its request journal shows native
fixture revocations returned 404. The new production gate correctly refused
installation. The only harness edit gives that existing native fixture a fresh
signed empty revocation list, matching the MCP fixture's supported contract.
Both complete suites then passed. The failed evidence is retained as Fail.

No new scenario, general runner, bridge or alternate visual application was
built. Inventory stays **108 IDs / 93 self-contained cases / 81 combined Node
self-tests**; this invocation ran the 78 runner tests, not all combined tests.
Retained reports/screenshots/logs/profiles are disposable nonsecret acceptance
evidence, not production dependencies or proof of file deletion. Keep the
maintained fixture while these product paths remain regression requirements.

## Remaining E4 work and scope limits

This is deferred adoption after verified downloads, **not crash-atomic durable
metadata storage**. The existing cache still writes independent document/signature
pairs and ignores write failures. Next implement one bounded atomic cache
selection, preserve current authenticated revocation enforcement and monotonic
floors after restart, and test interrupted/failed writes using product tests.
Do not build a new engine or compatibility migration for the old cache layout.

Equal-version metadata identity and protection against mixing independently
signed generations also need an explicit current-contract decision before
public activation. This block retains existing version-floor semantics and does
not claim full snapshot binding or TUF compliance. Future-spec roots/revocations
currently disable acquisition through offline fallback; UI explanation belongs
to E5.

Actual publisher custody/authorization, signed first activation, public HTTP
metadata/assets and app acceptance against that live publication remain open.
The experimental website remains parked. Permanent OAuth client document
identity is separate; **criterion 56 remains Pending**. No production key,
GitHub protection/environment, main branch or live publication was changed.

E4 is current; E5 management UI, E6 Agent modules, E7 measured integration and
E8 release readiness follow. Foundation **52 Pass / 1 Deferred** (live expiry),
forward **71** and the existing manual ledger are unchanged. No new user test
batch is needed for this block. Unrelated bindings and nested repository edits
are preserved.

## References

The [TUF metadata specification](https://github.com/theupdateframework/specification/blob/master/tuf-spec.md),
[security model](https://theupdateframework.io/docs/security/) and
[metadata documentation](https://theupdateframework.io/docs/metadata/) inform
expiry, rollback and authenticated refresh boundaries. Grain retains its current
Minisign contract; borrowing these principles does not make it a TUF client.
