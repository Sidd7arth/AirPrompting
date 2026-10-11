/** Small presentation helpers shared across pages. */

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 MB";
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${Math.round(bytes / 1024)} KB`;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return "0:00";
  const total = Math.round(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function formatLatency(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

/** "Today · 3:42 PM" / "Yesterday · …" / "12 Mar · 3:42 PM". */
export function formatHistoryStamp(unixMs: number): string {
  const date = new Date(unixMs);
  const now = new Date();
  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);

  const time = date.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit"
  });

  if (dayDiff === 0) return `Today · ${time}`;
  if (dayDiff === 1) return `Yesterday · ${time}`;
  const day = date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  return `${day} · ${time}`;
}

/** Bucket label used as a group header in the history list. */
export function historyGroupLabel(unixMs: number): string {
  const date = new Date(unixMs);
  const now = new Date();
  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (dayDiff === 0) return "Today";
  if (dayDiff === 1) return "Yesterday";
  if (dayDiff < 7) return `${dayDiff} days ago`;
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric"
  });
}

const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/**
 * Turn a backend accelerator ("CmdOrCtrl+Shift+Space") into display chips,
 * using the platform's own modifier glyphs.
 */
export function acceleratorChips(accelerator: string): string[] {
  if (!accelerator) return [];
  return accelerator.split("+").map((part) => {
    const key = part.trim();
    switch (key.toLowerCase()) {
      case "cmdorctrl":
      case "commandorcontrol":
        return IS_MAC ? "⌘" : "Ctrl";
      case "cmd":
      case "command":
      case "super":
      case "meta":
        return IS_MAC ? "⌘" : "Win";
      case "ctrl":
      case "control":
        return IS_MAC ? "⌃" : "Ctrl";
      case "alt":
      case "option":
        return IS_MAC ? "⌥" : "Alt";
      case "shift":
        return IS_MAC ? "⇧" : "Shift";
      case "space":
        return "Space";
      case "enter":
      case "return":
        return IS_MAC ? "↩" : "Enter";
      case "escape":
        return "Esc";
      case "tab":
        return "Tab";
      case "backspace":
        return IS_MAC ? "⌫" : "Backspace";
      default:
        return key.length === 1 ? key.toUpperCase() : key;
    }
  });
}

export function titleCase(value: string): string {
  return value.replace(/[_-]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** A short, safe single-line preview of a transcript. */
export function preview(text: string, max = 140): string {
  const collapsed = text.replace(/\s+/g, " ").trim();
  return collapsed.length > max ? `${collapsed.slice(0, max - 1)}…` : collapsed;
}
