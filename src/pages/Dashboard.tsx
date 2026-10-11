/**
 * Dashboard (spec §73).
 *
 * Answers three questions at a glance: is it ready, where does the audio go,
 * and what did I dictate recently.
 */
import { useEffect } from "react";
import { Copy, Mic, MicOff, Settings2, Sparkles, Square } from "lucide-react";

import { Waveform, LevelMeter } from "@/components/Waveform";
import {
  Button,
  Card,
  Dot,
  Pill,
  SectionHeading,
  Separator,
  Tooltip
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api } from "@/services/ipc";
import { acceleratorChips, formatHistoryStamp, preview } from "@/services/format";
import { useApp } from "@/stores/appStore";

export function Dashboard({
  onOpenModels,
  onOpenSettings
}: {
  onOpenModels: () => void;
  onOpenSettings: () => void;
}) {
  const settings = useApp((s) => s.settings);
  const models = useApp((s) => s.models);
  const history = useApp((s) => s.history);
  const overlay = useApp((s) => s.overlay);
  const level = useApp((s) => s.level);
  const permissions = useApp((s) => s.permissions);
  const pushToast = useApp((s) => s.pushToast);
  const refreshHistory = useApp((s) => s.refreshHistory);

  // The dashboard shows a short list; refresh when it becomes visible.
  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  if (!settings) return null;

  const activeModel = models.find((m) => m.id === settings.speech.modelId);
  const modelReady = activeModel?.status === "ready";
  const listening = overlay.state === "listening" || overlay.state === "starting";
  const busy =
    overlay.state === "transcribing" ||
    overlay.state === "enhancing" ||
    overlay.state === "inserting";

  const cloudAi =
    settings.enhancement.enabled && settings.enhancement.provider === "openai_compatible";

  const startDictation = async () => {
    try {
      await api.startDictation("standard");
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const stopDictation = async () => {
    try {
      await api.stopDictation();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const copy = async (text: string) => {
    try {
      await api.copyToClipboard(text);
      pushToast({ level: "success", message: "Copied to the clipboard." });
    } catch {
      pushToast({ level: "error", message: "Could not copy to the clipboard." });
    }
  };

  return (
    <div className="space-y-6">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">{APP_NAME}</h1>
          <div className="mt-1.5 flex flex-wrap items-center gap-2">
            <Pill tone={listening ? "danger" : busy ? "warning" : modelReady ? "success" : "warning"}>
              <Dot
                tone={listening ? "danger" : busy ? "warning" : modelReady ? "success" : "warning"}
              />
              {listening
                ? "Listening"
                : busy
                  ? `${overlay.state}…`
                  : modelReady
                    ? "Ready"
                    : "Needs a model"}
            </Pill>
            <Pill tone={cloudAi ? "warning" : "success"}>
              <Dot tone={cloudAi ? "warning" : "success"} />
              {cloudAi ? "Cloud AI enabled" : "Local only"}
            </Pill>
            <Pill tone={settings.enhancement.enabled ? "accent" : "neutral"}>
              <Sparkles className="h-3 w-3" aria-hidden />
              Enhancement {settings.enhancement.enabled ? "on" : "off"}
            </Pill>
          </div>
        </div>

        <div className="flex shrink-0 gap-2">
          {listening || busy ? (
            <Button variant="danger" onClick={() => void stopDictation()}>
              <Square className="h-3.5 w-3.5" aria-hidden />
              Stop
            </Button>
          ) : (
            <Button
              variant="primary"
              onClick={() => void startDictation()}
              disabled={!modelReady}
              title={modelReady ? undefined : "Install a speech model first"}
            >
              <Mic className="h-4 w-4" aria-hidden />
              Start dictation
            </Button>
          )}
          <Tooltip content="Speech settings">
            <Button variant="ghost" onClick={onOpenSettings} aria-label="Open settings">
              <Settings2 className="h-4 w-4" aria-hidden />
            </Button>
          </Tooltip>
        </div>
      </header>

      {!modelReady ? (
        <Card className="p-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium">No speech model installed</p>
              <p className="hint mt-1 max-w-md">
                {APP_NAME} transcribes entirely on this machine. Download a model to get
                started — Base is the recommended balance of speed and accuracy.
              </p>
            </div>
            <Button variant="primary" onClick={onOpenModels}>
              Choose a model
            </Button>
          </div>
        </Card>
      ) : null}

      <section>
        <SectionHeading
          title="Local speech"
          description="Audio never leaves this machine unless you explicitly enable a cloud provider."
        />
        <Card className="divide-y divide-border">
          <div className="row px-4">
            <div>
              <p className="text-sm">Speech engine</p>
              <p className="hint mt-0.5">
                {settings.speech.engine === "whisper_cpp"
                  ? "whisper.cpp (local)"
                  : settings.speech.engine}
              </p>
            </div>
            <Button size="sm" variant="ghost" onClick={onOpenSettings}>
              Change
            </Button>
          </div>

          <div className="row px-4">
            <div>
              <p className="text-sm">Model</p>
              <p className="hint mt-0.5">
                {activeModel
                  ? `${activeModel.name} · ${activeModel.languages} · ${activeModel.description}`
                  : "Not selected"}
              </p>
            </div>
            <Button size="sm" variant="ghost" onClick={onOpenModels}>
              Manage
            </Button>
          </div>

          <div className="row px-4">
            <div className="min-w-0">
              <p className="text-sm">Microphone</p>
              <p className="hint mt-0.5">
                {permissions?.microphone === "denied"
                  ? "Access denied — check System Settings → Privacy & Security → Microphone."
                  : (settings.speech.inputDevice ?? "System default")}
              </p>
              {permissions?.microphone === "denied" ? (
                <MicOff className="mt-1 h-3.5 w-3.5 text-danger" aria-hidden />
              ) : null}
            </div>
            <div className="w-40">
              <Waveform level={level} active={listening} />
            </div>
          </div>

          <div className="row px-4">
            <div>
              <p className="text-sm">Shortcut</p>
              <p className="hint mt-0.5">
                Hold to talk · {settings.dictation.activation === "push_to_talk" ? "push to talk" : "toggle"}
              </p>
            </div>
            <div className="flex items-center gap-1">
              {acceleratorChips(settings.shortcuts.dictation.accelerator).map((chip, index) => (
                <kbd key={`${chip}-${index}`} className="kbd">
                  {chip}
                </kbd>
              ))}
            </div>
          </div>
        </Card>
      </section>

      <section>
        <SectionHeading title="Recent dictations" />
        <Card>
          {history.length === 0 ? (
            <p className="px-4 py-6 text-center text-xs text-faint">
              Nothing dictated yet. Press your shortcut and start speaking.
            </p>
          ) : (
            <ul className="divide-y divide-border">
              {history.slice(0, 5).map((entry) => (
                <li key={entry.id} className="group flex items-start gap-3 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    <p className="text-xs text-muted">
                      {formatHistoryStamp(entry.createdAt)}
                      {entry.appName ? ` · ${entry.appName}` : ""}
                    </p>
                    <p className="mt-1 line-clamp-2 text-sm leading-relaxed">
                      {preview(entry.transcript, 180)}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label="Copy transcript"
                    onClick={() => void copy(entry.transcript)}
                  >
                    <Copy className="h-3.5 w-3.5" aria-hidden />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </section>

      <section>
        <SectionHeading title="Microphone check" />
        <Card className="space-y-3 p-4">
          <LevelMeter level={level} active={listening} label="Input level" />
          <Separator />
          <div className="flex items-center justify-between">
            <p className="hint max-w-md">
              Speak normally — the bar should sit in the middle. If it stays flat,
              check your input device in Settings → Audio.
            </p>
            <Button
              size="sm"
              onClick={() => void (listening ? api.stopMicTest() : api.startMicTest())}
            >
              {listening ? "Stop test" : "Test microphone"}
            </Button>
          </div>
        </Card>
      </section>
    </div>
  );
}
