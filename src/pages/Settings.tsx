/**
 * Settings (spec §38–§43, §47–§49, §77–§79).
 *
 * Every control writes through to the Rust core immediately; there is no "Save"
 * button to forget to press. Controls that cannot work are disabled with a
 * plain-language reason instead of silently doing nothing.
 */
import { useEffect, useState } from "react";
import {
  Check,
  ExternalLink,
  Key,
  Lock,
  Mic,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Unlock
} from "lucide-react";

import { LevelMeter } from "@/components/Waveform";
import { ShortcutRecorder } from "@/components/ShortcutRecorder";
import {
  Button,
  Card,
  Dot,
  Field,
  InfoNote,
  Pill,
  SectionHeading,
  Select,
  Separator,
  Slider,
  Toggle,
  ToggleRow
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api, type InputDevice } from "@/services/ipc";
import { useApp } from "@/stores/appStore";
import type {
  AiProviderKind,
  DictionaryEntry,
  EnhancementMode,
  Settings,
  SpeechEngineId
} from "@/types";

type SectionId =
  | "general"
  | "dictation"
  | "speech"
  | "dictionary"
  | "ai"
  | "appearance"
  | "privacy"
  | "shortcuts"
  | "permissions"
  | "advanced"
  | "about";

const SECTIONS: ReadonlyArray<{ id: SectionId; label: string }> = [
  { id: "general", label: "General" },
  { id: "dictation", label: "Dictation" },
  { id: "speech", label: "Speech" },
  { id: "dictionary", label: "Dictionary" },
  { id: "ai", label: "AI Enhancement" },
  { id: "appearance", label: "Appearance" },
  { id: "privacy", label: "Privacy" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "permissions", label: "Permissions" },
  { id: "advanced", label: "Advanced" },
  { id: "about", label: "About" }
];

export function SettingsPage({
  onSettingsChange
}: {
  onSettingsChange: (settings: Settings) => void;
}) {
  const settings = useApp((s) => s.settings);
  const [section, setSection] = useState<SectionId>("general");

  if (!settings) return null;

  // Editing helper so each panel stays declarative. `logLevel` is excluded
  // because it is a scalar, not a settings section.
  const set = <K extends SectionKey>(key: K, patch: Partial<Settings[K]>) =>
    onSettingsChange({ ...settings, [key]: { ...settings[key], ...patch } });

  return (
    <div className="space-y-6">
      <SectionHeading title="Settings" />
      <div className="flex gap-6">
        <nav aria-label="Settings sections" className="w-40 shrink-0">
          <ul className="space-y-0.5">
            {SECTIONS.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => setSection(item.id)}
                  aria-current={section === item.id ? "page" : undefined}
                  className={`w-full rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors ${
                    section === item.id
                      ? "bg-elevated font-medium text-fg"
                      : "text-muted hover:text-fg"
                  }`}
                >
                  {item.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>

        <div className="min-w-0 flex-1 space-y-5">
          {section === "general" ? <GeneralPanel settings={settings} set={set} /> : null}
          {section === "dictation" ? <DictationPanel settings={settings} set={set} /> : null}
          {section === "speech" ? <SpeechPanel settings={settings} set={set} /> : null}
          {section === "dictionary" ? <DictionaryPanel /> : null}
          {section === "ai" ? <AiPanel settings={settings} set={set} /> : null}
          {section === "appearance" ? <AppearancePanel settings={settings} set={set} /> : null}
          {section === "privacy" ? <PrivacyPanel settings={settings} set={set} /> : null}
          {section === "shortcuts" ? <ShortcutsPanel settings={settings} set={set} /> : null}
          {section === "permissions" ? <PermissionsPanel /> : null}
          {section === "advanced" ? <AdvancedPanel settings={settings} set={set} /> : null}
          {section === "about" ? <AboutPanel /> : null}
        </div>
      </div>
    </div>
  );
}

/** The object-valued settings sections. */
type SectionKey = Exclude<keyof Settings, "logLevel">;

type SetFn = <K extends SectionKey>(key: K, patch: Partial<Settings[K]>) => void;

/* ---------------------------------------------------------------- general */

function GeneralPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  return (
    <Card className="divide-y divide-border px-4">
      <ToggleRow
        title="Launch at login"
        description={`Starts ${APP_NAME} in the menu bar when you log in.`}
        checked={settings.general.launchAtLogin}
        onCheckedChange={(value) => set("general", { launchAtLogin: value })}
      />
      <ToggleRow
        title="Start minimized"
        description="Skip the dashboard and go straight to the menu bar."
        checked={settings.general.startMinimized}
        onCheckedChange={(value) => set("general", { startMinimized: value })}
      />
      <ToggleRow
        title="Show menu bar item"
        description={`${APP_NAME} keeps running with or without the icon.`}
        checked={settings.general.showTrayIcon}
        onCheckedChange={(value) => set("general", { showTrayIcon: value })}
      />
      <ToggleRow
        title="Copy the result automatically"
        description="Puts the final text on the clipboard as well as inserting it."
        checked={settings.general.autoCopyResult}
        onCheckedChange={(value) => set("general", { autoCopyResult: value })}
      />
      <ToggleRow
        title="Show model downloads in the overlay"
        checked={settings.general.showModelProgressInOverlay}
        onCheckedChange={(value) => set("general", { showModelProgressInOverlay: value })}
      />
    </Card>
  );
}

/* -------------------------------------------------------------- dictation */

function DictationPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  return (
    <>
      <Card className="space-y-4 p-4">
        <Field label="Activation">
          <Select
            ariaLabel="Activation mode"
            value={settings.dictation.activation}
            onValueChange={(value) =>
              set("dictation", { activation: value as Settings["dictation"]["activation"] })
            }
            options={[
              { value: "push_to_talk", label: "Push to talk", description: "Hold to record" },
              { value: "toggle", label: "Toggle", description: "Press to start, press to stop" }
            ]}
          />
        </Field>

        <Field
          label="Insert mode"
          hint="Paste is fastest and works almost everywhere. Typing avoids touching the clipboard."
        >
          <Select
            ariaLabel="Insert mode"
            value={settings.dictation.insertMode}
            onValueChange={(value) =>
              set("dictation", { insertMode: value as Settings["dictation"]["insertMode"] })
            }
            options={[
              { value: "paste", label: "Paste", description: "Clipboard + ⌘V, clipboard restored" },
              { value: "type", label: "Type", description: "Simulated keystrokes" },
              { value: "clipboard_only", label: "Clipboard only", description: "You paste manually" }
            ]}
          />
        </Field>
      </Card>

      <Card className="divide-y divide-border px-4">
        <ToggleRow
          title="Continuous dictation"
          description="Join onto text already in the field, with the right spacing."
          checked={settings.dictation.continuousDictation}
          onCheckedChange={(value) => set("dictation", { continuousDictation: value })}
        />
        <ToggleRow
          title="Spoken punctuation"
          description="“comma”, “period”, “new line” become real punctuation."
          checked={settings.dictation.spokenPunctuation}
          onCheckedChange={(value) => set("dictation", { spokenPunctuation: value })}
        />
        <ToggleRow
          title="Add a trailing space"
          checked={settings.dictation.trailingSpace}
          onCheckedChange={(value) => set("dictation", { trailingSpace: value })}
        />
        <ToggleRow
          title="Restore focus before inserting"
          description="Returns to the app you were in, so text never lands in the wrong place."
          checked={settings.dictation.restoreFocus}
          onCheckedChange={(value) => set("dictation", { restoreFocus: value })}
        />
        <ToggleRow
          title="Terminal mode"
          description="Lowercases the first letter and drops the trailing period."
          checked={settings.dictation.gaavMode}
          onCheckedChange={(value) => set("dictation", { gaavMode: value })}
        />
      </Card>
    </>
  );
}

/* ----------------------------------------------------------------- speech */

function SpeechPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  const devices: InputDevice[] = [];
  const [available, setAvailable] = useState<InputDevice[]>(devices);
  const [testing, setTesting] = useState(false);
  const level = useApp((s) => s.level);

  useEffect(() => {
    void api
      .inputDevices()
      .then(setAvailable)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!testing) return;
    void api.startMicTest();
    return () => {
      void api.stopMicTest();
    };
  }, [testing]);

  return (
    <>
      <Card className="space-y-4 p-4">
        <Field label="Speech engine">
          <Select
            ariaLabel="Speech engine"
            value={settings.speech.engine}
            onValueChange={(value) => set("speech", { engine: value as SpeechEngineId })}
            options={[
              { value: "whisper_cpp", label: "whisper.cpp (local)", description: "Recommended" },
              { value: "parakeet", label: "Parakeet v3", description: "macOS Apple Silicon only" },
              { value: "apple_speech", label: "Apple Speech", description: "macOS only" }
            ]}
          />
        </Field>

        <Field label="Language" hint="Leave as English unless you are dictating in another language.">
          <input
            className="input"
            value={settings.speech.language}
            aria-label="Language"
            onChange={(event) => set("speech", { language: event.target.value })}
          />
        </Field>

        <Field label="Input device">
          <Select
            ariaLabel="Input device"
            value={settings.speech.inputDevice ?? "__default"}
            onValueChange={(value) =>
              set("speech", { inputDevice: value === "__default" ? null : value })
            }
            options={[
              { value: "__default", label: "System default" },
              ...available.map((device) => ({ value: device.id, label: device.name }))
            ]}
          />
        </Field>
      </Card>

      <Card className="space-y-3 p-4">
        <div className="flex items-center justify-between">
          <SectionHeading
            title="Audio test"
            description="Speak normally — a healthy signal sits around the middle of the bar."
          />
          <Button onClick={() => setTesting((value) => !value)}>
            <Mic className="h-3.5 w-3.5" aria-hidden />
            {testing ? "Stop" : "Test"}
          </Button>
        </div>
        <LevelMeter level={level} active={testing} label="Input level" />
      </Card>

      <Card className="space-y-4 p-4">
        <Field label="Automatic silence detection">
          <Select
            ariaLabel="Silence detection"
            value={settings.speech.silence}
            onValueChange={(value) =>
              set("speech", { silence: value as Settings["speech"]["silence"] })
            }
            options={[
              { value: "off", label: "Off", description: "Recording stops only when you stop" },
              { value: "short", label: "Short", description: "Ends after 0.7 s of quiet" },
              { value: "balanced", label: "Balanced", description: "Ends after 1.2 s of quiet" },
              { value: "long", label: "Long", description: "Ends after 2 s of quiet" }
            ]}
          />
        </Field>
        <Separator />
        <ToggleRow
          title="Voice activity detection"
          checked={settings.speech.vadEnabled}
          onCheckedChange={(value) => set("speech", { vadEnabled: value })}
        />
        <Field label="Worker threads" hint={`0 lets ${APP_NAME} choose, leaving a core for the UI.`}>
          <input
            className="input w-24"
            type="number"
            min={0}
            max={64}
            value={settings.speech.threads}
            aria-label="Threads"
            onChange={(event) => set("speech", { threads: Number(event.target.value) || 0 })}
          />
        </Field>
      </Card>
    </>
  );
}

/* ------------------------------------------------------------- dictionary */

function DictionaryPanel() {
  const pushToast = useApp((s) => s.pushToast);
  const [entries, setEntries] = useState<DictionaryEntry[]>([]);

  const reload = async () => {
    try {
      setEntries(await api.dictionaryList());
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async (entry: DictionaryEntry) => {
    try {
      await api.dictionaryUpsert(entry);
      await reload();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  return (
    <>
      <InfoNote tone="neutral">
        Replacements run after transcription and before any AI enhancement. They are
        perfect for names, products and acronyms the recogniser gets wrong.
      </InfoNote>

      <Card className="divide-y divide-border">
        {entries.length === 0 ? (
          <p className="px-4 py-5 text-center text-xs text-faint">
            No entries yet. Add something like “next mart” → “NxtMart”.
          </p>
        ) : (
          entries.map((entry) => (
            <div key={entry.id} className="flex items-center gap-2 px-4 py-2.5">
              <input
                className="input"
                value={entry.spoken}
                aria-label="Spoken form"
                onChange={(event) =>
                  setEntries((list) =>
                    list.map((item) =>
                      item.id === entry.id ? { ...item, spoken: event.target.value } : item
                    )
                  )
                }
                onBlur={() => void save(entry)}
              />
              <span className="text-faint">→</span>
              <input
                className="input"
                value={entry.replacement}
                aria-label="Replacement"
                onChange={(event) =>
                  setEntries((list) =>
                    list.map((item) =>
                      item.id === entry.id
                        ? { ...item, replacement: event.target.value }
                        : item
                    )
                  )
                }
                onBlur={() => void save(entry)}
              />
              <Toggle
                checked={entry.enabled}
                label="Enabled"
                onCheckedChange={(value) => void save({ ...entry, enabled: value })}
              />
              <Button
                variant="ghost"
                size="sm"
                aria-label="Delete entry"
                onClick={() =>
                  void api.dictionaryDelete(entry.id).then(reload)
                }
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </div>
          ))
        )}
      </Card>

      <Button
        onClick={() =>
          void save({
            id: 0,
            spoken: "",
            replacement: "",
            wholeWord: true,
            enabled: true
          })
        }
      >
        <Plus className="h-3.5 w-3.5" aria-hidden />
        Add entry
      </Button>
    </>
  );
}

/* --------------------------------------------------------------------- ai */

const PROVIDERS: ReadonlyArray<{ value: AiProviderKind; label: string; local: boolean }> = [
  { value: "none", label: "None", local: true },
  { value: "llama_cpp", label: "llama.cpp server", local: true },
  { value: "ollama", label: "Ollama", local: true },
  { value: "lm_studio", label: "LM Studio", local: true },
  { value: "openai_compatible", label: "OpenAI-compatible (online)", local: false }
];

function AiPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  const pushToast = useApp((s) => s.pushToast);
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [testing, setTesting] = useState(false);

  const provider = settings.enhancement.provider;

  useEffect(() => {
    void api
      .apiKeyStatus(provider)
      .then(setHasKey)
      .catch(() => setHasKey(false));
  }, [provider, settings.enhancement.provider]);

  const blocker = settings.enhancement.enabled ? null : "AI enhancement is off.";
  const cloud = provider === "openai_compatible";

  const test = async () => {
    setTesting(true);
    try {
      const result = await api.testAi();
      pushToast({
        level: result.ok ? "success" : "error",
        message: result.latencyMs
          ? `${result.message} (${Math.round(result.latencyMs)} ms)`
          : result.message
      });
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
      {cloud ? (
        <InfoNote tone="warning">
          <strong>Online AI.</strong> Your transcript — and only your transcript — will
          leave this computer when enhancement runs. Audio is never uploaded.
        </InfoNote>
      ) : (
        <InfoNote tone="success">
          <strong>Local.</strong> Enhancement runs on this machine through a local
          server. Nothing leaves your computer.
        </InfoNote>
      )}

      <Card className="divide-y divide-border px-4">
        <ToggleRow
          title="Enable AI enhancement"
          description={blocker ?? "Rewrites the transcript after recognition."}
          checked={settings.enhancement.enabled}
          disabled={provider === "none"}
          onCheckedChange={(value) => set("enhancement", { enabled: value })}
        />
      </Card>

      <Card className="space-y-4 p-4">
        <Field label="Provider">
          <Select
            ariaLabel="AI provider"
            value={provider}
            onValueChange={(value) => {
              const kind = value as AiProviderKind;
              const meta = PROVIDERS.find((p) => p.value === kind);
              set("enhancement", {
                provider: kind,
                enabled: kind === "none" ? false : settings.enhancement.enabled,
                endpoint:
                  settings.enhancement.endpoint && settings.enhancement.provider === kind
                    ? settings.enhancement.endpoint
                    : ""
              });
              void meta;
            }}
            options={PROVIDERS.map((p) => ({
              value: p.value,
              label: p.label,
              description: p.local ? "Runs locally" : "Sends text to a remote service"
            }))}
          />
        </Field>

        {provider !== "none" ? (
          <>
            <Field
              label="API endpoint"
              hint="For llama.cpp use http://127.0.0.1:8080/v1. Ollama and LM Studio fill themselves in."
            >
              <input
                className="input"
                value={settings.enhancement.endpoint}
                placeholder="http://127.0.0.1:8080/v1"
                aria-label="API endpoint"
                onChange={(event) => set("enhancement", { endpoint: event.target.value })}
              />
            </Field>

            <Field label="Model" hint="The exact model name your provider expects.">
              <input
                className="input"
                value={settings.enhancement.model}
                placeholder="llama3.2"
                aria-label="Model name"
                onChange={(event) => set("enhancement", { model: event.target.value })}
              />
            </Field>

            {cloud ? (
              <Field
                label="API key"
                hint="Stored in your operating system's credential store, never in a file."
              >
                <div className="flex gap-2">
                  <input
                    className="input font-mono"
                    type="password"
                    value={apiKey}
                    placeholder={hasKey ? "•••••••••••• (stored)" : "sk-…"}
                    aria-label="API key"
                    onChange={(event) => setApiKey(event.target.value)}
                  />
                  <Button
                    onClick={() =>
                      void api
                        .setApiKey(provider, apiKey)
                        .then(() => {
                          setApiKey("");
                          setHasKey(true);
                          pushToast({ level: "success", message: "API key saved securely." });
                        })
                        .catch(() =>
                          pushToast({ level: "error", message: "Could not save the API key." })
                        )
                    }
                    disabled={!apiKey}
                  >
                    <Key className="h-3.5 w-3.5" aria-hidden />
                    Save
                  </Button>
                  {hasKey ? (
                    <Button
                      variant="ghost"
                      onClick={() =>
                        void api.deleteApiKey(provider).then(() => setHasKey(false))
                      }
                    >
                      Clear
                    </Button>
                  ) : null}
                </div>
              </Field>
            ) : null}

            <div className="flex items-center gap-2">
              <Button onClick={() => void test()} disabled={testing}>
                <RefreshCw className="h-3.5 w-3.5" aria-hidden />
                {testing ? "Testing…" : "Test connection"}
              </Button>
              {hasKey && cloud ? (
                <Pill tone="success">
                  <ShieldCheck className="h-3 w-3" aria-hidden /> Key stored securely
                </Pill>
              ) : null}
            </div>
          </>
        ) : null}
      </Card>

      <Card className="space-y-4 p-4">
        <Field label="Transformation">
          <Select
            ariaLabel="Enhancement mode"
            value={settings.enhancement.mode}
            onValueChange={(value) =>
              set("enhancement", { mode: value as EnhancementMode })
            }
            options={[
              { value: "natural", label: "Natural" },
              { value: "professional", label: "Professional" },
              { value: "email", label: "Email" },
              { value: "casual", label: "Casual" },
              { value: "technical", label: "Technical" },
              { value: "minimal", label: "Minimal cleanup" }
            ]}
          />
        </Field>

        <Separator />

        <ToggleRow
          title="Remove filler words"
          checked={settings.enhancement.removeFillers}
          onCheckedChange={(value) => set("enhancement", { removeFillers: value })}
        />
        <ToggleRow
          title="Auto punctuation"
          checked={settings.enhancement.autoPunctuation}
          onCheckedChange={(value) => set("enhancement", { autoPunctuation: value })}
        />
        <ToggleRow
          title="Auto capitalization"
          checked={settings.enhancement.autoCapitalization}
          onCheckedChange={(value) => set("enhancement", { autoCapitalization: value })}
        />

        <Field
          label="Extra instruction"
          hint="Optional. Added to the transformation prompt; the safety rules always win."
        >
          <textarea
            className="input min-h-[4.5rem] resize-y"
            value={settings.enhancement.customPrompt ?? ""}
            placeholder="Always use British spelling."
            aria-label="Custom prompt"
            onChange={(event) =>
              set("enhancement", { customPrompt: event.target.value || null })
            }
          />
        </Field>
      </Card>
    </>
  );
}

/* ------------------------------------------------------------- appearance */

function AppearancePanel({ settings, set }: { settings: Settings; set: SetFn }) {
  return (
    <>
      <Card className="space-y-4 p-4">
        <Field label="Theme">
          <Select
            ariaLabel="Theme"
            value={settings.appearance.theme}
            onValueChange={(value) =>
              set("appearance", { theme: value as Settings["appearance"]["theme"] })
            }
            options={[
              { value: "system", label: "System", description: "Follow the OS" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" }
            ]}
          />
        </Field>
      </Card>

      <Card className="space-y-4 p-4">
        <ToggleRow
          title="Show the overlay while dictating"
          checked={settings.appearance.overlayEnabled}
          onCheckedChange={(value) => set("appearance", { overlayEnabled: value })}
        />
        <ToggleRow
          title="Show the audio waveform"
          checked={settings.appearance.showWaveform}
          onCheckedChange={(value) => set("appearance", { showWaveform: value })}
        />

        <Field label="Overlay position">
          <Select
            ariaLabel="Overlay position"
            value={settings.appearance.overlayPosition}
            onValueChange={(value) =>
              set("appearance", {
                overlayPosition: value as Settings["appearance"]["overlayPosition"]
              })
            }
            options={[
              { value: "bottom", label: "Bottom of the screen" },
              { value: "center", label: "Centre" },
              { value: "top", label: "Top of the screen" },
              { value: "cursor", label: "Near the mouse pointer" }
            ]}
          />
        </Field>

        <Field label={`Overlay opacity — ${Math.round(settings.appearance.overlayOpacity * 100)}%`}>
          <Slider
            ariaLabel="Overlay opacity"
            min={0.5}
            max={1}
            step={0.01}
            value={settings.appearance.overlayOpacity}
            onValueChange={(value) => set("appearance", { overlayOpacity: value })}
          />
        </Field>
      </Card>
    </>
  );
}

/* ---------------------------------------------------------------- privacy */

function PrivacyPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  const cloud = settings.enhancement.provider === "openai_compatible";
  return (
    <>
      <Card className="space-y-4 p-4">
        <div className="flex items-center gap-2">
          {settings.privacy.localOnly ? (
            <Lock className="h-4 w-4 text-success" aria-hidden />
          ) : (
            <Unlock className="h-4 w-4 text-warning" aria-hidden />
          )}
          <span className="text-sm font-medium">
            {settings.privacy.localOnly ? "Local Only is on" : "Local Only is off"}
          </span>
        </div>
        <p className="hint">
          With Local Only on, {APP_NAME} refuses every network request at the backend —
          model downloads and online enhancement included. Dictation is unaffected, so
          the app works in airplane mode.
        </p>
        <ToggleRow
          title="Local Only mode"
          checked={settings.privacy.localOnly}
          onCheckedChange={(value) => set("privacy", { localOnly: value })}
        />
        {settings.privacy.localOnly && cloud ? (
          <InfoNote tone="warning">
            The selected AI provider is online, so enhancement has been switched off
            while Local Only is on.
          </InfoNote>
        ) : null}
      </Card>

      <Card className="divide-y divide-border px-4">
        <FactRow label="Microphone" value="Used only while dictating" />
        <FactRow label="Audio recordings" value="Never saved" tone="success" />
        <FactRow
          label="Transcript history"
          value={
            settings.privacy.saveHistory
              ? "Saved locally on this machine"
              : "Not saved"
          }
        />
        <FactRow
          label="Cloud AI"
          value={settings.privacy.localOnly ? "Blocked" : cloud ? "Enabled" : "Not configured"}
          tone={cloud && !settings.privacy.localOnly ? "warning" : "success"}
        />
        <FactRow label="Telemetry" value="Disabled" tone="success" />
      </Card>

      <Card className="divide-y divide-border px-4">
        <ToggleRow
          title="Save dictation history"
          checked={settings.privacy.saveHistory}
          onCheckedChange={(value) => set("privacy", { saveHistory: value })}
        />
        <ToggleRow
          title="Save audio recordings"
          description="Off by default. Turning this on writes audio to disk, which we recommend against."
          checked={settings.privacy.saveRecordings}
          onCheckedChange={(value) => set("privacy", { saveRecordings: value })}
        />
        <ToggleRow
          title="Send anonymous diagnostics"
          description="Off by default. Never includes audio or transcript content."
          checked={settings.privacy.telemetry}
          onCheckedChange={(value) => set("privacy", { telemetry: value })}
        />
      </Card>
    </>
  );
}

function FactRow({
  label,
  value,
  tone
}: {
  label: string;
  value: string;
  tone?: "success" | "warning";
}) {
  return (
    <div className="row">
      <span className="text-sm text-muted">{label}</span>
      <span className="flex items-center gap-1.5 text-sm">
        {tone ? <Dot tone={tone} /> : null}
        {value}
      </span>
    </div>
  );
}

/* -------------------------------------------------------------- shortcuts */

function ShortcutsPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  return (
    <>
      <Card className="space-y-4 p-4">
        <Field
          label="Dictation shortcut"
          hint={
            settings.dictation.activation === "push_to_talk"
              ? "Hold to record, release to insert."
              : "Press to start, press again to stop."
          }
        >
          <ShortcutRecorder
            value={settings.shortcuts.dictation.accelerator}
            onChange={(accelerator) =>
              set("shortcuts", {
                dictation: { accelerator, enabled: true }
              })
            }
          />
        </Field>

        <Separator />

        <Field label="Voice Actions shortcut" hint="Press, then speak a command.">
          <ShortcutRecorder
            value={settings.shortcuts.voiceCommand.accelerator}
            onChange={(accelerator) =>
              set("shortcuts", { voiceCommand: { accelerator, enabled: true } })
            }
          />
        </Field>

        <Separator />

        <ToggleRow
          title="Separate push-to-talk shortcut"
          description="Use a second combination for hold-to-talk while dictation stays on toggle."
          checked={settings.shortcuts.pushToTalk.enabled}
          onCheckedChange={(value) =>
            set("shortcuts", {
              pushToTalk: {
                accelerator:
                  settings.shortcuts.pushToTalk.accelerator ||
                  settings.shortcuts.dictation.accelerator,
                enabled: value
              }
            })
          }
        />
        {settings.shortcuts.pushToTalk.enabled ? (
          <ShortcutRecorder
            value={settings.shortcuts.pushToTalk.accelerator}
            onChange={(accelerator) =>
              set("shortcuts", { pushToTalk: { accelerator, enabled: true } })
            }
          />
        ) : null}
      </Card>

      <InfoNote tone="neutral">
        {APP_NAME} registers shortcuts at startup. If a combination is already taken by
        another app, it reports the conflict here instead of failing silently.
      </InfoNote>
    </>
  );
}

/* ------------------------------------------------------------ permissions */

function PermissionsPanel() {
  const permissions = useApp((s) => s.permissions);
  const refreshPermissions = useApp((s) => s.refreshPermissions);
  const pushToast = useApp((s) => s.pushToast);

  useEffect(() => {
    void refreshPermissions();
  }, [refreshPermissions]);

  const rows: Array<{
    kind: "microphone" | "accessibility" | "input_monitoring";
    title: string;
    why: string;
    state: string | undefined;
  }> = [
    {
      kind: "microphone",
      title: "Microphone",
      why: `Lets ${APP_NAME} hear you. Audio is transcribed on this machine and discarded.`,
      state: permissions?.microphone
    },
    {
      kind: "accessibility",
      title: "Accessibility",
      why: `Lets ${APP_NAME} insert your transcript into the app you are using. Your text stays on this Mac.`,
      state: permissions?.accessibility
    },
    {
      kind: "input_monitoring",
      title: "Input Monitoring",
      why: "Lets your global shortcut work while another app is focused.",
      state: permissions?.inputMonitoring
    }
  ];

  return (
    <>
      {rows.map((row) => (
        <Card key={row.kind} className="space-y-3 p-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium">{row.title}</span>
                <Pill
                  tone={
                    row.state === "granted"
                      ? "success"
                      : row.state === "unsupported"
                        ? "neutral"
                        : row.state === "denied"
                          ? "danger"
                          : "warning"
                  }
                >
                  <Dot
                    tone={
                      row.state === "granted"
                        ? "success"
                        : row.state === "unsupported"
                          ? "neutral"
                          : row.state === "denied"
                            ? "danger"
                            : "warning"
                    }
                  />
                  {row.state === "unsupported" ? "Not needed here" : (row.state ?? "unknown")}
                </Pill>
              </div>
              <p className="hint mt-1.5 max-w-md">{row.why}</p>
            </div>

            {row.state === "unsupported" ? null : (
              <div className="flex shrink-0 flex-col gap-2">
                <Button
                  size="sm"
                  variant="primary"
                  onClick={() =>
                    void api
                      .requestPermission(row.kind)
                      .then((status) => useApp.setState({ permissions: status }))
                      .catch(() =>
                        pushToast({
                          level: "error",
                          message: "Could not request this permission."
                        })
                      )
                  }
                >
                  Grant
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void api.openPermissionSettings(row.kind)}
                >
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden />
                  System Settings
                </Button>
              </div>
            )}
          </div>
        </Card>
      ))}

      <InfoNote tone="neutral">
        Dictation works without Accessibility access — the transcript simply goes to
        your clipboard instead of being inserted automatically.
      </InfoNote>
    </>
  );
}

/* --------------------------------------------------------------- advanced */

function AdvancedPanel({ settings, set }: { settings: Settings; set: SetFn }) {
  const pushToast = useApp((s) => s.pushToast);

  return (
    <>
      <Card className="space-y-4 p-4">
        <Field label="Enhancement timeout (seconds)">
          <input
            className="input w-24"
            type="number"
            min={5}
            max={120}
            value={settings.enhancement.timeoutSecs}
            aria-label="Enhancement timeout"
            onChange={(event) =>
              set("enhancement", { timeoutSecs: Number(event.target.value) || 20 })
            }
          />
        </Field>
      </Card>

      <Card className="divide-y divide-border px-4">
        <div className="row">
          <div>
            <p className="text-sm">Reset diagnostics</p>
            <p className="hint mt-0.5">Clears the collected latency samples.</p>
          </div>
          <Button
            size="sm"
            onClick={() =>
              void api
                .diagnostics()
                .then(() => pushToast({ level: "success", message: "Diagnostics refreshed." }))
                .catch(() =>
                  pushToast({ level: "error", message: "Could not refresh diagnostics." })
                )
            }
          >
            Refresh
          </Button>
        </div>

        <div className="row">
          <div>
            <p className="text-sm">Let Windows/macOS handle the microphone</p>
            <p className="hint mt-0.5">
              Reset the stored device choice if the microphone changed.
            </p>
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => set("speech", { inputDevice: null })}
          >
            Use system default
          </Button>
        </div>
      </Card>

      <InfoNote tone="danger">
        {APP_NAME} cannot run scripts or shell commands from voice input, by design. This
        is not configurable.
      </InfoNote>
    </>
  );
}

/* ------------------------------------------------------------------ about */

function AboutPanel() {
  const capabilities = useApp((s) => s.capabilities);
  const diagnostics = useApp((s) => s.diagnostics);

  return (
    <>
      <Card className="space-y-3 p-4">
        <div className="flex items-center gap-2">
          <span className="text-base font-semibold">{APP_NAME}</span>
          <Pill tone="neutral">v{diagnostics?.appVersion ?? "0.1.0"}</Pill>
        </div>
        <p className="hint">
          A local-first voice dictation and voice actions tool. Your speech is
          transcribed on this machine; nothing is uploaded unless you explicitly turn
          on an online AI provider.
        </p>
      </Card>

      <Card className="divide-y divide-border px-4">
        <FactRow label="Platform" value={capabilities?.platform ?? "unknown"} />
        <FactRow
          label="Accessibility integration"
          value={capabilities?.supports.accessibility ? "Available" : "Not available"}
          tone={capabilities?.supports.accessibility ? "success" : "warning"}
        />
        <FactRow
          label="Shortcuts automation"
          value={capabilities?.supports.shortcutsApp ? "Available" : "Not available"}
        />
        <FactRow
          label="Automatic updates"
          value={capabilities?.supports.autoUpdate ? "Enabled" : "Manual"}
        />
      </Card>

      <div className="flex items-center gap-2 text-xs text-muted">
        <Check className="h-3.5 w-3.5 text-success" aria-hidden />
        No account, no subscription, no telemetry.
      </div>
    </>
  );
}
