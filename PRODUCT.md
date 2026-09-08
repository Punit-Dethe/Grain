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
extensions while keeping capture, recommendation, permissions, and every
rendered surface under host control.

## Positioning

Grain combines an on-device, low-overhead ASR core with a capability-scoped
extension runtime. Extensions provide service capabilities and bounded data;
Grain controls intelligence, presentation, security, and resource lifetime.

## Operating Context

People invoke Grain while working in other desktop applications. Extension Mode
records a request through its own trigger, recommends a searchable extension,
and hands over the verbatim request only after selection. Extension outcomes are
finite tasks rather than extension-owned conversations.

## Capabilities and Constraints

- Rust/Tauri owns backend state; React/TypeScript surfaces communicate only via
  Tauri commands and events.
- Correctness, low RAM/CPU overhead, and explicit cleanup are mandatory.
- Extensions cannot supply UI. Grain renders declarative settings, Agent
  interactions, routing, progress, and finite text results using native choices.
- Authors may not supply HTML, CSS, component trees, themes, skins, windows, or
  replacement layouts.
- Rendering UI grants no functional capability. Extension actions continue to
  run through the authenticated, capability-checked worker runtime.
- The standard confirmation/result surface is keyboard-operable and retains no
  extension state after it closes.

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

- Give extension authors capability and data freedom, not pixels or host authority.
- Make trusted boundaries obvious without adding repetitive consent friction.
- Keep actions semantic, serializable, replayable, and capability-separated.
- Reclaim every worker, window, listener, tree, and credential when unused.
- Keep future rich rendering behind a Grain-owned semantic interaction contract.

## Accessibility & Inclusion

All standard extension surfaces must support keyboard navigation, visible focus,
semantic labels, screen-reader relationships, reduced motion, and sufficient
contrast in both Grain themes.
