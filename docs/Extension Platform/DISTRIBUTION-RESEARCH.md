# Distribution + Developer Mode — the research

**Read this before [DISTRIBUTION-PLAN.md](DISTRIBUTION-PLAN.md).** This document
is the *evidence*; the plan is the *decisions*. Every design choice in the plan
points back to a numbered finding here, so a reader can check the reasoning
instead of trusting it.

---

## 0. For a reader with no context

**Grain** is a local, low-RAM speech-to-text desktop app (Rust/Tauri backend,
React frontend, plus a native Rust pill window). Its **extension platform** lets
third parties add features without forking Grain. An extension is one of three
tiers: **pack** (data only — prompts, snippets, a pill theme; no code),
**scripted** (JavaScript in an isolated Web Worker), or **native** (a small
separate program the host starts and stops). Extensions declare capabilities in
a manifest; a Rust-side wall filters every message by those capabilities.

Phases 0–3 are built and shipped: the runtime, the security wall, settings,
extension-owned windows. **Two things were never designed:** where extensions
*live* (a store, with hosting, submission, review, and a trust signal that can't
be faked), and how anyone *builds* one (a developer mode with a real
build → run → debug loop). This research covers both.

### How to read it

- **F-n** = a finding. Each ends with **⇒ the rule it forces on Grain.**
- Confidence is stated inline. "Documented" = the vendor's own docs or spec.
  "Reported" = security research or press. "Inferred" = my reasoning from the
  evidence, and labelled as such.
- Sources are linked at the point of use and collected at the end.

---

## Part 1 — Prior art: how comparable platforms actually do it

Seven systems, chosen because each one solves a piece of Grain's problem. The
comparison that matters is the last column: **what it costs to run.**

| System | Where the index lives | How you publish | Who reviews | Artifact origin | Ops burden |
|---|---|---|---|---|---|
| **Obsidian** | `community-plugins.json` in a GitHub repo | PR adding one JSON entry | Team merges; bots pre-check | **Author's GitHub Release** | Near zero |
| **Raycast** | Monorepo of all extensions | PR adding your source | Team + CI checks | **Registry CI builds it** | Low |
| **Zed** | `extensions.toml` + git submodules | PR adding a submodule + version | CI validates; team merges | **Registry CI builds → S3** | Low |
| **VS Code** | Azure-hosted service + API | `vsce publish` with a token | Automated + targeted manual | Author uploads `.vsix`; **Marketplace signs it** | High (a service) |
| **Open VSX** | Java service + DB (Eclipse) | `ovsx publish` token, or auto-publish CI | Mostly automated | Author uploads | High (a service) |
| **Firefox AMO** | Mozilla service | Web upload or CLI | **Automated first, manual on risk** | Author uploads; **AMO signs, signing is mandatory** | High |
| **F-Droid** | Signed static repo | Metadata PR (recipe, not binary) | Build recipe review | **F-Droid builds from source** | Medium |

### F-1. A GitHub repo *is* a perfectly good registry at our scale — with one caveat

Obsidian, Raycast and Zed all use "the index is a file in a git repo, submission
is a pull request". None of them run a submission service. Obsidian's flow is
literally: append an entry to
[`community-plugins.json`](https://github.com/obsidianmd/obsidian-releases), open
a PR, a team member merges. Zed's is `extensions.toml` plus a git submodule, with
CI validating format, IDs, versioning and licence before a maintainer merges
([Zed docs](https://zed.dev/docs/extensions/developing-extensions)). Raycast's is
a monorepo PR with automated manifest/lint/asset checks, then human review, with
a stated first-response target of five business days
([Raycast docs](https://developers.raycast.com/basics/publish-an-extension)).

The caveat, and it is not hypothetical: git-as-a-database degrades. Cargo's
registry index was a git repo until the clone cost became the dominant part of a
build — 176 MiB of metadata, a `git clone` reporting 215 MiB transferred, "over
20 times more than a compressed tarball", worst in stateless CI. The fix
([RFC 2789, sparse index](https://rust-lang.github.io/rfcs/2789-sparse-index.html))
was to stop making clients clone a database and instead serve **plain static
files over HTTP**, fetching only what a client actually needs, which is also
"much simpler to cache on a CDN".

⇒ **Rule: git repo as the *source of truth* and submission surface; static
generated files on a CDN as the *client-facing* index. Never make the app clone
a repo.**

### F-2. Who builds the artifact is the single most consequential choice

Three models, in ascending order of what they guarantee:

1. **Author uploads a binary** (VS Code, Open VSX, AMO, Chrome). The registry
   reviews and signs whatever arrives. What was reviewed and what ships are the
   same *bytes*, but nobody outside the author knows those bytes correspond to
   the published source.
2. **Author's release is fetched** (Obsidian). The registry stores only a pointer;
   the author can change the release later.
3. **Registry builds from pinned source** (Raycast, Zed, F-Droid). The author
   never hands over a binary at all.

F-Droid takes model 3 furthest: it builds every app from source, and runs an
independent **verification server** that rebuilds published apps and compares
artifacts, publishing reproducibility logs — the explicit goal being to "catch
malware inserted in the build process, rather than the source code"
([F-Droid: Reproducible Builds](https://f-droid.org/docs/Reproducible_Builds/),
[Security Model](https://f-droid.org/en/docs/Security_Model/)). F-Droid is honest
about the limits: many apps are still not reproducible, and its main repo signs
with its own keys, which is a trust-on-first-use relationship with F-Droid
itself.

⇒ **Rule: Grain's registry builds the artifact from a pinned commit. Authors
submit source, never bytes.** This is what makes "reviewed" and "installed"
provably the same thing, and it is the precondition for the fast lane in F-15.

### F-3. Mandatory signing at the client is the norm, and it is cheap

VS Code's Marketplace signs every extension on publish and the editor **verifies
that signature at install time by default**, refusing to install on failure
([VS Code docs](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace),
[MS blog](https://developer.microsoft.com/blog/security-and-trust-in-visual-studio-marketplace/)).
The signature manifest is a JSON file carrying the size and SHA-256 of *every
file inside* the package — not one hash of the archive. Firefox goes further:
add-ons cannot be installed into release Firefox at all unless AMO has signed
them ([Extension Workshop](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/)).

Grain already ships this exact primitive. `src-tauri/tauri.conf.json` configures
the Tauri updater with a **minisign public key baked into the binary** — Ed25519,
detached signature, key pinned at compile time
([minisign](https://jedisct1.github.io/minisign/)).

⇒ **Rule: reuse the shape we already trust for app updates — Ed25519, pinned
public key, detached signature — for the extension index and artifacts. No new
cryptography, no new dependency class, no PKI to operate.**

### F-4. Serving this costs approximately nothing

Cloudflare R2 charges **$0/GB egress**, with a free tier of 10 GB storage, 1M
class-A and 10M class-B operations per month; Pages serves static sites with
unlimited bandwidth ([R2](https://www.cloudflare.com/products/r2/),
[developer platform plans](https://www.cloudflare.com/plans/developer-platform/)).
Combined with F-1 (static files, CDN-cacheable), the entire client-facing surface
is a bucket. For comparison, Open VSX — a *service* — now absorbs 300M downloads
a month and peaks past 200M requests a day, which is precisely why it needs an
organisation behind it
([Eclipse](https://newsroom.eclipse.org/news/announcements/eclipse-foundation-launches-open-vsx-managed-registry-0)).

⇒ **Rule: no server. A bucket, a CDN, and GitHub Actions. If we ever need a
service, we will have the traffic to justify it — and the static design migrates
into a service without changing the client.**

---

## Part 2 — What actually goes wrong (2023 → 2026)

This is the part that must not be hand-waved. Each incident is a specific attack
someone actually ran, and each one has a specific defence.

### F-5. "Verified" that means "owns a domain" means nothing

Aqua Security demonstrated that the VS Code Marketplace's verified check mark
"merely means that whoever the publisher is has proven ownership of a domain" —
and a publisher can buy any domain to get it. Worse, extension *names* and
publisher display details need not be unique, so an attacker can mirror a real
extension's name, and even replicate the linked GitHub project's commit times,
PRs and issues to look authentic
([Aqua](https://www.aquasec.com/blog/can-you-trust-your-vscode-extensions/); see
also [OX Security](https://www.ox.security/blog/can-you-trust-that-verified-symbol-exploiting-ide-extensions-is-easier-than-it-should-be/)).

⇒ **Rule: "verified" in Grain must mean *a human reviewed this exact version's
source*, and it must live only in metadata we sign. It is never a field an
author can put in a manifest, and never derived from domain ownership.**

### F-6. Clean at review, malicious at update — GlassWorm

The GlassWorm campaign (first flagged October 2025, still active through March
2026) published extensions that passed marketplace checks, then updated to pull
in a *separate* extension carrying the loader; the editor auto-installs
referenced extensions, so the payload arrives on update. It hid code using
**invisible Unicode characters** so that reviewers and editors literally do not
render the malicious source, and used blockchain-hosted C2 to survive takedowns.
72+ additional malicious Open VSX extensions were found from January 2026 alone;
400+ components across npm, VS Code Marketplace, Open VSX and GitHub were
compromised between roughly November 2025 and March 2026
([The Hacker News](https://thehackernews.com/2026/03/glassworm-supply-chain-attack-abuses-72.html),
[Truesec](https://www.truesec.com/hub/blog/glassworm-self-propagating-vscode-extension),
[Socket](https://socket.dev/blog/glasswasm-malware-open-vsx-extensions)).

⇒ **Three rules.** (a) **Trust is per-version**, never per-extension — a new
version is untrusted until its own review. (b) **No transitive install, ever**:
installing extension A must never cause extension B to be installed. Grain's
`provides:`/`requires:` design already fails an absent provider with a
"nothing provides this" message rather than fetching one — that is now a
security invariant with a test, not a UX choice. (c) **Reject invisible and
bidirectional Unicode** in any submitted source, at the CI gate *and* at import.

### F-7. The publishing pipeline is a bigger target than the extensions

Koi Security found that Open VSX's nightly auto-publish workflow ran `npm install`
over untrusted extension code **in a job that held `OVSX_PAT`** — a token able to
publish or overwrite *any* extension in the registry. `npm install` runs arbitrary
build scripts, of the extension and of every dependency, so any of them could
read the token. Disclosed 4 May 2025; six rounds of fixes; patched 25 June 2025;
~8M developers exposed
([Koi](https://www.koi.ai/blog/marketplace-takeover-how-we-couldve-taken-over-every-developer-using-a-vscode-fork-putting-millions-at-risk),
[The Hacker News](https://thehackernews.com/2025/06/critical-open-vsx-registry-flaw-exposes.html)).

⇒ **Rule (architectural, not a checklist item): the job that executes untrusted
code holds no credentials and has no network egress; the job that signs never
executes untrusted code. They communicate only by artifact hand-off.** F-2 makes
us a builder of untrusted code, so this is *load-bearing* for us specifically.

The industry's answer to the underlying problem — long-lived publish tokens
sitting in CI — is **trusted publishing**: short-lived OIDC credentials, no
stored token. PyPI shipped it in 2023; npm made it generally available on
31 July 2025 and auto-generates provenance attestations when you use it; it is an
OpenSSF standard also adopted by RubyGems
([GitHub changelog](https://github.blog/changelog/2025-07-31-npm-trusted-publishing-with-oidc-is-generally-available/),
[npm docs](https://docs.npmjs.com/trusted-publishers/)).

### F-8. Unpacking an archive is a vulnerability class

Zed shipped a **Zip Slip path traversal** (CVE-class CWE-22, CVSS 7.4, fixed in
v0.224.4): `extract_zip()` joined the destination path with the raw entry
filename, so an archive entry named `../escaped.txt` wrote outside the extension
directory — enough to tamper with configuration or plant files, defeating
extension isolation entirely
([GHSA-v385-xh3h-rrfr](https://github.com/zed-industries/zed/security/advisories/GHSA-v385-xh3h-rrfr)).
The fix: reject entries containing `..` or a leading slash, then canonicalise and
confirm the result is still inside the destination.

⇒ **Rule: Grain's multi-file pack format does not exist yet, which means we get
to write the extraction test suite *before* the extractor.** Reject `..`,
absolute paths and symlinks; canonicalise and re-check containment; cap entry
count, per-entry size and total size (zip bombs); extract to a temp directory and
rename into place only after verification.

### F-9. Download counts are a lie you can buy

Tenable documented **download pumping**: attackers published 700+ versions of one
package; mirrors, scanners and analysis bots downloaded each one automatically,
inflating the count to ~50,000 in three days with zero real users. npm's own
position, stated long ago, is that download stats do not consider source (IP,
user agent) — every download counts equally
([Tenable](https://www.tenable.com/blog/how-cyberattackers-inflate-malicious-package-npm-download-counts),
[ReversingLabs](https://www.reversinglabs.com/blog/download-pumping-trust-abuse),
[npm blog](https://blog.npmjs.org/post/92574016600/numeric-precision-matters-how-npm-download-counts-work.html)).
Aqua separately noted that download and star counts can simply be bought.

⇒ **Rule: install counts and stars may be *displayed*, but must never rank
results, gate trust, or appear next to the trust badge as if they were evidence.
Ship the honest signals first: reviewed-at date, capability list, size, last
update.**

### F-10. Developer mode is a social-engineering surface

The consistent finding across browser-extension threat reporting is that
sideloading through developer mode bypasses store review, and that in current
browsers, attacks increasingly work by **talking a user into enabling developer
mode** — no store interaction needed at all. Enterprise guidance is to disable
developer-mode loading on managed devices
([Island](https://island.io/browser-extension-security/browser-extension-security-defending-against-installation-behavior-patterns),
[Chrome Web Store review process](https://developer.chrome.com/docs/webstore/review-process)).

⇒ **Rule: developer mode is opt-in, obvious while it is on, and can never be
triggered by a link, a download, or an extension. Only an in-app folder picker
driven by a human. A dev-loaded extension gets the *same* capability wall and a
permanent badge, and can never display as verified.**

---

## Part 3 — The building blocks worth stealing

### F-11. TUF's four properties, without adopting TUF

The Update Framework defends a set of attacks that a naive "download index.json"
client is wide open to. From the [TUF security docs](https://theupdateframework.io/docs/security/)
and [specification](https://theupdateframework.github.io/specification/latest/):

| Attack | What it means | Defence |
|---|---|---|
| **Rollback** | Serving older metadata than the client already saw, to reintroduce a fixed vulnerability | Metadata carries a monotonic version; a client rejects any version lower than its trusted copy |
| **Indefinite freeze** | Serving the same valid-but-stale metadata forever, so the client never learns about fixes or revocations | Metadata carries an expiry; past it, the client refuses to treat it as fresh |
| **Mix-and-match** | Assembling a combination of package versions that never coexisted in the repository | One signed snapshot pins the whole set together |
| **Arbitrary software** | Serving anything at all | Metadata must be signed by a threshold of keys named in the trusted root |

Key compromise is handled by role separation with offline, thresholded root keys;
PyPI's [PEP 458](https://peps.python.org/pep-0458/) is the canonical worked
example. Practical key advice for a very small team: generate 3–5 offline keys
with a signing threshold of 2, on hardware where the private key cannot be
extracted ([offline PKI with YubiKeys](https://vincent.bernat.ch/en/blog/2025-offline-pki-yubikeys),
[YubiKey guide](https://github.com/drduh/YubiKey-Guide)).

⇒ **Rule: implement the four properties directly — signed index with a monotonic
`version` and an `expires`, one signature covering the whole index, root public
key pinned in the app. That is ~150 lines of Rust and gets the properties
without operating a TUF repository.** Full TUF, or [Sigstore](https://docs.sigstore.dev/quickstart/quickstart-cosign/)
with its transparency log, is the upgrade path if Grain ever has more than one
signer.

### F-12. Sparse static index — the shape that scales down as well as up

From F-1: serve small static JSON, let the CDN cache it, fetch only what is
needed. For an in-app store this also answers the offline question directly — a
static index can be **shipped inside the app** as a seed and refreshed with a
conditional GET; failure means "show the cached copy with a date", not an error
screen.

### F-13. One-click desktop install has a modern reference

Anthropic's Desktop Extensions (`.dxt`, now `.mcpb`) are ZIP bundles with a
`manifest.json` declaring metadata, runtime and **user configuration** — the app
renders a config UI, validates entries before enabling, and **stores sensitive
values in the OS keychain**, injecting them at runtime via `${user_config.api_key}`
template literals. Enterprise controls include pre-installing approved
extensions, blocklisting extensions or publishers, and disabling the directory
entirely. The developer tooling is two commands: `mcpb init` (interactive
manifest) and `mcpb pack` (bundle + validate)
([Anthropic](https://www.anthropic.com/engineering/desktop-extensions),
[mcpb](https://github.com/modelcontextprotocol/mcpb)).

⇒ **Rules.** Grain's settings schema has no secret kind yet — when one lands it
goes to the **OS keychain**, never to the settings JSON. And an author-facing CLI
should be two obvious verbs before it is anything clever.

### F-14. Review scales only if the queue is risk-sorted and mostly automatic

AMO's reviewer tools sort the queue **by weight — that is, by risk** — with the
riskiest at the top; a linter runs on upload; low-risk add-ons are auto-approved
and merely *listed* for reviewers to sample; anything can be escalated to an
admin queue ([Mozilla wiki](https://wiki.mozilla.org/AMO/Reviewers/Guide)).
Chrome runs everything through automated review and pulls in humans when
sensitive permissions are involved, with code-only updates typically landing in
1–3 days ([Chrome](https://developer.chrome.com/docs/webstore/review-process)).
Mozilla's Recommended badge inverts the trade deliberately: every new version of
a Recommended extension gets a full technical review, and authors accept waits of
up to two weeks for it
([Recommended extensions](https://extensionworkshop.com/documentation/publish/recommended-extensions/)).

⇒ **Rule: risk-weighted queue, automation-first, humans only where risk is real —
and the highest rung explicitly buys its trust with slower updates.**

### F-15. Grain has an advantage none of these platforms have

Chrome and VS Code cannot compute a good risk score, because their permission
models are coarse and much of what an extension does is invisible until it runs.
Research on permission risk scoring consistently lands on the same idea — score
individual permissions, then score *combinations* that imply escalation, and
present the reasoning for triage — but the input data is poor
([example](https://arxiv.org/html/2512.15781v1); on the ecosystem-wide
least-privilege failure, [Cybernews](https://cybernews.com/security/chrome-extensions-get-too-many-dangerous-permissions/)).

Grain's manifest is different in kind: **every power an extension has is a
declared capability enforced in Rust**, and the SPEC already names the dangerous
combination (`screen:capture` + `net:` always triggers human review). A pack-tier
extension has *no code at all*. That means a risk score computed from the
manifest is not a heuristic — it is close to a complete description of what the
extension can do.

⇒ **This is the fast lane.** Risk is machine-computable, so a no-capability data
pack can be auto-published in minutes while a native extension with network
egress waits for a human — and neither decision is a guess.

---

## Part 4 — What makes extension development *enjoyable*

The gate's stated bar is that authoring must not be hell. The prior art is
unusually clear about what "good" is.

### F-16. Raycast is the reference, and the details are the point

`npm run dev` starts a session where the CLI watches files, transpiles with
esbuild, and hot-deploys into the running app. Concretely, in dev mode: the
extension is pinned to the top of root search; commands reload automatically on
save; **error overlays carry full stack traces**; log messages appear in the
terminal; a status indicator in the navigation title shows build errors; React
DevTools work out of the box. Extensions run as **Node worker threads (v8
isolates) inside a separate Node process**, each with configurable memory limits
— "extensions that get too greedy will be stopped" — talking JSON-RPC over file
descriptors ([Raycast blog](https://www.raycast.com/blog/how-raycast-api-extensions-work),
[debugging](https://developers.raycast.com/basics/debug-an-extension),
[CLI](https://developers.raycast.com/information/developer-tools/cli)).

Two things to steal beyond hot reload: **per-extension resource limits enforced
by the host**, and **errors surfaced in the UI the author is already looking at**,
not buried in a log file.

### F-17. Load-unpacked, done properly, is a first-class feature

Zed: "Install Dev Extension" takes a *directory*; if a published version of the
same extension is installed it is uninstalled first, and the extensions page then
shows "Overridden by dev extension" — the override is explicit and visible
([Zed](https://zed.dev/docs/extensions/developing-extensions)). Zed's extensions
are WASM, so a failure stays contained and the module can be reloaded without
restarting the editor ([Zed decoded](https://zed.dev/blog/zed-decoded-extensions)).
VS Code's equivalent is `--extensionDevelopmentPath`, which launches a second
"Extension Development Host" window with breakpoints and source maps — but
reloading still needs a manual restart action, a known DX complaint
([issue #190917](https://github.com/microsoft/vscode/issues/190917)).

⇒ **Rule: reload must be automatic and sub-second. Grain is well placed here —
a scripted extension *is* a Web Worker, so "reload" is "kill the worker and spawn
it again", which the host already does on every idle reap.**

### F-18. Zed's capability errors are the model for denial messages

Zed gates `process:exec` and `npm:install` behind declared capabilities and
returns an error naming the missing capability when an extension calls the API
without it ([Zed capabilities](https://zed.dev/docs/extensions/capabilities)).

⇒ **Rule: a denial is a typed error carrying the capability name, the call that
was refused, and the exact manifest line to add. Never a silent empty result —
that is the difference between five minutes and an afternoon.**

---

## Part 5 — Findings that change what we already built

Research done against our own code, not just against other people's platforms.

### F-19. Extension surface windows had no Tauri capability *(defect, fixed)*

In Tauri v2, application commands registered with `invoke_handler` are callable
from every window by default, **but core APIs are permission-gated** — `listen()`
needs `core:event`, granted through a capability file that names the window
([Tauri capabilities](https://v2.tauri.app/security/capabilities/),
[permissions](https://v2.tauri.app/security/permissions/)). `src-tauri/capabilities/`
had entries for `main`, `agent`, `grain-space` and `extension-host` — and none
matching the `ext-surface-*` / `ext-overlay-*` labels that Phase 3 introduced.
The `invoke` calls in `extension-surface.ts` would have worked; the three
`listen()` calls — sleep, revive, payload — would have been denied, breaking the
sleep/wake handshake that the whole low-RAM surface design rests on.

Fixed in `src-tauri/capabilities/extension-surface.json`, scoped to
`core:event:default` only (the host owns every window operation, so a surface
must not be able to move, resize or close itself). Window labels accept glob
patterns, confirmed against the generated schema.

**Why it was missed:** Phase 3 verified the backend, the bindings and a clean
boot, and deferred window end-to-end to "the first real workspace extension" —
which cannot exist until developer mode does. That is the gap this whole plan
closes, and it is the argument for building developer mode *before* anything
else.

### F-20. The local WebSocket server does not validate `Origin` *(hardening)*

`events_server.rs` binds `127.0.0.1:7124`, requires a token in the first frame,
and drops unauthenticated clients after 3 seconds — all correct. But browsers do
**not** apply the same-origin policy to WebSocket connections: any web page can
open a connection to any localhost port, and only server-side `Origin` validation
stops it ([OWASP WebSocket cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html),
[PortSwigger](https://portswigger.net/web-security/websockets/cross-site-websocket-hijacking)).

Classic cross-site WebSocket hijacking does **not** apply to us — our auth is a
token that a web page cannot obtain, not an ambient cookie. The residual risks
are real but smaller: any website can fingerprint that Grain is running (this is
exactly the localhost-tracking pattern that made headlines in 2025), and every
pre-auth parser is exposed to the open internet's worth of drive-by traffic.

⇒ Validate `Origin` during the handshake (allow only Grain's own webview origins
and absent-origin non-browser clients), and cap concurrent unauthenticated
connections. Cheap, and it removes a category.

### F-21. `entry_source` is a review and debugging dead end *(design)*

A scripted pack today embeds its JavaScript as a **string inside the manifest
JSON**. That was right for Phase 2 — one shareable file, no bundler. It does not
survive contact with F-6 and F-16: a minified blob in a JSON string is precisely
where invisible-Unicode payloads hide, there is nowhere to put a source map, and
"go to the line that threw" is not expressible.

⇒ Keep the single-file pack as the *distribution* artifact, but make it a
**build output**, produced by the registry from a source tree (F-2), with a
source map alongside it for dev mode. Add invisible/bidi Unicode rejection at
both the CI gate and the import path.

### F-22. Nothing stops a pack from *claiming* to be trusted *(invariant to test)*

The manifest schema has no trust field, and the installer does not read one, so
this is currently true by accident rather than by construction. Given F-5, it is
the single most important invariant in the system.

⇒ Make it explicit and permanent: trust is only ever read from signed index
metadata; the import path ignores every unknown field (it already does); add a
test that a pack carrying `"trust": "verified"` installs as `community`.

### F-23. No per-extension resource ceiling *(gap)*

We have a transform timeout and a strike system, which bounds *latency*. Raycast
additionally bounds *memory* per extension and kills over-budget workers (F-16).
Grain's whole premise is low RAM on edge devices; an extension that allocates
until the machine swaps is a Grain problem, not the author's.

⇒ Add a per-worker memory ceiling with the same strike semantics as timeouts.

### F-24. Observation: Grain's own updater is a working precedent, not dead code

`tauri.conf.json` still registers `tauri-plugin-updater` — pointed at Grain's own
releases with a pinned minisign key, i.e. Grain's own update channel, not the
removed Handy one. Two consequences: the Ed25519/pinned-key pattern in F-3 is
already proven in this codebase, and whoever operates Grain releases already
holds a signing key, so extension signing adds a *use* of existing practice
rather than a new one.

---

## Sources

**Prior art —** [Obsidian releases repo](https://github.com/obsidianmd/obsidian-releases) ·
[Obsidian submission guide](https://marcusolsson.github.io/obsidian-plugin-docs/publishing/submit-your-plugin) ·
[Raycast: publish](https://developers.raycast.com/basics/publish-an-extension) ·
[Raycast: how extensions work](https://www.raycast.com/blog/how-raycast-api-extensions-work) ·
[Raycast: CLI](https://developers.raycast.com/information/developer-tools/cli) ·
[Raycast: debugging](https://developers.raycast.com/basics/debug-an-extension) ·
[Zed: developing extensions](https://zed.dev/docs/extensions/developing-extensions) ·
[Zed: capabilities](https://zed.dev/docs/extensions/capabilities) ·
[Zed decoded: extensions](https://zed.dev/blog/zed-decoded-extensions) ·
[zed-industries/extensions](https://github.com/zed-industries/extensions) ·
[VS Code marketplace docs](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace) ·
[VS Code publishing](https://code.visualstudio.com/api/working-with-extensions/publishing-extension) ·
[Microsoft: security and trust in the Marketplace](https://developer.microsoft.com/blog/security-and-trust-in-visual-studio-marketplace/) ·
[Open VSX FAQ](https://www.eclipse.org/legal/open-vsx-registry-faq/) ·
[Eclipse: Open VSX managed registry](https://newsroom.eclipse.org/news/announcements/eclipse-foundation-launches-open-vsx-managed-registry-0) ·
[AMO signing and distribution](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/) ·
[AMO recommended extensions](https://extensionworkshop.com/documentation/publish/recommended-extensions/) ·
[AMO reviewers guide](https://wiki.mozilla.org/AMO/Reviewers/Guide) ·
[Chrome Web Store review process](https://developer.chrome.com/docs/webstore/review-process) ·
[F-Droid reproducible builds](https://f-droid.org/docs/Reproducible_Builds/) ·
[F-Droid security model](https://f-droid.org/en/docs/Security_Model/) ·
[F-Droid: making reproducible builds visible](https://f-droid.org/en/2025/05/21/making-reproducible-builds-visible.html) ·
[Anthropic: desktop extensions](https://www.anthropic.com/engineering/desktop-extensions) ·
[modelcontextprotocol/mcpb](https://github.com/modelcontextprotocol/mcpb) ·
[Cargo RFC 2789 sparse index](https://rust-lang.github.io/rfcs/2789-sparse-index.html) ·
[Cargo registry index reference](https://doc.rust-lang.org/cargo/reference/registry-index.html)

**Incidents and attacks —** [Aqua: can you trust your VS Code extensions](https://www.aquasec.com/blog/can-you-trust-your-vscode-extensions/) ·
[OX Security: can you trust that verified symbol](https://www.ox.security/blog/can-you-trust-that-verified-symbol-exploiting-ide-extensions-is-easier-than-it-should-be/) ·
[Koi: marketplace takeover](https://www.koi.ai/blog/marketplace-takeover-how-we-couldve-taken-over-every-developer-using-a-vscode-fork-putting-millions-at-risk) ·
[THN: Open VSX registry flaw](https://thehackernews.com/2025/06/critical-open-vsx-registry-flaw-exposes.html) ·
[THN: GlassWorm abuses 72 Open VSX extensions](https://thehackernews.com/2026/03/glassworm-supply-chain-attack-abuses-72.html) ·
[Truesec: GlassWorm](https://www.truesec.com/hub/blog/glassworm-self-propagating-vscode-extension) ·
[Socket: GlassWASM](https://socket.dev/blog/glasswasm-malware-open-vsx-extensions) ·
[Zed GHSA-v385-xh3h-rrfr (Zip Slip)](https://github.com/zed-industries/zed/security/advisories/GHSA-v385-xh3h-rrfr) ·
[Tenable: download pumping](https://www.tenable.com/blog/how-cyberattackers-inflate-malicious-package-npm-download-counts) ·
[ReversingLabs: download pumping](https://www.reversinglabs.com/blog/download-pumping-trust-abuse) ·
[npm: how download counts work](https://blog.npmjs.org/post/92574016600/numeric-precision-matters-how-npm-download-counts-work.html) ·
[Island: extension installation behaviour patterns](https://island.io/browser-extension-security/browser-extension-security-defending-against-installation-behavior-patterns) ·
[Cybernews: extension permission overreach](https://cybernews.com/security/chrome-extensions-get-too-many-dangerous-permissions/)

**Mechanisms —** [TUF security](https://theupdateframework.io/docs/security/) ·
[TUF specification](https://theupdateframework.github.io/specification/latest/) ·
[PEP 458](https://peps.python.org/pep-0458/) ·
[Sigstore quickstart](https://docs.sigstore.dev/quickstart/quickstart-cosign/) ·
[sigstore/cosign](https://github.com/sigstore/cosign) ·
[npm trusted publishing GA](https://github.blog/changelog/2025-07-31-npm-trusted-publishing-with-oidc-is-generally-available/) ·
[npm trusted publishers docs](https://docs.npmjs.com/trusted-publishers/) ·
[minisign](https://jedisct1.github.io/minisign/) ·
[Offline PKI with YubiKeys](https://vincent.bernat.ch/en/blog/2025-offline-pki-yubikeys) ·
[drduh YubiKey guide](https://github.com/drduh/YubiKey-Guide) ·
[Cloudflare R2](https://www.cloudflare.com/products/r2/) ·
[Cloudflare developer platform pricing](https://www.cloudflare.com/plans/developer-platform/) ·
[OWASP WebSocket cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html) ·
[PortSwigger: cross-site WebSocket hijacking](https://portswigger.net/web-security/websockets/cross-site-websocket-hijacking) ·
[Tauri v2 capabilities](https://v2.tauri.app/security/capabilities/) ·
[Tauri v2 permissions](https://v2.tauri.app/security/permissions/) ·
[VS Code hot reload issue #190917](https://github.com/microsoft/vscode/issues/190917) ·
[LLM-based permission risk scoring (arXiv)](https://arxiv.org/html/2512.15781v1)
