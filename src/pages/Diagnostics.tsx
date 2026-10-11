/**
 * Diagnostics (spec §60, §98).
 *
 * Aggregate timings and machine facts only — never transcript content, never
 * audio. Anything shown here is safe to paste into a bug report, and there is a
 * button that assembles exactly that.
 */
import { useEffect } from "react";
import { Clipboard, FileText, RefreshCw } from "lucide-react";

import {
  Button,
  Card,
  Dot,
  Pill,
  SectionHeading,
  Select,
  Separator
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api } from "@/services/ipc";
import { formatLatency } from "@/services/format";
import { useApp } from "@/stores/appStore";
import type { LogLevel } from "@/types";

const LOG_LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];

export function Diagnostics() {
  const diagnostics = useApp((s) => s.diagnostics);
  const refreshDiagnostics = useApp((s) => s.refreshDiagnostics);
  const pushToast = useApp((s) => s.pushToast);
  const settings = useApp((s) => s.settings);
  const patchSettings = useApp((s) => s.patchSettings);

  useEffect(() => {
    void refreshDiagnostics();
  }, [refreshDiagnostics]);

  const copy = async () => {
    try {
      const text = await api.diagnosticsText();
      await api.copyToClipboard(text);
      pushToast({ level: "success", message: "Diagnostic information copied." });
    } catch {
      pushToast({ level: "error", message: "Could not copy diagnostics." });
    }
  };

  const latency = diagnostics?.latency;

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <SectionHeading
          title="Diagnostics"
          description="Machine facts and aggregate timings. No transcript content appears here."
        />
        <div className="flex shrink-0 gap-2">
          <Button onClick={() => void refreshDiagnostics()}>
            <RefreshCw className="h-3.5 w-3.5" aria-hidden />
            Refresh
          </Button>
          <Button variant="primary" onClick={() => void copy()}>
            <Clipboard className="h-3.5 w-3.5" aria-hidden />
            Copy diagnostics
          </Button>
        </div>
      </div>

      <Card className="divide-y divide-border px-4">
        <Row label="Operating system" value={diagnostics?.os ?? "—"} />
        <Row label="Architecture" value={diagnostics?.arch ?? "—"} />
        <Row label={`${APP_NAME} version`} value={diagnostics?.appVersion ?? "—"} />
        <Row label="Speech engine" value={diagnostics?.speechEngine ?? "—"} />
        <Row
          label="Model"
          value={
            diagnostics
              ? `${diagnostics.modelId || "none"}${diagnostics.modelReady ? "" : " (not ready)"}`
              : "—"
          }
        />
        <Row label="Microphone" value={diagnostics?.microphone ?? "System default"} />
        <Row label="Dictation shortcut" value={diagnostics?.hotkey ?? "—"} />
        <Row
          label="Accessibility"
          value={
            <Pill
              tone={
                diagnostics?.accessibility === "granted"
                  ? "success"
                  : diagnostics?.accessibility === "unsupported"
                    ? "neutral"
                    : "warning"
              }
            >
              <Dot
                tone={
                  diagnostics?.accessibility === "granted"
                    ? "success"
                    : diagnostics?.accessibility === "unsupported"
                      ? "neutral"
                      : "warning"
                }
              />
              {diagnostics?.accessibility ?? "unknown"}
            </Pill>
          }
        />
        <Row
          label="AI provider"
          value={
            diagnostics?.aiProvider && diagnostics.aiProvider !== "none"
              ? `${diagnostics.aiProvider} · ${diagnostics.aiModel || "no model"}`
              : "Not configured"
          }
        />
        <Row
          label="Privacy mode"
          value={
            <Pill tone={diagnostics?.localOnly ? "success" : "warning"}>
              <Dot tone={diagnostics?.localOnly ? "success" : "warning"} />
              {diagnostics?.localOnly ? "Local only" : "Cloud allowed"}
            </Pill>
          }
        />
        <Row
          label="CPU / memory"
          value={
            diagnostics
              ? `${diagnostics.cpuPercent?.toFixed(1) ?? "—"}% · ${
                  diagnostics.memoryMb?.toFixed(0) ?? "—"
                } MB`
              : "—"
          }
        />
      </Card>

      <section>
        <SectionHeading
          title="Timings"
          description="Rolling medians in milliseconds, measured on this machine. Nothing is uploaded."
        />
        <Card className="divide-y divide-border px-4">
          <Row label="Shortcut → audio capture" value={formatLatency(latency?.hotkeyToAudioMs)} />
          <Row label="Audio capture → first partial" value={formatLatency(latency?.firstPartialMs)} />
          <Row label="Speech end → final transcript" value={formatLatency(latency?.finalTranscriptMs)} />
          <Row label="Enhancement" value={formatLatency(latency?.enhancementMs)} />
          <Row label="Text insertion" value={formatLatency(latency?.insertionMs)} />
          <Row label="Total, shortcut → inserted" value={formatLatency(latency?.totalMs)} />
        </Card>
      </section>

      <Separator />

      <Card className="divide-y divide-border px-4">
        <div className="row">
          <div>
            <p className="text-sm">Log level</p>
            <p className="hint mt-0.5">
              Production default is Info. Debug and Trace add detail but never
              transcript content.
            </p>
          </div>
          <div className="w-40">
            <Select
              ariaLabel="Log level"
              value={settings?.logLevel ?? "info"}
              onValueChange={(value) =>
                void patchSettings((draft) => {
                  draft.logLevel = value as LogLevel;
                  return draft;
                }).then(() => api.setLogLevel(value))
              }
              options={LOG_LEVELS.map((level) => ({
                value: level,
                label: level.charAt(0).toUpperCase() + level.slice(1)
              }))}
            />
          </div>
        </div>

        <div className="row">
          <div>
            <p className="text-sm">Log file</p>
            <p className="hint mt-0.5">Share this when reporting a problem.</p>
          </div>
          <Button
            size="sm"
            onClick={() =>
              void api
                .revealLogFile()
                .catch(() => pushToast({ level: "error", message: "Could not open the log folder." }))
            }
          >
            <FileText className="h-3.5 w-3.5" aria-hidden />
            Reveal
          </Button>
        </div>
      </Card>
    </div>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="row">
      <span className="text-sm text-muted">{label}</span>
      <span className="max-w-[60%] truncate text-right text-sm">{value}</span>
    </div>
  );
}
