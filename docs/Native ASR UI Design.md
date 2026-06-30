# Native ASR UI Design Document

This document outlines the visual and behavioral design for the Native ASR frontend. It specifies *how* the UI should look and feel, serving as the blueprint for the UI implementation agent. 

The backend events and state management (`native_asr_enabled`, `selected_asr_model`, `DaemonEvent`s) are already implemented. The UI agent's job is purely to build the React components to match the designs below.

---

## 1. Model Management & Settings (Model Library)

**Design Inspiration:** The existing Batch/Rolling Model Library (`ModelLibrary.tsx`).

**Visuals & Layout:**
- We will replicate the exact visual design of the current model browser.
- The current model selection UI (which shows title, subtext, languages, accuracy, speed, comparison, model size, and sorting) will be duplicated.
- **Top Section:** Rename the existing local model section to something consumer-friendly like "Batch / Rolling Models".
- **Bottom Section:** Create an identical section directly below it for "Streaming / ASR Models".
- Users can browse, view specs, and select one model for Batch/Rolling and one model for ASR independently (or select nothing).
- This keeps the UI unified; the user just sees two categories of models they can download and select using the exact same beautiful interface.

---

## 2. The Transcription Overlay (The "Studio Window" Box)

**Design Inspiration:** A premium, contained overlay box with the dot-matrix pill embedded in the top right.

**Visuals & Layout:**
- **The Box:** When dictation begins, a large, premium rectangular window appears. It features a blurred dark background (glassmorphism) or a solid pitch-black fill.
- **The Pill Anchor:** The classic dot-matrix indicator is embedded neatly in the top-right corner of this box, reading "Transcribing..." next to it. This keeps the familiar status indicator while making room for text.
- **The Text Area:** Below the pill, the transcribed text streams freely. Because it is a box, it naturally supports text wrapping for long, multi-line dictations without breaking the aspect ratio or symmetry of the UI.
- **Screen Positioning:** Because the box takes up space, users should be able to choose its position on the screen (e.g., Center, Bottom, Top) so it fits their specific workflow and avoids blocking their taskbar.
- **Partial vs Committed:** 
  - *Committed Text* (`AsrCommit` events): Solid, standard text.
  - *Tentative Text* (`AsrPartial` events): Visually distinct (e.g., italicized, faded, grayed out, or blurred) to indicate the engine is still processing and guessing the end of the sentence.
- **The Processing & Finalization State:**
  - When the user stops dictating (triggering finalization), the embedded dot-matrix pill switches to its standard "Processing" animation state.
  - The preview text freezes exactly where it was (no further visual changes to the text box).
  - Once the backend completes finalization, the entire Preview Window overlay smoothly disappears, and the final text is automatically pasted into the user's active window (handled by the backend).

---

## 3. The Activation Trigger (Shortcuts & Quick Panel)

**Design Inspiration:** Zero friction. No extra menus to click through before dictating.

**Visuals & Layout:**
- Native ASR is triggered entirely by a **Global Shortcut**, exactly like Batch and Rolling. The engine handles all loading and unloading automatically in the background.
- **Settings Tab (General > Hotkeys):** 
  - Display the shortcuts sequentially: Batch, Rolling, and Native ASR one after another.
- **Quick Panel (Top-Left Corner):**
  - In the "Voice to AI" shortcut display area of the Quick Panel, update it to show the 3 available shortcuts side-by-side: Batch, Rolling, and Native ASR.
  - **No other changes** should be made to the Quick Panel. The rest of the panel remains exactly as it is today.
