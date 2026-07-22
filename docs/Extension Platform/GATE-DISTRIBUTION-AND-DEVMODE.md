# GATE — Distribution platform + Developer mode

> **STATUS: LIFTED 2026-07-22.** The design pass this gate demanded is
> [DISTRIBUTION-PLAN.md](DISTRIBUTION-PLAN.md), backed by the evidence in
> [DISTRIBUTION-RESEARCH.md](DISTRIBUTION-RESEARCH.md).
>
> **This page is now the requirements record, kept unchanged**, so the plan can
> be audited against what was actually asked for. Every question below is
> answered in the plan; §5's exit condition is met by its §§2–7 (hosting,
> submission/review, trust ladder with the anti-forgery guarantee, metrics,
> install/update/remove, developer mode and debugging) plus the step-by-step
> guide in its §10.
>
> **Raised by the user 2026-07-21.** The instruction was explicit: *think about
> it, record it, do not plan it yet, and hold the roadmap until it is worked
> out.* It has now been worked out.

---

## 1. Why this gate exists

The phase table in [SPEC.md](SPEC.md) §8 carries the runtime all the way to
tier-C extensions (Phase 4) and an index repo with browse/install/update
(Phase 5). But it assumes two things the project has **never actually
designed**:

1. **A place for extensions to live, and a trust pipeline for them.** Phase 5
   says "index repo live; hash verification; trust badges; review checklist" in
   one line. That line hides an entire product: hosting, submission, review,
   moderation, trust progression, and the anti-forgery guarantees that make a
   "verified" badge mean anything.
2. **A way for a developer to actually build an extension.** Nothing in the
   roadmap gives an extension author a build/test/debug loop. Without it the
   Grain Space Test (Phase 3's acceptance bar) is passable only by us, because
   only we can run an extension without a developer surface.

Neither is a Phase-3 concern, so Phase 3 proceeds. **Both must be designed
before Phase 4/5 work begins.**

---

## 2. Open questions — distribution platform

Grouped as raised. Every one is unanswered.

### 2.1 Hosting and shape
- Where does the extension index live? A **new, separate repository** is the
  assumed starting point — but repo-as-database vs. a real service is open.
- Is there a **public web surface** (a URL anyone can browse), or is the store
  only reachable **inside the app**? The user explicitly left this open; what is
  NOT open is that the in-app store surface must exist.
- How does the in-app store page get its data — bundled index, fetched index,
  API? What happens offline?

### 2.2 Submission and review
- How does an author **submit** an extension for review?
- What does an author have to provide? A **GitHub repository** is assumed
  mandatory. What else — manifest, screenshots, a description, a category,
  contact?
- How does an extension get published as **experimental** (pre-review) vs.
  reviewed? Can users install experimental extensions, and with what warning?
- **We need a review dashboard**: a simple internal surface showing what is
  queued for review, its state, and its history. Who can see it, who can act.

### 2.3 Trust progression
- The progression **experimental → verified → core** needs a defined ladder:
  what each rung means, what evidence moves an extension up, and who decides.
- What can a rung change — install friction, warnings, default capabilities,
  store placement?
- Can an extension be **demoted** (a verified extension that turns malicious or
  is abandoned)? What is the revocation story for something already installed?

### 2.4 Security (the part that must not be hand-waved)
- **An author must not be able to make their own extension appear verified.**
  The trust signal has to be server-authoritative and unforgeable by whoever
  controls the extension's repo or its pack file. This is the single hardest
  requirement here.
- Hash/signature verification of the artifact a user actually installs, and
  binding it to what was reviewed (a review of v1.0 must not bless v2.0).
- Supply-chain: what stops a reviewed repo from changing after review? Pinned
  commits/tags? Re-review on update?
- How do the store's trust claims reach the app in a way the app can verify
  rather than trust blindly.

### 2.5 Metrics and signals
- **GitHub stars**, fetched for the linked repo.
- **Download/install counts** — which requires deciding what we count, where we
  count it, and how that survives being gamed.
- Whatever else belongs on a card (last updated, maintenance status, capability
  list, size).
- Privacy: what telemetry is acceptable to collect at all.

### 2.6 Install experience
- Installing from the store must be **smooth and automatic** — fetch, verify,
  place, register, enable — with no manual file handling.
- Update and removal on the same rails, including what happens to an
  extension's stored data on removal.

---

## 3. Open questions — developer mode

Treated as a **first-class product surface**, not a debug afterthought. The
stated bar: security matters, and so does making extension development
*enjoyable* rather than hell.

- A **developer mode inside the app**, in the spirit of Chrome's
  "Load unpacked": point Grain at a local extension and run it, without
  packaging, publishing, or signing.
- A real **build → run → inspect → fix** loop. Reload without restarting Grain.
- **Excellent debugging**: the author must be able to see what their extension
  is doing — logs, host calls and their results, capability denials, activation
  events, timing (especially against the 150 ms transform budget).
- **Excellent error handling**: errors must be legible and actionable, pointing
  at the author's code, not swallowed by the host or surfaced as an opaque
  strike. A denied capability should say which capability and why.
- What is the authoring story — a template/scaffold, a CLI, types for the
  `grain` API surface, docs?
- How developer mode interacts with the security model: unreviewed local code
  gets the same capability wall, with clear in-app indication that a
  developer-loaded extension is running.
- Prior art worth studying before designing: Chrome/Firefox extension dev
  surfaces, VS Code's Extension Development Host, Obsidian, Raycast.

---

## 4. What this gate blocks

- **Does not block Phase 3.** Phase 3 (settings schema, `workspace`, overlay,
  pill slots, store *slide-over shell*, Grain Space Test) proceeds. If Phase 3
  builds a store surface, it builds the **shell only** — no index, no
  submission, no trust badges.
- **Blocks Phase 5 entirely** (index repo, browse/install/update, hash
  verification, trust badges, review checklist).
- **Blocks the trust-dependent parts of Phase 4.** Tier-C native extensions
  execute real binaries; shipping that through a distribution channel whose
  trust model is undesigned is exactly the wrong order.

## 5. Exit condition

This gate lifts when there is a written design covering: hosting shape, the
submission/review pipeline, the trust ladder with its anti-forgery guarantee,
the metrics set, the install/update/remove flow, and the developer-mode surface
with its debugging story — at the standard of the existing design docs, with a
prescriptive implementation guide to follow it.
