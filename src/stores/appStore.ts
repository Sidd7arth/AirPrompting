/**
 * The single source of UI truth.
 *
 * One store (rather than a store plus contexts plus prop drilling) because the
 * dashboard, overlay and tray all observe the same handful of facts: current
 * overlay state, the live transcript, settings and the model list.
 */
import { create } from "zustand";
import { APP_NAME } from "@/identity";
import { api, isDesktop, subscribe } from "@/services/ipc";
import type {
  AirPromptEvent,
  CapabilityReport,
  DiagnosticsSnapshot,
  HistoryEntry,
  ModelInfo,
  ModelProgressEvent,
  OverlayState,
  PermissionStatus,
  Settings,
  ToastEvent
} from "@/types";

export interface Toast extends ToastEvent {
  id: number;
}

interface AppState {
  /* -------------------------------------------------------------- lifecycle */
  ready: boolean;
  /** False when running outside Tauri (plain browser). */
  desktop: boolean;
  initError: string | null;
  capabilities: CapabilityReport | null;

  /* ----------------------------------------------------------------- state */
  settings: Settings | null;
  models: ModelInfo[];
  permissions: PermissionStatus | null;
  diagnostics: DiagnosticsSnapshot | null;

  /* --------------------------------------------------------------- runtime */
  overlay: OverlayState;
  level: number;
  committed: string;
  interim: string;
  history: HistoryEntry[];
  toasts: Toast[];

  /* ---------------------------------------------------------------- actions */
  init: () => Promise<void>;
  applySettings: (next: Settings) => Promise<void>;
  patchSettings: (fn: (draft: Settings) => Settings) => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshHistory: (search?: string) => Promise<void>;
  refreshPermissions: () => Promise<void>;
  refreshDiagnostics: () => Promise<void>;
  refreshCapabilities: () => Promise<void>;
  pushToast: (toast: ToastEvent) => void;
  dismissToast: (id: number) => void;
  /** Local-only optimistic overlay update (used by the overlay window itself). */
  setOverlay: (state: OverlayState) => void;
}

let toastSeq = 0;

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  desktop: isDesktop(),
  initError: null,
  capabilities: null,
  settings: null,
  models: [],
  permissions: null,
  diagnostics: null,
  overlay: { state: "hidden" },
  level: 0,
  committed: "",
  interim: "",
  history: [],
  toasts: [],

  init: async () => {
    if (get().ready) return;
    if (!isDesktop()) {
      set({
        ready: true,
        desktop: false,
        initError:
          `Running in a browser without the ${APP_NAME} backend. Launch the desktop app with \`npm run tauri dev\`.`
      });
      return;
    }

    try {
      const [settings, capabilities, overlay] = await Promise.all([
        api.getSettings(),
        api.capabilities(),
        api.currentOverlayState().catch(() => ({ state: "hidden" }) as OverlayState)
      ]);
      set({ settings, capabilities, overlay });

      // Missing permissions must never block startup; they surface as prompts.
      await Promise.all([
        get().refreshPermissions(),
        get().refreshModels(),
        get().refreshHistory()
      ]);

      await subscribe(handleEvent(set, get));
      set({ ready: true, initError: null });
    } catch (error) {
      set({
        ready: true,
        initError: error instanceof Error ? error.message : String(error)
      });
    }
  },

  applySettings: async (next) => {
    const previous = get().settings;
    set({ settings: next }); // optimistic: the UI must never lag a toggle
    try {
      const saved = await api.updateSettings(next);
      set({ settings: saved });
    } catch (error) {
      set({ settings: previous });
      get().pushToast({
        level: "error",
        message: `Could not save settings: ${message(error)}`
      });
    }
  },

  patchSettings: async (fn) => {
    const current = get().settings;
    if (!current) return;
    await get().applySettings(fn(structuredClone(current)));
  },

  refreshModels: async () => {
    try {
      set({ models: await api.listModels() });
    } catch (error) {
      if (isDesktop()) {
        get().pushToast({ level: "error", message: `Models unavailable: ${message(error)}` });
      }
    }
  },

  refreshHistory: async (search) => {
    try {
      set({ history: await api.historyList(search ? { search } : {}) });
    } catch (error) {
      if (isDesktop()) {
        get().pushToast({ level: "error", message: `History unavailable: ${message(error)}` });
      }
    }
  },

  refreshPermissions: async () => {
    try {
      set({ permissions: await api.permissionStatus() });
    } catch {
      /* permissions are advisory; the UI degrades gracefully */
    }
  },

  refreshDiagnostics: async () => {
    try {
      set({ diagnostics: await api.diagnostics() });
    } catch (error) {
      get().pushToast({ level: "error", message: `Diagnostics unavailable: ${message(error)}` });
    }
  },

  refreshCapabilities: async () => {
    try {
      set({ capabilities: await api.capabilities() });
    } catch {
      /* ignore */
    }
  },

  pushToast: (toast) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { ...toast, id }] }));
    // Errors stay longer: they carry actionable text.
    const ttl = toast.level === "error" ? 8000 : 4000;
    window.setTimeout(() => get().dismissToast(id), ttl);
  },

  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  setOverlay: (overlay) => set({ overlay })
}));

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function handleEvent(
  set: (partial: Partial<AppState> | ((s: AppState) => Partial<AppState>)) => void,
  get: () => AppState
): (event: AirPromptEvent) => void {
  return (event) => {
    switch (event.type) {
      case "overlay":
        // A new dictation resets the live transcript.
        if (event.state.state === "starting" || event.state.state === "listening") {
          set({ overlay: event.state, committed: "", interim: "" });
        } else {
          set({ overlay: event.state });
        }
        break;

      case "level":
        set({ level: event.level });
        break;

      case "transcript":
        set({ committed: event.committed, interim: event.interim });
        break;

      case "model":
        set((s) => ({ models: mergeModel(s.models, event.model) }));
        break;

      case "history":
        set((s) => ({ history: [event.entry, ...s.history] }));
        break;

      case "permissions":
        set({ permissions: event.status });
        break;

      case "toast":
        get().pushToast(event.toast);
        break;

      case "diagnostics":
        set({ diagnostics: event.snapshot });
        break;

      case "command":
        // Command-mode results are rendered by the overlay from `overlay` state.
        break;
    }
  };
}

function mergeModel(models: ModelInfo[], update: ModelProgressEvent): ModelInfo[] {
  return models.map((m) =>
    m.id === update.id
      ? {
          ...m,
          status: update.status,
          progress: update.progress,
          downloadedBytes: update.downloadedBytes,
          error: update.error,
          // A finished download needs its real path, so re-read the catalog.
          path: update.status === "ready" ? m.path : m.path
        }
      : m
  );
}
