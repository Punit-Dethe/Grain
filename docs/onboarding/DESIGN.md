---
name: Grain onboarding
description: Scoped design record for native three-step first-run setup.
colors:
  light-paper: "#fcfcfb"
  light-bg-1: "#f5f4f0"
  light-bg-3: "#eeebe3"
  light-text-1: "#1a1a1a"
  light-text-2: "#45433f"
  light-text-3: "#625f59"
  light-hairline: "rgba(26, 26, 26, 0.1)"
  light-danger: "#a34436"
  dark-bg-0: "#171717"
  dark-bg-1: "#202020"
  dark-bg-2: "#252525"
  dark-bg-3: "#303030"
  dark-text-1: "#f1f1ef"
  dark-text-2: "#d2d2cf"
  dark-text-3: "#b0b0ad"
  dark-hairline: "rgba(255, 255, 255, 0.065)"
  dark-accent: "#e4e4df"
  dark-danger: "#ff938a"
typography:
  headline:
    fontFamily: '"IBM Plex Sans Variable", "Segoe UI Variable Text", "Segoe UI", ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif'
    fontSize: "28px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.03em"
  title:
    fontSize: "14px"
    fontWeight: 550
  body:
    fontSize: "14px"
    lineHeight: 1.5
  label:
    fontSize: "12px"
    fontWeight: 500
rounded:
  control: "7px"
  panel: "12px"
spacing:
  tight: "8px"
  related: "12px"
  group: "16px"
  inset: "18px"
  generous: "24px"
components:
  button-primary-light:
    backgroundColor: "{colors.light-text-1}"
    textColor: "{colors.light-paper}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "38px"
  button-primary-dark:
    backgroundColor: "{colors.dark-accent}"
    textColor: "{colors.dark-bg-0}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "38px"
  select-light:
    backgroundColor: "{colors.light-bg-1}"
    textColor: "{colors.light-text-1}"
    rounded: "{rounded.control}"
    padding: "8px 32px 8px 12px"
    height: "40px"
  select-dark:
    backgroundColor: "{colors.dark-bg-2}"
    textColor: "{colors.dark-text-1}"
    rounded: "{rounded.control}"
    padding: "8px 32px 8px 12px"
    height: "40px"
---

# Design System: Grain onboarding

## Overview

**Creative North Star: "Grain's setup workbench"**

This record applies only to onboarding inside Grain's established visual identity. A compact horizontal header tracks exactly three tasks: microphone configuration, model downloads, and shortcut configuration. Completing shortcuts opens the application. The Grain mark, inherited type, neutral surfaces, and smaller controls keep setup calm and focused.

Source authority is `src/app/components/onboarding/`, shared `WindowChrome.tsx`, scoped `GrainApp.tsx` integration, and inherited `src/app/app.css` tokens. [PRODUCT.md](../../PRODUCT.md) supplies product constraints. The implemented source wins over the surface brief.

**Key Characteristics:**

- Warm paper in light mode and neutral charcoal in dark mode.
- IBM Plex Sans, thin borders, compact controls, and space between form groups.
- One centered configuration form per step, with no mode tour or practice task.
- Native window chrome, a scrollable stage, and an always-reachable footer.

**Verification boundary:** this is a source-derived record. Rendered screenshots and real-application visual verification were unavailable for this revision. Run `bun run dev:onboarding` from the repository root to review the real Tauri application; user visual approval is required before accepting the design.

## Colors

The frontmatter records the reused light and dark values. Runtime controls inherit the application's `--bg-*`, `--text-*`, `--hairline`, and `--accent` tokens.

### Primary

Primary actions, current-step markers, and download progress use the inherited accent and accent-ink pairing. Hover uses the inherited accent-hover value. Keep these theme-relative assignments.

### Neutral

The base surface holds the header; the first background tone holds the stage. The second tone holds form controls and model cards; the third marks selected model families and quiet hover states. Hairlines separate the header, cards, and footer.

### Named Rules

**The State Color Rule.** The onboarding danger color marks model errors. Completed steps and installed models use checks and status text; never rely on color alone.

## Typography

**Display and Body Font:** the inherited IBM Plex Sans Variable stack. Shortcut values retain the incumbent monospace treatment.

### Hierarchy

- **Headline:** the current task, with balanced wrapping and the compact override in Layout.
- **Title:** model-family and shortcut row headings.
- **Body:** the shell default. Introductory copy uses (13px), a line height of (1.6), and a maximum measure of (65ch).
- **Label:** field names. Controls use (13px); supporting copy and status use (11–12px).

## Layout

The shell stacks native chrome, a horizontal journey header, and a shrink-safe workbench. The header pairs the Grain mark with three equally spaced progress items. The workbench contains a `minmax(0, 1fr)` stage and persistent footer. Task content, navigation, and footer contents are bounded to (640px); the form centers within the stage. Only the stage scrolls, with keyboard focus available. Wide stage padding is (40px 36px).

At widths of (760px) or less, the header wraps the stepper below the brand, stage padding becomes (28px 24px), and headings reduce to (26px). At (580px) or less, progress labels remain visible below their markers, model-card fields stack, permission actions wrap below their descriptions, and shortcut rows wrap. Download progress moves below its status row. At heights of (650px) or less, header and stage padding compact. Preserve readable controls and space between groups.

**The Reachable Actions Rule.** Preserve the native chrome and footer outside the content scroller. Let task content scroll instead of imposing a tall minimum stage. Native window sizing uses the actual monitor work area; CSS must remain shrink-safe when the available area is smaller.

## Elevation & Depth

The onboarding shell uses tonal layering and fine borders rather than card shadows or decorative imagery. Focus uses the inherited accent-focus outline (2px) with an offset (3px), without glow or shadow.

## Shapes

Controls use the control radius; model-family, permission, and shortcut panels share the panel radius. Step markers are compact rounded squares (24px) with (6px) corners. Use the existing Lucide icons and Grain mark.

## Components

### Buttons and fields

Primary actions are compact, solid, and theme-relative, with minimum heights matching the recorded component tokens. Back, refresh, and cancellation actions use quiet text styling with tonal hover feedback. Disabled buttons remain visible at half opacity. Native selects retain associated labels, bounded width, and ellipsis. The microphone picker is slightly taller (48px) and includes a microphone icon.

### Three-step progress

The ordered journey covers Microphone, Models, and Shortcuts. The current item uses `aria-current="step"`; completed items show checks. Progress items are informational, not navigation controls. Task headings receive focus once when each step appears; the observer waiting for a delayed heading is disconnected after focus or unmount.

### Microphone configuration

The first form provides a persisted device picker and a device refresh action. Required operating-system permissions stay within this step: microphone access on Windows and macOS, plus accessibility access for macOS shortcuts. Permission checks, request/wait states, and recovery feedback remain visible in the same shell. Continue stays disabled while required access or device-setting updates are pending. Permission polling stops when its target is granted, on error, or on unmount.

There is no input meter, waveform, recording check, transcription test, or test command/event lifecycle in onboarding.

### Model-family cards

Batch and native ASR are independent real checkboxes, each paired with a native model picker. Preserve the parent-owned family/model draft on Back. Zero selections are valid editing state with guidance and a disabled installation action. Download size excludes installed models; progress, extraction, verification, errors, retry, and cancellation reflect actual model-store state. Family controls and Back are disabled during installation. Cancellation stops the active download and queued installation; unmount also cancels the active download. Successful installation advances directly to shortcut configuration.

### Shortcut configuration

The final form reuses Grain's existing shortcut editor. It shows the batch transcription shortcut for the selected batch family and the native ASR shortcut for the selected ASR family. Shortcut values retain their monospace treatment in compact fields. Back returns to model downloads with the shared draft intact; Finish enters the application. Both actions wait while settings load, shortcut capture starts or runs, or a binding save is pending. Native capture completion after unmount and late event subscriptions release their resources.

There is no mode walkthrough or practice capture between configuration and the application.

## Do's and Don'ts

### Do:

- **Do** inherit Grain's theme, font, mark, icons, and native chrome.
- **Do** preserve the scrollable stage, reachable footer, visible focus, and semantic labels.
- **Do** keep family selection independent and derive shortcut rows from the shared draft.
- **Do** use brief functional transitions (160ms); disable animation and transitions when reduced motion is requested.
- **Do** keep normal-state copy concise and show diagnostic or recovery detail when needed.
- **Do** verify visuals with `bun run dev:onboarding` in the real Tauri app. This debug replay does not reset installed models or saved settings.

### Don't:

- **Don't** promote this onboarding composition into a global rule for other Grain surfaces.
- **Don't** substitute simulated states for real download, permission, device-selection, or shortcut APIs.
- **Don't** add a mode tour, microphone test, or transcription practice step.
- **Don't** introduce external imagery, new runtime dependencies, autoplay decoration, or a tall fixed showcase.
- **Don't** use screenshot replicas, mock Tauri, or alternate visual render paths. Any authorized acceptance automation must exercise the real application through the maintained agent harness.
