/**
 * Shared domain types.
 *
 * These mirror the `serde` representations of the Rust core types in
 * `src-tauri/src/core`. Rust serialises enums in `snake_case`, so the string
 * unions below are the wire format — do not "prettify" them without changing
 * both sides.
 */

/* ------------------------------------------------------------------ theme */

export type ThemeName = "light" | "dark" | "system";

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

/* --------------------------------------------------------------- settings */

export type ActivationMode = "push_to_talk" | "toggle";
export type InsertMode = "paste" | "type" | "clipboard_only";
export type SilencePreset = "off" | "short" | "balanced" | "long";

export type SpeechEngineId = "whisper_cpp" | "apple_speech" | "parakeet";

export type EnhancementMode =
  | "natural"
  | "professional"
  | "email"
  | "casual"
  | "technical"
  | "minimal";

export type AiProviderKind =
  | "none"
  | "llama_cpp"
  | "ollama"
  | "lm_studio"
  | "openai_compatible";

export interface ShortcutBinding {
  /** Human-readable accelerator, e.g. "Alt+Space" or "CmdOrCtrl+Shift+Space". */
  accelerator: string;
  enabled: boolean;
}

export interface Shortcuts {
  dictation: ShortcutBinding;
  voiceCommand: ShortcutBinding;
  /** Optional secondary binding for the other activation mode. */
  pushToTalk: ShortcutBinding;
}

export interface SpeechSettings {
  engine: SpeechEngineId;
  /** Active model id, e.g. "whisper-base-en". Empty means "not chosen yet". */
  modelId: string;
  language: string;
  /** 0 = let the engine decide. */
  threads: number;
  useGpu: boolean;
  vadEnabled: boolean;
  silence: SilencePreset;
  inputDevice: string | null;
}

export interface EnhancementSettings {
  enabled: boolean;
  provider: AiProviderKind;
  /** Only for OpenAI-compatible providers. */
  endpoint: string;
  model: string;
  mode: EnhancementMode;
  customPrompt: string | null;
  removeFillers: boolean;
  autoPunctuation: boolean;
  autoCapitalization: boolean;
  /** Seconds. */
  timeoutSecs: number;
}

export interface PrivacySettings {
  /**
   * When true every network path is refused in Rust, not merely hidden in the UI.
   * The app stays fully functional offline.
   */
  localOnly: boolean;
  saveHistory: boolean;
  /** 0 = never. */
  historyRetentionDays: number;
  saveRecordings: boolean;
  telemetry: boolean;
}

export interface DictationSettings {
  activation: ActivationMode;
  insertMode: InsertMode;
  /** Insert a separating space when dictating again into the same app. */
  continuousDictation: boolean;
  /** Spoken punctuation: "comma" -> ",". */
  spokenPunctuation: boolean;
  trailingSpace: boolean;
  /** Lowercase the first letter and drop a trailing period (GAAV mode). */
  gaavMode: boolean;
  /** Restore focus to the captured target app before inserting. */
  restoreFocus: boolean;
}

export interface AppearanceSettings {
  theme: ThemeName;
  overlayEnabled: boolean;
  /** Overlay anchor as a fraction of the working area, 0..1. */
  overlayPosition: "top" | "center" | "bottom" | "cursor";
  overlayOpacity: number;
  showWaveform: boolean;
}

export interface GeneralSettings {
  launchAtLogin: boolean;
  startMinimized: boolean;
  showTrayIcon: boolean;
  autoCopyResult: boolean;
  /** Show "Downloading model 42%" inside the overlay. */
  showModelProgressInOverlay: boolean;
}

export interface Settings {
  general: GeneralSettings;
  dictation: DictationSettings;
  speech: SpeechSettings;
  enhancement: EnhancementSettings;
  privacy: PrivacySettings;
  appearance: AppearanceSettings;
  shortcuts: Shortcuts;
  logLevel: LogLevel;
}

/* ---------------------------------------------------------------- history */

export type DictationModeUsed = "standard" | "enhanced" | "command" | "file";

export interface HistoryEntry {
  id: number;
  /** Unix milliseconds. */
  createdAt: number;
  /** Milliseconds of captured audio, 0 when unknown. */
  durationMs: number;
  /** Target application name, empty when the platform could not report one. */
  appName: string;
  transcript: string;
  mode: DictationModeUsed;
  modelId: string;
  enhanced: boolean;
  /** Populated for command-mode entries. */
  commandIntent: string | null;
}

/* ----------------------------------------------------------------- models */

export type ModelStatus =
  | "not_downloaded"
  | "downloading"
  | "paused"
  | "verifying"
  | "ready"
  | "error";

export interface ModelInfo {
  id: string;
  name: string;
  tier: "tiny" | "base" | "small" | "medium";
  /** Bytes. */
  sizeBytes: number;
  languages: string;
  /** 1..5, for the sortable "speed" column. */
  speed: number;
  /** 1..5. */
  accuracy: number;
  /** Approximate resident memory in megabytes. */
  ramMb: number;
  description: string;
  status: ModelStatus;
  /** 0..1 while downloading. */
  progress: number;
  downloadedBytes: number;
  /** Absolute path once ready. */
  path: string | null;
  recommended: boolean;
  error: string | null;
}

/* --------------------------------------------------------------- commands */

export type CommandIntent =
  | "open_app"
  | "open_url"
  | "open_file"
  | "open_folder"
  | "run_shortcut"
  | "copy_text"
  | "search_web"
  | "dictate"
  | "stop_dictation"
  | "change_model"
  | "open_settings";

export type RiskLevel = "low" | "medium" | "high";

export interface ParsedCommand {
  intent: CommandIntent;
  target: string;
  /** 0..1. */
  confidence: number;
  risk: RiskLevel;
  /** Human sentence describing what will happen, for the confirmation UI. */
  summary: string;
  /** True when the deterministic parser handled it (no model involved). */
  local: boolean;
}

export interface ActionStep {
  intent: CommandIntent;
  target: string;
}

export interface CustomCommand {
  id: string;
  name: string;
  triggers: string[];
  steps: ActionStep[];
  enabled: boolean;
  requiresConfirmation: boolean;
}

/* ------------------------------------------------------------- dictionary */

export interface DictionaryEntry {
  id: number;
  /** What the recogniser tends to produce. */
  spoken: string;
  /** What the user actually wants. */
  replacement: string;
  /** Case-insensitive whole-word match. */
  wholeWord: boolean;
  enabled: boolean;
}

/* ------------------------------------------------------------ app profiles */

export interface AppProfile {
  id: number;
  /** Matched case-insensitively as a substring of the target app name. */
  appPattern: string;
  insertMode: InsertMode | null;
  enhancementMode: EnhancementMode | null;
  enhancementEnabled: boolean | null;
  enabled: boolean;
}

/* ------------------------------------------------------------- overlay/ui  */

export type OverlayState =
  | { state: "hidden" }
  | { state: "starting" }
  | { state: "listening" }
  | { state: "transcribing" }
  | { state: "enhancing" }
  | { state: "inserting" }
  | { state: "success"; text: string }
  | { state: "error"; message: string };

export type OverlayStateName = OverlayState["state"];

/* ------------------------------------------------------------ permissions */

export type PermissionState = "not_determined" | "granted" | "denied" | "unsupported";

export interface PermissionStatus {
  microphone: PermissionState;
  /** macOS: Accessibility (AXUIElement). Windows: always "granted". */
  accessibility: PermissionState;
  /** macOS: Input Monitoring, needed for a global event tap. */
  inputMonitoring: PermissionState;
}

/* ------------------------------------------------------------ diagnostics */

export interface DiagnosticsSnapshot {
  os: string;
  arch: string;
  appVersion: string;
  speechEngine: string;
  modelId: string;
  modelReady: boolean;
  microphone: string;
  hotkey: string;
  accessibility: PermissionState;
  aiProvider: string;
  aiModel: string;
  localOnly: boolean;
  /**
   * Rolling latency percentiles in milliseconds. Aggregate numbers only —
   * never transcript content (spec §60, §94).
   */
  latency: {
    hotkeyToAudioMs: number | null;
    firstPartialMs: number | null;
    finalTranscriptMs: number | null;
    enhancementMs: number | null;
    insertionMs: number | null;
    totalMs: number | null;
  };
  /** Process metrics, refreshed on demand. */
  cpuPercent: number | null;
  memoryMb: number | null;
}

/* ----------------------------------------------------------------- events  */

export interface ModelProgressEvent {
  id: string;
  status: ModelStatus;
  progress: number;
  downloadedBytes: number;
  error: string | null;
}

export interface ToastEvent {
  level: "info" | "success" | "warning" | "error";
  message: string;
}

/** Everything the Rust core pushes to the UI over one event channel. */
export type AirPromptEvent =
  | { type: "overlay"; state: OverlayState }
  | { type: "level"; level: number }
  | { type: "transcript"; committed: string; interim: string }
  | { type: "model"; model: ModelProgressEvent }
  | { type: "history"; entry: HistoryEntry }
  | { type: "permissions"; status: PermissionStatus }
  | { type: "toast"; toast: ToastEvent }
  | { type: "command"; command: ParsedCommand | null; raw: string }
  | { type: "diagnostics"; snapshot: DiagnosticsSnapshot };

/* ------------------------------------------------------- capability report */

export interface CapabilityReport {
  /** False when running in a plain browser (npm run dev without Tauri). */
  desktop: boolean;
  platform: string;
  /** Features the current platform/backend genuinely supports. */
  supports: {
    accessibility: boolean;
    systemShortcuts: boolean;
    shortcutsApp: boolean;
    tray: boolean;
    launchAtLogin: boolean;
    autoUpdate: boolean;
  };
}
