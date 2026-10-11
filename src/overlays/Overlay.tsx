/**
 * The floating overlay (spec §17, §18, §108).
 *
 * Runs in its own always-on-top window that never takes keyboard focus. It
 * subscribes to the same core event channel as the dashboard, so there is
 * exactly one source of truth for what is happening.
 *
 * Animations stay in the 150–300 ms band and the layout never reflows: the
 * text area has a fixed height so showing a longer sentence cannot make the
 * window jump under the user's cursor.
 */
import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, Check, Loader2, Mic, Sparkles, Type } from "lucide-react";

import { Waveform } from "@/components/Waveform";
import { api } from "@/services/ipc";
import { useApp } from "@/stores/appStore";
import type { OverlayState } from "@/types";

export function Overlay() {
  const init = useApp((s) => s.init);
  const overlay = useApp((s) => s.overlay);
  const level = useApp((s) => s.level);
  const committed = useApp((s) => s.committed);
  const interim = useApp((s) => s.interim);
  const settings = useApp((s) => s.settings);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void init().then(() => {
      // Tell the backend the overlay is mounted, so it can show the next
      // recording without racing the window's creation.
      void api.overlayReady().catch(() => undefined);
    });
  }, [init]);

  // Keep the newest words in view without moving focus.
  useEffect(() => {
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [committed, interim]);

  const visible = overlay.state !== "hidden";

  return (
    <div className="flex h-full w-full items-end justify-center p-2">
      <AnimatePresence>
        {visible ? (
          <motion.div
            key="overlay"
            initial={{ opacity: 0, scale: 0.98 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.98 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="overlay-panel w-full max-w-xl overflow-hidden"
            style={{ opacity: settings?.appearance.overlayOpacity ?? 0.97 }}
            role="status"
            aria-live="polite"
          >
            <Header state={overlay} />

            <div
              ref={scrollRef}
              className="max-h-32 min-h-[3.25rem] overflow-y-auto px-4 pb-2"
            >
              {overlay.state === "error" ? (
                <p className="flex items-start gap-2 text-sm leading-relaxed text-danger">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
                  {overlay.message}
                </p>
              ) : overlay.state === "success" ? (
                <p className="text-sm leading-relaxed text-fg">{overlay.text}</p>
              ) : committed || interim ? (
                <p className="text-sm leading-relaxed text-fg">
                  {committed}
                  {committed && interim ? " " : ""}
                  {/* The unstable tail is dimmed, so the user can see what is
                      still being revised rather than guessing. */}
                  <span className="text-muted">{interim}</span>
                  <BlinkingCaret />
                </p>
              ) : (
                <p className="text-sm text-faint">Your words will appear here…</p>
              )}
            </div>

            {settings?.appearance.showWaveform !== false ? (
              <div className="px-4 pb-3">
                <Waveform
                  level={level}
                  active={overlay.state === "listening" || overlay.state === "starting"}
                />
              </div>
            ) : null}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function Header({ state }: { state: OverlayState }) {
  const { icon, label, tone } = describe(state);
  return (
    <div className="flex items-center gap-2 px-4 pb-1.5 pt-3">
      <span className={tone}>{icon}</span>
      <span className="text-xs font-medium tracking-tight text-fg">{label}</span>
    </div>
  );
}

function describe(state: OverlayState): {
  icon: React.ReactNode;
  label: string;
  tone: string;
} {
  switch (state.state) {
    case "starting":
      return {
        icon: <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />,
        label: "Starting…",
        tone: "text-muted"
      };
    case "listening":
      return {
        icon: <Mic className="h-3.5 w-3.5" aria-hidden />,
        label: "Listening",
        tone: "text-recording"
      };
    case "transcribing":
      return {
        icon: <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />,
        label: "Transcribing…",
        tone: "text-accent"
      };
    case "enhancing":
      return {
        icon: <Sparkles className="h-3.5 w-3.5" aria-hidden />,
        label: "Enhancing…",
        tone: "text-accent"
      };
    case "inserting":
      return {
        icon: <Type className="h-3.5 w-3.5" aria-hidden />,
        label: "Inserting…",
        tone: "text-accent"
      };
    case "success":
      return {
        icon: <Check className="h-3.5 w-3.5" aria-hidden />,
        label: "Inserted",
        tone: "text-success"
      };
    case "error":
      return {
        icon: <AlertTriangle className="h-3.5 w-3.5" aria-hidden />,
        label: "Something went wrong",
        tone: "text-danger"
      };
    default:
      return { icon: null, label: "", tone: "text-muted" };
  }
}

/** A caret that pulses only while new audio is arriving. */
function BlinkingCaret() {
  return (
    <span
      className="ml-0.5 inline-block h-3.5 w-0.5 animate-pulse-soft bg-accent align-middle"
      aria-hidden
    />
  );
}
