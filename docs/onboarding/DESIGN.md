---
name: Grain onboarding
description: Scoped design record for the native first-run setup workbench.
colors:
  light-paper: "#fcfcfb"
  light-bg-1: "#f5f4f0"
  light-bg-3: "#eeebe3"
  light-text-1: "#1a1a1a"
  light-text-2: "#45433f"
  light-text-3: "#625f59"
  light-hairline: "rgba(26, 26, 26, 0.1)"
  light-sage: "#7fb493"
  light-danger: "#a34436"
  dark-paper: "#0e1013"
  dark-bg-0: "#090a0c"
  dark-bg-1: "#0e1013"
  dark-bg-3: "#181c21"
  dark-text-1: "#f3f4f2"
  dark-text-2: "#b4b8b8"
  dark-text-3: "#7d8385"
  dark-hairline: "rgba(255, 255, 255, 0.065)"
  dark-sage: "#a8c9a8"
  dark-danger: "#ff938a"
typography:
  headline:
    fontFamily: '"IBM Plex Sans Variable", "Segoe UI Variable Text", "Segoe UI", ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif'
    fontSize: "clamp(27px, 3vw, 34px)"
    fontWeight: 630
    lineHeight: 1.15
    letterSpacing: "-0.03em"
  title:
    fontSize: "17px"
    fontWeight: 580
    letterSpacing: "-0.01em"
  body:
    fontSize: "14px"
    lineHeight: 1.55
  preview:
    fontSize: "26px"
    lineHeight: 1.42
    letterSpacing: "-0.025em"
  label:
    fontSize: "13px"
    fontWeight: 500
rounded:
  control: "8px"
  panel: "13px"
spacing:
  tight: "8px"
  related: "12px"
  group: "16px"
  inset: "18px"
  panel: "22px"
  generous: "24px"
components:
  button-primary-light:
    backgroundColor: "{colors.light-text-1}"
    textColor: "{colors.light-paper}"
    rounded: "{rounded.control}"
    padding: "0 16px"
    height: "40px"
  button-primary-dark:
    backgroundColor: "{colors.dark-text-1}"
    textColor: "{colors.dark-bg-0}"
    rounded: "{rounded.control}"
    padding: "0 16px"
    height: "40px"
  select-light:
    backgroundColor: "{colors.light-paper}"
    textColor: "{colors.light-text-1}"
    rounded: "{rounded.control}"
    padding: "9px 32px 9px 12px"
    height: "42px"
  select-dark:
    backgroundColor: "{colors.dark-paper}"
    textColor: "{colors.dark-text-1}"
    rounded: "{rounded.control}"
    padding: "9px 32px 9px 12px"
    height: "42px"
---

# Design System: Grain onboarding

## Overview

**Creative North Star: "Grain's setup workbench"**

This record applies only to onboarding. It records the Operate-mode overhaul inside Grain's established visual identity; it does not establish a global visual world. A quiet journey rail frames one focused setup task. The Grain mark, inherited type, and surfaces connect setup to the main application. The rail has no artwork: user feedback favored less decoration and more breathing room.

Source authority is `src/app/components/onboarding/`, shared `WindowChrome.tsx`, scoped `GrainApp.tsx` integration, and inherited `src/app/app.css` tokens. [PRODUCT.md](../../PRODUCT.md) supplies product constraints. The implemented source wins over the surface brief.

**Key Characteristics:**

- Warm paper and charcoal in light mode; existing Grain dark theme.
- IBM Plex Sans, thin borders, rounded task panels, and generous space between groups.
- One replayable mode preview replaces numbered instructions and repeated explanations.
- Native window chrome, a scrollable stage, and an always-reachable footer.

**Verification boundary:** this is a source-derived record. Production frontend build, lint, formatting, type checks, Rust check, settings parity, and 115 unit tests passed in the refinement session. The native build passed in the preceding implementation session. Rendered visual confirmation remains pending the user in the real Tauri application; no browser harness or automated screenshot was used.

## Colors

The frontmatter records the reused light and dark values. Runtime controls inherit `--color-paper`, `--bg-*`, `--text-*`, `--hairline`, and `--sage` from the application.

### Primary

Primary actions and selected modes use primary text against the base background. Hover uses secondary text. Keep this theme-relative pairing.

### Neutral

Paper holds the workbench; the first background tone groups related content. The third background tone marks completed steps and quiet hover states. Hairlines separate cards and footer regions.

### Named Rules

**The State Color Rule.** Sage reports a successful input/result state; the onboarding danger color reports recording or an error. Pair color with status text or an icon.

## Typography

**Display and Body Font:** the inherited IBM Plex Sans Variable stack. Shortcut values retain the incumbent monospace treatment.

### Hierarchy

- **Headline:** the frontmatter ramp for the current task; balanced wrapping, with compact overrides in Layout.
- **Title:** model-family and microphone headings.
- **Preview:** large transcript text (26px), reducing to (24px) on compact wide windows and (23px) on narrow windows.
- **Body:** introductory copy, limited to 65 characters per line where the layout allows.
- **Label:** field names and controls. Secondary status and supporting copy use (12–13px).

## Layout

The wide shell reserves a journey rail (254px) beside a shrink-safe workbench. The rail narrows to (224px) between (861–1050px). The workbench has a header, `minmax(0, 1fr)` stage, and footer. Its task width is bounded to (700px). Only the stage scrolls; its visible scrollbar and keyboard focus remain available.

At widths of (860px) or less, the journey becomes horizontal progress. At (580px) or less, progress labels yield to numbered steps while the workbench header retains the current step count; model cards stack and controls wrap. On wide windows at heights of (720px) or less, spacing and header height compact, and the task heading becomes (29px). Narrow headings use (28px), then (27px). Preserve readable controls and space between groups; cut redundant text before compressing the layout.

**The Reachable Actions Rule.** Preserve the native chrome and footer outside the content scroller. Let task content scroll instead of imposing a tall minimum stage. Native window sizing uses the actual monitor work area; CSS must remain shrink-safe when the available area is smaller.

## Elevation & Depth

The onboarding shell uses tonal layering and fine borders rather than card shadows or decorative imagery. Focus uses a secondary-text outline (2px) with an offset (3px), without glow or shadow.

## Shapes

Controls use the control radius; model, microphone, test, and shortcut panels share the panel radius. The illustrative preview uses (14px). The workbench is gently rounded (16px), reducing to (12px) at the narrow breakpoint. Progress numbers and microphone controls are circular. Use the existing Lucide icons and Grain mark.

## Components

### Buttons and fields

Primary actions are compact, solid, and theme-relative, with minimum heights matching the recorded component tokens. Back and Skip are quiet text actions with tonal hover feedback. Disabled buttons remain visible at half opacity. Native selects retain associated labels, bounded width, and ellipsis.

### Journey and mode navigation

The ordered five-step journey covers Microphone, Modes, Models, Try, and Shortcuts. The current item uses `aria-current="step"`; completed items show checks. Task headings receive focus once when each step appears. Mode selectors use pressed buttons in a labeled group.

Mode learning centers one clearly labeled illustrative preview. Play runs a finite recording/work lane and transcript sequence; Replay runs it again. Standard waits to process until capture stops. Flow processes earlier speech during capture; both batch modes show final text after stopping. Streaming reveals words during capture. One short takeaway supplies the model-family distinction. There are no autoplay loops or numerical latency claims. Reduced motion uses three manual Next stages with no timers or progressive word animation. Mode changes reset the preview; dispose all timers and the motion-preference listener. Announce status and the final phrase, not every word.

### Model-family cards

Batch and native ASR are independent real checkboxes, each paired with a native model picker. Batch enables Standard and compatible Flow; native ASR enables Streaming. Preserve the parent-owned family/model draft on Back. Zero selections are valid editing state with guidance and a disabled installation action. Download size excludes installed models; progress, extraction, verification, errors, retry, and cancellation reflect actual model-store state.

### Input, practice, and shortcuts

The microphone meter reflects actual input. Permission, signal, success, and error copy stays visible within the same shell. Try and Shortcuts expose only modes supported by the chosen setup: ASR-only exposes Streaming; Batch exposes Standard and compatible Flow. Flow is omitted for unreviewed models or translation-to-English settings. Flow practice is a batch-model check; rolling capture is used through its shortcut after setup.

Practice status and live output report real Tauri commands/events. Cancel capture on mode change, leaving the step, unmount, or native close; release listeners and timers. Shortcuts reuse Grain's existing shortcut editor.

## Do's and Don'ts

### Do:

- **Do** inherit Grain's theme, font, mark, icons, and native chrome.
- **Do** preserve the scrollable stage, reachable footer, visible focus, and semantic labels.
- **Do** keep family selection independent and derive Try/Shortcuts availability from the shared draft.
- **Do** use brief functional transitions (160–200ms); honor reduced motion with manual preview stages and no animation or transitions.
- **Do** keep normal-state copy concise and show diagnostic or recovery detail when needed.
- **Do** verify visuals with `bun run dev:onboarding` in the real Tauri app. This debug replay does not reset installed models or saved settings.

### Don't:

- **Don't** promote this rail/workbench composition into a global rule for other Grain surfaces.
- **Don't** substitute simulated states for real download, permission, microphone, or Try APIs; the learning preview is explicitly illustrative.
- **Don't** present the Flow practice check as the rolling-capture experience.
- **Don't** introduce external imagery, new runtime dependencies, autoplay decoration, or a tall fixed showcase.
- **Don't** use browser/computer automation, screenshot replicas, mock Tauri, or alternate visual render paths.
