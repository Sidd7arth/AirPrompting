/**
 * Typed bridge to the Rust core.
 *
 * Every call goes through `invoke`. When the page is opened in a plain browser
 * (`npm run dev` without Tauri) there is no backend: the bridge reports that
 * honestly instead of silently returning fabricated data (spec §109).
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { APP_NAME, EVENT_CHANNEL } from "@/identity";
import type {
  AirPromptEvent,
  AppProfile,
  CapabilityReport,
  CustomCommand,
  DiagnosticsSnapshot,
  DictionaryEntry,
  HistoryEntry,
  ModelInfo,
  ParsedCommand,
  PermissionStatus,
  Settings,
  ShortcutBinding,
  AiProviderKind,
  OverlayState
} from "@/types";

export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export class BackendUnavailableError extends Error {
  constructor() {
    super(
      `${APP_NAME} desktop backend is not running. Start the app with \`npm run tauri dev\`.`
    );
    this.name = "BackendUnavailableError";
  }
}

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop()) return Promise.reject(new BackendUnavailableError());
  return tauriInvoke<T>(command, args);
}

/** Subscribe to the single core event channel. No-ops in a browser. */
export async function subscribe(
  handler: (event: AirPromptEvent) => void
): Promise<UnlistenFn> {
  if (!isDesktop()) return () => {};
  return tauriListen<AirPromptEvent>(EVENT_CHANNEL, (e) => handler(e.payload));
}

export interface InputDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface ModelRecommendation {
  recommended: string;
  fastest: string;
  accurate: string;
  reason: string;
}

export interface SystemProfile {
  os: string;
  arch: string;
  cpuCores: number;
  totalRamMb: number;
  freeDiskMb: number;
  hasGpu: boolean;
  accelerator: string;
}

export interface UpdateInfo {
  version: string;
  notes: string;
  date: string | null;
}

export interface HistoryQuery {
  /** Free-text search over transcripts and app names. */
  search?: string;
  limit?: number;
  offset?: number;
  mode?: string;
  appName?: string;
}

export interface AiTestResult {
  ok: boolean;
  message: string;
  latencyMs: number | null;
}

export interface ActionOutcome {
  ok: boolean;
  message: string;
  /** True when the action needs the user's confirmation before running. */
  needsConfirmation: boolean;
  plan: string[];
}

/**
 * The whole core surface, in one place.
 *
 * Kept as a flat object rather than one wrapper function per call so the IPC
 * contract is readable at a glance and matches `src/commands.rs`.
 */
export const api = {
  /* ------------------------------------------------------------- lifecycle */
  capabilities: () => call<CapabilityReport>("capabilities"),

  /* -------------------------------------------------------------- settings */
  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (settings: Settings) =>
    call<Settings>("update_settings", { settings }),
  resetSettings: () => call<Settings>("reset_settings"),

  /* ----------------------------------------------------------- permissions */
  permissionStatus: () => call<PermissionStatus>("permission_status"),
  requestPermission: (kind: string) =>
    call<PermissionStatus>("request_permission", { kind }),
  openPermissionSettings: (kind: string) =>
    call<void>("open_permission_settings", { kind }),

  /* ---------------------------------------------------------------- models */
  listModels: () => call<ModelInfo[]>("list_models"),
  downloadModel: (id: string) => call<void>("download_model", { id }),
  pauseModelDownload: (id: string) => call<void>("pause_model_download", { id }),
  resumeModelDownload: (id: string) => call<void>("resume_model_download", { id }),
  deleteModel: (id: string) => call<void>("delete_model", { id }),
  verifyModel: (id: string) => call<boolean>("verify_model", { id }),
  setActiveModel: (id: string) => call<void>("set_active_model", { id }),
  recommendModel: () => call<ModelRecommendation>("recommend_model"),
  systemProfile: () => call<SystemProfile>("system_profile"),
  modelsDiskUsage: () => call<number>("models_disk_usage"),

  /* -------------------------------------------------------------- dictation */
  startDictation: (mode?: string) =>
    call<void>("start_dictation", { mode: mode ?? null }),
  startCommandMode: () => call<void>("start_command_mode"),
  stopDictation: () => call<void>("stop_dictation"),
  cancelDictation: () => call<void>("cancel_dictation"),
  overlayReady: () => call<void>("overlay_ready"),
  currentOverlayState: () => call<OverlayState>("current_overlay_state"),

  /* ---------------------------------------------------------------- history */
  historyList: (query: HistoryQuery = {}) =>
    call<HistoryEntry[]>("history_list", { query }),
  historyDelete: (id: number) => call<void>("history_delete", { id }),
  historyDeleteAll: () => call<void>("history_delete_all"),
  historyExport: (format: "txt" | "markdown" | "json") =>
    call<string>("history_export", { format }),
  pasteLast: () => call<void>("paste_last"),
  copyToClipboard: (text: string) => call<void>("copy_to_clipboard", { text }),

  /* ------------------------------------------------------------- dictionary */
  dictionaryList: () => call<DictionaryEntry[]>("dictionary_list"),
  dictionaryUpsert: (entry: Omit<DictionaryEntry, "id"> & { id?: number }) =>
    call<DictionaryEntry>("dictionary_upsert", { entry }),
  dictionaryDelete: (id: number) => call<void>("dictionary_delete", { id }),

  /* --------------------------------------------------------------- commands */
  commandsList: () => call<CustomCommand[]>("commands_list"),
  commandsUpsert: (command: CustomCommand) =>
    call<CustomCommand>("commands_upsert", { command }),
  commandsDelete: (id: string) => call<void>("commands_delete", { id }),
  parseCommand: (text: string) => call<ParsedCommand | null>("parse_command", { text }),
  runCommand: (text: string) => call<ActionOutcome>("run_command", { text }),
  executeCommand: (command: ParsedCommand) =>
    call<ActionOutcome>("execute_command", { command }),
  /** Installed applications / shortcuts available to the command engine. */
  discoverTargets: () =>
    call<{ apps: string[]; shortcuts: string[] }>("discover_targets"),
  runShortcut: (name: string) => call<ActionOutcome>("run_shortcut", { name }),

  /* ----------------------------------------------------------- app profiles */
  profilesList: () => call<AppProfile[]>("profiles_list"),
  profilesUpsert: (profile: Omit<AppProfile, "id"> & { id?: number }) =>
    call<AppProfile>("profiles_upsert", { profile }),
  profilesDelete: (id: number) => call<void>("profiles_delete", { id }),

  /* ------------------------------------------------------------------- audio */
  inputDevices: () => call<InputDevice[]>("input_devices"),
  startMicTest: () => call<void>("start_mic_test"),
  stopMicTest: () => call<void>("stop_mic_test"),

  /* ------------------------------------------------------------- ai / cloud */
  testAi: () => call<AiTestResult>("test_ai"),
  listAiModels: () => call<string[]>("list_ai_models"),
  setApiKey: (provider: AiProviderKind, key: string) =>
    call<void>("set_api_key", { provider, key }),
  apiKeyStatus: (provider: AiProviderKind) =>
    call<boolean>("api_key_status", { provider }),
  deleteApiKey: (provider: AiProviderKind) =>
    call<void>("delete_api_key", { provider }),

  /* ----------------------------------------------------------- file transcribe */
  transcribeFile: (path: string) =>
    call<HistoryEntry>("transcribe_file", { path }),

  /* ------------------------------------------------------------ diagnostics */
  diagnostics: () => call<DiagnosticsSnapshot>("diagnostics"),
  diagnosticsText: () => call<string>("diagnostics_text"),
  setLogLevel: (level: string) => call<void>("set_log_level", { level }),
  logFilePath: () => call<string>("log_file_path"),
  revealLogFile: () => call<void>("reveal_log_file"),

  /* ----------------------------------------------------------------- update */
  checkUpdate: () => call<UpdateInfo | null>("check_update"),
  installUpdate: () => call<void>("install_update"),

  /* ---------------------------------------------------------------- widgets */
  setOverlayShortcutHint: (binding: ShortcutBinding | null) =>
    call<void>("set_overlay_shortcut_hint", { binding }),
  openDashboard: () => call<void>("open_dashboard"),
  quitApp: () => call<void>("quit_app")
};

export type Api = typeof api;
