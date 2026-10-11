/**
 * Application shell: navigation, theme, and the startup gates.
 *
 * The shell owns exactly three things a page should not: loading settings, the
 * desktop-backend banner, and the onboarding gate.
 */
import { useEffect, useState } from "react";
import {
  Activity,
  Command,
  Cpu,
  History as HistoryIcon,
  LayoutDashboard,
  Settings as SettingsIcon
} from "lucide-react";
import clsx from "clsx";

import { APP_NAME, setStoredTheme } from "@/identity";
import { Toasts } from "@/components/Toasts";
import { Dot, Pill, TooltipProvider } from "@/components/ui";
import { Dashboard } from "@/pages/Dashboard";
import { Models } from "@/pages/Models";
import { HistoryPage } from "@/pages/History";
import { Commands } from "@/pages/Commands";
import { Diagnostics } from "@/pages/Diagnostics";
import { SettingsPage } from "@/pages/Settings";
import { Onboarding } from "@/pages/Onboarding";
import { useApp } from "@/stores/appStore";
import type { ThemeName } from "@/types";

type PageId = "dashboard" | "models" | "history" | "commands" | "settings" | "diagnostics";

const NAV: ReadonlyArray<{ id: PageId; label: string; icon: typeof Activity }> = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "models", label: "Speech Models", icon: Cpu },
  { id: "history", label: "History", icon: HistoryIcon },
  { id: "commands", label: "Voice Actions", icon: Command },
  { id: "settings", label: "Settings", icon: SettingsIcon },
  { id: "diagnostics", label: "Diagnostics", icon: Activity }
];

export function App() {
  const init = useApp((s) => s.init);
  const ready = useApp((s) => s.ready);
  const initError = useApp((s) => s.initError);
  const desktop = useApp((s) => s.desktop);
  const settings = useApp((s) => s.settings);
  const applySettings = useApp((s) => s.applySettings);
  const overlay = useApp((s) => s.overlay);

  const [page, setPage] = useState<PageId>("dashboard");

  useEffect(() => {
    void init();
  }, [init]);

  // Keep the document class in sync with the stored theme so the light/dark
  // choice survives a reload and follows the OS when set to "system".
  useEffect(() => {
    const theme: ThemeName = settings?.appearance.theme ?? "system";
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && query.matches);
      document.documentElement.classList.toggle("dark", dark);
    };
    apply();
    setStoredTheme(theme);
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, [settings?.appearance.theme]);

  if (!ready) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="hint">Starting {APP_NAME}…</p>
      </div>
    );
  }

  // Onboarding is gated on "no speech model chosen yet" rather than a separate
  // flag, so it cannot get out of sync with reality.
  const needsOnboarding = desktop && settings !== null && settings.speech.modelId === "";

  return (
    <TooltipProvider>
      <div className="flex h-full flex-col">
        {!desktop && initError ? (
          <div className="border-b border-warning/40 bg-warning/10 px-4 py-2 text-xs text-fg">
            {initError}
          </div>
        ) : null}

        {needsOnboarding ? (
          <Onboarding
            onDone={() => {
              // Onboarding writes settings itself; just move to the dashboard.
              setPage("dashboard");
            }}
          />
        ) : (
          <div className="flex min-h-0 flex-1">
            <nav
              aria-label="Sections"
              className="flex w-52 shrink-0 flex-col border-r border-border bg-surface"
            >
              <div className="px-3 py-3.5">
                <span className="text-sm font-semibold tracking-tight">{APP_NAME}</span>
              </div>

              <ul className="flex-1 space-y-0.5 px-1.5">
                {NAV.map((item) => {
                  const active = page === item.id;
                  return (
                    <li key={item.id}>
                      <button
                        type="button"
                        onClick={() => setPage(item.id)}
                        aria-current={active ? "page" : undefined}
                        className={clsx(
                          "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors",
                          active
                            ? "bg-elevated font-medium text-fg"
                            : "text-muted hover:bg-elevated/60 hover:text-fg"
                        )}
                      >
                        <item.icon className="h-4 w-4 shrink-0" aria-hidden />
                        {item.label}
                      </button>
                    </li>
                  );
                })}
              </ul>

              <div className="space-y-2 border-t border-border p-3">
                <StatusFooter />
              </div>
            </nav>

            <main className="min-h-0 flex-1 overflow-y-auto">
              <div className="mx-auto max-w-3xl px-6 py-6">
                {page === "dashboard" ? (
                  <Dashboard
                    onOpenModels={() => setPage("models")}
                    onOpenSettings={() => setPage("settings")}
                  />
                ) : null}
                {page === "models" ? <Models /> : null}
                {page === "history" ? <HistoryPage /> : null}
                {page === "commands" ? <Commands /> : null}
                {page === "settings" ? (
                  <SettingsPage onSettingsChange={(next) => void applySettings(next)} />
                ) : null}
                {page === "diagnostics" ? <Diagnostics /> : null}
              </div>
            </main>
          </div>
        )}
      </div>

      {/* The shell's own live state, so the window is useful while recording. */}
      {overlay.state !== "hidden" && !needsOnboarding ? (
        <div className="pointer-events-none fixed bottom-3 left-3">
          <Pill tone={overlay.state === "error" ? "danger" : "accent"}>
            <Dot tone={overlay.state === "error" ? "danger" : "accent"} />
            {overlay.state}
          </Pill>
        </div>
      ) : null}

      <Toasts />
    </TooltipProvider>
  );
}

/** Always-visible summary of the two facts that matter most. */
function StatusFooter() {
  const settings = useApp((s) => s.settings);
  const permissions = useApp((s) => s.permissions);

  if (!settings) return null;
  // "Cloud AI" means enhancement is on and pointed at a remote endpoint.
  const cloudAi =
    settings.enhancement.enabled && settings.enhancement.provider === "openai_compatible";

  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap gap-1.5">
        <Pill tone={cloudAi ? "warning" : "success"}>
          <Dot tone={cloudAi ? "warning" : "success"} />
          {cloudAi ? "Cloud AI enabled" : "Local only"}
        </Pill>
        {permissions?.microphone === "denied" ? (
          <Pill tone="danger">
            <Dot tone="danger" /> Mic blocked
          </Pill>
        ) : null}
        {permissions?.accessibility === "denied" ? (
          <Pill tone="warning">
            <Dot tone="warning" /> No auto-insert
          </Pill>
        ) : null}
      </div>
      <p className="text-2xs leading-relaxed text-faint">
        {settings.enhancement.enabled ? "AI enhancement on" : "AI enhancement off"}
      </p>
    </div>
  );
}
