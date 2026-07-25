// Settings section components
export { GeneralSettings } from "./general/GeneralSettings";
export { AdvancedSettings } from "./advanced/AdvancedSettings";
export { DebugSettings } from "./debug/DebugSettings";
export { HistorySettings } from "./history/HistorySettings";
export { AboutSettings } from "./about/AboutSettings";
export { PostProcessingSettings } from "./post-processing/PostProcessingSettings";
export { SpeechToTextSettings } from "./speech-to-text/SpeechToTextSettings";
export { ExperimentationsSettings } from "./experimentations/ExperimentationsSettings";
// NOTE: GrainSpaceSettings is deliberately NOT re-exported here. This barrel is
// the sidebar's section components, and Grain Space no longer has a tab — it is
// a builtin-tier extension whose settings render as the host view
// `grain://grain-space/settings` (see experimentations/hostViews.tsx).

// Individual setting components
export { MicrophoneSelector } from "./MicrophoneSelector";
export { ClamshellMicrophoneSelector } from "./ClamshellMicrophoneSelector";
export { OutputDeviceSelector } from "./OutputDeviceSelector";
export { AlwaysOnMicrophone } from "./AlwaysOnMicrophone";
export { PushToTalk } from "./PushToTalk";
export { AudioFeedback } from "./AudioFeedback";
export { ShowOverlay } from "./ShowOverlay";
export { GlobalShortcutInput } from "./GlobalShortcutInput";
export { HandyKeysShortcutInput } from "./HandyKeysShortcutInput";
export { ShortcutInput } from "./ShortcutInput";
export { TranslateToEnglish } from "./TranslateToEnglish";
export { CustomWords } from "./CustomWords";
export { PostProcessingToggle } from "./PostProcessingToggle";
export { PostProcessingSettingsApi } from "./PostProcessingSettingsApi";
export { PostProcessingSettingsPrompts } from "./PostProcessingSettingsPrompts";
export { AppDataDirectory } from "./AppDataDirectory";
export { ModelUnloadTimeoutSetting } from "./ModelUnloadTimeout";
export { StartHidden } from "./StartHidden";
export { HistoryLimit } from "./HistoryLimit";
export { RecordingRetentionPeriodSelector } from "./RecordingRetentionPeriod";
export { AutostartToggle } from "./AutostartToggle";
export { UpdateChecksToggle } from "./UpdateChecksToggle";
