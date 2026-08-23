# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Grain serves people who want fast, local-first speech-to-text on desktop systems,
including low-memory edge devices. Extension authors integrate external tools and
specialized workflows without making those tools feel like separate applications.

## Product Purpose

Grain turns speech into text and routes explicit requests to user-approved
extensions while keeping capture, recommendation, permissions, and trusted UI
under host control.

## Positioning

Grain combines an on-device, low-overhead ASR core with a capability-scoped
extension runtime. Extensions control workflow and content; Grain controls the
security boundary, native interaction language, and resource lifetime.

## Operating Context

People invoke Grain while working in other desktop applications. Extension Mode
records a request through its own trigger, recommends a searchable extension,
and hands over the verbatim request only after selection. Extension outcomes are
finite tasks rather than extension-owned conversations.

## Capabilities and Constraints

- Rust/Tauri owns backend state; React/TypeScript surfaces communicate only via
  Tauri commands and events.
- Correctness, low RAM/CPU overhead, and explicit cleanup are mandatory.
- Standard extension UI is an allowlisted remote component tree rendered by
  Grain. Authors may compose layout, content, fields, and semantic actions, but
  may not supply HTML, CSS, scripts, arbitrary colors, or window behavior.
- Rendering UI grants no functional capability. Extension actions continue to
  run through the authenticated, capability-checked worker runtime.
- Interaction targets use stable IDs and every tree/event is quota-validated.
- The standard confirmation/result surface is keyboard-operable and retains no
  extension state after it closes.
- Arbitrary custom UI is not part of the standard V1 surface.

## Brand Commitments

The product is Grain. Extension content must remain visibly inside Grain-owned
identity, navigation, security messaging, controls, typography, color, spacing,
focus, and motion.

## Evidence on Hand

- `docs/Extensions V1/PLAN.md` records the Extension Mode lifecycle and finite
  Choose, Confirm, Notice, and Result use cases.
- The native pill and Agent panel are incumbent interaction and motion references.
- No third-party claims, testimonials, or benchmark data should be fabricated.

## Product Principles

- Give extension authors layout freedom, not styling or host authority.
- Make trusted boundaries obvious without adding repetitive consent friction.
- Keep actions semantic, serializable, replayable, and capability-separated.
- Reclaim every worker, window, listener, tree, and credential when unused.
- Grow the component vocabulary from real extension needs.

## Accessibility & Inclusion

All standard extension surfaces must support keyboard navigation, visible focus,
semantic labels, screen-reader relationships, reduced motion, and sufficient
contrast in both Grain themes.
