/**
 * Speech model manager (spec §9, §10, §76).
 *
 * Every control here maps to a real backend operation: download streams from
 * the network with progress, pause keeps the partial file, resume continues
 * with an HTTP Range request, and verify re-reads the file. Nothing is a
 * placeholder.
 */
import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Check,
  Download,
  HardDrive,
  Pause,
  Play,
  ShieldCheck,
  Star,
  Trash2
} from "lucide-react";

import {
  Button,
  Card,
  Dot,
  InfoNote,
  Pill,
  ProgressBar,
  SectionHeading,
  Separator,
  ToggleRow
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api, type ModelRecommendation, type SystemProfile } from "@/services/ipc";
import { formatBytes } from "@/services/format";
import { useApp } from "@/stores/appStore";
import type { ModelInfo, ModelStatus } from "@/types";
import clsx from "clsx";

const STATUS_LABEL: Record<ModelStatus, string> = {
  not_downloaded: "Not downloaded",
  downloading: "Downloading",
  paused: "Paused",
  verifying: "Verifying",
  ready: "Downloaded",
  error: "Error"
};

export function Models() {
  const models = useApp((s) => s.models);
  const refreshModels = useApp((s) => s.refreshModels);
  const pushToast = useApp((s) => s.pushToast);
  const settings = useApp((s) => s.settings);

  const [recommendation, setRecommendation] = useState<ModelRecommendation | null>(null);
  const [profile, setProfile] = useState<SystemProfile | null>(null);
  const [usage, setUsage] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    void refreshModels();
    void (async () => {
      try {
        const [rec, sys, used] = await Promise.all([
          api.recommendModel(),
          api.systemProfile(),
          api.modelsDiskUsage()
        ]);
        setRecommendation(rec);
        setProfile(sys);
        setUsage(used);
      } catch {
        /* the page still works without the hardware summary */
      }
    })();
  }, [refreshModels]);

  const guarded = async (id: string, action: () => Promise<void>) => {
    setBusy(id);
    try {
      await action();
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      // A cancelled/paused download is a normal outcome, not an error.
      if (!/cancel/i.test(text)) {
        pushToast({ level: "error", message: text });
      }
    } finally {
      setBusy(null);
      await refreshModels();
      try {
        setUsage(await api.modelsDiskUsage());
      } catch {
        /* ignore */
      }
    }
  };

  const describeSize = (model: ModelInfo) => {
    if (profile && model.sizeBytes > profile.freeDiskMb * 1024 * 1024) {
      return `${formatBytes(model.sizeBytes)} — larger than your free disk space`;
    }
    return formatBytes(model.sizeBytes);
  };

  return (
    <div className="space-y-6">
      <SectionHeading
        title="Speech models"
        description="Models are stored on this machine and run locally. Downloading is only ever started by you."
      />

      {recommendation ? (
        <InfoNote tone="accent">
          <p className="font-medium text-fg">{recommendation.reason}</p>
          <p className="mt-1">
            Fastest: <strong>{tierName(recommendation.fastest)}</strong> · Highest accuracy:{" "}
            <strong>{tierName(recommendation.accurate)}</strong>
          </p>
        </InfoNote>
      ) : null}

      <div className="space-y-3">
        {models.map((model) => {
          const isActive = settings?.speech.modelId === model.id;
          const working = busy === model.id;
          return (
            <Card key={model.id} className="p-4">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-sm font-semibold">{model.name}</h3>
                    {model.recommended ? (
                      <Pill tone="accent">
                        <Star className="h-3 w-3" aria-hidden /> Recommended
                      </Pill>
                    ) : null}
                    {isActive ? (
                      <Pill tone="success">
                        <Check className="h-3 w-3" aria-hidden /> Active
                      </Pill>
                    ) : null}
                    <Pill
                      tone={
                        model.status === "ready"
                          ? "success"
                          : model.status === "error"
                            ? "danger"
                            : model.status === "downloading"
                              ? "accent"
                              : "neutral"
                      }
                    >
                      <Dot
                        tone={
                          model.status === "ready"
                            ? "success"
                            : model.status === "error"
                              ? "danger"
                              : model.status === "downloading"
                                ? "accent"
                                : "neutral"
                        }
                      />
                      {STATUS_LABEL[model.status]}
                    </Pill>
                  </div>

                  <p className="hint mt-1.5">{model.description}</p>

                  <dl className="mt-2.5 flex flex-wrap gap-x-5 gap-y-1 text-2xs text-muted">
                    <div className="flex gap-1">
                      <dt>Size</dt>
                      <dd className="text-fg">{describeSize(model)}</dd>
                    </div>
                    <div className="flex gap-1">
                      <dt>Languages</dt>
                      <dd className="text-fg">{model.languages}</dd>
                    </div>
                    <div className="flex gap-1">
                      <dt>Speed</dt>
                      <dd className="text-fg">
                        <Stars value={model.speed} />
                      </dd>
                    </div>
                    <div className="flex gap-1">
                      <dt>Accuracy</dt>
                      <dd className="text-fg">
                        <Stars value={model.accuracy} />
                      </dd>
                    </div>
                    <div className="flex gap-1">
                      <dt>Memory</dt>
                      <dd className="text-fg">~{model.ramMb} MB</dd>
                    </div>
                  </dl>

                  {model.status === "downloading" || model.status === "paused" || model.status === "verifying" ? (
                    <div className="mt-3 space-y-1.5">
                      <ProgressBar value={model.progress} />
                      <p className="text-2xs text-faint">
                        {formatBytes(model.downloadedBytes)} of {formatBytes(model.sizeBytes)} ·{" "}
                        {Math.round(model.progress * 100)}%
                      </p>
                    </div>
                  ) : null}

                  {model.error ? (
                    <p className="mt-2 flex items-center gap-1.5 text-xs text-danger">
                      <AlertTriangle className="h-3.5 w-3.5" aria-hidden />
                      {model.error}
                    </p>
                  ) : null}
                </div>

                <div className="flex shrink-0 flex-col items-end gap-2">
                  {model.status === "not_downloaded" || model.status === "error" ? (
                    <Button
                      variant="primary"
                      size="sm"
                      disabled={working}
                      onClick={() =>
                        void guarded(model.id, () => api.downloadModel(model.id))
                      }
                    >
                      <Download className="h-3.5 w-3.5" aria-hidden />
                      {working ? "Starting…" : "Download"}
                    </Button>
                  ) : null}

                  {model.status === "downloading" ? (
                    <Button
                      size="sm"
                      disabled={working}
                      onClick={() =>
                        void guarded(model.id, () => api.pauseModelDownload(model.id))
                      }
                    >
                      <Pause className="h-3.5 w-3.5" aria-hidden />
                      Pause
                    </Button>
                  ) : null}

                  {model.status === "paused" ? (
                    <Button
                      size="sm"
                      disabled={working}
                      onClick={() =>
                        void guarded(model.id, () => api.resumeModelDownload(model.id))
                      }
                    >
                      <Play className="h-3.5 w-3.5" aria-hidden />
                      Resume
                    </Button>
                  ) : null}

                  {model.status === "ready" ? (
                    <>
                      {isActive ? (
                        <Pill tone="success">
                          <Check className="h-3 w-3" aria-hidden /> In use
                        </Pill>
                      ) : (
                        <Button
                          size="sm"
                          variant="primary"
                          disabled={working}
                          onClick={() =>
                            void guarded(model.id, () => api.setActiveModel(model.id))
                          }
                        >
                          Use this model
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={working}
                        onClick={() =>
                          void guarded(model.id, async () => {
                            const ok = await api.verifyModel(model.id);
                            pushToast({
                              level: ok ? "success" : "error",
                              message: ok
                                ? `${model.name} passed its integrity check.`
                                : `${model.name} failed its integrity check — re-download it.`
                            });
                          })
                        }
                      >
                        <ShieldCheck className="h-3.5 w-3.5" aria-hidden />
                        Verify
                      </Button>
                    </>
                  ) : null}

                  {model.status !== "not_downloaded" ? (
                    <Button
                      size="sm"
                      variant="danger"
                      disabled={working || isActive}
                      title={isActive ? "Select a different model first" : undefined}
                      onClick={() =>
                        void guarded(model.id, () => api.deleteModel(model.id))
                      }
                    >
                      <Trash2 className="h-3.5 w-3.5" aria-hidden />
                      Delete
                    </Button>
                  ) : null}
                </div>
              </div>
            </Card>
          );
        })}
      </div>

      <Card className="divide-y divide-border">
        <div className="row px-4">
          <div>
            <p className="text-sm">Storage used</p>
            <p className="hint mt-0.5">
              Models live in {APP_NAME}'s own directory, so updating the app never
              re-downloads them.
            </p>
          </div>
          <span className="flex items-center gap-1.5 text-sm text-muted">
            <HardDrive className="h-3.5 w-3.5" aria-hidden />
            {formatBytes(usage)}
          </span>
        </div>

        <div className="row px-4">
          <div>
            <p className="text-sm">GPU acceleration</p>
            <p className="hint mt-0.5">
              {profile?.accelerator
                ? `Detected: ${profile.accelerator}`
                : "Uses the fastest backend available on this machine."}
            </p>
          </div>
          <ToggleSwitchForGpu />
        </div>
      </Card>

      <Separator />

      <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-2xs text-faint">
        <span>
          {profile
            ? `${profile.os} · ${profile.arch} · ${profile.cpuCores} cores · ${Math.round(profile.totalRamMb / 1024)} GB RAM`
            : "Detecting hardware…"}
        </span>
        {profile ? <span>{formatBytes(profile.freeDiskMb * 1024 * 1024)} free disk</span> : null}
      </div>
    </div>
  );
}

function ToggleSwitchForGpu() {
  const settings = useApp((s) => s.settings);
  const patchSettings = useApp((s) => s.patchSettings);
  if (!settings) return null;
  return (
    <div className="w-11">
      <ToggleRowInline
        checked={settings.speech.useGpu}
        onChange={(value) =>
          void patchSettings((draft) => {
            draft.speech.useGpu = value;
            return draft;
          })
        }
        label="GPU acceleration"
      />
    </div>
  );
}

function ToggleRowInline({
  checked,
  onChange,
  label
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
}) {
  return (
    <div className={clsx("flex justify-end")}>
      <ToggleRow title={label} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function Stars({ value }: { value: number }) {
  const filled = Math.max(0, Math.min(5, Math.round(value)));
  return (
    <span aria-label={`${filled} out of 5`}>
      {"★".repeat(filled)}
      <span className="text-faint">{"★".repeat(5 - filled)}</span>
    </span>
  );
}

function tierName(id: string): string {
  const tier = id.replace("whisper-", "").replace("-en", "").replace("_", " ");
  return tier.charAt(0).toUpperCase() + tier.slice(1);
}

/** Re-exported so the settings page can reuse the label map. */
export { STATUS_LABEL };
export type { ModelInfo };
