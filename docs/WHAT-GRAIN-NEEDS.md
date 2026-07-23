# What Grain Needs to Become

## Purpose

This document describes the product state Grain is intended to reach. It is a
statement of **what Grain needs to be**, not an implementation plan or
roadmap.

## The product

Grain should be a lightweight, private, local-first speech platform built on a
dependable Handy-compatible transcription foundation.

It should serve two audiences at once:

- people who want a fast, simple, dependable dictation application out of the
  box; and
- people who want to shape speech, context, notes, automation, and AI
  interactions around their own workflows.

Grain is not intended to be a default installation containing every possible
speech feature. It is intended to be the best small foundation on which those
features can exist.

## A clear identity

Grain must have a clear ownership boundary:

- Handy remains the recognisable, compatible transcription foundation.
- Grain owns its identity, user experience, lifecycle, data, and feature
  ecosystem.
- Grain-specific capabilities must not be scattered invisibly through the
  Handy-derived foundation.
- Upstream Handy updates must remain understandable, reviewable, and
  predictable.

The desired outcome is that Grain can continue benefiting from Handy's
battle-tested work without being defined as a permanently tangled fork.

## The default experience

The default Grain experience must remain small, fast, reliable, and useful
without any extension installed.

It must preserve Grain's core principles:

- local-first and privacy-respecting by default;
- low RAM and CPU use;
- no persistent resources for disabled or unused capabilities;
- dependable recording, transcription, model handling, and output;
- a focused native experience rather than a crowded control panel.

The application should ship the broadly useful capabilities ordinary users
expect. It should not make every specialised, experimental, or opinionated
idea part of the default application.

## An extension platform

Grain must allow features to be added without turning the core application
into a monolith.

Extensions should be able to introduce useful new ways to work with speech,
including, for example:

- context-awareness sources, policies, and actions;
- prompt packs, snippets, transforms, and output destinations;
- agent tools, workflows, and integrations;
- Grain Space note types, retrieval experiences, layouts, and sync providers;
- pill interactions, visual modes, themes, and optional views;
- speech providers, post-processing providers, and automation integrations.

This means the ecosystem can support unusual, niche, experimental, or
competing ideas without forcing every user to install, understand, or pay the
resource cost for them.

## A first-class author ecosystem

People who create a Grain extension should be able to treat it as their own
work:

- it can live in its own repository;
- it can have its own releases, documentation, users, and identity;
- it can evolve independently while remaining compatible with Grain;
- it can be discovered by users without needing to become a core Grain pull
  request.

Grain should welcome both small improvements and ambitious new interaction
models. A contributor should not need to create a complete speech application
from scratch merely to explore a good idea.

## An integrated marketplace

Grain needs an in-application place where users can discover, understand,
install, update, disable, and remove extensions.

The marketplace must make choices understandable:

- what an extension does;
- who maintains it;
- which Grain versions it supports;
- what access it requires;
- whether it is built-in, verified, community-maintained, or experimental;
- whether it is active and what resources it uses.

The marketplace is part of the product experience, not an external hurdle a
user must understand before Grain becomes useful.

## Trust, privacy, and user control

An extension ecosystem must make users more capable without making them less
safe.

Users must retain clear control over access to sensitive capabilities,
including microphone audio, clipboard content, personal notes, files, network
services, and AI-provider credentials.

Extensions must be distinguishable by trust level. Grain must support a
healthy place for experimentation while making the difference between
maintained, verified, and experimental work clear.

Installing an extension must never quietly convert Grain into a permanently
heavier, less private application. Users must be able to inspect, enable,
disable, remove, and understand the consequences of an extension.

## A composable product, not a bloated one

The long-term goal is not one application with every feature permanently
installed. The goal is one dependable application capable of hosting the
features people genuinely need.

The core should stay opinionated about quality, privacy, performance, and
reliability. The ecosystem should stay open to many different answers to
questions such as:

- How should context be understood?
- How should a note-taking experience work?
- What should happen after a transcript is produced?
- Which interactions belong in the pill?
- How should speech connect to an individual's tools, data, and workflows?

When an extension proves broadly useful and dependable, Grain may adopt it as
an optional built-in capability. It should not need to absorb every valid
idea.

## What success looks like

Grain has reached this intended state when:

- it is trusted as a lightweight, dependable speech application even with no
  extensions installed;
- Handy updates are no longer a source of opaque, project-wide uncertainty;
- contributors can build and share meaningful Grain capabilities without
  needing to fork the application;
- users can make Grain fit their workflows by choosing capabilities rather
  than waiting for one core team to anticipate every use case; and
- the project has room for both careful core maintenance and fast community
  experimentation.

## Deliberately not specified here

This document does not choose the technical implementation, runtime model,
package format, marketplace protocol, or migration sequence. Those decisions
must be made separately without compromising the product requirements above.
