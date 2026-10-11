/**
 * Voice Actions (spec §27, §31, §32, §65).
 *
 * Shows the deterministic parser's real output before anything runs, so the
 * user can see exactly which intent was recognised and how confident it was.
 * Medium-risk actions are confirmed here rather than executed silently.
 */
import { useEffect, useState } from "react";
import { AlertTriangle, Command as CommandIcon, Play, Plus, Trash2 } from "lucide-react";

import {
  Button,
  Card,
  Dot,
  EmptyState,
  InfoNote,
  Pill,
  SectionHeading,
  Separator,
  ToggleRow
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api, type ActionOutcome } from "@/services/ipc";
import { useApp } from "@/stores/appStore";
import type { ActionStep, CommandIntent, CustomCommand, ParsedCommand } from "@/types";

const INTENT_LABEL: Record<CommandIntent, string> = {
  open_app: "Open an application",
  open_url: "Open a website",
  open_file: "Open a file",
  open_folder: "Open a folder",
  run_shortcut: "Run a macOS Shortcut",
  copy_text: "Copy text",
  search_web: "Search the web",
  dictate: "Start dictating",
  stop_dictation: "Stop dictating",
  change_model: "Change the speech model",
  open_settings: "Open settings"
};

export function Commands() {
  const pushToast = useApp((s) => s.pushToast);

  const [commands, setCommands] = useState<CustomCommand[]>([]);
  const [apps, setApps] = useState<string[]>([]);
  const [shortcuts, setShortcuts] = useState<string[]>([]);
  const [phrase, setPhrase] = useState("");
  const [parsed, setParsed] = useState<ParsedCommand | null>(null);
  const [outcome, setOutcome] = useState<ActionOutcome | null>(null);
  const [pending, setPending] = useState(false);

  const reload = async () => {
    try {
      const [list, targets] = await Promise.all([
        api.commandsList(),
        api.discoverTargets()
      ]);
      setCommands(list);
      setApps(targets.apps);
      setShortcuts(targets.shortcuts);
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

  const tryParse = async (text: string) => {
    setPhrase(text);
    setOutcome(null);
    if (!text.trim()) {
      setParsed(null);
      return;
    }
    try {
      setParsed(await api.parseCommand(text));
    } catch {
      setParsed(null);
    }
  };

  const run = async () => {
    if (!parsed) return;
    setPending(true);
    try {
      const result = await api.executeCommand(parsed);
      setOutcome(result);
      if (result.ok) {
        pushToast({ level: "success", message: result.message });
      }
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setPending(false);
    }
  };

  const addCommand = async () => {
    const command: CustomCommand = {
      id: crypto.randomUUID(),
      name: "New command",
      triggers: ["new command"],
      steps: [{ intent: "open_app", target: "" }],
      enabled: true,
      requiresConfirmation: false
    };
    try {
      await api.commandsUpsert(command);
      await reload();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const saveCommand = async (command: CustomCommand) => {
    try {
      await api.commandsUpsert(command);
      await reload();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const deleteCommand = async (id: string) => {
    try {
      await api.commandsDelete(id);
      await reload();
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  return (
    <div className="space-y-6">
      <SectionHeading
        title="Voice Actions"
        description={`Press the command shortcut, say a phrase, and ${APP_NAME} performs a safe, predefined action. Nothing is ever run through a shell.`}
      />

      <InfoNote tone="accent">
        Commands are matched locally and instantly. An AI model is never asked to
        produce a command, and the app cannot run arbitrary scripts.
      </InfoNote>

      <Card className="space-y-3 p-4">
        <p className="text-sm font-medium">Try a phrase</p>
        <div className="flex gap-2">
          <input
            className="input"
            placeholder="open chrome"
            aria-label="Command phrase"
            value={phrase}
            onChange={(event) => void tryParse(event.target.value)}
          />
          <Button
            variant="primary"
            disabled={!parsed || pending || parsed.confidence < 0.72}
            onClick={() => void run()}
          >
            <Play className="h-3.5 w-3.5" aria-hidden />
            Run
          </Button>
        </div>

        {phrase.trim() && !parsed ? (
          <p className="text-xs text-muted">
            Nothing recognised. Try “open Safari”, “open Downloads” or “run my Focus
            shortcut”.
          </p>
        ) : null}

        {parsed ? (
          <div className="space-y-2 rounded-lg border border-border bg-elevated p-3">
            <div className="flex flex-wrap items-center gap-2">
              <Pill tone="accent">{INTENT_LABEL[parsed.intent]}</Pill>
              <Pill tone={parsed.confidence >= 0.9 ? "success" : "warning"}>
                {Math.round(parsed.confidence * 100)}% confident
              </Pill>
              {parsed.risk === "medium" ? (
                <Pill tone="warning">
                  <AlertTriangle className="h-3 w-3" aria-hidden /> Asks first
                </Pill>
              ) : null}
              {parsed.local ? <Pill tone="neutral">Matched locally</Pill> : null}
            </div>
            <p className="text-sm">{parsed.summary}</p>
            {parsed.target ? (
              <p className="text-2xs text-faint">Target: {parsed.target}</p>
            ) : null}
            {parsed.confidence < 0.72 ? (
              <p className="text-xs text-warning">
                Low confidence — say it again more precisely.
              </p>
            ) : null}
          </div>
        ) : null}

        {outcome ? (
          <p className={outcome.ok ? "text-xs text-success" : "text-xs text-danger"}>
            {outcome.message}
          </p>
        ) : null}
      </Card>

      <Separator />

      <div className="flex items-center justify-between">
        <SectionHeading
          title="Your commands"
          description="Give a phrase to a sequence of actions you approved once."
        />
        <Button onClick={() => void addCommand()}>
          <Plus className="h-3.5 w-3.5" aria-hidden />
          Add command
        </Button>
      </div>

      {commands.length === 0 ? (
        <Card>
          <EmptyState
            title="No custom commands yet"
            description="A custom command maps phrases like “start coding” onto a fixed list of actions you choose."
            action={
              <Button variant="primary" onClick={() => void addCommand()}>
                Add your first command
              </Button>
            }
          />
        </Card>
      ) : (
        <div className="space-y-3">
          {commands.map((command) => (
            <CommandCard
              key={command.id}
              command={command}
              onSave={saveCommand}
              onDelete={deleteCommand}
            />
          ))}
        </div>
      )}

      <Separator />

      <Card className="divide-y divide-border px-4">
        <div className="row">
          <div>
            <p className="text-sm">Applications found</p>
            <p className="hint mt-0.5">
              {apps.length > 0
                ? apps.slice(0, 8).join(", ") + (apps.length > 8 ? "…" : "")
                : "None detected yet"}
            </p>
          </div>
          <span className="text-sm text-muted">{apps.length}</span>
        </div>
        <div className="row">
          <div>
            <p className="text-sm">Shortcuts available</p>
            <p className="hint mt-0.5">
              {shortcuts.length > 0
                ? shortcuts.slice(0, 8).join(", ")
                : "None found, or this platform has no Shortcuts app."}
            </p>
          </div>
          <span className="text-sm text-muted">{shortcuts.length}</span>
        </div>
      </Card>
    </div>
  );
}

function CommandCard({
  command,
  onSave,
  onDelete
}: {
  command: CustomCommand;
  onSave: (command: CustomCommand) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState(command);
  const [dirty, setDirty] = useState(false);

  const update = (next: Partial<CustomCommand>) => {
    setDraft((current) => ({ ...current, ...next }));
    setDirty(true);
  };

  const updateStep = (index: number, step: ActionStep) => {
    const steps = [...draft.steps];
    steps[index] = step;
    update({ steps });
  };

  return (
    <Card className="space-y-3 p-4">
      <div className="flex items-center gap-2">
        <CommandIcon className="h-4 w-4 shrink-0 text-faint" aria-hidden />
        <input
          className="input font-medium"
          value={draft.name}
          aria-label="Command name"
          onChange={(event) => update({ name: event.target.value })}
        />
        <Button
          variant="primary"
          size="sm"
          disabled={!dirty}
          onClick={() => void onSave(draft).then(() => setDirty(false))}
        >
          Save
        </Button>
        <Button
          variant="ghost"
          size="sm"
          aria-label="Delete command"
          onClick={() => void onDelete(draft.id)}
        >
          <Trash2 className="h-3.5 w-3.5" aria-hidden />
        </Button>
      </div>

      <div className="space-y-1.5">
        <label className="label">Trigger phrases (comma separated)</label>
        <input
          className="input"
          value={draft.triggers.join(", ")}
          aria-label="Trigger phrases"
          onChange={(event) =>
            update({
              triggers: event.target.value
                .split(",")
                .map((t) => t.trim())
                .filter(Boolean)
            })
          }
        />
      </div>

      <div className="space-y-2">
        <label className="label">Actions, run in order</label>
        {draft.steps.map((step, index) => (
          <div key={index} className="flex items-center gap-2">
            <select
              className="input w-44"
              value={step.intent}
              aria-label={`Action ${index + 1} type`}
              onChange={(event) =>
                updateStep(index, { ...step, intent: event.target.value as CommandIntent })
              }
            >
              {(Object.keys(INTENT_LABEL) as CommandIntent[])
                .filter((intent) => intent !== "stop_dictation")
                .map((intent) => (
                  <option key={intent} value={intent}>
                    {INTENT_LABEL[intent]}
                  </option>
                ))}
            </select>
            <input
              className="input"
              value={step.target}
              placeholder="Target"
              aria-label={`Action ${index + 1} target`}
              onChange={(event) => updateStep(index, { ...step, target: event.target.value })}
            />
            <Button
              variant="ghost"
              size="sm"
              aria-label="Remove action"
              onClick={() =>
                update({ steps: draft.steps.filter((_, i) => i !== index) })
              }
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </div>
        ))}
        <Button
          size="sm"
          onClick={() => update({ steps: [...draft.steps, { intent: "open_app", target: "" }] })}
        >
          <Plus className="h-3.5 w-3.5" aria-hidden />
          Add action
        </Button>
      </div>

      <div className="divide-y divide-border">
        <ToggleRow
          title="Enabled"
          checked={draft.enabled}
          onCheckedChange={(value) => update({ enabled: value })}
        />
        <ToggleRow
          title="Always ask before running"
          description="Recommended for anything that launches more than one app."
          checked={draft.requiresConfirmation}
          onCheckedChange={(value) => update({ requiresConfirmation: value })}
        />
      </div>

      <p className="flex items-center gap-1.5 text-2xs text-faint">
        <Dot tone="neutral" />
        Steps are executed by {APP_NAME} itself — never passed to a shell.
      </p>
    </Card>
  );
}
