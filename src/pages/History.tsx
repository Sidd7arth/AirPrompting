/**
 * History (spec §36, §75).
 *
 * Search, copy, delete and export all hit the real SQLite store. Audio is never
 * stored, so there is nothing here but text the user chose to keep.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Copy, Download, Search, Trash2 } from "lucide-react";

import {
  Button,
  Card,
  Dot,
  EmptyState,
  InfoNote,
  Pill,
  SectionHeading,
  Select,
  ToggleRow
} from "@/components/ui";
import { APP_NAME } from "@/identity";
import { api } from "@/services/ipc";
import { formatDuration, formatHistoryStamp, historyGroupLabel, preview } from "@/services/format";
import { useApp } from "@/stores/appStore";
import type { HistoryEntry } from "@/types";

const RETENTION_OPTIONS = [
  { value: "0", label: "Never" },
  { value: "1", label: "After 1 day" },
  { value: "7", label: "After 7 days" },
  { value: "30", label: "After 30 days" }
];

export function HistoryPage() {
  const history = useApp((s) => s.history);
  const refreshHistory = useApp((s) => s.refreshHistory);
  const pushToast = useApp((s) => s.pushToast);
  const settings = useApp((s) => s.settings);
  const patchSettings = useApp((s) => s.patchSettings);

  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<HistoryEntry | null>(null);
  const [exportFormat, setExportFormat] = useState<"txt" | "markdown" | "json">("markdown");

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  // Debounce so typing does not hammer SQLite with a query per keystroke.
  useEffect(() => {
    const timer = window.setTimeout(() => void refreshHistory(search || undefined), 180);
    return () => window.clearTimeout(timer);
  }, [search, refreshHistory]);

  const groups = useMemo(() => {
    const map = new Map<string, HistoryEntry[]>();
    for (const entry of history) {
      const label = historyGroupLabel(entry.createdAt);
      const bucket = map.get(label);
      if (bucket) bucket.push(entry);
      else map.set(label, [entry]);
    }
    return [...map.entries()];
  }, [history]);

  const copy = useCallback(
    async (text: string) => {
      try {
        await api.copyToClipboard(text);
        pushToast({ level: "success", message: "Copied to the clipboard." });
      } catch {
        pushToast({ level: "error", message: "Could not copy to the clipboard." });
      }
    },
    [pushToast]
  );

  const remove = async (id: number) => {
    try {
      await api.historyDelete(id);
      if (selected?.id === id) setSelected(null);
      await refreshHistory(search || undefined);
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const removeAll = async () => {
    if (!window.confirm("Delete the entire dictation history? This cannot be undone.")) {
      return;
    }
    try {
      await api.historyDeleteAll();
      setSelected(null);
      await refreshHistory();
      pushToast({ level: "success", message: "History cleared." });
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const exportHistory = async () => {
    try {
      const path = await api.historyExport(exportFormat);
      pushToast({ level: "success", message: `Exported to ${path}` });
    } catch (error) {
      pushToast({
        level: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <SectionHeading
          title="History"
          description={`Saved locally in ${APP_NAME}'s database. Audio is never recorded.`}
        />
        <div className="flex shrink-0 items-center gap-2">
          <Select
            ariaLabel="Export format"
            value={exportFormat}
            onValueChange={(value) => setExportFormat(value as "txt" | "markdown" | "json")}
            options={[
              { value: "markdown", label: "Markdown" },
              { value: "txt", label: "Plain text" },
              { value: "json", label: "JSON" }
            ]}
          />
          <Button onClick={() => void exportHistory()} disabled={history.length === 0}>
            <Download className="h-3.5 w-3.5" aria-hidden />
            Export
          </Button>
        </div>
      </div>

      <div className="relative">
        <Search
          className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-faint"
          aria-hidden
        />
        <input
          className="input pl-8"
          placeholder="Search transcripts and applications…"
          value={search}
          aria-label="Search history"
          onChange={(event) => setSearch(event.target.value)}
        />
      </div>

      {settings ? (
        <Card className="divide-y divide-border px-4">
          <ToggleRow
            title="Save history"
            description="When off, nothing is written to the database."
            checked={settings.privacy.saveHistory}
            onCheckedChange={(value) =>
              void patchSettings((draft) => {
                draft.privacy.saveHistory = value;
                return draft;
              })
            }
          />
          <div className="row">
            <div>
              <p className="text-sm">Automatically delete after</p>
              <p className="hint mt-0.5">Applied when the app starts.</p>
            </div>
            <div className="w-44">
              <Select
                ariaLabel="Retention"
                value={String(settings.privacy.historyRetentionDays)}
                onValueChange={(value) =>
                  void patchSettings((draft) => {
                    draft.privacy.historyRetentionDays = Number(value);
                    return draft;
                  })
                }
                options={RETENTION_OPTIONS}
              />
            </div>
          </div>
        </Card>
      ) : null}

      {!settings?.privacy.saveHistory ? (
        <InfoNote tone="warning">
          History saving is off. Existing entries remain until you clear them.
        </InfoNote>
      ) : null}

      {history.length === 0 ? (
        <Card>
          <EmptyState
            title={search ? "No matches" : "Nothing here yet"}
            description={
              search
                ? "Try a different word, or clear the search to see everything."
                : "Dictate something with your shortcut and it will appear here."
            }
          />
        </Card>
      ) : (
        <div className="space-y-5">
          {groups.map(([label, entries]) => (
            <section key={label}>
              <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-faint">
                {label}
              </h3>
              <Card className="divide-y divide-border">
                {entries.map((entry) => (
                  <div key={entry.id} className="flex items-start gap-3 px-4 py-3">
                    <button
                      type="button"
                      onClick={() => setSelected(entry)}
                      className="min-w-0 flex-1 text-left"
                    >
                      <div className="flex flex-wrap items-center gap-2 text-2xs text-muted">
                        <span>{formatHistoryStamp(entry.createdAt)}</span>
                        {entry.appName ? <span>· {entry.appName}</span> : null}
                        {entry.durationMs > 0 ? (
                          <span>· {formatDuration(entry.durationMs)}</span>
                        ) : null}
                        {entry.enhanced ? (
                          <Pill tone="accent">
                            <Dot tone="accent" /> Enhanced
                          </Pill>
                        ) : null}
                        {entry.mode !== "standard" ? (
                          <Pill tone="neutral">{entry.mode}</Pill>
                        ) : null}
                      </div>
                      <p className="mt-1 line-clamp-2 text-sm leading-relaxed">
                        {preview(entry.transcript, 200)}
                      </p>
                    </button>
                    <div className="flex shrink-0 gap-1">
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label="Copy"
                        onClick={() => void copy(entry.transcript)}
                      >
                        <Copy className="h-3.5 w-3.5" aria-hidden />
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label="Delete"
                        onClick={() => void remove(entry.id)}
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden />
                      </Button>
                    </div>
                  </div>
                ))}
              </Card>
            </section>
          ))}

          <Button variant="danger" onClick={() => void removeAll()}>
            <Trash2 className="h-3.5 w-3.5" aria-hidden />
            Delete all history
          </Button>
        </div>
      )}

      {selected ? (
        <Card className="p-4">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="text-2xs text-muted">
                {formatHistoryStamp(selected.createdAt)}
                {selected.appName ? ` · ${selected.appName}` : ""} · model{" "}
                {selected.modelId || "unknown"}
              </p>
              <p className="mt-2 whitespace-pre-wrap text-sm leading-relaxed">
                {selected.transcript}
              </p>
            </div>
            <div className="flex shrink-0 flex-col gap-2">
              <Button size="sm" onClick={() => void copy(selected.transcript)}>
                Copy
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setSelected(null)}>
                Close
              </Button>
            </div>
          </div>
        </Card>
      ) : null}
    </div>
  );
}
