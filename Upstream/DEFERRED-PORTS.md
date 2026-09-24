# Deferred Handy runtime ports

**Read this before the next upstream sync.** The 2026-09-23 merge incorporated
Handy through `8f9cf53cd1410cda26beea39ff802ac306e39585` into Git ancestry,
but the behavior below was deliberately **not** adopted by Grain. A zero-behind
count and a passing divergence ratchet do not close these items. Keep them
deferred until separately implemented and verified; do not partially copy their
Handy code across Grain's capture, Flow, pill, or window contracts.

| Handy commit (exact SHA and subject) | What remains deferred in Grain | Next review / completion evidence |
| --- | --- | --- |
| `5ec2276a40ddfa1276ffa4fd845763e9dc31d1c3` — single writer tray icon (#1952) | Serialized tray updates and race handling. Grain retained its branded tray and native-pill transitions. | Adapt the state writer to Grain's tray and session events; test rapid start/stop and Secure Input changes. |
| `20ada47d7f98f916d554cf883940dcb83aa9143f` — experimental earshot vad implementation (#1967) | Optional Earshot backend, selector, dependencies, and variable-frame VAD integration. Grain still uses its Silero capture contract. | Decide whether Grain should offer Earshot, then review recorder, audio manager, settings, and rolling together. |
| `c6fa60da2f13a5af660fba17f37af548855119c5` — fix(shortcut): keep toggle parity when presses arrive mid-pipeline (#1910); `c62a5fcdef4196e0ab36ea56cd3863f8f17fd9c5` — auto push to talk mode (#1971) | Busy-key parity and hold-or-toggle activation. Grain kept its action-ID/Flow routing and current push-to-talk contract. | Design and test the coordinator, shortcut handler, settings migration, and Flow start/stop behavior as one change. |
| `00d255492d3aa3585c762142d54273447b6752bc` — preserve held Secure Input shortcuts during reconciliation (#2002) | macOS registration preservation while a shortcut is physically held. | Adapt to Grain's shortcut registration and test held keys while Secure Input toggles. |
| `fbd4e15fa14a721c66c57006ae110428b9e255b3` — launch as accessory (#2003) | macOS accessory startup and Dock activation lifecycle. | Review Grain's native pill, Agent window, and main-window activation before changing launch policy; test on macOS. |
| `d54c88eb35abab47cdf1d0e2e82add346cc2a9b9` — make microphone callback real-time safe (#1954) | Recorder callback/ring-buffer rewrite and its standalone tests. Grain retained bounded rolling and conditioning hooks. | Port the recorder and audio-manager contract together; verify no lost samples, capture cancellation, Flow, and resource cleanup. |
| `bf8756c3a88a59c964259b929d6cd99f2059a115` — fix: disable WebView2 browser accelerator keys in the main window (#2060) | Accelerator suppression in Grain's lazy Windows main-window builder. The dependency is already present. | Apply the setting at Grain's actual window creation point and test typing, navigation, and global shortcuts on Windows. |

Related decision: `df2168326e360f7c52d57b896cc624fb66e1c035`
(stop losing tail audio when a recording ends, #1958) is **already covered** by
Grain's guarded resampler delay drain. Its upstream `finish()` replacement was
not copied because it would remove Grain's tail guarantee. Recheck this
contract if a later Handy recorder or resampler change touches it.

The detailed merge resolutions and budget rationale are in
[REVIEW-2026-09-23.md](REVIEW-2026-09-23.md). Structured decisions for mapped or
suppressed paths are in `verdicts.json`. Remove a deferred row only when the
Grain runtime destination, behavioral verification, and verdict note (where
applicable) are recorded together.
