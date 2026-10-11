/**
 * Live audio meter.
 *
 * Driven by real microphone levels from the Rust audio thread, not a CSS loop,
 * so a dead microphone is obvious: the bars stay flat. Bar heights keep a short
 * history so the meter reads as a waveform rather than a single dancing line.
 */
import { useEffect, useRef, useState } from "react";
import clsx from "clsx";

const BAR_COUNT = 28;
/** How quickly a bar falls back to zero; higher is snappier. */
const DECAY = 0.28;

export function Waveform({
  level,
  active,
  className,
  bars = BAR_COUNT
}: {
  /** Peak level of the most recent audio frame, 0..1. */
  level: number;
  active: boolean;
  className?: string;
  bars?: number;
}) {
  const [heights, setHeights] = useState<number[]>(() => new Array(bars).fill(0));
  const levelRef = useRef(level);
  levelRef.current = level;

  useEffect(() => {
    if (!active) {
      setHeights(new Array(bars).fill(0));
      return;
    }

    let frame = 0;
    const tick = () => {
      frame = window.requestAnimationFrame(tick);
      setHeights((previous) => {
        const current = levelRef.current;
        // A gentle per-bar offset makes a steady tone look alive without
        // randomising it into noise.
        const next = previous.map((value, index) => {
          const phase = Math.sin((performance.now() / 140 + index * 0.7) % Math.PI);
          const target = current * (0.45 + 0.55 * Math.abs(phase));
          return value + (target - value) * DECAY;
        });
        // Shift new energy in from the right so it scrolls like a waveform.
        next.pop();
        next.unshift(Math.min(current, 1));
        return next;
      });
    };

    frame = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frame);
  }, [active, bars]);

  return (
    <div
      className={clsx("flex h-6 items-center gap-[2px]", className)}
      aria-hidden
      data-testid="waveform"
    >
      {heights.map((height, index) => (
        <span
          key={index}
          className={clsx(
            "w-[3px] shrink-0 rounded-full transition-[height] duration-75",
            active ? "bg-recording/80" : "bg-border"
          )}
          style={{ height: `${Math.max(2, Math.min(height, 1) * 100)}%` }}
        />
      ))}
    </div>
  );
}

/**
 * A horizontal signal bar for the microphone test in Audio settings.
 * Shows the current level plus a plain-language verdict.
 */
export function LevelMeter({
  level,
  active,
  label
}: {
  level: number;
  active: boolean;
  label: string;
}) {
  const pct = Math.round(Math.min(Math.max(level, 0), 1) * 100);
  const verdict = !active
    ? "Not listening"
    : level < 0.01
      ? "No signal"
      : level < 0.05
        ? "Quiet signal"
        : level < 0.6
          ? "Good signal"
          : "Very loud — may clip";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <span className="label">{label}</span>
        <span
          className={clsx(
            "text-2xs",
            active && level >= 0.05 && level < 0.6
              ? "text-success"
              : active && level > 0
                ? "text-warning"
                : "text-faint"
          )}
        >
          {verdict}
        </span>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-border">
        <div
          className={clsx(
            "h-full rounded-full transition-[width] duration-75",
            level >= 0.6 ? "bg-warning" : "bg-accent"
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
