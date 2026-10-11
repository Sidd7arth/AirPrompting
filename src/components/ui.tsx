/**
 * Shared UI primitives.
 *
 * Deliberately small: a handful of Radix-backed controls plus layout helpers.
 * Pages compose these instead of styling raw elements, which is what keeps the
 * two themes consistent (spec §71) and the keyboard behaviour correct (§102).
 */
import * as React from "react";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as SelectPrimitive from "@radix-ui/react-select";
import * as SliderPrimitive from "@radix-ui/react-slider";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as SeparatorPrimitive from "@radix-ui/react-separator";
import { Check, ChevronDown, Info } from "lucide-react";
import clsx from "clsx";

/* ------------------------------------------------------------------ layout */

export function Card({
  className,
  children,
  ...rest
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={clsx("card", className)} {...rest}>
      {children}
    </div>
  );
}

export function SectionHeading({
  title,
  description
}: {
  title: string;
  description?: string;
}) {
  return (
    <div className="mb-3">
      <h2 className="section-title">{title}</h2>
      {description ? <p className="hint mt-0.5">{description}</p> : null}
    </div>
  );
}

export function Separator({ className }: { className?: string }) {
  return (
    <SeparatorPrimitive.Root className={clsx("h-px w-full bg-border", className)} />
  );
}

/* ------------------------------------------------------------------ button */

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md";
}

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: "btn-primary",
  secondary: "btn-secondary",
  ghost: "btn-ghost",
  danger: "btn-danger"
};

export function Button({
  variant = "secondary",
  size = "md",
  className,
  ...rest
}: ButtonProps) {
  return (
    <button
      type="button"
      className={clsx(
        BUTTON_VARIANTS[variant],
        size === "sm" && "px-2 py-1 text-xs",
        className
      )}
      {...rest}
    />
  );
}

/* ------------------------------------------------------------------- field */

export function Field({
  label,
  hint,
  children,
  htmlFor
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="space-y-1.5">
      <label className="label block" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
      {hint ? <p className="hint">{hint}</p> : null}
    </div>
  );
}

/* ------------------------------------------------------------------ switch */

export function Toggle({
  checked,
  onCheckedChange,
  disabled,
  id,
  label
}: {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
  id?: string;
  /** Accessible name when the switch has no visible label of its own. */
  label: string;
}) {
  return (
    <SwitchPrimitive.Root
      id={id}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={label}
      className={clsx(
        "relative h-5 w-9 shrink-0 rounded-full border border-transparent transition-colors",
        "data-[state=checked]:bg-accent data-[state=unchecked]:bg-border",
        "disabled:opacity-50"
      )}
    >
      <SwitchPrimitive.Thumb
        className={clsx(
          "block h-4 w-4 rounded-full bg-white shadow transition-transform",
          "translate-x-0.5 data-[state=checked]:translate-x-[1.15rem]"
        )}
      />
    </SwitchPrimitive.Root>
  );
}

/** A labelled settings row: description on the left, control on the right. */
export function ToggleRow({
  title,
  description,
  checked,
  onCheckedChange,
  disabled
}: {
  title: string;
  description?: string;
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  const id = React.useId();
  return (
    <div className="row">
      <div className="min-w-0">
        <label htmlFor={id} className="cursor-pointer text-sm text-fg">
          {title}
        </label>
        {description ? <p className="hint mt-0.5">{description}</p> : null}
      </div>
      <Toggle
        id={id}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        label={title}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ select */

export interface SelectOption {
  value: string;
  label: string;
  description?: string;
}

export function Select({
  value,
  onValueChange,
  options,
  placeholder = "Select…",
  disabled,
  ariaLabel
}: {
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  ariaLabel: string;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={onValueChange} disabled={disabled}>
      <SelectPrimitive.Trigger
        aria-label={ariaLabel}
        className="input flex items-center justify-between gap-2 text-left"
      >
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon>
          <ChevronDown className="h-3.5 w-3.5 text-faint" aria-hidden />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={4}
          className="z-50 max-h-72 min-w-[12rem] overflow-hidden rounded-lg border border-border bg-surface shadow-xl"
        >
          <SelectPrimitive.Viewport className="p-1">
            {options.map((option) => (
              <SelectPrimitive.Item
                key={option.value}
                value={option.value}
                className={clsx(
                  "relative flex cursor-default select-none items-center gap-2 rounded-md px-2 py-1.5 text-sm",
                  "data-[highlighted]:bg-elevated data-[highlighted]:outline-none"
                )}
              >
                <SelectPrimitive.ItemIndicator>
                  <Check className="h-3.5 w-3.5 text-accent" aria-hidden />
                </SelectPrimitive.ItemIndicator>
                <div className="min-w-0">
                  <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
                  {option.description ? (
                    <p className="hint">{option.description}</p>
                  ) : null}
                </div>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

/* ------------------------------------------------------------------ slider */

export function Slider({
  value,
  onValueChange,
  min,
  max,
  step,
  ariaLabel
}: {
  value: number;
  onValueChange: (value: number) => void;
  min: number;
  max: number;
  step: number;
  ariaLabel: string;
}) {
  return (
    <SliderPrimitive.Root
      value={[value]}
      min={min}
      max={max}
      step={step}
      aria-label={ariaLabel}
      onValueChange={([next]) => onValueChange(next ?? value)}
      className="relative flex h-5 w-full touch-none items-center"
    >
      <SliderPrimitive.Track className="relative h-1 w-full grow rounded-full bg-border">
        <SliderPrimitive.Range className="absolute h-full rounded-full bg-accent" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb
        aria-label={ariaLabel}
        className="block h-3.5 w-3.5 rounded-full border border-border bg-surface shadow"
      />
    </SliderPrimitive.Root>
  );
}

/* ------------------------------------------------------------------ status */

export type PillTone = "neutral" | "accent" | "success" | "warning" | "danger";

const PILL_TONES: Record<PillTone, string> = {
  neutral: "border-border text-muted",
  accent: "border-accent/40 text-accent",
  success: "border-success/40 text-success",
  warning: "border-warning/40 text-warning",
  danger: "border-danger/40 text-danger"
};

export function Pill({
  tone = "neutral",
  children,
  className
}: {
  tone?: PillTone;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      className={clsx(
        "inline-flex items-center gap-1.5 rounded-full border bg-surface px-2 py-0.5 text-2xs font-medium",
        PILL_TONES[tone],
        className
      )}
    >
      {children}
    </span>
  );
}

/** A small coloured dot; the only decorative element the app uses. */
export function Dot({ tone = "neutral" }: { tone?: PillTone }) {
  const colour =
    tone === "success"
      ? "bg-success"
      : tone === "danger"
        ? "bg-danger"
        : tone === "warning"
          ? "bg-warning"
          : tone === "accent"
            ? "bg-accent"
            : "bg-faint";
  return <span className={clsx("h-1.5 w-1.5 shrink-0 rounded-full", colour)} aria-hidden />;
}

/* ----------------------------------------------------------------- tooltip */

export function Tooltip({
  content,
  children
}: {
  content: string;
  children: React.ReactNode;
}) {
  return (
    <TooltipPrimitive.Root delayDuration={350}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          sideOffset={6}
          className="z-50 max-w-xs rounded-md border border-border bg-elevated px-2 py-1 text-xs text-fg shadow-lg"
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export function TooltipProvider({ children }: { children: React.ReactNode }) {
  return <TooltipPrimitive.Provider>{children}</TooltipPrimitive.Provider>;
}

/* --------------------------------------------------------------- bits */

export function InfoNote({
  tone = "neutral",
  children
}: {
  tone?: PillTone;
  children: React.ReactNode;
}) {
  return (
    <div
      className={clsx(
        "flex items-start gap-2 rounded-lg border p-2.5 text-xs leading-relaxed",
        tone === "danger"
          ? "border-danger/30 bg-danger/5 text-fg"
          : tone === "warning"
            ? "border-warning/30 bg-warning/5 text-fg"
            : tone === "accent"
              ? "border-accent/30 bg-accent/5 text-fg"
              : "border-border bg-elevated text-muted"
      )}
    >
      <Info className="mt-0.5 h-3.5 w-3.5 shrink-0 text-faint" aria-hidden />
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function EmptyState({
  title,
  description,
  action
}: {
  title: string;
  description: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
      <p className="text-sm font-medium text-fg">{title}</p>
      <p className="hint max-w-sm">{description}</p>
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

export function ProgressBar({ value }: { value: number }) {
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-border">
      <div
        className="h-full rounded-full bg-accent transition-[width] duration-200"
        style={{ width: `${Math.round(Math.min(Math.max(value, 0), 1) * 100)}%` }}
      />
    </div>
  );
}

export function KeyCap({ children }: { children: React.ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}
