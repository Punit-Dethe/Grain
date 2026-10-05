# Grain extension authoring — provisional tools-only profile

This guide describes the current CLI. The older broad extension specifications
are superseded for authoring: extensions provide tools; Grain's agent owns
screen/context access, prompts and execution approval. This is a provisional
development profile, not a frozen public SDK or a production publishing promise.

An extension can implement its tools in JavaScript inside Grain's restricted
worker, or describe an existing remote MCP server. No universal Grain API key
is required to author either kind. Provider credentials and user authorization
are separate from the extension's source metadata.

Build the CLI from this workspace:

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
`grain.d.ts`, build scripts and developer documentation. Add your own valid
512×512 `icon.png` before doctor/pack can pass. Declare tools, their supported
arguments and risk in the manifest, then register matching handlers with
`grain.actions`. The scaffold's return value uses `{ ok: { title, body } }`.
Handlers receive validated arguments and a read-only idempotency context.
Grain owns confirmation, provenance and receipts; a lost response must not cause
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
endpoint. The file is limited to 8 KiB. Headers, secrets, client IDs, host
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
The signed test-store path has been verified against the real application;
direct local descriptor import and public store listing are not promised by
this CLI block. Direct user-configured MCPs use the
host's separate credential-free connection definition and do not require an
extension package.

## Listing and publishing status

Both scaffolds create `DESCRIPTION.md` for user-facing listing text; README is
developer/source documentation. `submit` now drafts a local source-pointer
submission for either project kind. It does not clone, install dependencies,
upload, create a pull request, sign or publish anything.

From the project root, with an existing local registry checkout:

```powershell
grain-ext submit --registry C:\path\to\registry --repo https://github.com/owner/repository --tag v1.0.0 --commit <full-lowercase-40-character-commit> --license MIT --contact "Maintainer contact"
grain-registry check-submission --dir C:\path\to\registry
```

Build the maintainer checker with `cargo build --locked -p grain-registry-tools
--bin grain-registry`. Schema 1 records the artifact kind, project identity/API,
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
commit. E4 must verify the pinned source and build.

Submission requires nonempty UTF-8 `DESCRIPTION.md` (maximum 64 KiB); README is
never a fallback. Optional `media/` contains at most six flat `.webp`/`.gif`
files, each at most 4 MiB, with a total listing budget of 16 MiB including the
description. Images must decode with width/height at most 2048 pixels. The
requested 16 MiB decoder allocation limit is best effort; only the first frame
is decoded. This is not full animation validation or a rendering-safety claim.
Symlinks, unsupported files, case-insensitive duplicate names and multiple
cover assets refuse. Size/hash metadata and cover-first ordering are stable.

The CLI creates `extensions/<id>/DESCRIPTION.md` and publishes `submission.toml`
last as its completion marker. Existing submission directories are never
overwritten; updating a prior submission is not yet supported by this command.
Media remains at the source pointer with hashes/sizes in the submission. The
maintainer checker shares the schema and verifies the description snapshot;
E4 must match source, media, build and signed catalogue together. The legacy
publisher refuses these new source submissions until that migration exists.

E3's author tools are provisionally accepted. Protected builds/signing and
catalogue producer/consumer migration remain E4; store/connection UI and safe
description rendering remain E5. Do not publish a final SDK or rely on a
production marketplace based on this checkpoint.

Focused checks:

```powershell
cargo test --locked -p grain-sdk -p grain-ext-cli -p grain-extension-checks -p grain-registry-tools --all-targets
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/author-contract.mjs
node tests/agent-harness/run.mjs --suite extension-contract
node tests/agent-harness/run.mjs --suite store-mcp
```

These reuse the maintained real-app runner. They do not need a real provider
account or dependency installation for generated-project verification.
