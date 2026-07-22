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

### 0.1 Operating assumptions — read these before judging any decision below

These are the project owner's stated expectations (2026-07-22), and several
decisions only make sense in their light:

- **Volume: 10–20 new extensions per month, for the first three to four months**,
  growing gradually. Grain's extensions are functional *and* visual, which is the
  slower-growing kind: Zen Browser, a large project, has ~77 mods in total.
- **Therefore: every extension is reviewed by a human. 100%.** No auto-publish
  rung. At this volume it is affordable, and it is a far stronger promise than
  any badge — *if it is in the store, a person read its source*.
- **One reviewer** (the project owner). This is the real capacity constraint, and
  the reason §3.4 exists at all.
- **Budget: zero.** Hosting must cost nothing until donations change that. The
  public site runs on a free Vercel deployment with its auto-assigned
  `*.vercel.app` domain; a purchased domain comes later.
- **Signing keys live on removable media** (an encrypted pen drive), sometimes
  on the owner's computer. No hardware security module, no key ceremony with
  multiple custodians. §2.2 is designed for *that*, not for an idealised setup.

**When to revisit:** if the review queue's median wait exceeds ~5 working days,
or submissions exceed ~40/month including updates, the `experimental` rung in
§3.1 gets switched on. It is built and left disabled precisely so that growing
past one reviewer is a policy change, not a redesign.

---

## 1. The decisions, on one page

| Question | Decision | Why (finding) |
|---|---|---|
| Where does the index live? | A new public GitHub repo, `grain-extensions`. Submissions are pull requests. | Obsidian, Raycast and Zed all do this and none runs a submission service (**F-1**) |
| What does the app talk to? | **Static signed JSON + artifacts on GitHub Releases** of that repo, never the git tree, and **never the website** | Git-as-database degrades; releases have no pausing bandwidth cap; signing makes the host untrusted anyway (**F-1**, **F-12**, §2.3) |
| Is there a public website? | Yes, on **Vercel** with its free `*.vercel.app` domain — but it is a *shop window only*. The app never depends on it. | Vercel's free tier hard-caps at 100 GB and then **pauses the deployment**; the in-app store must not be able to go offline (§2.3) |
| Who builds the artifact? | **We do.** The registry's CI builds from a pinned commit. Authors submit source, never bytes. | The only way "reviewed" and "installed" are provably the same thing (**F-2**) |
| How does the app know it is genuine? | Ed25519 signature over the index, **root public key pinned in the binary** — the same shape as Grain's own updater | Already proven in this codebase; no new crypto (**F-3**, **F-24**) |
| How is "verified" made unforgeable? | Trust exists **only** inside signed index metadata. No manifest field, no domain check, no author input. | Domain-ownership "verified" is theatre (**F-5**, **F-22**) |
| What stops review-then-swap? | Trust is **per version**, bound to `(id, version, sha256)`; we built the bytes ourselves | GlassWorm shipped clean, then updated dirty (**F-6**) |
| Who reviews? | **A human reads every extension, and every update.** No auto-publish, so no risk *score* — just a flagged-combination list that says how deep to read. | 10–20/month is one reviewer's day per week; "we read everything" beats any badge (§0.1, **F-14**) |
| How does that stay sustainable? | We build the bytes, so an update's review surface is a **source diff**, not a codebase | Diff-only re-review is the difference between 20 reviews and 20 re-reads (**F-2**, §3.4) |
| Where is our dashboard? | **GitHub's own PR UI**, plus one bot comment per submission carrying the review briefing | Zero infra, free auth, complete audit trail — and at ~20/month there is no queue to sort (§13) |
| What does hosting cost? | $0 — GitHub Releases and Actions are free for public repos; Vercel Hobby is free for the site | (**F-4**, §2.3) |
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
                            ├─ JOB 1  build + check (untrusted code)   │ verify with
                            │    NO secrets, egress blocked            │ PINNED root key
                            │    posts the review briefing comment      ▼
                            ├─ human review — EVERY submission    fetch blob/<sha256>
                            └─ JOB 2  sign + publish ──▶ Releases  verify hash → unpack
                                 on merge; runs no untrusted code   → atomic install
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
/v1/index.json               signed catalogue — EVERYTHING, including descriptions
/v1/index.json.minisig
/v1/roots.json               signed by the offline root key: publishing key + base URLs
/v1/roots.json.minisig
/v1/blob/<sha256>.grainpack  content-addressed artifacts (immutable, cache-forever)
/v1/revocations.json         kill switch
/v1/revocations.json.minisig
```

**One index file, not a sparse per-extension split** (simplified 2026-07-23,
§13). Twenty entries with full descriptions is a few tens of kilobytes — smaller
than one screenshot. Splitting into `ext/<id>.json` is a scale optimisation that
buys nothing below a few hundred extensions and costs a generator, a second
fetch path, and a cache story. **Split it when `index.json` passes ~500 KB**; the
client reads the same fields either way, so it is a generator change.

Paths are **relative**. Absolute bases come from `roots.json`, which is signed —
so moving hosts, or adding a mirror, is a signed metadata change rather than an
app update. The app ships a hard-coded bootstrap list as a last resort.

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
  "author": "example",               // the GitHub account that submitted it
  "reviewed_at": "2026-07-20",
  "reviewed_commit": "a1b2c3…",     // shown on the card: what a human actually read
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

Four edge cases that would otherwise be shipped bugs:

- **The seed index is exempt from expiry until the first successful refresh.**
  An app installed a year after its release has a long-expired seed; without this
  rule its store is dead on arrival.
- **Expiry allows generous clock skew** (24 h) and never hard-fails an install
  that is already underway. A wrong system clock must not brick the store.
- **An unknown `spec` major is not an error, it is a message**: "update Grain to
  browse the store". Forward compatibility is what lets the index evolve.
- **`min_grain_api` above the running app** shows the card greyed with "needs
  Grain *x.y*" rather than hiding it — a missing extension is a support ticket, a
  labelled one is not.

### 2.2 Keys — designed for a pen drive, because that is the truth

| Key | Where it lives | What it signs | How often it is used |
|---|---|---|---|
| **Root A** (Ed25519, passphrase-encrypted) | Encrypted removable media. **Never on a build machine, never in CI.** | `roots.json` only | Almost never — publishing-key rotation |
| **Root B** (the spare) | A *second* piece of media, stored separately | The same, if A is lost | Ideally never |
| **Publishing** (Ed25519) | A GitHub Actions secret, used *only* in the signing job | `index.json`, `revocations.json`, artifacts | Every publish |

**Both root public keys are pinned in the app from day one.** That is the whole
recovery story: losing one root key is an inconvenience, not a re-release.
Minisign secret keys are passphrase-encrypted by default, so "encrypted pen
drive + strong passphrase" is a coherent custody model for one maintainer — the
thing to get right is that the **passphrase is written down somewhere the drive
is not**.

Two levels, ~150 lines of verification code, and the TUF properties that matter
without operating a TUF repository (**F-11**). Multi-custodian thresholds and
hardware tokens are the right answer for a team; for one person they add a
failure mode (lost quorum) without removing one.

**What this protects against, stated honestly:**

| Threat | Covered? |
|---|---|
| Someone tampering with the index or an artifact in transit or at rest | **Yes** — signature + hash, verified before use |
| A stolen GitHub Actions publishing key | **Yes** — sign a new `roots.json` with a root key; no app update |
| An author faking `verified` | **Yes** — they never touch signed metadata (§3.2) |
| The owner's machine compromised *while the root drive is plugged in and unlocked* | **No.** Mitigation is behavioural: plug it in only to rotate, which is a once-a-year event |
| Both root drives lost **and** the passphrase forgotten | **No** — recovery is an app release with a new pinned key. Documented, not catastrophic |

> **Bootstrapping note.** Until the root keys exist, everything downstream is
> theatre. Generating them is **Step 5A.1**, before any code that verifies them.

### 2.3 Where the files actually live — and why not on the website

| What | Host | Why |
|---|---|---|
| `index.json`, `roots.json`, `revocations.json`, artifacts | **GitHub Releases** on the `grain-extensions` repo | Free, built for binary distribution, no bandwidth cap that can pause, permanent URLs, and it is already where the source of truth lives |
| The public website | **Vercel**, free tier, `*.vercel.app` | Zero cost, instant deploys, and it is generated from the same index |

**The app must never fetch anything from the website.** Vercel's Hobby tier is
capped at 100 GB/month and, on hitting the cap, **pauses the deployment** rather
than throttling — the site simply goes offline. That is an acceptable failure for
a shop window and an unacceptable one for the store inside a user's app. It is
also non-commercial-use-only, which is fine for a donation-funded project but is
one more reason not to build a dependency on it.

Because every file is **signed and content-addressed, the host is untrusted by
construction**. That makes this reversible: adding a mirror, or moving to a
purchased domain, or moving to R2 later, is a `roots.json` change. Nothing in the
client cares where bytes came from — only that they verify.

**Publishing the site early: yes.** It costs nothing, it gives authors somewhere
to read the rules before submitting, and shipping it before the in-app store
means the first real submission arrives with documentation already public. The
only thing to avoid is putting an "Install" button on it that does anything other
than open a page in Grain (§5.2).

---

## 3. Trust: the ladder and the guarantee

### 3.1 The ladder

| Rung | What it means | How you get there | What it changes |
|---|---|---|---|
| `dev` | Loaded from a local folder by a human, in developer mode | Load unpacked | Permanent badge; never in the store; cannot be promoted; capability wall identical |
| `verified` | **A human read this exact version's source.** | Every accepted submission and every accepted update | The store's baseline. Shown as "reviewed on *date*, at commit *sha*" |
| `core` | Written and maintained by us | Built from the `grain` repo by our own CI | May pre-install; may claim core slots |
| `experimental` | Automation passed; **no human read it** | **Currently unreachable — the policy is off** (§0.1) | Reserved so that outgrowing one reviewer is a config change, not a redesign |

**At launch the store has exactly one promise, and it is a strong one: *if it is
listed, a person read its source*.** That is deliberately simpler than a ladder.
The badge therefore does not distinguish between listed extensions — what varies
per card is *evidence*: the review date, the reviewed commit, the capability
list, and any flagged combination. Age is shown as a fact ("first published *date*"), never
as trust.

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

**Every submission is read by a human** (§0.1). Since nothing is ever
auto-published, a *numeric* score has no routing job left to do — so it is cut
(2026-07-23, §13). What remains is a lookup, not a scoring system:

| Class | Rule | The read | Target |
|---|---|---|---|
| **No capabilities** | tier `pack`, `permissions: []` | Payload sanity; there is no code | same day |
| **Ordinary** | anything else | Full source read, focused on the declared capabilities | 2–3 days |
| **Flagged** | one of the combinations below | Full read **plus** a written justification from the author for each flagged capability, and a runtime observation in developer mode | up to a week, and it may be refused |

**The flagged combinations** — a `const` list in `grain-sdk`, checked by CI and
shown in the bot comment: `screen:capture` + `net:*` (already the SPEC's rule) ·
`events:transcripts` + `net:*` · any `native` tier with `net:*`. Each says "this
extension can see something private *and* can send it somewhere", which is the
only question a risk number was ever really answering.

Automation does not replace the reviewer; it does the mechanical parts (lint,
Unicode scan, capability diff, build, hash) so the human spends their time
reading logic. **Reinstate the numeric score** if the `experimental` rung is ever
switched on — a number is only needed when something publishes without a human.

**Published expectations.** The review policy page states the target turnaround
per band and says plainly that there is one reviewer. If the queue stalls —
illness, travel — submissions go to a **paused** state with a visible banner
rather than silently ageing. One reviewer is a real bus factor; the mitigation is
honesty about latency, not a promise we cannot keep.

### 3.4 How one reviewer survives — and how authors get verified *quickly*

Reviewing 100% only works if each review is small. **The real load is not the
20 new extensions a month — it is their updates.** Twenty extensions each
shipping monthly is another twenty reviews, and that is the number that grows
without bound. Four mechanics keep it flat, none of which rushes the human:

1. **We built the bytes.** There is no "did this binary come from that source"
   question to answer, because there is no upload (**F-2**). That question is
   otherwise the most tedious part of reviewing an update.
2. **Diff-only re-review — the load-bearing one.** Both versions were built from
   pinned commits, so an update's review surface is the *source diff*. Identical
   capability set + all-green checks → **the reviewer reads a diff, not a
   codebase**, and the extension keeps its rung. Any capability change, or a diff
   past the threshold, means a full read again. A mature extension's monthly
   update becomes a five-minute job instead of an hour.
3. **`grain-ext doctor` runs the exact CI suite locally.** Same code, same
   version, same output. Submissions then pass first time, and *round-trips* —
   not review itself — are what make verification feel slow.
4. **Automation does the mechanical reading.** Lint, invisible-Unicode scan,
   capability diff, size, build, hash, typosquat check — all presented as a
   summary at the top of the review page, so the human opens the source already
   knowing where to look.

**For our own extensions:** anything built from the `grain` repo by our own CI is
`core` by construction — same signing job, no queue. Not a shortcut in the trust
model; the same rule applied to a publisher who happens to be us.

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

### 4.2 CI: two jobs, and only one of them holds a key

**Two jobs, and the split between them is not negotiable** (simplified from
three, 2026-07-23 — the lint/check work folds into the first job because both
are secretless; the *boundary* stays exactly where it was):

| Job | Runs | Holds | Network | Trigger |
|---|---|---|---|---|
| **build-and-check** | The author's build — untrusted code, `npm ci` and its transitive install scripts — then lint, manifest validation, Unicode scan, size caps, capability diff | **nothing** | egress **blocked** | on the PR |
| **publish** | Hash, sign, publish the release, regenerate the index | the publishing key | upload only | **on merge only** |

> ### Why this cannot become one workflow
>
> It is tempting — one "merge → publish" job is fewer lines. It is also the
> **exact vulnerability that took over Open VSX in 2025**: their nightly workflow
> ran `npm install` over untrusted extension code *in a job that held
> `OVSX_PAT`*, a token able to publish or overwrite any extension in the
> registry. `npm install` executes arbitrary build scripts — from the extension
> and from every one of its dependencies — so any of them could read the token.
> Disclosed 4 May 2025, six rounds of fixes, patched 25 June, ~8 million
> developers exposed (**F-7**).
>
> Because we build authors' code ourselves (§2, and it is what makes "reviewed"
> mean anything), **we are exactly the kind of registry that attack targets.**
>
> The cost of keeping the boundary is roughly ten lines of YAML: two jobs, and
> `secrets` referenced in only the second. That is not over-engineering — it is
> the cheapest security property in this entire plan, and the only one whose
> absence has a body count.

Everything else about the pipeline is fair game to simplify. This is not.

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

**GitHub *is* the dashboard.** No separate web app (simplified 2026-07-23, §13).
PRs are submissions, labels are state, checks are evidence, comments are the
audit trail — and at ~20 submissions a month there is no queue to sort, so the
one thing a custom dashboard would have added over GitHub's own UI is the thing
scale hasn't demanded yet.

What replaces it is **one bot comment per pull request**, posted by the check job
and updated in place — the review briefing, where the review already happens:

- **capability diff** against the previous version (the single most important
  line: "this update newly requests `net:`")
- lint findings, invisible-Unicode scan, size, typosquat check
- build status and the artifact hash we produced
- source diff link, and for an update the **diff-only** verdict (§3.4)
- any forced-human flag (`screen:capture` + `net:` and friends)

Decisions stay labels (`decision:verified`, `hold:security`, `decision:reject`)
executed by a workflow on merge, so the audit trail is intact and free.

**Add a real dashboard when** GitHub's filters stop being enough — realistically
past ~40 open submissions. It is a generator over the same labels, so nothing
built now is wasted.

**Why it is not inside Grain either:** nobody but us would ever open it, and it
would ship in every user's binary — the "destroy if not in use" rule.

**One thing the dashboard must show that is easy to forget: the revocation
button.** Finding out an extension is malicious is the moment the whole system is
tested, and it must not be the moment someone reads a runbook for the first time.
Publish a security contact address next to the review policy, and rehearse a
revocation against a fixture before the store opens (5A step 6, 5B step 5).

---

## 5. Install, update, remove

### 5.1 Pack format v2

One extension, one `.grainpack` file, two physical shapes detected by the first
byte:

| Shape | First byte | Contents | Used by |
|---|---|---|---|
| JSON | `{` | manifest with embedded payloads (today's format, unchanged) | tier `pack` |
| ZIP | `PK` | `manifest.json`, `entry.js`, assets, per-platform binaries | tiers `scripted`, `native` |

**One hash over the whole artifact, not a per-file manifest** (simplified
2026-07-23, §13). VS Code hashes every file inside the package because *authors*
upload it and tampering wants localising (**F-3**); **we build the artifact
ourselves**, so the archive hash in the signed index already binds every byte to
what we produced. Per-file hashes would add a format, a generator and a verifier
to re-answer a question we already answered.

What is **not** cut is path-safe extraction — that is a CVE class, not a scale
concern (§5.2).

### 5.2 The install transaction

```
fetch /v1/blob/<sha256>            content-addressed; cache-forever; resumable
  ↓ verify sha256 == index entry   (index itself already signature-verified)
  ↓ unpack to <appdata>/extensions/.staging/<id>-<version>/
  ↓   PATH-SAFE extraction, see below
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

- the index is fetched **on store open**, and **piggybacked on Grain's existing
  update check** — never on its own timer, never on the transcription path.
  (The dedicated daily background refresh was cut 2026-07-23, §13. Grain already
  runs an update check against its own releases; the revocation list rides along
  with it, so revocation reach is preserved without adding a scheduler or a
  second phone-home. If that check is not currently automatic, wire the
  revocation fetch to it when it becomes so.)
- **revocations are also enforced from cache at enable time** — before a worker
  is ever spawned. Free, and it means a revoked extension cannot run again even
  if the machine has been offline since the revocation was published.
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
sleeping like every other surface.

**One chronological event stream, not a dashboard** (simplified 2026-07-23,
§13). A developer chasing a bug reads a timeline; six panes is more code and a
worse debugger. Entry kinds: `log` (the author's output) · `error`
(source-mapped stack) · `denied` (**red**, naming the capability, the refused
call, and the manifest line to add) · `call` (`storage.get → ok (3 ms)`) ·
`slow` (`transform took 187 ms, budget 150 ms — strike 1 of 3`) · `life`
(spawn / reap / reload / activation).

Filter chips **All · Calls · Denials · Errors**, and one **"Copy diagnostics"**
button producing a paste-able report — the thing that turns a bug report into a
fix.

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
| Capabilities + any flagged combination | Manifest | Yes — the honest signal | No |
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
| **5B** | **The registry** | `grain-extensions` repo, two-job CI, bot review comment, store UI, public site | 5A |
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

> **Build from [PHASE3.5-GUIDE.md](PHASE3.5-GUIDE.md)**, which expands these ten
> steps with the repo's file paths, build commands and gotchas, and is written to
> be followed with no prior knowledge of the project. The summary below is the
> shape; the guide is the instruction.

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

**1. Key ceremony (C-10).** Generate **two** root keys, passphrase-encrypted, on
media that is not a build machine; store them separately; pin **both** public
keys in the app; sign the first `roots.json` (publishing key + base URLs + mirror
list). Write the runbook — rotation *and* recovery — before the keys are used in
anger.
*Done:* a fixture `roots.json` verifies against either pinned key in a unit test,
and the runbook has been followed once end to end on throwaway keys.

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
**3.** The flagged-combination list in `grain-sdk`, consumed by CI and shown on
the store card, so what a user sees is what the reviewer was warned about.
**4.** Publish pipeline: build → hash → sign → publish to a GitHub Release →
regenerate `index.json` → bump `version`, set `expires`.
**5.** The bot review comment of §4.3, decisions as labels, and **revocation
rehearsed once against a fixture before the store opens**.
**6.** The sustainability mechanics of §3.4: diff-only re-review, the automated
summary at the top of each review page, `core` for our own CI-built extensions.
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

## 12. Questions that were open, and how they were answered

Settled by the project owner on 2026-07-22. Recorded here so the reasoning is not
lost, and so a future reader can tell a decision from an assumption.

1. **Scale → 10–20 extensions/month for the first few months.** Everything in
   §0.1 follows from this, most of all the choice to review 100%.
2. **Review coverage → every extension, every update, read by a human.** The
   `experimental` rung stays built but switched off (§3.1), so growth is a policy
   change rather than a redesign.
3. **Domain → none yet.** A free `*.vercel.app` deployment until donations fund a
   purchased one. This is why absolute base URLs live in signed `roots.json`
   (§2.1) and why nothing the app depends on is served by the website (§2.3) —
   the eventual move costs one signed file, not an app release.
4. **Key custody → encrypted removable media, one maintainer.** §2.2 is written
   for exactly that, including two pinned root keys so losing one drive is
   recoverable, and an explicit statement of what it does *not* protect against.
5. **Publish the site early → yes.** It costs nothing, and it puts the rules and
   the review policy in public before the first submission arrives.

**Still genuinely open** (neither blocks Phase 3.5):

- The `id` namespace convention for first-party extensions once `grain.*` is
  reserved — cosmetic, decide when 5B starts.
- Whether install counts are collected at all. §7 says they may never carry
  authority; not collecting them at launch is the simpler and more private
  default, and nothing breaks if we never add them.

---

## 13. Deliberate simplifications for low volume

Decided 2026-07-23, mid-Phase-3.5. The plan was written against the *shape* of
the problem; this pass cuts the parts that were sized for a scale we do not have
(§0.1: 10–20 extensions/month, one reviewer). **Each cut names what brings it
back**, so a future maintainer can tell a deferral from an oversight.

| Cut | Was | Now | Reinstate when |
|---|---|---|---|
| **Developer panel** | Six live panes (activity, calls, denials, budget percentiles, memory) | **One chronological event stream** + four filter chips | Never, probably — a timeline is the better debugger. Add a memory entry when the C-7 ceiling lands |
| **Review dashboard** | A generated, access-restricted static web app with a risk-sorted queue | **GitHub's own PR UI** + one bot comment carrying the review briefing | GitHub's filters stop coping — realistically past ~40 open submissions |
| **CI jobs** | Three (build / check / sign) | **Two** — check folds into build; both are secretless | Never. The 2-job split is a boundary, not a structure (§4.2) |
| **Risk score** | Weighted table, numeric score, three routing lanes | **A flagged-combination lookup**; everything is human-reviewed anyway | The `experimental` rung is switched on — a number is only needed when something publishes without a human |
| **Index layout** | `index.json` + per-extension `ext/<id>.json` (sparse) | **One `index.json` with everything** | `index.json` passes ~500 KB |
| **Pack integrity** | Per-file `files.json` SHA-256 manifest | **One hash over the artifact** — we built it, the signed index binds it | Authors ever upload artifacts (they should not) |
| **Store refresh** | Dedicated daily background timer | **On store open**, plus riding Grain's existing update check | A revocation ever needs to land faster than a user opens the store |
| **`grain-ext submit`** | A GitHub-authenticated PR-opening flow | **Print the pre-filled URL and open a browser** (~5 lines) | Submissions get frequent enough that the round-trip annoys |
| **Phase 6** | Reproducible-build verification server | **Not built** | An ecosystem large enough that someone other than us wants to verify our builds |

**What was explicitly NOT cut, and why**, since these are the tempting ones:

- **The two-job CI boundary** — §4.2. Collapsing it re-creates the exact bug that
  compromised Open VSX, and we are the kind of registry that attack targets
  because we build authors' code ourselves.
- **Signature + rollback + expiry checks** — about twenty lines of Rust, and the
  reason revocation can ever reach a user (**F-11**).
- **Path-safe archive extraction** — a CVE class (Zed shipped one at CVSS 7.4),
  not a scale concern (**F-8**).
- **Invisible/bidi Unicode rejection** — the technique GlassWorm used to hide
  payloads from reviewers. A regex and a test (**F-6**).
- **Trust bound to `(id, version, sha256)`** — a struct field and a test. Without
  it, reviewing v1.0 blesses v2.0 (**F-6**).
- **Two pinned root keys** — generating a second key at the same ceremony is
  free; needing one later is not.

The pattern: **cut anything sized for volume; keep everything sized for an
adversary.** Volume we do not have yet. An adversary needs one user.

---
