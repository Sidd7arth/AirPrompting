/**
 * Transient messages from the core (spec §61).
 *
 * Errors are never console-only: every failure the backend reports reaches the
 * user here, with the actionable sentence the Rust error type provides.
 */
import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, Check, Info, X, XCircle } from "lucide-react";
import { useApp } from "@/stores/appStore";
import clsx from "clsx";

const TONE_STYLES = {
  info: "border-border text-fg",
  success: "border-success/40 text-fg",
  warning: "border-warning/40 text-fg",
  error: "border-danger/50 text-fg"
} as const;

const TONE_ICONS = {
  info: Info,
  success: Check,
  warning: AlertTriangle,
  error: XCircle
} as const;

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);

  return (
    <div
      className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-80 flex-col gap-2"
      role="status"
      aria-live="polite"
    >
      <AnimatePresence initial={false}>
        {toasts.map((toast) => {
          const Icon = TONE_ICONS[toast.level];
          return (
            <motion.div
              key={toast.id}
              layout
              initial={{ opacity: 0, y: 8, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 4, scale: 0.98 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
              className={clsx(
                "pointer-events-auto flex items-start gap-2.5 rounded-xl border bg-surface/98 p-3 shadow-xl backdrop-blur",
                TONE_STYLES[toast.level]
              )}
            >
              <Icon
                className={clsx(
                  "mt-0.5 h-4 w-4 shrink-0",
                  toast.level === "error"
                    ? "text-danger"
                    : toast.level === "success"
                      ? "text-success"
                      : toast.level === "warning"
                        ? "text-warning"
                        : "text-faint"
                )}
                aria-hidden
              />
              <p className="min-w-0 flex-1 text-xs leading-relaxed">{toast.message}</p>
              <button
                type="button"
                onClick={() => dismiss(toast.id)}
                aria-label="Dismiss"
                className="rounded p-0.5 text-faint hover:text-fg"
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </button>
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
