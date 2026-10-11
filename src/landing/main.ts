/**
 * The landing page itself.
 *
 * Three CTA states, each honest:
 *   resolving   — brief, while the Releases API answers
 *   resolved    — direct download for the detected OS + the other choice
 *   none        — the fallback: explicit platform links to the releases index
 *
 * The fallback is the *initial* state in practice today (no installers exist
 * yet), so it is styled as a deliberate "releases are coming from CI" panel,
 * not an error.
 */

import "../styles/globals.css";
import { APP_NAME } from "../identity";
import { detectOs, isMobileish, type DetectedOs } from "./detect";
import {
  fallbackUrl,
  resolveLatestRelease,
  type ResolvedDownload,
} from "./release";

/* ------------------------------------------------------------------ copy --- */

const HERO_TITLE = "Press. Speak. Done.";
const HERO_SUB =
  "Hold a shortcut, talk, and your words appear in whatever app you're using. " +
  "Everything runs on your machine — no account, no subscription, no telemetry.";

const STEPS = [
  {
    title: "Press and speak",
    body: "Hold the dictation shortcut and talk normally. A small overlay shows the words as they arrive.",
  },
  {
    title: "Your words appear",
    body: "Release the key and the finished transcript is inserted into the app you were using — formatting cleaned up, fillers removed.",
  },
  {
    title: "Your data stays yours",
    body: "Speech recognition runs locally with whisper.cpp. Optional AI cleanup can point at your own local server. Nothing is uploaded to us — there is no us to upload to.",
  },
] as const;

const NEXT_STEPS = [
  "Open the downloaded file and install AirPrompt.",
  "Launch it and allow microphone access when asked (macOS also asks for Accessibility — the app explains why).",
  "Install a speech model from the Models tab (the small one is a fine start).",
  "Click into any text field, hold the shortcut, and speak.",
] as const;

/* ----------------------------------------------------------------- html --- */

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function heroSection(): HTMLElement {
  const hero = el("section", "py-20 md:py-28 text-center");

  const badge = el(
    "span",
    "inline-block rounded-full border border-border bg-elevated px-3 py-1 text-xs text-muted mb-6"
  );
  badge.textContent = `${APP_NAME} · local-first voice dictation`;

  const title = el(
    "h1",
    "text-4xl md:text-6xl font-semibold tracking-tight text-fg mb-5"
  );
  title.textContent = HERO_TITLE;

  const sub = el("p", "max-w-2xl mx-auto text-lg text-muted leading-relaxed");
  sub.textContent = HERO_SUB;

  hero.append(badge, title, sub, ctaRoot());
  return hero;
}

/** The one root the CTA states render into. Marked for the playtest. */
function ctaRoot(): HTMLElement {
  return el("div", "mt-10 max-w-xl mx-auto", undefined);
}

function stepsSection(): HTMLElement {
  const section = el("section", "py-14 border-t border-border");
  const grid = el("div", "grid md:grid-cols-3 gap-8 max-w-5xl mx-auto px-6");
  STEPS.forEach((step, i) => {
    const card = el("div", "text-left");
    const num = el(
      "div",
      "text-accent text-sm font-semibold mb-2",
      `0${i + 1}`
    );
    card.append(
      num,
      el("h3", "text-fg font-medium mb-2", step.title),
      el("p", "text-sm text-muted leading-relaxed", step.body)
    );
    grid.append(card);
  });
  section.append(grid);
  return section;
}

function footerSection(): HTMLElement {
  const footer = el("footer", "py-10 border-t border-border text-center");
  footer.append(
    el(
      "p",
      "text-xs text-faint",
      `${APP_NAME} is free, open source, and does not phone home. Speech models are downloaded from Hugging Face; AI cleanup is optional and can point at your own machine.`
    )
  );
  return footer;
}

/* ------------------------------------------------------------------ CTA --- */

function platformLabel(platform: "macos" | "windows"): string {
  return platform === "macos" ? "macOS" : "Windows";
}

/**
 * One download button, wired to the confirmation dialog.
 *
 * `primary` is the filled button for the visitor's detected platform.
 * `choice` is the same button without the emphasis: used when the platform was
 * *not* detected, so the page states no preference it cannot justify.
 */
function downloadButton(
  download: ResolvedDownload,
  confirm: (d: ResolvedDownload) => void,
  variant: "primary" | "secondary" | "choice"
): HTMLElement {
  const button = el(
    "button",
    variant === "primary"
      ? "rounded-xl bg-accent px-6 py-3 text-accent-fg font-medium hover:opacity-90 transition-opacity"
      : "rounded-xl border border-border px-6 py-3 text-fg hover:bg-elevated transition-colors"
  );
  const label =
    variant === "secondary"
      ? `Also for ${platformLabel(download.platform)}`
      : `Download for ${platformLabel(download.platform)}`;
  button.textContent = label;
  button.addEventListener("click", () => confirm(download));
  return button;
}

/**
 * The fallback panel: explicit platform links to the releases index.
 *
 * This is the real initial state until CI has produced artifacts, so it reads
 * as intentional: what exists, what does not, and where releases will land.
 */
function fallbackPanel(detected: DetectedOs): HTMLElement {
  const panel = el(
    "div",
    "rounded-2xl border border-border bg-surface p-6 text-left"
  );

  const head = el("h2", "text-fg font-medium mb-1", "Installers are not published yet");
  const note = el(
    "p",
    "text-sm text-muted mb-4 leading-relaxed",
    `${APP_NAME} builds its installers automatically on GitHub for every release. None has been published yet — when one is, this button becomes a direct download.`
  );

  const row = el("div", "flex flex-wrap gap-3");
  for (const platform of ["macos", "windows"] as const) {
    const link = el(
      "a",
      "rounded-xl border border-border px-5 py-2.5 text-sm text-fg hover:bg-elevated transition-colors"
    );
    link.href = fallbackUrl();
    link.rel = "noopener";
    const label = platformLabel(platform);
    link.textContent =
      detected === platform
        ? `${label} releases →`
        : `${label} releases`;
    row.append(link);
  }

  const viewAll = el("a", "inline-block mt-4 text-sm text-accent hover:underline");
  viewAll.href = fallbackUrl();
  viewAll.rel = "noopener";
  viewAll.textContent = "View all releases on GitHub →";

  panel.append(head, note, row, viewAll);
  return panel;
}

/** Brief state while the API answers. */
function resolvingPanel(): HTMLElement {
  return el(
    "div",
    "rounded-2xl border border-border bg-surface p-6 text-sm text-muted",
    "Checking for the latest release…"
  );
}

/* --------------------------------------------------------- confirmation --- */

/**
 * Download-started confirmation: what the file is and what happens next.
 * Triggered by a click, so the actual download is a user gesture.
 */
function showConfirmation(download: ResolvedDownload): void {
  document.getElementById("airprompt-confirm")?.remove();

  const overlay = el(
    "div",
    "fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  );
  overlay.id = "airprompt-confirm";

  const dialog = el(
    "div",
    "rounded-2xl bg-surface border border-border p-6 max-w-md w-full text-left"
  );

  dialog.append(
    el(
      "h2",
      "text-lg font-medium text-fg mb-2",
      `Downloading ${download.fileName}`
    ),
    el(
      "p",
      "text-sm text-muted mb-4",
      `Release ${download.releaseTag} for ${platformLabel(download.platform)}.` +
        (download.prerelease ? " This is a prerelease — expect rough edges." : "")
    )
  );

  const list = el("ol", "list-decimal list-inside text-sm text-muted space-y-1.5 mb-5");
  NEXT_STEPS.forEach((step) => list.append(el("li", "", step)));
  dialog.append(list);

  const row = el("div", "flex justify-end gap-3");
  const cancel = el(
    "button",
    "rounded-lg border border-border px-4 py-2 text-sm text-fg hover:bg-elevated"
  );
  cancel.textContent = "Cancel";
  cancel.addEventListener("click", () => overlay.remove());

  const go = el(
    "button",
    "rounded-lg bg-accent px-4 py-2 text-sm text-accent-fg font-medium hover:opacity-90"
  );
  go.textContent = "Download";
  go.addEventListener("click", () => {
    // Programmatic click on a real anchor: the browser treats it as a download.
    const a = document.createElement("a");
    a.href = download.url;
    a.download = download.fileName;
    a.rel = "noopener";
    document.body.append(a);
    a.click();
    a.remove();
    overlay.remove();
  });

  row.append(cancel, go);
  dialog.append(row);
  overlay.append(dialog);
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      overlay.remove();
      cleanup();
    }
  });
  // Escape closes, like Cancel. The listener removes itself so rapid
  // open/close cycles never stack handlers.
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      overlay.remove();
      cleanup();
    }
  };
  const cleanup = () => document.removeEventListener("keydown", onKey);
  document.addEventListener("keydown", onKey);
  document.body.append(overlay);
  go.focus();
}

/* ------------------------------------------------------------------ main --- */

function renderResolution(
  root: HTMLElement,
  resolution: Awaited<ReturnType<typeof resolveLatestRelease>>,
  detected: DetectedOs
): void {
  root.textContent = "";

  if (resolution.kind === "none" || resolution.downloads.length === 0) {
    root.append(fallbackPanel(detected));
    return;
  }

  // No platform detected (Linux, an unusual browser, a phone): guessing would
  // put a confident "Download for macOS" in front of someone who may not use a
  // Mac. Present both installers as equal choices instead.
  if (detected === "unknown") {
    root.append(choicePanel(resolution.downloads));
    return;
  }

  const preferred =
    resolution.downloads.find((d) => d.platform === detected) ??
    resolution.downloads[0];
  if (!preferred) {
    root.append(fallbackPanel(detected));
    return;
  }
  const other = resolution.downloads.find((d) => d !== preferred);

  const panel = el("div", "flex flex-col items-center gap-3");
  panel.append(
    downloadButton(preferred, showConfirmation, "primary"),
    el(
      "p",
      "text-xs text-faint",
      `${preferred.fileName} · release ${preferred.releaseTag}`
    )
  );
  if (other) panel.append(downloadButton(other, showConfirmation, "secondary"));

  // A phone can install nothing here, whatever the OS said.
  if (isMobileish(navigator.userAgent)) {
    panel.append(
      el(
        "p",
        "text-xs text-faint",
        "AirPrompt is a desktop app for macOS and Windows — open this page on your computer to install."
      )
    );
  }
  root.append(panel);
}

/**
 * The undetected-platform panel: both installers, side by side, no preference.
 * Same dialog behind each button as the detected path.
 */
function choicePanel(downloads: ResolvedDownload[]): HTMLElement {
  const panel = el("div", "flex flex-col items-center gap-3");
  panel.append(
    el(
      "p",
      "text-sm text-muted",
      `${APP_NAME} runs on macOS and Windows — choose the installer for this computer:`
    )
  );

  const row = el("div", "flex flex-wrap justify-center gap-3");
  for (const download of downloads) {
    row.append(downloadButton(download, showConfirmation, "choice"));
  }
  panel.append(row);

  const first = downloads[0];
  if (first) {
    panel.append(
      el("p", "text-xs text-faint", `release ${first.releaseTag}`)
    );
  }
  return panel;
}

function main(): void {
  const mount = document.getElementById("landing");
  if (!mount) return;

  const page = el("div", "min-h-full bg-bg text-fg");
  page.append(heroSection(), stepsSection(), footerSection());
  mount.append(page);

  const cta = page.querySelector(".mt-10.max-w-xl");
  if (!(cta instanceof HTMLElement)) return;

  const detected = detectOs(navigator.userAgent);
  cta.append(resolvingPanel());
  void resolveLatestRelease().then((resolution) =>
    renderResolution(cta, resolution, detected)
  );
}

main();
