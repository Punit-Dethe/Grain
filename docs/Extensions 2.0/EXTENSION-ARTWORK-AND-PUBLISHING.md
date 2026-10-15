# Extension artwork and publishing

Updated 15 October 2026. This documents the current authoring contract, not a
promise that an unsigned source change is already live in the store.

## Which image goes where

| Source file | Required shape and limits | Grain uses it for |
| --- | --- | --- |
| `media/icon.png` | Exactly **512×512 PNG**, at most **512 KiB** | Small icon beside the extension title and in Installed |
| `media/cover.webp` or `media/cover.gif` | Wide gallery image, at most **4 MiB** | Store browsing card and first large detail image |
| Other `media/*.webp` / `media/*.gif` | Same gallery limits | Further detail previews, in filename order |

For gallery artwork, **1280×720** is the recommended export size. Accepted
dimensions are **640–2048px wide**, **360–1152px high**, and a landscape ratio
between **3:2 and 2:1**. This accommodates genuine product screenshots as well
as brand artwork. The viewport is consistently **16:9**, using contain sizing
so Grain does not stretch or crop the supplied image. A square image is refused
as gallery media. An icon never becomes a hero when the gallery is absent.

Up to **six gallery images plus one icon** are allowed. The description, icon
and gallery together must fit **16 MiB**. Files are flat; nested directories,
symlinks, duplicate names and multiple cover files refuse. Formats must match
their lowercase extensions. Image decoding is bounded; GIF validation inspects
the first frame, not every animation frame. The gallery has no autoplay and
loads only the selected full image; its numbered selectors request no images.

Listing artwork is optional. Without an icon, Grain uses a neutral initial;
without gallery media, it omits the hero. Authors should provide both. Native
extensions also keep their existing `manifest.icon` / embedded **512×512 PNG**
runtime icon. Listing imagery belongs to publishing metadata, not MCP transport
configuration, OAuth credentials, tools or permissions.

## Why GitHub and Notion looked different

Previously the catalogue had one generic media array and no distinct listing
icon. Native packaged icons already had a strict square contract, but hosted
MCP listings reused the first media file for both square and wide placements.
GitHub supplied a 1024×575 cover; Notion supplied a 192×192 logo. Both passed
the former maximum-size checks, which enforced neither landscape shape nor a
minimum resolution. The signed catalogue now carries a distinct `icon` reference
and a gallery-only `media` array. The publisher separates reserved `icon.png`
from cover/preview files. The client never guesses their role from pixel shape.

## Author workflow

1. Create a native tool extension or an MCP definition with Grain's CLI. Keep
   source instructions in `README.md` and user-facing content in `DESCRIPTION.md`.
2. Add `media/icon.png`, `media/cover.webp` and optional further preview files.
   Keep permission to use the images and record official source attribution.
   Images demonstrate the service; do not imply provider authorship of Grain.
3. Run `grain-ext doctor` and `grain-ext pack`. Publish the source commit and a
   fixed version tag. `grain-ext submit` performs the listing/image checks and
   creates a local registry submission; it does not upload or publish anything.
4. Commit the submission to the registry and open a PR. See the
   [CLI guide](../../crates/grain-ext-cli/README.md) for the exact command flags.
5. Maintainer review checks the pinned source, description, icon and gallery.
   Trusted CI prepares the artifact and listing from those bytes, then records
   build evidence. Signing and publication happen separately after approval.

This image contract is shared by native and MCP extensions. OAuth registration
is a separate concern: artwork publication cannot grant an account, change
provider permissions, or embed client secrets.

## Storage, integrity and updates

The author owns image files in the extension source repository. Submissions pin
their names, sizes and SHA-256 hashes. Verified preparation copies the exact
bytes into content-addressed catalogue files such as `v1/media/<hash>.png` and
`v1/media/<hash>.webp`. The signed index references the square icon separately
from the ordered gallery. The app fetches/cache-checks those bytes by hash,
honors signed sizes, and validates square PNG icons again before presentation.
It does not hotlink third-party images at runtime.

Published `(extension ID, version)` entries are immutable in the current
publishing system. An artwork update therefore needs a new extension version,
source tag and reviewed submission, even when URL, authentication and tools are
unchanged. **That is an extension update, not a new Grain installer release.**
There is no separate artwork-only publishing lane today. Do not patch the live
signed index or reuse approval for a different source commit. Existing tester
approval exceptions cover exact candidates; new candidates require approval.

## First-party examples

GitHub 0.1.2 and Notion 0.1.1 are prepared with one square icon and three
wide images each, using official provider assets. Their source `assets/README.md`
records URLs, pixel sizes and normalization. These are test-listing images;
they do not certify every depicted service feature through Grain. Public store
activation remains separate until their exact candidates are approved/signed.
Both fixed source tags are pushed; the combined candidate is in
[registry PR #5](https://github.com/Punit-Dethe/Grain-Extention/pull/5).
The user withdrew accidental approval on 15 October, then explicitly
reauthorized publication of both exact candidates in the next request. PR #5
is merged; PR #6 aligns producer metadata with the pinned tool build. Signing
and public activation are in progress; no new Grain installer is required.
