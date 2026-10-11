/**
 * Shortcut capture control (spec §24).
 *
 * Records a real key combination and hands back an accelerator string in the
 * same grammar the Rust shortcut manager uses. Modifier-only presses are
 * rejected so the user cannot bind a bare Shift.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, KeyCap } from "@/components/ui";
import { acceleratorChips } from "@/services/format";

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph"]);

/** Map a DOM key name onto the accelerator token Tauri expects. */
function keyToken(event: KeyboardEvent): string | null {
  const { key, code } = event;
  if (MODIFIER_KEYS.has(key)) return null;

  if (key === " " || code === "Space") return "Space";
  if (key === "Escape") return null; // reserved for cancelling
  if (key === "Enter") return "Enter";
  if (key === "Tab") return "Tab";
  if (key === "Backspace") return "Backspace";
  if (key === "Delete") return "Delete";
  if (key.startsWith("Arrow")) return key.replace("Arrow", "");
  if (key.length === 1) return key.toUpperCase();
  // F1..F24, PageUp, Home, etc. pass through unchanged.
  if (/^F\d{1,2}$/.test(key)) return key;
  if (/^[A-Za-z]+$/.test(key)) return key;
  return null;
}

export function ShortcutRecorder({
  value,
  onChange,
  onCancel
}: {
  /** Current accelerator, or "" when unset. */
  value: string;
  onChange: (accelerator: string) => void;
  /** Called when the user abandons capture. */
  onCancel?: () => void;
}) {
  const [recording, setRecording] = useState(false);
  const [preview, setPreview] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const stop = useCallback(() => {
    setRecording(false);
    setPreview([]);
  }, []);

  useEffect(() => {
    if (!recording) return;

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        stop();
        onCancel?.();
        return;
      }

      const parts: string[] = [];
      if (event.ctrlKey) parts.push("Ctrl");
      if (event.altKey) parts.push("Alt");
      if (event.shiftKey) parts.push("Shift");
      if (event.metaKey) parts.push("CmdOrCtrl");

      const token = keyToken(event);
      setPreview(token ? [...parts, token] : parts);

      if (!token) {
        setError("Add a letter, number or function key — modifiers alone are not enough.");
        return;
      }
      if (parts.length === 0) {
        setError("Include at least one modifier (Ctrl, Alt, Shift or Command).");
        return;
      }

      setError(null);
      onChange([...parts, token].join("+"));
      stop();
    };

    const onKeyUp = (event: KeyboardEvent) => {
      // Cancel if the user releases everything without completing a combo.
      if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        !event.metaKey &&
        preview.length > 0 &&
        preview.length < 4
      ) {
        // keep waiting: they may press another modifier next
      }
    };

    const onClickAway = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        stop();
        onCancel?.();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("mousedown", onClickAway, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("mousedown", onClickAway, true);
    };
  }, [recording, onChange, onCancel, stop, preview.length]);

  return (
    <div className="space-y-1.5" ref={containerRef}>
      <div className="flex items-center gap-2">
        {recording ? (
          <div className="input flex flex-1 items-center gap-1.5 text-faint">
            {preview.length > 0 ? (
              preview.map((token) => <KeyCap key={token}>{token}</KeyCap>)
            ) : (
              <span className="text-xs">Press the keys you want to use…</span>
            )}
          </div>
        ) : value ? (
          <div className="input flex flex-1 items-center gap-1.5">
            {acceleratorChips(value).map((chip, index) => (
              <KeyCap key={`${chip}-${index}`}>{chip}</KeyCap>
            ))}
          </div>
        ) : (
          <div className="input flex flex-1 items-center text-xs text-faint">
            Not set
          </div>
        )}

        <Button onClick={() => (recording ? stop() : setRecording(true))}>
          {recording ? "Cancel" : "Change"}
        </Button>
      </div>

      {error ? <p className="text-xs text-danger">{error}</p> : null}
      {recording ? (
        <p className="hint">Press Escape to cancel.</p>
      ) : null}
    </div>
  );
}
