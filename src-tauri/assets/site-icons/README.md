# Bundled site icons

Icons for hosts that **cannot be fetched**, compiled into the binary by
`src-tauri/src/pill_icon.rs` (`site_fetch::BUNDLED`).

This directory is an exception to how site icons normally work. Grain fetches a
site's icon from the site: the registry in `context_detect.rs` holds site
*identities*, never site *assets*, so adding a supported site is one row and its
logo arrives by itself. Read `site_fetch::BUNDLED` before adding anything here.

**A file belongs here only if the host refuses to serve its icon.** Not because
the logo is nicer this way, and not because the site is important. The test is
mechanical: if a plain HTTP client can retrieve the icon, it does not go here.

Every file must be public domain or otherwise licensed for redistribution, and
must have its provenance recorded below. These bytes ship inside the binary, so
"grabbed it off their CDN" is not an acceptable answer.

## Files

### `chatgpt.png` — 128×128 RGBA

| | |
|---|---|
| Hosts | `chatgpt.com`, `chat.openai.com` (and subdomains of each) |
| Source | [`File:ChatGPT-Logo.svg`](https://commons.wikimedia.org/wiki/File:ChatGPT-Logo.svg) on Wikimedia Commons |
| Licence | Public domain (simple geometry, not eligible for copyright) |
| Author | OpenAI |
| Added | 2026-08-15 |

Why it is bundled: every OpenAI origin returns **403** to a plain HTTP client —
`chatgpt.com` and `openai.com`, both `/` and `/favicon.ico`. Verified with a
full Chrome User-Agent as well as Grain's, so it is not a User-Agent problem; it
is a TLS-fingerprint and JS challenge. `cdn.oaistatic.com` does answer, but only
at content-hashed paths (`apple-touch-icon-mz9nytnj.webp`) that change on every
deploy, so there is no stable URL to point at.

Processing: rendered from the source SVG at 250px, recoloured to white keeping
the alpha channel, downscaled to 128px (bicubic). **White is deliberate** — the
source is a pure black glyph on transparency and the pill surface is `#1E1E20`,
so the original would have been invisible on it. White-on-dark is also how
OpenAI presents the mark themselves.

The recolour sets RGB to white on *every* pixel including fully transparent
ones, before downscaling. Interpolating a transparent black pixel against an
opaque white one produces a grey fringe; making the whole buffer one colour
means there is nothing to bleed.
