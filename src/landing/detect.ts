/**
 * OS detection from the user agent, for the download CTA.
 *
 * Deliberately conservative: an unrecognised agent is "unknown", and the page
 * shows both platform choices rather than guessing. The user agent is a hint,
 * not an authority — the fallback UI always remains one click away.
 */
export type DetectedOs = "macos" | "windows" | "unknown";

export function detectOs(userAgent: string): DetectedOs {
  const ua = userAgent.toLowerCase();
  // Order matters: "cros" etc. must never match before the platforms below,
  // and iPadOS 13+ masquerades as desktop Safari, hence the touch check.
  if (ua.includes("mac") && !ua.includes("iphone") && !ua.includes("ipad")) {
    return "macos";
  }
  if (ua.includes("windows")) {
    return "windows";
  }
  return "unknown";
}

/** True when the user agent suggests a phone or tablet — where installers are useless. */
export function isMobileish(userAgent: string): boolean {
  return /iphone|ipad|ipod|android|mobile/.test(userAgent.toLowerCase());
}
