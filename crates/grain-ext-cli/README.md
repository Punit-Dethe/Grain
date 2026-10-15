# Grain extension authoring — provisional tools-only profile

This guide describes the current CLI. The older broad extension specifications
are superseded for authoring: extensions provide tools; Grain's agent owns
screen/context access, prompts and execution approval. This is a provisional
development profile, not a frozen public SDK or a production publishing promise.

An extension can implement its tools in JavaScript inside Grain's restricted
worker, or describe an existing remote MCP server. No universal Grain API key
is required to author either kind. Provider credentials and user authorization
are separate from the extension's source metadata.

## Get the CLI

The **Extension CLI artifacts** GitHub Actions workflow builds the CLI separately
from Grain on Windows, Linux and macOS. Download the artifact for your OS from a
successful run and extract it. `BUILD.json` records the exact source commit, CLI
version and actual Rust host architecture; check it matches your machine. These
are development downloads, not an announced stable SDK release. A GitHub login
may be needed to download Actions artifacts; they expire after 30 days.

Verify the executable against `SHA256SUMS` before running it. The checksum detects
corruption; trust also depends on downloading the intended successful workflow
from Grain's repository. It is not an independent publisher signature. Windows:

```powershell
Get-FileHash .\grain-ext.exe -Algorithm SHA256
.\grain-ext.exe --version
```

On macOS/Linux, use `shasum -a 256 grain-ext` / `sha256sum grain-ext`, then
`chmod +x grain-ext` and `./grain-ext --version`. Move it to a directory on your
PATH, or use its absolute path. No Grain source checkout, Rust or Node is needed
to run the downloaded CLI for an MCP descriptor. Native project builds need a
Node.js LTS installation with npm; the generated project installs TypeScript and
esbuild locally. The CLI reports its own version with `--version`; generated
projects target Grain extension API `^1.0` and need a matching Grain application.

CLI-source pushes run the workflow; maintainers can also run it manually on a
reviewed source ref. It grants only
repository read permission and produces three OS-specific artifacts with source
identity, guide and checksums. It does not create or modify a GitHub release.
Cross-platform artifact checks must pass before claiming those downloads work.

For source development, build from the workspace using Rust 1.96 or later:

```powershell
cargo build --locked -p grain-ext-cli --bin grain-ext
```

Use `cargo metadata --format-version 1 --no-deps` to locate the configured
target directory; the executable is `debug/grain-ext.exe` on Windows.

## Native tool project

```powershell
grain-ext init "Spotify tools" --id com.example.spotify-tools
cd spotify-tools
npm install
npm run build
grain-ext doctor
grain-ext pack
```

`init` creates `manifest.json`, a typed hello action in `src/main.ts`, generated
`grain.d.ts`, build scripts, developer documentation and a neutral 512×512
`icon.png`. The starter passes doctor without having to find artwork first;
replace that placeholder with your own icon before publication. Declare tools, their supported
arguments and risk in the manifest, then register matching handlers with
`grain.actions`. The scaffold's return value uses `{ ok: { title, body } }`.
Handlers receive validated arguments and a read-only idempotency context.
Grain owns execution policy, provenance and receipts; a lost response must not cause
an automatic repeated write. The generated declarations describe the supported
result/error forms and restricted storage, network and authentication services.

Enable Developer Mode in the real Grain application and load the project folder
to try it. `grain-ext dev` uses the existing scoped local developer connection
for native hot reload. It does not grant retired screen, OS, prompt, Space or
daemon-event capabilities. Remote requests require declared allowed hosts and
the Grain network broker; raw service tokens are not returned to extension code.

## Remote MCP project

```powershell
grain-ext init "Remote tools" --id com.example.remote-tools --mcp-url https://tools.example.com/mcp
cd remote-tools
grain-ext doctor
grain-ext pack
```

OAuth is the default. Use `--authentication none` only for an intentionally
unauthenticated server. The CLI creates `mcp.json`, README, DESCRIPTION and a
gitignore; it creates no JavaScript entry, generated global, Node package,
watcher or server process. Implement the server using a standard MCP SDK.

`mcp.json` uses descriptor schema 1 and Grain API `^1.0`, with an explicit
`streamable-http` HTTPS endpoint and `oauth` or `none` authentication. The shared
host validator checks identity/version/display bounds and canonicalizes the
endpoint. Optional `transport.headers` declares public provider options:

```json
"transport": {
  "type": "streamable-http",
  "url": "https://api.githubcopilot.com/mcp/",
  "headers": { "x-mcp-toolsets": "all" }
}
```

Names must be lower-case valid HTTP names, at most 64 bytes each; values must
be trimmed printable ASCII, at most 512 bytes each. There are at most 16 options
and 2 KiB of combined name/value bytes. Duplicate names are refused. Authentication,
cookie, routing and MCP session/protocol headers are reserved for Grain.
These values are public package metadata: never include tokens, API keys or
secrets, and do not expect environment substitution or vault lookup. Validation
cannot prove an arbitrary provider value is non-secret; publisher review must
enforce that policy. Options go only to the MCP resource transport, not OAuth
metadata/registration/token endpoints. `doctor` and `pack` use the host validator.

For a server using a pre-registered **public** OAuth client, the author can add:

```json
"authentication": { "type": "oauth" },
"oauthClient": {
  "clientId": "public-client-registered-with-the-provider",
  "issuer": "https://auth.example.com/tenant"
}
```

The publisher registers that public/native client once, with redirect URI
`http://127.0.0.1:31938/mcp/oauth/callback`. Users then sign in through Grain;
they do not need to create their own developer application for this supported
route. The client ID is public metadata, not an API key. The issuer must match
discovered metadata exactly, including path/trailing slash. Grain requires S256
PKCE and advertised `token_endpoint_auth_methods_supported` containing `none`.
Client IDs are bounded to 512 bytes and issuers use the existing HTTPS destination
rules. No secret, scope, authorization/token endpoint or redirect override is
accepted here. Auth endpoints/scopes remain provider-discovered; grants remain
connection-owned vault data. Changing/removing the declaration requires sign-in
again and invalidates old approvals.

Explicit per-connection client setup takes precedence over the declaration.
Without one, the declaration is used; otherwise existing host-supported clients,
client metadata registration and DCR retain their existing paths. A declared
client failure never silently falls back to a different registration. Providers
requiring a secret are outside this publisher-public-client profile; use supported
host integration or explicit vault setup, never a packaged secret. GitHub's
existing built-in tester identity is unchanged. This source contract is verified
with normal SDK tests; live third-party acceptance is not yet claimed.

Direct connection JSON uses the same optional `oauthClient`, alongside name,
url, authentication and public headers. Custom management remains hidden in
release/alpha while its backend and development editor remain usable.

The descriptor is limited to 8 KiB and its connection projection to 4 KiB.
Secrets, host
connection IDs, accounts, trust flags, enablement and subprocess commands are
not descriptor fields. Both `manifest.json` and `mcp.json` in one root are an
error; the CLI never guesses which kind to build.

`pack` produces `<id>-<version>.mcp.json` using the canonical descriptor.
It performs no network, DNS, OAuth or server launch. Validation is not trust or
authorization: the host still admits destinations, verifies store provenance,
creates its own connection/account identities and obtains explicit enablement
and account consent. Failed validation preserves an existing artifact. Built
output cannot replace either project definition.

`dev` refuses MCP projects because there is no Grain worker to reload.
In Grain, open **Extensions → Installed → Developer** to load a native project.
Close that drawer to manage the shared Installed extension list. Open an
extension's page for account setup and preferences. In development, direct MCP
connections use the form or credential-free JSON and need no package.
Store MCPs are installed from verified catalogue descriptors; their URL/auth
definition is changed through a verified store update, not the custom editor.
Installation leaves the connection inactive. Sign in and enable it separately.
These application flows retain the Developer Mode release gate.

## Listing and publishing status

Both scaffolds create `DESCRIPTION.md` for user-facing listing text; README is
developer/source documentation. `submit` now drafts a local source-pointer
submission for either project kind. It does not clone, install dependencies,
upload, create a pull request, sign or publish anything.

From the project root, with an existing local registry checkout:

```powershell
grain-ext submit --registry C:\path\to\registry --repo https://github.com/owner/repository --tag v1.0.0 --commit <full-lowercase-40-character-commit> --license MIT --contact "Maintainer contact"
```

The author CLI runs the shared submission checks; authors do not need to compile
the separate maintainer checker. Schema 1 records the artifact kind, project identity/API,
source pointer, listing hashes/sizes and maintainer text. TOML uses structured
serialization, including quotes and backslashes. Unknown fields and duplicate
singleton CLI flags refuse. Category defaults to, and must be exactly, `tools`.
License/contact are bounded text, not verified SPDX identifiers or identities.
Summary comes from the project's description (or native name when absent), not
a separate CLI flag.

The current source profile accepts canonical `https://github.com/owner/repository`
URLs without credentials, query, fragment or `.git` suffix; a bounded literal
ASCII Git tag; and a nonzero lowercase 40-character commit. This validates the
pointer's shape, not repository ownership or whether that tag resolves to that
commit. The maintainer workflow verifies the pinned source and build.

Submission requires nonempty UTF-8 `DESCRIPTION.md` (maximum 64 KiB); README is
never a fallback. Optional `media/icon.png` is the square listing identity:
exactly 512×512 PNG, at most 512 KiB. It appears beside the title and in Installed;
it is never substituted with a gallery cover. Native `manifest.icon` remains
the same 512×512 PNG contract for its packaged runtime icon.

Add up to six flat `.webp`/`.gif` gallery files, each at most 4 MiB. Gallery images
must be 640–2048px wide and 360–1152px high, with an aspect ratio between 3:2 and
2:1; **1280×720 is recommended**. Grain fits them inside a 16:9 viewport without
cropping or distortion. `cover.webp` or `cover.gif` sorts first and supplies the
store-card image. Other images follow in filename order. The icon is separate
from the six gallery slots. Total listing budget: 16 MiB including description,
icon and gallery. The
requested 16 MiB decoder allocation limit is best effort; only the first frame
is decoded. This is not full animation validation or a rendering-safety claim.
Symlinks, unsupported files, case-insensitive duplicate names and multiple
cover assets refuse. Size/hash metadata and cover-first ordering are stable.

The CLI stages `extensions/<id>/DESCRIPTION.md` and `submission.toml`, validates
them together and installs the complete directory. Existing submissions are never
overwritten implicitly. To draft an
update, change the project's version, publish a different source commit/tag, then
add `--update-from <current-registry-version>` to the same submit command:

```powershell
grain-ext submit --registry C:\path\to\registry --repo https://github.com/owner/repository --tag v1.0.1 --commit <new-full-commit> --license MIT --contact "Maintainer contact" --update-from 1.0.0
```

The expected version must match the existing draft. Version, tag and commit must
all differ from the old submission. This command does not order semver versions;
maintainer review/publication controls the advertised release. Source repository
and native/MCP kind must match; transfers need maintainer review. A corrupt,
incomplete or linked existing draft, extra files in its directory, or a concurrent
submission command causes refusal rather than a forced overwrite. A versioned
update is still subject to source ownership/approval checks, not authorized by
this local flag.

Both files are staged and validated before replacing the directory. Ordinary
replacement failure restores the original; a failed restore retains its path in
the error for recovery. A process killed between directory moves can leave the
old draft in `extensions/.grain-submit-*/previous` and a `.grain-submit-<id>.lock`
at the registry root. Inspect/restore that draft and remove its lock only after
confirming no submission process is running. This local drafting operation is
not a crash-atomic Git transaction. No dirty-file reset or automatic Git push is
performed.
Media remains at the source pointer with hashes/sizes in the submission. The
maintainer checker shares the schema and verifies the description snapshot;
The current maintainer workflow matches source, media, build and signed
catalogue together. See [registry tooling](../grain-registry-tools/README.md)
for source review, isolated build, signing and publication. A local submission
draft has not been uploaded: commit its submission directory on a registry
branch and open a pull request to the registry after publishing the source/tag.

### Optional GitHub PR helper

Install Git and GitHub CLI (checked with GH 2.100.0), and sign in using `gh auth
login`. For a community submission, create a personal fork/clone using the normal
GH workflow from a parent directory, then create a dedicated submission branch:

```powershell
gh repo fork REGISTRY_OWNER/registry --clone
cd registry
git fetch upstream main
git switch -c submission/com.example.tools-v1 upstream/main
```

Run the existing `grain-ext submit` command from the extension project, pointing
at that checkout. Push the extension's source commit/tag separately. Then, in the
registry checkout, explicitly commit/push just its two submission files:

```powershell
git add -- extensions/com.example.tools/submission.toml extensions/com.example.tools/DESCRIPTION.md
git commit -m "Submit com.example.tools"
git push -u origin HEAD
grain-ext pr --registry . --id com.example.tools --repo REGISTRY_OWNER/registry --head-repo YOUR_ACCOUNT/registry
```

Maintainers using the registry directly can clone it normally, branch from
`origin/main`, and use the registry's own identity as `--head-repo`. Renamed forks
are supported by passing their actual owner/name. The current GH create command
supports personal fork heads; use the normal GitHub UI for organization-owned
forks. `--base` defaults to `main`; fetch its history first when using another base.

The helper validates the native/MCP submission, clean checkout, exact committed
bytes and already-pushed head. The PR diff may contain only this ID's submission
and DESCRIPTION; rename detection cannot hide an unrelated deleted file. It
creates a **draft** PR with an explicit head, or returns the matching open PR
after checking its source repository and head. An ordinary update uses
`submit --update-from`, a normal commit/push and the same `pr` command. Existing
PR descriptions, titles and review state are preserved; edit them manually when
desired. GitHub's list/create commands have different head syntax, handled here.

This convenience command never forks, fetches, commits, pushes, rewrites branches,
stashes work, merges, approves or publishes. It uses the signed-in author's ordinary
GH access, not registry signing authority. A branch remains mutable; CI, source
ownership and maintainer review still decide acceptance. Children are bounded and
reaped; failures identify the operation/exit status without printing raw credential
errors. On an interrupted/uncertain create, inspect the branch's GitHub PR before
repeating; there is no automatic retry. Oversized/ambiguous PR lookup refuses.
Registry main-protection and publisher custody remain operator responsibilities.

Source authoring, packaging and submission tooling are available. Production
catalogue activation and signing custody remain separate operator decisions;
the public SDK is not frozen. README is source documentation; the app renders
the signed DESCRIPTION document directly in the extension's Overview.

## Verification

For CLI edits, use the normal affected tests and build:

```powershell
cargo test --locked -p grain-ext-cli -p grain-extension-checks --lib
cargo build --locked -p grain-ext-cli --bin grain-ext
```

Generated declarations/builds use the existing `tests/agent-harness/author-contract.mjs`
check when given a current stamped CLI. It runs TypeScript positive/negative
checks and a bundled hello action, without opening Grain or using an account.
Run real-application suites only when install/auth/runtime behavior changes;
ordinary CLI edits do not require a new installer or complete Agent harness run.
