/**
 * First run (spec §44, §45, §81).
 *
 * Six short steps, each of which performs a real action: request the microphone
 * permission, download a model, bind a shortcut, then confirm. Nothing is
 * skipped silently, and every permission is explained before it is requested.
 * The user can leave at any point and the app stays usable.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { ArrowRight, Check, Download, Mic, ShieldCheck, Sparkles } from "lucide-react";

import { ShortcutRecorder } from "@/components/ShortcutRecorder";
import {
  Button,
  Card,
  Dot,
  InfoNote,
  Pill,
  ProgressBar,
  Separator,
  ToggleRow
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api, type ModelRecommendation } from "@/services/ipc";
import { acceleratorChips, formatBytes } from "@/services/format";
import { useApp } from "@/stores/appStore";
import type { ModelInfo } from "@/types";

const STEPS = ["Welcome", "Microphone", "Shortcut", "Model", "Enhancement", "Ready"] as const;
type Step = (typeof STEPS)[number];

export function Onboarding({ onDone }: { onDone: () => void }) {
  const settings = useApp((s) => s.settings);
  const models = useApp((s) => s.models);
  const permissions = useApp((s) => s.permissions);
  const patchSettings = useApp((s) => s.patchSettings);
  const refreshModels = useApp((s) => s.refreshModels);
  const refreshPermissions = useApp((s) => s.refreshPermissions);
  const pushToast = useApp((s) => s.pushToast);

  const [step, setStep] = useState<Step>("Welcome");
  const [recommendation, setRecommendation] = useState<ModelRecommendation | null>(null);
  const [finishing, setFinishing] = useState(false);

  useEffect(() => {
    void refreshModels();
    void refreshPermissions();
    void api
      .recommendModel()
      .then(setRecommendation)
      .catch(() => undefined);
  }, [refreshModels, refreshPermissions]);

  const index = STEPS.indexOf(step);
  const next = () => setStep(STEPS[Math.min(index + 1, STEPS.length - 1)] ?? "Ready");
  const back = () => setStep(STEPS[Math.max(index - 1, 0)] ?? "Welcome");

  const finish = async () => {
    setFinishing(true);
    try {
      // Persist the launch-at-login choice, which onboarding asked for.
      if (settings) {
        await api.updateSettings(settings);
      }
      onDone();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setFinishing(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="w-full max-w-lg">
        <ol className="mb-6 flex items-center gap-1.5" aria-label="Progress">
          {STEPS.map((name, i) => (
            <li
              key={name}
              className={`h-1 flex-1 rounded-full ${
                i <= index ? "bg-accent" : "bg-border"
              }`}
              aria-current={i === index ? "step" : undefined}
            />
          ))}
        </ol>

        <motion.div
          key={step}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
        >
          <Card className="p-6">
            {step === "Welcome" ? (
              <div className="space-y-3">
                <h1 className="text-xl font-semibold tracking-tight">
                  Speak naturally. Write anywhere.
                </h1>
                <p className="hint">
                  {APP_NAME} turns your voice into text in any application, and runs safe
                  voice actions — all on this machine.
                </p>
                <ul className="space-y-1.5 pt-1 text-xs text-muted">
                  <li className="flex items-center gap-2">
                    <Check className="h-3.5 w-3.5 text-success" aria-hidden />
                    Speech recognition runs locally
                  </li>
                  <li className="flex items-center gap-2">
                    <Check className="h-3.5 w-3.5 text-success" aria-hidden />
                    Audio recordings are never saved
                  </li>
                  <li className="flex items-center gap-2">
                    <Check className="h-3.5 w-3.5 text-success" aria-hidden />
                    Works with no internet connection
                  </li>
                </ul>
              </div>
            ) : null}

            {step === "Microphone" ? (
              <div className="space-y-3">
                <h2 className="text-lg font-semibold">Microphone</h2>
                <p className="hint">
                  {APP_NAME} needs microphone access to hear you. Audio is transcribed on
                  this machine and then discarded — it is never written to disk or sent
                  anywhere.
                </p>
                <div className="flex items-center gap-2 pt-1">
                  <Pill
                    tone={
                      permissions?.microphone === "granted"
                        ? "success"
                        : permissions?.microphone === "denied"
                          ? "danger"
                          : "warning"
                    }
                  >
                    <Dot
                      tone={
                        permissions?.microphone === "granted"
                          ? "success"
                          : permissions?.microphone === "denied"
                            ? "danger"
                            : "warning"
                      }
                    />
                    {permissions?.microphone ?? "unknown"}
                  </Pill>
                </div>
                <div className="flex gap-2 pt-1">
                  <Button
                    variant="primary"
                    onClick={() =>
                      void api
                        .requestPermission("microphone")
                        .then(setPermissionResult)
                        .catch(() =>
                          pushToast({ level: "error", message: "Could not request microphone access." })
                        )
                    }
                  >
                    <Mic className="h-3.5 w-3.5" aria-hidden />
                    Allow microphone
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => void api.openPermissionSettings("microphone")}
                  >
                    Open System Settings
                  </Button>
                </div>
              </div>
            ) : null}

            {step === "Shortcut" ? (
              <div className="space-y-3">
                <h2 className="text-lg font-semibold">Your shortcut</h2>
                <p className="hint">
                  Hold this shortcut, speak, then release. {APP_NAME} inserts the text into
                  whatever application is focused.
                </p>
                <ShortcutRecorder
                  value={settings?.shortcuts.dictation.accelerator ?? ""}
                  onChange={(accelerator) =>
                    void patchSettings((draft) => {
                      draft.shortcuts.dictation.accelerator = accelerator;
                      draft.shortcuts.dictation.enabled = true;
                      if (draft.shortcuts.pushToTalk.enabled) {
                        draft.shortcuts.pushToTalk.accelerator = accelerator;
                      }
                      return draft;
                    })
                  }
                />
                <InfoNote tone="neutral">
                  If another app already uses this combination, {APP_NAME} will refuse to
                  register it and tell you rather than silently doing nothing.
                </InfoNote>
              </div>
            ) : null}

            {step === "Model" ? (
              <div className="space-y-3">
                <h2 className="text-lg font-semibold">Speech model</h2>
                {recommendation ? (
                  <p className="hint">{recommendation.reason}</p>
                ) : (
                  <p className="hint">Checking what this machine can run…</p>
                )}
                <div className="space-y-2">
                  {models.slice(0, 3).map((model) => (
                    <ModelChoice key={model.id} model={model} />
                  ))}
                </div>
                <p className="text-2xs text-faint">
                  Models download from Hugging Face. Nothing is downloaded unless you
                  press Download.
                </p>
              </div>
            ) : null}

            {step === "Enhancement" ? (
              <div className="space-y-3">
                <h2 className="text-lg font-semibold">Optional AI enhancement</h2>
                <p className="hint">
                  Enhancement cleans up punctuation and filler words using a local model.
                  It is off by default, and dictation works perfectly without it.
                </p>
                <Separator />
                <ToggleRow
                  title="Keep everything local"
                  description="Blocks every network feature. Dictation is unaffected."
                  checked={settings?.privacy.localOnly ?? false}
                  onCheckedChange={(value) =>
                    void patchSettings((draft) => {
                      draft.privacy.localOnly = value;
                      return draft;
                    })
                  }
                />
                <ToggleRow
                  title="Launch at login"
                  checked={settings?.general.launchAtLogin ?? false}
                  onCheckedChange={(value) =>
                    void patchSettings((draft) => {
                      draft.general.launchAtLogin = value;
                      return draft;
                    })
                  }
                />
                <InfoNote tone="neutral">
                  You can connect OpenAI, Groq, Ollama or LM Studio later in Settings →
                  AI Enhancement. No account is needed for anything in this app.
                </InfoNote>
              </div>
            ) : null}

            {step === "Ready" ? (
              <div className="space-y-3">
                <div className="flex items-center gap-2">
                  <ShieldCheck className="h-5 w-5 text-success" aria-hidden />
                  <h2 className="text-lg font-semibold">You're ready</h2>
                </div>
                <p className="hint">
                  Hold{" "}
                  {acceleratorChips(settings?.shortcuts.dictation.accelerator ?? "").map(
                    (chip, i) => (
                      <kbd key={`${chip}-${i}`} className="kbd mx-0.5">
                        {chip}
                      </kbd>
                    )
                  )}{" "}
                  and start speaking. {APP_NAME} runs quietly in the menu bar.
                </p>
                <ul className="space-y-1.5 text-xs text-muted">
                  <li>
                    • Dictate into any app: hold the shortcut and talk.
                  </li>
                  <li>• Voice actions: press your command shortcut and say “open Safari”.</li>
                  <li>• Everything is in Settings if you want to change it.</li>
                </ul>
              </div>
            ) : null}
          </Card>
        </motion.div>

        <div className="mt-4 flex items-center justify-between">
          <Button variant="ghost" onClick={back} disabled={index === 0}>
            Back
          </Button>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              onClick={() => void finish()}
              disabled={finishing}
            >
              Skip
            </Button>
            {step === "Ready" ? (
              <Button variant="primary" onClick={() => void finish()} disabled={finishing}>
                <Sparkles className="h-3.5 w-3.5" aria-hidden />
                Start using {APP_NAME}
              </Button>
            ) : (
              <Button variant="primary" onClick={next}>
                Continue
                <ArrowRight className="h-3.5 w-3.5" aria-hidden />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );

  function setPermissionResult(status: Awaited<ReturnType<typeof api.requestPermission>>) {
    useApp.setState({ permissions: status });
  }
}

/** One downloadable model with live progress. */
function ModelChoice({ model }: { model: ModelInfo }) {
  const pushToast = useApp((s) => s.pushToast);
  const refreshModels = useApp((s) => s.refreshModels);
  const patchSettings = useApp((s) => s.patchSettings);
  const [working, setWorking] = useState(false);

  const start = async () => {
    setWorking(true);
    try {
      await api.downloadModel(model.id);
      // Selecting it as active is what ends onboarding, so do it once downloaded.
      await api.setActiveModel(model.id);
      await patchSettings((draft) => {
        draft.speech.modelId = model.id;
        return draft;
      });
      await refreshModels();
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (!/cancel/i.test(text)) {
        pushToast({ level: "error", message: text });
      }
    } finally {
      setWorking(false);
      await refreshModels();
    }
  };

  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-border p-2.5">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{model.name}</span>
          {model.recommended ? <Pill tone="accent">Recommended</Pill> : null}
        </div>
        <p className="hint mt-0.5">
          {formatBytes(model.sizeBytes)} · {model.languages}
        </p>
        {model.status === "downloading" || model.status === "verifying" ? (
          <div className="mt-1.5">
            <ProgressBar value={model.progress} />
          </div>
        ) : null}
      </div>

      {model.status === "ready" ? (
        <Pill tone="success">
          <Check className="h-3 w-3" aria-hidden /> Ready
        </Pill>
      ) : (
        <Button
          size="sm"
          variant={model.recommended ? "primary" : "secondary"}
          disabled={working || model.status === "downloading"}
          onClick={() => (working ? api.pauseModelDownload(model.id) : void start())}
        >
          <Download className="h-3.5 w-3.5" aria-hidden />
          {working ? "Downloading…" : "Download"}
        </Button>
      )}
    </div>
  );
}
