# Extension visual ownership

Status: accepted for the pre-release extension contract.

## Decision

Extensions provide capabilities and structured result data. Grain owns every
pixel. Public extensions cannot supply HTML, component trees, themes, skins,
surface layouts, settings panels, or replacements for Grain UI.

This keeps the initial platform agent-first: the Agent selects an exact action,
the extension performs it, and Grain renders the result as text/markdown using
its own native surfaces.

## Public boundary

The following declarations are rejected at every pack-validation boundary:

- workspace or overlay surfaces;
- custom settings panels;
- pill themes and pill UI capabilities;
- visual slots, variant slots, and `overrides:*` claims.

The worker SDK does not expose workspace, overlay, or extension-view APIs. A
request handler may return only a finite message, decline, or error. Structured
action outcomes remain data and are rendered by the Agent.

## Built-in choices

Removing extension customization does not remove Grain's own choices:

- Pill: `Wave` and `Matrix` remain native `PillSkin` values.
- Agent: `Side card` and `Center panel` remain native
  `agent_panel_position` values.

The Agent center panel no longer depends on `grain.agent-center-layout`, an
installed pack, or `agent.reply-surface` slot ownership.

## Implementation sequence

- [x] Establish Grain-owned visual boundary.
- [x] Reject visual manifest declarations and capabilities.
- [x] Remove visual APIs from the author SDK and request reply contract.
- [x] Make both Agent layouts unconditionally available as native settings.
- [x] Retire the center-layout pack/slot coupling and hide obsolete store cards.
- [x] Verify SDK, extension checks, core, Tauri, and frontend builds/tests.
