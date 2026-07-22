# Distribution + Developer Mode — the plan

**Status: DESIGNED. This document lifts the gate in
[GATE-DISTRIBUTION-AND-DEVMODE.md](GATE-DISTRIBUTION-AND-DEVMODE.md).**
Evidence and sources: [DISTRIBUTION-RESEARCH.md](DISTRIBUTION-RESEARCH.md)
(findings are cited below as **F-n**).

---

## 0. Start here — everything you need to know to read this

**Grain** is a local, low-RAM speech-to-text desktop app: a Rust/Tauri backend
(`src-tauri/`), a React frontend (`src/`), shared Rust crates (`crates/`), and a
native Rust pill window. It is built on an upstream project called *Handy*, whose
code lives verbatim in `src-tauri/src/handy/` and must not gain features.

**An extension** adds a feature to Grain without forking it. There are three
tiers:

- **pack** — data only (prompts, snippets, a pill colour theme). No code runs.
- **scripted** — JavaScript in an isolated Web Worker, spawned on activation and
  reaped when idle.
- **native** — a small separate program Grain starts and stops.

Every power an extension has is a **capability** it declares in its manifest
(`storage`, `llm`, `events:transcripts`, `screen:capture`, …). Capabilities are
enforced **in Rust**, per connection, on every message — not in JavaScript.
Extensions never create windows; they *declare* surfaces and the host builds,
sleeps and destroys them.

**Where the project stands:** phases 0–3 are shipped — the security wall, the
runtime, settings, extension-owned windows. Grain's own features (Snippets,
Context Awareness, Agent, Grain Space) are being moved onto this same contract.

**The two holes this plan fills:**

1. There is nowhere for an extension to *live*. No store, no submission, no
   review, no trust signal — and no answer to "how do we stop someone faking a
   verified badge".
2. There is no way for anyone to *build* one. No developer mode, no reload loop,
   no debugging. Today an extension can only be authored by us, by hand.

**What this document is:** the decisions, the reasoning behind each, where each
piece lands in the roadmap, and a step-by-step guide an implementer can follow
without asking anyone anything.

---

## 1. The decisions, on one page

| Question | Decision | Why (finding) |
|---|---|---|
| Where does the index live? | A new public GitHub repo, `grain-extensions`. Submissions are pull requests. | Obsidian, Raycast and Zed all do this and none runs a submission service (**F-1**) |
| What does the app talk to? | **Static signed JSON on a CDN** (Cloudflare R2 + Pages), never the git repo | Git-as-database degrades; static is CDN-cacheable and works offline (**F-1**, **F-12**) |
| Who builds the artifact? | **We do.** The registry's CI builds from a pinned commit. Authors submit source, never bytes. | The only way "reviewed" and "installed" are provably the same thing (**F-2**) |
| How does the app know it is genuine? | Ed25519 signature over the index, **root public key pinned in the binary** — the same shape as Grain's own updater | Already proven in this codebase; no new crypto (**F-3**, **F-24**) |
| How is "verified" made unforgeable? | Trust exists **only** inside signed index metadata. No manifest field, no domain check, no author input. | Domain-ownership "verified" is theatre (**F-5**, **F-22**) |
| What stops review-then-swap? | Trust is **per version**, bound to `(id, version, sha256)`; we built the bytes ourselves | GlassWorm shipped clean, then updated dirty (**F-6**) |
| How does review stay fast? | Risk score computed from the manifest → three lanes: auto / auto+audit / human | Grain's capabilities make risk *machine-computable*, unlike Chrome or VS Code (**F-14**, **F-15**) |
| Where is our dashboard? | A generated static page over GitHub PRs + labels. GitHub is the database. | Zero infra, free auth, complete audit trail (**F-14**) |
| Is there a public website? | Yes — a static site generated from the same index. Free, and it is how people share links. | Cost is ~zero (**F-4**) |
| What does hosting cost? | Effectively $0 — R2 has no egress charge, Pages has no bandwidth cap, Actions is free for public repos | (**F-4**) |
| How do people build extensions? | `grain-ext` CLI (`init`/`dev`/`doctor`/`pack`/`submit`) + in-app developer mode with load-unpacked, sub-second reload, and a developer panel | Raycast's DX is the bar; Zed's load-unpacked is the model (**F-16**, **F-17**) |
| What ships first? | **Developer mode, before Phase 4.** | Nothing else can be authored or verified without it (**F-19**) |

---

## 2. The shape of the system

```
  AUTHOR                        US                             USER'S GRAIN
  ──────                        ──                             ────────────
  grain-ext init                                               Extensions ▸ Store
  grain-ext dev    ──▶ local dev loop (no network, no review)  ┌──────────────┐
  grain-ext doctor      (identical checks to CI)               │ index.json   │◀── CDN
  grain-ext submit ──▶ PR to github.com/…/grain-extensions     │ + signature  │
                            │                                  └──────┬───────┘
                            ├─ CI job 1: BUILD (untrusted)             │ verify with
                            │    no secrets, no network egress         │ PINNED root key
                            ├─ CI job 2: CHECK (lint, risk score)      ▼
                            ├─ human review IF the lane says so   fetch blob/<sha256>
                            └─ CI job 3: SIGN + PUBLISH ──▶ R2     verify hash → unpack
                                 (never runs untrusted code)       → atomic install
```

Three separations carry the whole design:

1. **Source of truth (git) ≠ what the app reads (static signed files).**
2. **The job that runs untrusted code holds no keys; the job that signs runs no
   untrusted code.** This is not a checklist item — it is the exact mistake that
   compromised Open VSX (**F-7**).
3. **Trust metadata travels separately from the artifact**, signed by us, so an
   author who fully controls their own repo and their own pack still cannot make
   it appear verified (**F-5**).

### 2.1 What is published (the static layout)

```
/v1/index.json               signed catalogue — small fields only
/v1/index.json.minisig
/v1/roots.json               signed by the offline root key: the current publishing key
/v1/roots.json.minisig
/v1/ext/<id>.json            full detail: long description, screenshots, changelog, version history
/v1/blob/<sha256>.grainpack  content-addressed artifacts (immutable, cache-forever)
/v1/revocations.json         kill switch
/v1/revocations.json.minisig
```

`index.json` is the only file the app must have. One entry:

```jsonc
{
  "id": "com.example.spaces",
  "name": "Spaces",
  "version": "1.2.0",
  "tier": "scripted",
  "trust": "verified",              // ← ONLY ever set here, by us
  "capabilities": ["storage", "embed", "llm"],
  "risk": 3,                         // machine-computed, shown in the UI
  "sha256": "…",                    // of the artifact
  "size": 41233,
  "min_grain_api": "1.0",
  "repo": "https://github.com/example/spaces",
  "source_commit": "a1b2c3…",       // what we built
  "reviewed_at": "2026-07-20",
  "updated_at": "2026-07-20",
  "stars": 214                       // fetched by CI, never by the client
}
```

The envelope carries the anti-tamper properties (**F-11**):

```jsonc
{ "spec": 1, "version": 42, "expires": "2026-08-22T00:00:00Z", "entries": [ … ] }
```

**Client rules, in order.** Verify the signature against the pinned root key (via
`roots.json`) → reject if `version` is lower than the stored copy (**rollback**)
→ if `expires` has passed, keep serving the cached copy but mark the store
"offline — last updated *date*" and refuse *new* installs (**indefinite freeze**)
→ one signature covers the whole catalogue, so entries cannot be mixed across
generations (**mix-and-match**).

### 2.2 Keys

| Key | Where it lives | What it signs | Rotation |
|---|---|---|---|
| **Root** (Ed25519, 3 keys, threshold 2) | Offline. Hardware or paper-backed, never on a build machine. | `roots.json` only | Manually, rarely; a new app build re-pins |
| **Publishing** (Ed25519) | A GitHub Actions secret used *only* in the signing job | `index.json`, `revocations.json`, artifacts | Sign a new `roots.json` — **no app update needed** |

Two levels, ~150 lines of verification code, and the properties of TUF that
matter without operating a TUF repository (**F-11**). Practical guidance for a
team this size is 3–5 offline keys with a threshold of 2; if there is ever more
than one signer, the upgrade path is Sigstore's transparency log, not a bigger
homegrown scheme.

> **Bootstrapping note.** Until the root keys exist, everything downstream is
> theatre. Generating them is **Step 5A.1**, before any code that verifies them.

---

## 3. Trust: the ladder and the guarantee

### 3.1 The ladder

| Rung | What it means | How you get there | What it changes |
|---|---|---|---|
| `dev` | Loaded from a local folder by a human, in developer mode | Load unpacked | Permanent badge; never in the store; cannot be promoted; capability wall identical |
| `experimental` | Automation passed. **No human has read this.** | Merged submission in lane 0 or 1 | Install sheet says so plainly; not featured; fully searchable and installable |
| `verified` | **A human read this exact version's source.** | Lane 2, or a promotion request | Badge; featured-eligible; shown as reviewed with a date |
| `core` | Written and maintained by us | Built from the `grain` repo by our own CI | May pre-install; may claim core slots |

Two negative states, both delivered by the signed `revocations.json`:

- **`deprecated`** — no new installs, existing installs keep working, card shows
  why. For abandonment.
- **`revoked`** — the kill switch. On the next index refresh the client
  **disables** the extension, shows a red banner naming the reason, and offers
  one-click removal. It does not delete user data without consent (that stays the
  uninstall rule: default keep, explicit purge).

Revocation is what makes demotion real. Without it "verified" is a promise we
cannot take back, and every registry that lacked it has regretted it (**F-6**).

### 3.2 The anti-forgery guarantee, stated precisely

> An author who controls their source repo, their build, their pack file, their
> website and their domain **still cannot cause any Grain client to display their
> extension as `verified` or `core`.**

It holds because of four properties that must each be tested:

1. **The manifest has no trust field**, and the import path ignores unknown
   fields. A pack containing `"trust": "verified"` installs as `community`.
   *(Test: `a_pack_claiming_trust_installs_untrusted`.)*
2. **Trust is read only from verified index metadata.** The install path takes
   trust as a separate argument sourced from the verified index — it is not
   reachable from pack bytes. *(Test: the installer's trust parameter has exactly
   one caller.)*
3. **The index is signed by a key the author does not have**, and the client
   pins the root. A modified index fails before any entry is read.
4. **Trust is bound to `(id, version, sha256)`.** A verified 1.0 confers nothing
   on 1.1. *(Test: `trust_does_not_survive_a_version_bump`.)*

The failure mode we are explicitly designing against is VS Code's, where
"verified" means "bought a domain", and extension names are not unique so an
attacker can present a perfect visual clone (**F-5**).

### 3.3 Risk score — one table, three lanes

Weights live in **one place**, `grain-sdk`, next to the capability vocabulary, so
the app, the CLI and CI cannot disagree:

| Weight | Capabilities |
|---|---|
| 0 | `storage`, `embed`, settings-only packs |
| 1 | `llm`, `shortcuts`, `surface:*`, `events:sessions` |
| 2 | `events:transcripts`, `clipboard:read`, `clipboard:write`, `capture:selection`, `session:start`, `overrides:*` |
| 3 | `provides:*` |
| 4 | `screen:capture`, `net:*` |
| + tier | pack `0`, scripted `2`, native `5` |

**Forced-human combinations** (score irrelevant): `screen:capture` + `net:*`
(already the SPEC's rule) · `events:transcripts` + `net:*` · any `native` + `net:*`.

| Lane | Trigger | Path | Target |
|---|---|---|---|
| **0 — auto** | tier `pack`, zero capabilities | Lint + build + publish, no human | minutes |
| **1 — auto + audit** | score ≤ 5, no forced combination | Auto-publish as `experimental`; a sampled share get read later | hours |
| **2 — human** | score > 5, any forced combination, or a promotion request | Queued, risk-sorted | days |

This mirrors AMO (risk-weighted queue, auto-approve the low end, sample the
middle) and Chrome (automate everything, humans on sensitive permissions) — with
the advantage that our inputs are exact rather than heuristic (**F-14**, **F-15**).

### 3.4 The fast lane — how an extension gets verified *quickly*

Four mechanics, each of which removes work rather than rushing it:

1. **We built the bytes.** There is no "did the uploaded binary come from this
   source" question to answer, because there is no upload (**F-2**).
2. **Diff-only re-review.** Both versions were built from pinned commits, so an
   update's review surface is the *source diff*. Identical capability set +
   all-green checks + a diff under the review threshold → **keeps its rung
   automatically**. Any capability change → straight back to lane 2. This is
   what makes "verified" survivable for an actively-developed extension instead
   of a tax on every release.
3. **`grain-ext doctor` runs the exact CI suite locally.** Same code path, same
   version, same output. Most submissions then pass first time, which is the
   single biggest determinant of how long verification *feels*.
4. **Zero-capability packs skip the queue entirely** (lane 0). A prompt pack or
   a pill theme has no code and no powers; it does not deserve a human's day.

**For our own extensions:** anything built from the `grain` repo by our own CI is
`core` by construction — same signing job, no queue, no review round-trip. That
is not a shortcut in the trust model; it is the same rule applied to a publisher
who is us.

---

## 4. Submission and review

### 4.1 The author's path

```bash
grain-ext init my-extension     # scaffold + manifest + types
grain-ext dev                   # build → run in Grain → hot reload
grain-ext doctor                # the exact CI checks, locally
grain-ext submit                # opens the PR against grain-extensions
```

`submit` writes one directory to the registry repo and opens a pull request:

```
extensions/com.example.spaces/
  submission.toml    id, source repo, tag, commit sha, categories, licence
  README.md          store copy
  screenshots/       optional
```

**Required of the author:** a public source repository, a licence, a pinned tag
and commit, a one-line description, a category, and a contact. **Not required:**
a built artifact, an account, a signing key, a payment method.

### 4.2 CI, in three jobs that cannot see each other's secrets

| Job | Runs | Holds | Network |
|---|---|---|---|
| **build** | The author's build (untrusted code, `npm ci` and friends) | **nothing** | egress **blocked** |
| **check** | Lint, manifest validation, risk score, Unicode scan, size caps, diff summary | nothing | none |
| **sign** | Hash, sign, upload, regenerate index | the publishing key | upload only |

Job **sign** never executes untrusted code; job **build** never holds a
credential. They communicate only by artifact hand-off. Open VSX was compromised
by exactly the collapse of this boundary — `npm install` ran arbitrary build
scripts in a job that held a token able to overwrite any extension in the
registry (**F-7**).

**Check-job gates (all blocking):**

- manifest validates against `grain-sdk`; capabilities are known names
- **no invisible or bidirectional Unicode** anywhere in submitted source — the
  exact technique GlassWorm used to hide payloads from reviewers (**F-6**)
- no minified or obfuscated source in the *source* tree (we produce the minified
  artifact ourselves)
- size caps; artifact reproducible from the pinned commit
- id is reverse-DNS, unique, and not a near-miss of an existing id
  (typosquat check on normalised edit distance)
- licence present (Zed made this a hard CI failure for the same reason)

### 4.3 Our dashboard

**GitHub is the database.** PRs are submissions, labels are state, checks are
evidence, comments are the audit trail. On top sits a **generated static
dashboard** (Cloudflare Pages, access-restricted) showing:

- the queue **sorted by risk weight**, highest first (AMO's model — **F-14**)
- per submission: risk score *with its breakdown*, capability diff against the
  previous version, source diff link, lint findings, build status and artifact
  hash, reproducibility result, author history, time in queue
- decisions taken as labels (`decision:verified`, `hold:security`,
  `decision:reject`), executed by a bot workflow on merge

**Why not build it inside Grain:** nobody but us would ever open it, and it would
ship in every user's binary. That is precisely the "destroy if not in use" rule
the project runs on.

---

## 5. Install, update, remove

### 5.1 Pack format v2

One extension, one `.grainpack` file, two physical shapes detected by the first
byte:

| Shape | First byte | Contents | Used by |
|---|---|---|---|
| JSON | `{` | manifest with embedded payloads (today's format, unchanged) | tier `pack` |
| ZIP | `PK` | `manifest.json`, `files.json` (per-file SHA-256), `entry.js`, assets, per-platform binaries | tiers `scripted`, `native` |

`files.json` carries a hash for **every file**, not one hash for the archive —
VS Code's signature-manifest design, which localises tampering to a file rather
than an all-or-nothing verdict (**F-3**).

### 5.2 The install transaction

```
fetch /v1/blob/<sha256>            content-addressed; cache-forever; resumable
  ↓ verify sha256 == index entry   (index itself already signature-verified)
  ↓ unpack to <appdata>/extensions/.staging/<id>-<version>/
  ↓   PATH-SAFE extraction, see below
  ↓ verify every files.json hash
  ↓ atomic rename → extensions/<id>/<version>/
  ↓ registry entry written (installed, disabled)
  ↓ user enables → permission sheet → capabilities granted
```

**Path-safe extraction is a test suite before it is an extractor** (**F-8** — Zed
shipped a CVSS 7.4 Zip Slip in exactly this code):

- reject any entry containing `..`, an absolute path, a drive letter, or a
  symlink
- canonicalise the destination and re-verify containment *after* joining
- cap entry count, per-entry uncompressed size, and total uncompressed size
  (zip bombs), and enforce a compression-ratio ceiling
- stage in a temp directory; the only non-atomic step is a rename

**Rules that are security invariants, not preferences:**

- **No transitive install, ever.** Installing A never installs B. A missing
  provider stays a dead end with a "find a provider" link (**F-6**).
- **Update with new permissions installs but stays disabled** until the
  permission *diff* is approved (already SPEC §6).
- **The previous version directory survives** until the new one enables cleanly,
  so a bad update is one rename away from rolled back.
- **Uninstall stays one transaction**: storage wiped unless kept, token revoked,
  shortcuts unregistered, slots released, windows destroyed.
- **A `grain://` link may open a store page. It may never install anything.**
  Install requires an in-app click, always.

### 5.3 Overhead — the non-negotiable

The store must cost nothing when it is not open. Concretely:

- the index is fetched **on store open**, and at most once per day in the
  background — never on the transcription path, never at startup
- a seed index ships inside the app, so first open is instant and offline works
- parsed index is dropped when the store slide-over closes; only the small
  installed-extension registry stays resident
- the store surface is the existing slide-over, not a new window
- verification is one Ed25519 check and one SHA-256 over an already-downloaded
  file — microseconds and milliseconds respectively

---

## 6. Developer mode

Treated as a product surface, not a debug flag. The bar from the gate: security
matters, *and* authoring must be enjoyable.

### 6.1 The CLI — `grain-ext`

Rust, shipped beside Grain and installable standalone. Five verbs, no cleverness
(**F-13**):

| Command | Does |
|---|---|
| `init` | Scaffolds a manifest, an entry file, TypeScript types, and a README |
| `dev` | Builds, pushes into a running Grain, watches, rebuilds, reloads |
| `doctor` | Runs the CI check suite locally — same code, same output |
| `pack` | Produces a `.grainpack` for manual sharing |
| `submit` | Opens the registry PR |

Tier-B authors need Node + esbuild for bundling, exactly as Raycast requires;
tier A and C need neither. Say so in `init` output rather than failing later.

### 6.2 In-app

Extensions ▸ **Developer** (hidden until enabled):

- **"Developer mode" toggle.** Explicit, in-app, human-only. While on, a
  persistent chip is visible in the Extensions tab.
- **"Load unpacked…"** — a folder picker. Never a URL, never a download, never
  triggered by an extension or a link (**F-10**).
- Loaded extensions are listed with a `dev` badge, and if a store version of the
  same id is installed it is shown as **overridden by the dev extension** —
  Zed's phrasing, because the ambiguity is otherwise a real time sink
  (**F-17**).
- Dev extensions get **the identical capability wall**, can never display as
  verified, and are never promotable.

### 6.3 Reload

A scripted extension *is* a Web Worker, and the host already kills and respawns
workers on every idle reap. So reload is: rebuild → kill worker → respawn →
re-mount surface iframes. **Target: under 300 ms, no app restart** — the thing
VS Code still cannot do (**F-17**).

Transport: the CLI connects to the existing local WebSocket on a **dev channel**,
authenticating with a token written to a 0600 file in the app data directory,
created only while developer mode is on. Identity stays bound to the channel (the
platform's existing rule); the dev token grants dev-control methods only — never
extension capabilities.

### 6.4 The developer panel — where the DX is won

A Grain-owned workspace surface, **built only when developer mode is on**, and
sleeping like every other surface. It shows, live:

| Panel | Content |
|---|---|
| **Activity** | Activation events, worker spawn/reap, with timestamps |
| **Host calls** | Method, params, result or error, **duration** — the extension's whole conversation with Rust |
| **Denials** | Every refused call in red, naming the missing capability and the exact manifest line to add |
| **Budget** | Transform timings against the 150 ms budget, p50/p95, strikes accrued |
| **Resources** | Worker memory against its ceiling |
| **Console** | `log.*` output and thrown errors, **source-mapped to the author's file and line** |

Plus one **"Copy diagnostics"** button producing a paste-able report — the thing
that turns a bug report into a fix.

### 6.5 Errors

Every host error is typed and carries a fix:

```jsonc
{ "code": "E_CAPABILITY_DENIED", "capability": "events:transcripts",
  "message": "This extension is not permitted to receive transcripts.",
  "hint": "Add \"events:transcripts\" to permissions in your manifest.",
  "docs": "https://…/capabilities#events-transcripts" }
```

Never a silent empty result. Zed returns an error naming the missing capability
and it is the single highest-leverage DX decision in their extension API
(**F-18**).

---

## 7. Metrics, and what we refuse to collect

| Signal | Source | Shown | Ranks results? |
|---|---|---|---|
| Trust rung + reviewed date | Signed index | Yes, prominently | Yes |
| Capabilities + risk score | Manifest | Yes — the honest signal | No |
| Size, last updated, licence | Build | Yes | No |
| GitHub stars | Fetched **by CI**, baked into the index | Yes, muted | **No** |
| Install count | Opt-in, aggregate, deduped per install-id per version | Later, muted, if at all | **Never** |

Counts are purchasable and pumpable — 700 versions of one package produced ~50k
downloads in three days with no human involved (**F-9**). They may inform a
human; they may never carry authority. The client never contacts GitHub (rate
limits, and a request per card is a tracking surface). Default telemetry: none.

---

## 8. Corrections to what we already built

Found by researching our own code against the incident record. Each is placed.

| # | Finding | Severity | Where it lands |
|---|---|---|---|
| C-1 | **Extension surface windows had no Tauri capability** — `listen()` would be denied, breaking the sleep/revive handshake (**F-19**) | **Bug** | ✅ **Fixed** (`capabilities/extension-surface.json`); runtime e2e in 3.5 |
| C-2 | Local WebSocket server does not validate `Origin`; browsers do not apply same-origin to WebSockets (**F-20**) | Hardening | **Phase 3.5, step 1** |
| C-3 | Trust must be unreachable from pack bytes — currently true by accident, not by construction (**F-22**) | Invariant | **Phase 5A, step 2** (with tests) |
| C-4 | No rejection of invisible/bidi Unicode in extension source (**F-6**) | Security | **3.5** (`doctor` lint) + **5B** (CI gate) + import path |
| C-5 | Archive extraction does not exist yet — write the traversal/zip-bomb tests *before* the extractor (**F-8**) | Security | **Phase 5A, step 4** |
| C-6 | `entry_source` as an embedded string blocks source maps and hides payloads (**F-21**) | Design | **3.5** (source maps in dev) + **5B** (registry builds it) |
| C-7 | No per-extension memory ceiling; only latency is bounded (**F-23**) | Doctrine | **Phase 4** |
| C-8 | Secrets have nowhere safe to live — when a `password` setting kind lands it must use the OS keychain, never settings JSON (**F-13**) | Design | **Phase 4** (level-3 settings) |
| C-9 | No transitive install is a UX property today; make it a tested security invariant (**F-6**) | Invariant | **Phase 5A, step 5** |
| C-10 | Grain's own updater is live and pinned-key — reuse its crypto shape rather than inventing one (**F-24**) | Reuse | **Phase 5A, step 1** |

---

## 9. Where this goes in the roadmap

**The order is forced by one fact:** Phase 3's acceptance test is only passable
by us, because nobody outside the team can run an extension at all. That is also
why C-1 sat undetected. Developer mode is therefore not a late convenience — it
is the instrument every later phase is verified with.

| Phase | Name | Ships | Depends on |
|---|---|---|---|
| 0–3 | *(done)* | Security wall, runtime, settings, surfaces | — |
| **3.5** | **Developer Mode & SDK** | CLI, load-unpacked, hot reload, developer panel, typed errors, docs, C-2/C-4/C-6 | nothing |
| 4 | Contract completion | `session:start`, native tier, pill action chips, level-3 settings, re-platforming built-ins, C-7/C-8 | 3.5 to author and test it |
| **5A** | **Trust rails (client)** | Keys, signed index verification, install/update/remove pipeline, revocation, C-3/C-5/C-9/C-10 | 3.5 |
| **5B** | **The registry** | `grain-extensions` repo, three-job CI, risk lanes, review dashboard, store UI, public site | 5A |
| 6 *(optional)* | Independent verification | Reproducible-build verifier, third-party rebuild attestation (F-Droid's model) | an ecosystem that justifies it |

**The gate's blocks resolve as:**

- Phase 4 **is unblocked**, with one condition: **a native-tier extension is
  loadable in developer mode but not distributable until 5A ships.** Trust rails
  land before anything that executes a binary can travel.
- Phase 5 proceeds as 5A → 5B.
- 5A and 5B can overlap once the key ceremony (5A.1) is done — the client
  verifier and the registry generator are written against the same fixture
  files.

---

## 10. Implementation guide

Each step states what to build and what "done" means. Steps are ordered; do not
reorder within a phase.

### Phase 3.5 — Developer Mode & SDK

**1. Harden the local channel (C-2).**
Validate `Origin` in the WebSocket handshake: accept Grain's own webview origins
and connections with no `Origin` header (non-browser clients — the CLI, the
pill); reject everything else. Cap concurrent unauthenticated connections.
*Done:* a unit test proves a browser-shaped `Origin` is refused and the pill's
origin is accepted; the app boots and the pill authenticates unchanged.

**2. `grain-ext init`.**
Scaffold: manifest, entry file, generated TypeScript types for the `grain` API,
README, `.gitignore`. Types generate from `grain-sdk` — one source of truth.
*Done:* a fresh scaffold passes `doctor` with zero findings.

**3. Load unpacked.**
Developer-mode toggle, folder picker, dev registry entries with the `dev` rung,
"overridden by dev extension" when a store version shares the id, persistent
developer-mode chip. Same capability wall; never promotable; never URL-triggered.
*Done:* a scaffolded extension loads from disk, appears in Overview badged
`dev`, and its capability denials behave identically to an installed one.

**4. `grain-ext dev` + hot reload.**
File watch → rebuild → dev-channel push → worker kill/respawn → surface remount.
Dev token in a 0600 file, created only while developer mode is on.
*Done:* an edit is live in under 300 ms without restarting Grain; RAM after ten
reloads equals RAM after one (no leaked workers — measure it).

**5. Source maps (C-6).**
Dev builds emit a source map; the host maps thrown stacks back to author files.
*Done:* a deliberate `throw` reports the author's file and line, not
`entry_source:1`.

**6. The developer panel.**
The six panes from §6.4, built only when developer mode is on, sleeping like any
surface, with "Copy diagnostics".
*Done:* an intentional capability denial appears within a second, naming the
capability and the manifest line; with developer mode off, no panel window
exists and idle RAM is unchanged (measure both).

**7. Typed errors.**
`E_CAPABILITY_DENIED`, `E_TIMEOUT`, `E_QUOTA`, `E_INVALID_MANIFEST`, … each with
`hint` and `docs`. Sweep the host API for silent empty returns.
*Done:* every host-API error path returns a typed code; a test asserts no path
returns a bare empty success on refusal.

**8. `grain-ext doctor` (C-4).**
Manifest validation, capability names, **invisible/bidi Unicode rejection**, size
caps, activation sanity, budget lint. The identical code CI will run.
*Done:* a manifest with a zero-width character in its source fails with the
offending file, line and codepoint.

**9. Author documentation.**
Quickstart, capability reference, three worked examples (a pack, a scripted
extension, a surface), and the debugging guide.
*Done:* someone outside the team builds and runs an extension **without asking a
question** — that is the acceptance test, and it is worth treating literally.

**10. Verify C-1 for real.**
With a dev extension that declares a workspace surface, confirm open, sleep,
revive and payload delivery, and that sleeping returns to baseline RAM.
*Done:* the Phase 3 surface handshake is proven end to end, closing the gap that
hid C-1.

### Phase 5A — Trust rails (client)

**1. Key ceremony (C-10).** Generate root keys offline (3, threshold 2), publish
the public keys, pin them in the app, sign the first `roots.json`. Write the
runbook — including recovery — before using the keys.
*Done:* a fixture `roots.json` verifies against the pinned key in a unit test.

**2. Index verification + the trust invariant (C-3).** Signature check, rollback
check, expiry handling, revocation application, and the four tests from §3.2.
*Done:* all four pass, including `a_pack_claiming_trust_installs_untrusted`.

**3. Store data path.** Seed index in the app, conditional refresh on store open
and at most daily, cached copy on failure with an "offline — last updated" state,
parsed index dropped on close.
*Done:* with the network unplugged the store renders from cache and refuses new
installs; idle RAM with the store closed is unchanged.

**4. Pack format v2 + safe extraction (C-5).** Write the traversal, symlink,
zip-bomb and ratio tests **first**, then the extractor.
*Done:* every malicious fixture is rejected; a good archive round-trips.

**5. Install / update / remove (C-9).** The transaction in §5.2, permission-diff
gating, previous-version retention, and a test that installing A never installs
anything else.
*Done:* install, update with new permissions, rollback and purge all behave as
specified, each covered by a test.

**6. Revocation UX.** Disable, red banner naming the reason, one-click removal,
data kept unless purged.
*Done:* a fixture revocation disables an installed extension on refresh.

### Phase 5B — The registry

**1.** Create `grain-extensions`; define `submission.toml`; write CONTRIBUTING
and the review policy (including what gets rejected, published openly).
**2.** The three CI jobs with the secret/egress boundary of §4.2 — **build the
isolation before the convenience** (**F-7**).
**3.** Risk scoring in `grain-sdk`, consumed by CI, the dashboard and the store
card, so the number a user sees is the number that routed the review.
**4.** Publish pipeline: build → hash → sign → upload to R2 → regenerate
`index.json` and `ext/<id>.json` → bump `version`, set `expires`.
**5.** The review dashboard of §4.3, risk-sorted, decisions as labels.
**6.** The fast lane of §3.4: lane routing, diff-only re-review, auto-verify for
zero-capability packs, `core` for our own CI-built extensions.
**7.** Store UI: fill the existing slide-over shell — search, cards, capability
sheet before first enable, trust badge, install/update/remove.
**8.** The public static site, generated from the same index.
**9.** Ship one real third-party extension end to end. Until that has happened,
none of this is verified.

---

## 11. What we are deliberately not building

Recorded so nobody re-opens them without a reason:

- **A backend service.** Static files and CI until traffic proves otherwise —
  the design migrates into a service without changing the client (**F-4**).
- **Accounts, payments, ratings, comments.** No moderation surface, no fraud
  surface, no obligations. Reconsider only with an ecosystem that demands it.
- **Runtime dependency installation.** No `npm install` on a user's machine,
  ever. Everything an extension needs is in its artifact (**F-6**, **F-7**).
- **One-click install from the web.** A link may open a store page; only an
  in-app click installs.
- **Full TUF or Sigstore, for now.** We take TUF's four properties directly; the
  transparency-log upgrade waits for a second signer (**F-11**).
- **Telemetry by default.** Grain is local-first; the store is a static GET.

---

## 12. Open questions for the user

Small, and none blocks starting Phase 3.5:

1. **Domain.** The index, the public site and the dashboard want a domain
   (`extensions.grain.app/v1/…` or similar). Which one?
2. **Root-key custody.** Three offline keys with a threshold of two is the
   recommendation; where they physically live is your call (hardware token,
   encrypted offline media, or both).
3. **Public site now or later?** It is nearly free and generated from the same
   data, but it can land after the in-app store without changing anything.
4. **Lane 1's audit rate.** What share of auto-published `experimental`
   extensions gets read later — and by whom, at what cadence.
