# Build and publish a tool extension

Extensions provide tools to the Agent. They do not receive screen, OCR, selected
text, prompt customization, OS/Space hooks or transcript access. There is no
universal Grain API key: provider accounts are authorized separately.

The maintained [CLI guide](../../crates/grain-ext-cli/README.md) is the source of
truth for manifests, descriptors, generated types, limits and command options.
This entry point replaces the old broad-capability author workflow.

## Native tools

From a Grain checkout, install the CLI with `cargo install --path crates/grain-ext-cli`.
Then create a project:

```powershell
grain-ext init "My tools" --id com.example.my-tools
cd my-tools
npm install
npm run build
```

Register declared handlers with `grain.actions`; use the generated `grain.d.ts`.
Add a valid 512x512 `icon.png`, then run `grain-ext doctor` and `grain-ext pack`.
In the real application, open **Extensions → Installed → Developer → Load
unpacked** and choose the project root. Enable the extension and review its
permissions. `grain-ext dev` provides native hot reload. Returned tool data
reaches the Agent through Grain's existing execution and approval boundary.

## MCP extensions and direct connections

To package an existing remote MCP server:

```powershell
grain-ext init "Remote tools" --id com.example.remote-tools --mcp-url https://tools.example.com/mcp
cd remote-tools
grain-ext doctor
grain-ext pack
```

OAuth is the default; `--authentication none` is for an intentionally anonymous
server. MCP projects use `mcp.json` and have no Grain JavaScript worker or Node
build. The descriptor contains no tokens, headers, client secrets, trust flags
or subprocess commands. Grain handles its account and connection lifecycle.

Users do not need a package for a private/custom MCP. In **Extensions → Installed**,
enable Developer Mode and close its drawer. Under **Your MCP connections**, add a
name, HTTPS Streamable HTTP URL and authentication, or enter the same definition
as JSON. Sign in/enable separately. Optional OAuth app credentials use the system
vault; the form shows the callback URI. Store-managed definitions are read-only.

## Listing and publication

Write user-facing store copy in **DESCRIPTION.md**, separate from README. Optional
screenshots belong in `media/`; the CLI guide defines accepted formats and bounds.
Publish your extension source and release tag on GitHub. From that project, draft
a pinned submission into a local checkout of the
[registry](https://github.com/Punit-Dethe/Grain-Extention):

```powershell
grain-ext submit --registry C:\path\to\registry --repo https://github.com/owner/repository --tag v1.0.0 --commit <full-lowercase-40-character-commit> --license MIT --contact "Maintainer contact"
```

`submit` writes a local draft; it does not upload or publish. Commit the generated
`extensions/<id>/` directory on a registry branch and open a pull request. Review,
isolated builds, signing and publication use the existing
[maintainer workflow](../../crates/grain-registry-tools/README.md).

Catalogue activation is still an operator gate. Until release activation, these
application features remain development-gated. Test-store verification does not
mean an extension has been approved or published publicly.
